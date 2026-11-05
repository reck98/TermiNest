//! QEMU executable discovery, system resolution, and bounded capability probing.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::capabilities::{parse_accelerators, parse_version, QemuCapabilities, QemuVersionPolicy};
use super::error::QemuError;

/// Default timeout for probing QEMU commands (`--version` and `-accel help`).
pub const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum output buffer capture size for probing commands (64 KiB).
pub const MAX_PROBE_OUTPUT_BYTES: usize = 65_536;

/// Primary facade for QEMU executable discovery and environment capability validation.
pub struct QemuDiscovery;

impl QemuDiscovery {
    /// Resolves the QEMU binary path following strict search precedence:
    ///
    /// 1. Explicit path parameter (if provided).
    /// 2. `TERMINEST_QEMU_PATH` environment variable.
    /// 3. Platform well-known installation paths.
    /// 4. System `PATH` directories.
    ///
    /// Returns [`QemuError::BinaryNotFound`] if no valid executable can be located.
    pub fn find_executable(explicit_path: Option<&Path>) -> Result<PathBuf, QemuError> {
        find_executable_in(
            explicit_path,
            |var| std::env::var(var).ok(),
            &platform_system_paths(),
            &system_path_dirs(),
        )
    }

    /// Probes the capabilities of the QEMU binary at `path` by executing
    /// `--version` and `-accel help` with bounded timeouts and memory caps.
    pub fn probe_executable(path: &Path) -> Result<QemuCapabilities, QemuError> {
        probe_executable_with_runner(
            path,
            &ProcessCommandRunner,
            DEFAULT_PROBE_TIMEOUT,
            MAX_PROBE_OUTPUT_BYTES,
        )
    }

    /// Discovers the QEMU executable, probes its capabilities, enforces the
    /// version policy, and validates platform hardware acceleration support.
    pub fn discover(
        explicit_path: Option<&Path>,
        policy: Option<&QemuVersionPolicy>,
    ) -> Result<(PathBuf, QemuCapabilities), QemuError> {
        discover_with_runner(
            explicit_path,
            policy,
            &ProcessCommandRunner,
            DEFAULT_PROBE_TIMEOUT,
            MAX_PROBE_OUTPUT_BYTES,
        )
    }
}

/// Abstract runner for executing probing commands with timeout and output bounds.
pub(crate) trait CommandRunner: Send + Sync {
    fn run_bounded(
        &self,
        program: &Path,
        args: &[&str],
        timeout: Duration,
        max_bytes: usize,
    ) -> Result<String, QemuError>;
}

/// Production command runner executing real host subprocesses.
pub(crate) struct ProcessCommandRunner;

impl CommandRunner for ProcessCommandRunner {
    fn run_bounded(
        &self,
        program: &Path,
        args: &[&str],
        timeout: Duration,
        max_bytes: usize,
    ) -> Result<String, QemuError> {
        run_process_bounded(program, args, timeout, max_bytes)
    }
}

/// Result of draining a pipe stream with independent memory retention bounds.
#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct DrainResult {
    /// In-memory retained buffer, bounded to `max_bytes`.
    pub retained: Vec<u8>,
    /// Total number of bytes drained from the pipe before reaching EOF.
    pub total_bytes: usize,
    /// Whether the output exceeded `max_bytes` and was truncated in retention.
    pub truncated: bool,
}

/// Continuously drains a pipe stream to EOF in chunks, retaining at most `max_bytes`.
///
/// Excess bytes beyond `max_bytes` are safely discarded from memory while continuing
/// to drain the pipe, preventing the child process from blocking on a full OS pipe buffer.
fn drain_stream<R: std::io::Read>(mut reader: R, max_bytes: usize) -> std::io::Result<DrainResult> {
    let mut retained = Vec::new();
    let mut total_bytes = 0;
    let mut buf = [0u8; 8192];

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        total_bytes += n;
        if retained.len() < max_bytes {
            let space = max_bytes - retained.len();
            let to_copy = std::cmp::min(space, n);
            retained.extend_from_slice(&buf[..to_copy]);
        }
    }

    let truncated = total_bytes > max_bytes;
    Ok(DrainResult {
        retained,
        total_bytes,
        truncated,
    })
}

/// Executes a child process directly (without a shell) with concurrent stream draining,
/// bounded execution time, and independent memory retention bounds.
fn run_process_bounded(
    program: &Path,
    args: &[&str],
    timeout: Duration,
    max_bytes: usize,
) -> Result<String, QemuError> {
    run_process_bounded_with_env(program, args, &[], timeout, max_bytes)
}

