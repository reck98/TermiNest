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

/// Executes a child process directly (without a shell) with bounded execution time and memory.
fn run_process_bounded(
    program: &Path,
    args: &[&str],
    timeout: Duration,
    max_bytes: usize,
) -> Result<String, QemuError> {
    use std::io::Read;
    use std::process::{Command, Stdio};

    let mut cmd = Command::new(program);
    cmd.args(args);
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

    let mut stdout = child.stdout.take().ok_or_else(|| {
        QemuError::System("Failed to capture stdout pipe from child process".into())
    })?;
    let mut stderr = child.stderr.take().ok_or_else(|| {
        QemuError::System("Failed to capture stderr pipe from child process".into())
    })?;

    let (tx, rx) = std::sync::mpsc::channel();
    let reader_handle = std::thread::spawn(move || {
        let mut out_buf = Vec::new();
        let mut err_buf = Vec::new();
        let out_res = (&mut stdout)
            .take(max_bytes as u64)
            .read_to_end(&mut out_buf);
        let err_res = (&mut stderr)
            .take(max_bytes as u64)
            .read_to_end(&mut err_buf);
        let _ = tx.send((out_res, out_buf, err_res, err_buf));
    });

    match rx.recv_timeout(timeout) {
        Ok((out_res, out_buf, _err_res, err_buf)) => {
            let _ = reader_handle.join();

            let status = child
                .wait()
                .map_err(|e| QemuError::System(format!("Failed to wait for process exit: {e}")))?;

            if !status.success() {
                let stderr_tail = String::from_utf8_lossy(&err_buf).trim().to_string();
                return Err(QemuError::ProcessUnexpectedExit {
                    exit_status: status.code(),
                    stderr_tail,
                });
            }

            out_res.map_err(|e| QemuError::System(format!("Failed reading stdout: {e}")))?;
            let stdout_str = String::from_utf8(out_buf).map_err(|e| {
                QemuError::VersionParseError(format!("Output contains invalid UTF-8: {e}"))
            })?;

            Ok(stdout_str)
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader_handle.join();
            Err(QemuError::Timeout(timeout))
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader_handle.join();
            Err(QemuError::System(
                "Output reader thread disconnected prematurely".into(),
            ))
        }
    }
}

/// Validates whether a path points to an existing executable file.
pub fn is_executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }

    #[cfg(windows)]
    {
        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            ext.eq_ignore_ascii_case("exe")
                || ext.eq_ignore_ascii_case("cmd")
                || ext.eq_ignore_ascii_case("bat")
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