/// Executes a child process directly with optional environment variables, concurrent pipe draining,
/// bounded execution time, and process reaping on timeout.
pub fn run_process_bounded_with_env(
    program: &Path,
    args: &[&str],
    envs: &[(&str, &str)],
    timeout: Duration,
    max_bytes: usize,
) -> Result<String, QemuError> {
    use std::process::{Command, Stdio};

    let mut cmd = Command::new(program);
    cmd.args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            QemuError::BinaryNotFound(program.to_path_buf())
        } else {
            QemuError::System(format!("Failed to spawn {}: {e}", program.display()))
        }
    })?;

    let stdout_pipe = child.stdout.take().ok_or_else(|| {
        QemuError::System("Failed to capture stdout pipe from child process".into())
    })?;
    let stderr_pipe = child.stderr.take().ok_or_else(|| {
        QemuError::System("Failed to capture stderr pipe from child process".into())
    })?;

    // Spawn TWO concurrent reader threads to drain stdout and stderr simultaneously,
    // completely eliminating pipe buffer deadlocks.
    let (tx_out, rx_out) = std::sync::mpsc::channel();
    let stdout_handle = std::thread::spawn(move || {
        let res = drain_stream(stdout_pipe, max_bytes);
        let _ = tx_out.send(res);
    });

    let (tx_err, rx_err) = std::sync::mpsc::channel();
    let stderr_handle = std::thread::spawn(move || {
        let res = drain_stream(stderr_pipe, max_bytes);
        let _ = tx_err.send(res);
    });

    let start_time = std::time::Instant::now();

    // Await completion of both streams within overall timeout
    let out_outcome = rx_out.recv_timeout(timeout);
    let remaining_timeout = timeout.saturating_sub(start_time.elapsed());
    let err_outcome = if out_outcome.is_ok() {
        rx_err.recv_timeout(remaining_timeout)
    } else {
        rx_err
            .try_recv()
            .map_err(|_| std::sync::mpsc::RecvTimeoutError::Timeout)
    };

    match (out_outcome, err_outcome) {
        (Ok(out_res), Ok(err_res)) => {
            // Both streams completed naturally within timeout!
            let _ = stdout_handle.join();
            let _ = stderr_handle.join();

            let status = child
                .wait()
                .map_err(|e| QemuError::System(format!("Failed to wait for process exit: {e}")))?;

            let out_drain = out_res
                .map_err(|e| QemuError::System(format!("Failed reading child stdout: {e}")))?;
            let err_drain = err_res
                .map_err(|e| QemuError::System(format!("Failed reading child stderr: {e}")))?;

            if !status.success() {
                let stderr_tail = String::from_utf8_lossy(&err_drain.retained)
                    .trim()
                    .to_string();
                return Err(QemuError::ProcessUnexpectedExit {
                    exit_status: status.code(),
                    stderr_tail,
                });
            }

            let stdout_str = String::from_utf8(out_drain.retained).map_err(|e| {
                QemuError::VersionParseError(format!("Output contains invalid UTF-8: {e}"))
            })?;

            Ok(stdout_str)
        }
        _ => {
            // Execution timed out on at least one stream!
            // 1. Forcefully terminate the direct child process
            let _ = child.kill();
            // 2. Reap the direct child process table entry
            let _ = child.wait();

            // 3. Bounded cleanup for reader threads.
            // When child.kill() runs, its pipe write handles are closed.
            // If the direct child held the pipes, reader threads will reach EOF promptly.
            // We give up to 200ms grace to join completed readers.
            let drain_grace = Duration::from_millis(200);
            let grace_start = std::time::Instant::now();
            while grace_start.elapsed() < drain_grace {
                if stdout_handle.is_finished() && stderr_handle.is_finished() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }

            if stdout_handle.is_finished() {
                let _ = stdout_handle.join();
            }
            if stderr_handle.is_finished() {
                let _ = stderr_handle.join();
            }
            // If either handle is NOT finished, an uncooperative descendant process is holding
            // open the inherited pipe. We deliberately do NOT block indefinitely on unjoined handles.

            Err(QemuError::Timeout(timeout))
        }
    }
}

/// Validates whether a path points to an existing executable file.
///
/// - On Windows (`cfg(windows)`): Requires a regular file with the `.exe` extension (case-insensitive).
///   Scripts (`.cmd`, `.bat`), directories, and extension-less binaries are strictly rejected
///   to conform with direct process execution without a shell.
/// - On Unix (`cfg(unix)`): Requires a regular file with at least one executable permission bit set
///   (`mode & 0o111 != 0`).
pub fn is_executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }

    #[cfg(windows)]
    {
        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            ext.eq_ignore_ascii_case("exe")
        } else {
            false
        }
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = path.metadata() {
            metadata.permissions().mode() & 0o111 != 0
        } else {
            false
        }
    }
}

/// Returns the standard QEMU executable file name for the current platform.
pub const fn default_binary_name() -> &'static str {
    if cfg!(windows) {
        "qemu-system-x86_64.exe"
    } else {
        "qemu-system-x86_64"
    }
}

/// Collects well-known platform system installation paths.
pub fn platform_system_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    #[cfg(windows)]
    {
        if let Ok(prog_files) = std::env::var("ProgramFiles") {
            paths.push(
                PathBuf::from(prog_files)
                    .join("qemu")
                    .join("qemu-system-x86_64.exe"),
            );
        }
        if let Ok(prog_files_x86) = std::env::var("ProgramFiles(x86)") {
            paths.push(
                PathBuf::from(prog_files_x86)
                    .join("qemu")
                    .join("qemu-system-x86_64.exe"),
            );
        }
        if let Ok(local_appdata) = std::env::var("LOCALAPPDATA") {
            paths.push(
                PathBuf::from(local_appdata)
                    .join("Programs")
                    .join("qemu")
                    .join("qemu-system-x86_64.exe"),
            );
        }
        if let Ok(prog_data) = std::env::var("ProgramData") {
            paths.push(
                PathBuf::from(prog_data)
                    .join("chocolatey")
                    .join("bin")
                    .join("qemu-system-x86_64.exe"),
            );
        }

        // Hardcoded standard Windows fallbacks in case environment variables are missing
        let std_c_prog = PathBuf::from("C:\\Program Files\\qemu\\qemu-system-x86_64.exe");
        if !paths.contains(&std_c_prog) {
            paths.push(std_c_prog);
        }
        let std_c_prog_x86 = PathBuf::from("C:\\Program Files (x86)\\qemu\\qemu-system-x86_64.exe");
        if !paths.contains(&std_c_prog_x86) {
            paths.push(std_c_prog_x86);
        }
    }

    #[cfg(not(windows))]
    {
        paths.push(PathBuf::from("/usr/bin/qemu-system-x86_64"));
        paths.push(PathBuf::from("/usr/local/bin/qemu-system-x86_64"));
    }

    paths
}

/// Returns directories parsed from the host `PATH` environment variable.
pub fn system_path_dirs() -> Vec<PathBuf> {
    match std::env::var_os("PATH") {
        Some(paths) => std::env::split_paths(&paths).collect(),
        None => Vec::new(),
    }
}

/// Pure resolution engine for locating the QEMU binary across prioritized sources.
pub(crate) fn find_executable_in<F>(
    explicit_path: Option<&Path>,
    env_lookup: F,
    system_paths: &[PathBuf],
    path_dirs: &[PathBuf],
) -> Result<PathBuf, QemuError>
where
    F: Fn(&str) -> Option<String>,
{
    // 1. Explicit path parameter
    if let Some(explicit) = explicit_path {
        if is_executable(explicit) {
            return Ok(explicit.to_path_buf());
        }
        return Err(QemuError::BinaryNotFound(explicit.to_path_buf()));
    }

    // 2. Environment variable override
    if let Some(val) = env_lookup("TERMINEST_QEMU_PATH") {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            let env_path = PathBuf::from(trimmed);
            if is_executable(&env_path) {
                return Ok(env_path);
            }
            return Err(QemuError::BinaryNotFound(env_path));
        }
    }

    // 3. Platform well-known paths
    for candidate in system_paths {
        if is_executable(candidate) {
            return Ok(candidate.clone());
        }
    }

    // 4. System PATH resolution
    let binary_name = default_binary_name();
    for dir in path_dirs {
        let candidate = dir.join(binary_name);
        if is_executable(&candidate) {
            return Ok(candidate);
        }
    }

    Err(QemuError::BinaryNotFound(PathBuf::from(binary_name)))
}

/// Internal capability probing using an abstract command runner.
pub(crate) fn probe_executable_with_runner<R: CommandRunner>(
    path: &Path,
    runner: &R,
    timeout: Duration,
    max_bytes: usize,
) -> Result<QemuCapabilities, QemuError> {
    let version_raw = runner.run_bounded(path, &["--version"], timeout, max_bytes)?;
    let version = parse_version(&version_raw)?;

    let accel_raw = runner.run_bounded(path, &["-accel", "help"], timeout, max_bytes)?;
    let accelerators = parse_accelerators(&accel_raw)?;

    Ok(QemuCapabilities::new(version, accelerators))
}

/// Internal discovery and validation sequence using an abstract command runner.
pub(crate) fn discover_with_runner<R: CommandRunner>(
    explicit_path: Option<&Path>,
    policy: Option<&QemuVersionPolicy>,
    runner: &R,
    timeout: Duration,
    max_bytes: usize,
) -> Result<(PathBuf, QemuCapabilities), QemuError> {
    let path = QemuDiscovery::find_executable(explicit_path)?;
    let caps = probe_executable_with_runner(&path, runner, timeout, max_bytes)?;

    if let Some(p) = policy {
        p.validate(&caps.version)?;
    } else {
        QemuVersionPolicy::default().validate(&caps.version)?;
    }

    caps.validate_platform_support()?;

    Ok((path, caps))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    #[cfg(test)]
    struct MockRunner<F>
    where
        F: Fn(&Path, &[&str]) -> Result<String, QemuError> + Send + Sync,
    {
        run_fn: F,
    }

    impl<F> CommandRunner for MockRunner<F>
    where
        F: Fn(&Path, &[&str]) -> Result<String, QemuError> + Send + Sync,
    {
        fn run_bounded(
            &self,
            program: &Path,
            args: &[&str],
            _timeout: Duration,
            _max_bytes: usize,
        ) -> Result<String, QemuError> {
            (self.run_fn)(program, args)
        }
    }

    #[test]
    fn test_is_executable_validation_rules() {
        let temp_dir = tempfile::tempdir().expect("create tempdir");

        let exe_file = temp_dir.path().join("qemu.exe");
        File::create(&exe_file).unwrap();

        let cmd_file = temp_dir.path().join("qemu.cmd");
        File::create(&cmd_file).unwrap();

        let bat_file = temp_dir.path().join("qemu.bat");
        File::create(&bat_file).unwrap();

        let no_ext_file = temp_dir.path().join("qemu");
        File::create(&no_ext_file).unwrap();

        let non_existent = temp_dir.path().join("missing.exe");

        #[cfg(windows)]
        {
            assert!(is_executable(&exe_file), ".exe must be accepted on Windows");
            assert!(
                !is_executable(&cmd_file),
                ".cmd must be rejected on Windows (no shell)"
            );
            assert!(
                !is_executable(&bat_file),
                ".bat must be rejected on Windows (no shell)"
            );
            assert!(
                !is_executable(&no_ext_file),
                "extension-less file must be rejected on Windows"
            );
            assert!(
                !is_executable(temp_dir.path()),
                "directories must be rejected"
            );
            assert!(
                !is_executable(&non_existent),
                "missing files must be rejected"
            );
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert!(
                !is_executable(&no_ext_file),
                "file without +x must be rejected on Unix"
            );

            std::fs::set_permissions(&no_ext_file, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert!(
                is_executable(&no_ext_file),
                "file with +x must be accepted on Unix"
            );
            assert!(
                !is_executable(temp_dir.path()),
                "directories must be rejected"
            );
            assert!(
                !is_executable(&non_existent),
                "missing files must be rejected"
            );
        }
    }

    #[test]
    fn test_discovery_explicit_path_success() {
        let temp_dir = tempfile::tempdir().expect("create tempdir");
        let fake_exe = temp_dir.path().join(default_binary_name());
        File::create(&fake_exe).expect("create fake exe");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake_exe, std::fs::Permissions::from_mode(0o755))
                .expect("set mode");
        }

        let res = find_executable_in(Some(&fake_exe), |_| None, &[], &[]);
        assert_eq!(res.expect("found explicit path"), fake_exe);
    }

    #[test]
    fn test_discovery_explicit_path_missing() {
        let missing = PathBuf::from("E:\\nonexistent\\qemu-system-x86_64.exe");
        let res = find_executable_in(Some(&missing), |_| None, &[], &[]);
        assert_eq!(res, Err(QemuError::BinaryNotFound(missing)));
    }

    #[test]
    fn test_discovery_path_env_var() {
        let temp_dir = tempfile::tempdir().expect("create tempdir");
        let env_exe = temp_dir.path().join(default_binary_name());
        File::create(&env_exe).expect("create fake exe");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&env_exe, std::fs::Permissions::from_mode(0o755))
                .expect("set mode");
        }

        let res = find_executable_in(
            None,
            |var| {
                if var == "TERMINEST_QEMU_PATH" {
                    Some(env_exe.to_string_lossy().to_string())
                } else {
                    None
                }
            },
            &[],
            &[],
        );
        assert_eq!(res.expect("found from env var"), env_exe);
    }

    #[test]
    fn test_discovery_precedence() {
        let temp_dir = tempfile::tempdir().expect("create tempdir");
        let explicit_exe = temp_dir
            .path()
            .join(format!("explicit_{}", default_binary_name()));
        let env_exe = temp_dir
            .path()
            .join(format!("env_{}", default_binary_name()));
        let sys_exe = temp_dir
            .path()
            .join(format!("sys_{}", default_binary_name()));
        let path_exe = temp_dir.path().join(default_binary_name());

        File::create(&explicit_exe).expect("create explicit");
        File::create(&env_exe).expect("create env");
        File::create(&sys_exe).expect("create sys");
        File::create(&path_exe).expect("create path");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for p in &[&explicit_exe, &env_exe, &sys_exe, &path_exe] {
                std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755))
                    .expect("set mode");
            }
        }

        // 1. Explicit beats env
        let res = find_executable_in(
            Some(&explicit_exe),
            |_| Some(env_exe.to_string_lossy().to_string()),
            std::slice::from_ref(&sys_exe),
            &[temp_dir.path().to_path_buf()],
        );
        assert_eq!(res.expect("explicit wins"), explicit_exe);

        // 2. Env beats system paths
        let res = find_executable_in(
            None,
            |_| Some(env_exe.to_string_lossy().to_string()),
            std::slice::from_ref(&sys_exe),
            &[temp_dir.path().to_path_buf()],
        );
        assert_eq!(res.expect("env wins"), env_exe);

        // 3. System paths beat PATH
        let res = find_executable_in(
            None,
            |_| None,
            std::slice::from_ref(&sys_exe),
            &[temp_dir.path().to_path_buf()],
        );
        assert_eq!(res.expect("sys wins"), sys_exe);

        // 4. PATH resolves when others absent
        let res = find_executable_in(None, |_| None, &[], &[temp_dir.path().to_path_buf()]);
        assert_eq!(res.expect("path wins"), path_exe);
    }

    #[test]
    fn test_discovery_not_found_returns_error() {
        let res = find_executable_in(None, |_| None, &[], &[]);
        assert_eq!(
            res,
            Err(QemuError::BinaryNotFound(PathBuf::from(
                default_binary_name()
            )))
        );
    }

    #[test]
    fn test_probe_executable_with_mock_runner() {
        let fake_path = PathBuf::from("/mock/qemu-system-x86_64");
        let runner = MockRunner {
            run_fn: |_p: &Path, args: &[&str]| {
                if args == ["--version"] {
                    Ok("QEMU emulator version 9.2.0".to_string())
                } else if args == ["-accel", "help"] {
                    Ok("Accelerators supported in QEMU binary:\ntcg\nwhpx\n".to_string())
                } else {
                    Err(QemuError::System("unexpected args".into()))
                }
            },
        };

        let caps =
            probe_executable_with_runner(&fake_path, &runner, Duration::from_secs(1), 65_536)
                .expect("probe caps");

        assert_eq!(caps.version.major, 9);
        assert_eq!(caps.version.minor, 2);
        assert_eq!(caps.version.patch, 0);
        assert!(caps.supports_accelerator(crate::backend::qemu::QemuAccelerator::Whpx));
        assert!(caps.supports_accelerator(crate::backend::qemu::QemuAccelerator::Tcg));
    }

    #[test]
    fn test_probe_executable_version_error() {
        let fake_path = PathBuf::from("/mock/qemu");
        let runner = MockRunner {
            run_fn: |_p: &Path, _args: &[&str]| Ok("corrupted output".to_string()),
        };

        let res = probe_executable_with_runner(&fake_path, &runner, Duration::from_secs(1), 65_536);
        assert!(matches!(res, Err(QemuError::VersionParseError(_))));
    }

    #[test]
    fn test_probe_executable_timeout() {
        let fake_path = PathBuf::from("/mock/qemu");
        let runner = MockRunner {
            run_fn: |_p: &Path, _args: &[&str]| Err(QemuError::Timeout(Duration::from_secs(2))),
        };

        let res = probe_executable_with_runner(&fake_path, &runner, Duration::from_secs(2), 65_536);
        assert_eq!(res, Err(QemuError::Timeout(Duration::from_secs(2))));
    }
}
