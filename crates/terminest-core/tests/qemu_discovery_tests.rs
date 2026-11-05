use std::fs::File;
use std::path::PathBuf;
use std::time::Duration;

use terminest_core::backend::qemu::discovery::run_process_bounded_with_env;
use terminest_core::backend::qemu::{
    is_executable, parse_accelerators, parse_version, safe_truncate_str, AcceleratorHostStatus,
    QemuAccelerator, QemuCapabilities, QemuDiscovery, QemuError, QemuReleaseKind, QemuVersion,
    QemuVersionPolicy,
};

/// Controlled child worker entrypoint invoked by child test scenarios.
/// When executed during normal `cargo test`, returns immediately without effect.
#[test]
fn test_child_worker_entrypoint() {
    let scenario = match std::env::var("TERMINEST_TEST_CHILD_SCENARIO") {
        Ok(s) => s,
        Err(_) => return,
    };

    use std::io::Write;
    match scenario.as_str() {
        "fill_stderr_while_stdout_open" => {
            // Write 128 KiB to stderr while stdout remains open without data.
            // On un-drained pipes this will fill the OS pipe buffer (~4-64 KiB) and deadlock.
            let data = vec![b'E'; 128 * 1024];
            let mut err = std::io::stderr();
            err.write_all(&data).expect("write stderr");
            err.flush().expect("flush stderr");

            // Now write to stdout to confirm reader completes
            let mut out = std::io::stdout();
            out.write_all(b"done_stdout\n").expect("write stdout");
            out.flush().expect("flush stdout");
            std::process::exit(0);
        }
        "overflow_stdout" => {
            // Write 256 KiB to stdout to test continuous draining beyond retention limit
            let data = vec![b'O'; 256 * 1024];
            let mut out = std::io::stdout();
            out.write_all(&data).expect("write stdout");
            out.flush().expect("flush stdout");
            std::process::exit(0);
        }
        "overflow_stderr" => {
            // Write 256 KiB to stderr and exit with failure code 1
            let data = vec![b'E'; 256 * 1024];
            let mut err = std::io::stderr();
            err.write_all(&data).expect("write stderr");
            err.flush().expect("flush stderr");
            std::process::exit(1);
        }
        "heavy_simultaneous" => {
            // Simultaneously write 128 KiB to stdout and 128 KiB to stderr in concurrent threads
            let t1 = std::thread::spawn(|| {
                let chunk = vec![b'O'; 4096];
                let mut out = std::io::stdout();
                for _ in 0..32 {
                    out.write_all(&chunk).expect("write stdout");
                    std::thread::yield_now();
                }
                out.flush().expect("flush stdout");
            });
            let t2 = std::thread::spawn(|| {
                let chunk = vec![b'E'; 4096];
                let mut err = std::io::stderr();
                for _ in 0..32 {
                    err.write_all(&chunk).expect("write stderr");
                    std::thread::yield_now();
                }
                err.flush().expect("flush stderr");
            });
            t1.join().expect("join t1");
            t2.join().expect("join t2");
            std::process::exit(0);
        }
        "timeout_hang" => {
            // Hang indefinitely until terminated by supervisor
            std::thread::sleep(Duration::from_secs(60));
            std::process::exit(0);
        }
        "large_output_clean_exit" => {
            // Write 512 KiB to stdout and exit cleanly
            let chunk = vec![b'X'; 8192];
            let mut out = std::io::stdout();
            for _ in 0..64 {
                out.write_all(&chunk).expect("write stdout");
            }
            out.flush().expect("flush stdout");
            std::process::exit(0);
        }
        _ => {
            std::process::exit(2);
        }
    }
}

fn run_child_scenario(
    scenario: &str,
    timeout: Duration,
    max_bytes: usize,
) -> Result<String, QemuError> {
    let current_exe = std::env::current_exe().expect("current exe path");
    run_process_bounded_with_env(
        &current_exe,
        &["--nocapture", "--exact", "test_child_worker_entrypoint"],
        &[("TERMINEST_TEST_CHILD_SCENARIO", scenario)],
        timeout,
        max_bytes,
    )
}

// ============================================================================
// FINDING B2-R1 TESTS: Concurrent Bounded Draining & Subprocess Safety
// ============================================================================

#[test]
fn test_bounded_drain_fill_stderr_while_stdout_open() {
    // Child writes 128 KiB to stderr while stdout remains open.
    // Must complete cleanly without deadlocking.
    let res = run_child_scenario(
        "fill_stderr_while_stdout_open",
        Duration::from_secs(5),
        65_536,
    );
    let output = res.expect("must not deadlock when stderr fills while stdout is open");
    assert!(output.contains("done_stdout"));
}

#[test]
fn test_bounded_drain_overflow_stdout() {
    // Child writes 256 KiB to stdout with a 1024-byte retention cap.
    // Child must not stall, and retained buffer must be capped at 1024.
    let res = run_child_scenario("overflow_stdout", Duration::from_secs(5), 1024);
    let output = res.expect("must drain stdout beyond retention limit without stall");
    assert_eq!(output.len(), 1024);
}

#[test]
fn test_bounded_drain_overflow_stderr() {
    // Child writes 256 KiB to stderr, exits with code 1, with a 2048-byte retention cap.
    let res = run_child_scenario("overflow_stderr", Duration::from_secs(5), 2048);
    match res {
        Err(QemuError::ProcessUnexpectedExit {
            exit_status,
            stderr_tail,
        }) => {
            assert_eq!(exit_status, Some(1));
            assert_eq!(stderr_tail.len(), 2048);
        }
        other => panic!("expected ProcessUnexpectedExit, got {other:?}"),
    }
}

#[test]
fn test_bounded_drain_heavy_simultaneous() {
    // Child writes 128 KiB to stdout and 128 KiB to stderr simultaneously.
    let res = run_child_scenario("heavy_simultaneous", Duration::from_secs(5), 4096);
    let output = res.expect("must drain simultaneous streams without deadlocking");
    assert_eq!(output.len(), 4096);
}

#[test]
fn test_bounded_drain_timeout_hang() {
    // Child hangs indefinitely; timeout should terminate and reap child in ~200ms.
    let start = std::time::Instant::now();
    let res = run_child_scenario("timeout_hang", Duration::from_millis(200), 65_536);
    let elapsed = start.elapsed();

    assert!(
        matches!(res, Err(QemuError::Timeout(_))),
        "expected Timeout error, got {res:?}"
    );
    assert!(
        elapsed < Duration::from_secs(3),
        "timeout cleanup must conclude promptly, elapsed: {elapsed:?}"
    );
}

#[test]
fn test_bounded_drain_large_output_clean_exit() {
    // Child writes 512 KiB and exits 0 cleanly.
    let res = run_child_scenario("large_output_clean_exit", Duration::from_secs(5), 8192);
    let output = res.expect("large output clean exit must succeed");
    assert_eq!(output.len(), 8192);
}

// ============================================================================
// FINDING B2-R2 TESTS: Strict Windows Executable Validation (.exe only)
// ============================================================================

#[test]
fn test_windows_executable_validation_exe_only() {
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
            ".cmd must be rejected on Windows (no shell execution)"
        );
        assert!(
            !is_executable(&bat_file),
            ".bat must be rejected on Windows (no shell execution)"
        );
        assert!(
            !is_executable(&no_ext_file),
            "extension-less files must be rejected on Windows"
        );
        assert!(
            !is_executable(temp_dir.path()),
            "directories must be rejected"
        );
        assert!(
            !is_executable(&non_existent),
            "missing files must be rejected"
        );

        // Explicit discovery path must reject .cmd and .bat
        assert!(matches!(
            QemuDiscovery::find_executable(Some(&cmd_file)),
            Err(QemuError::BinaryNotFound(_))
        ));
        assert!(matches!(
            QemuDiscovery::find_executable(Some(&bat_file)),
            Err(QemuError::BinaryNotFound(_))
        ));
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

// ============================================================================
// FINDING B2-R3 TESTS: UTF-8-Safe Parser Truncation
// ============================================================================

#[test]
fn test_utf8_safe_parser_truncation_multibyte_crossing() {
    // String where a 4-byte UTF-8 character (crab) crosses the 65,536 boundary
    let mut input = "a".repeat(65_534);
    input.push('\u{1F980}'); // 4 bytes: F0 9F A6 80, crosses byte 65,536
    input.push_str("QEMU emulator version 9.2.0\n");

    let truncated = safe_truncate_str(&input, 65_536);
    assert_eq!(truncated.len(), 65_534);
    assert!(truncated.is_char_boundary(truncated.len()));

    // Must return controlled error and never panic
    let res = parse_version(&input);
    assert!(matches!(res, Err(QemuError::VersionParseError(_))));
}

#[test]
fn test_utf8_safe_parser_token_before_boundary() {
    // Version token is in the first line, followed by 70 KiB of multibyte unicode
    let mut input = "QEMU emulator version 9.2.0\n".to_string();
    for _ in 0..15_000 {
        input.push('\u{1F980}'); // 60 KiB of crabs
    }
    let v = parse_version(&input).expect("parse version before boundary");
    assert_eq!(v.major, 9);
    assert_eq!(v.minor, 2);
    assert_eq!(v.patch, 0);
}

#[test]
fn test_utf8_safe_parser_token_beyond_boundary() {
    // 70 KiB of crabs before the version line
    let mut input = String::new();
    for _ in 0..18_000 {
        input.push('\u{1F980}'); // 72 KiB of crabs
    }
    input.push_str("\nQEMU emulator version 9.2.0\n");

    let res = parse_version(&input);
    assert!(
        matches!(res, Err(QemuError::VersionParseError(_))),
        "version line beyond boundary must return controlled error without panic"
    );
}

// ============================================================================
// FINDING B2-R4 & B2-R5 TESTS: SemVer Precedence & Snapshot Classification
// ============================================================================

#[test]
fn test_semver_precedence_rc2_vs_rc10() {
    let rc2 = QemuVersion::parse("9.0.0-rc2").unwrap();
    let rc10 = QemuVersion::parse("9.0.0-rc10").unwrap();
    assert!(
        rc2 < rc10,
        "rc2 must be less than rc10 in SemVer precedence"
    );

    let rc_dot2 = QemuVersion::parse("9.0.0-rc.2").unwrap();
    let rc_dot10 = QemuVersion::parse("9.0.0-rc.10").unwrap();
    assert!(rc_dot2 < rc_dot10, "rc.2 must be less than rc.10");
}

#[test]
fn test_semver_precedence_numeric_vs_alphanumeric() {
    let v_num = QemuVersion::parse("9.0.0-1").unwrap();
    let v_alpha = QemuVersion::parse("9.0.0-alpha").unwrap();
    assert!(
        v_num < v_alpha,
        "Numeric identifier must have lower precedence than alphanumeric"
    );
}

#[test]
fn test_semver_precedence_final_vs_prerelease() {
    let final_rel = QemuVersion::parse("9.0.0").unwrap();
    let rc1 = QemuVersion::parse("9.0.0-rc1").unwrap();
    let snap = QemuVersion::parse("9.0.0-snapshot").unwrap();

    assert!(final_rel > rc1, "Final release must precede rc1");
    assert!(final_rel > snap, "Final release must precede snapshot");
}

#[test]
fn test_equal_core_versions_precedence() {
    let v1 = QemuVersion::parse("8.0.0").unwrap();
    let v2 = QemuVersion::parse("8.0.0").unwrap();
    assert_eq!(v1, v2);
}

#[test]
fn test_snapshot_tags_and_commit_extraction() {
    let snap = QemuVersion::parse("11.1.0-12130-ge470268ff4").unwrap();
    assert_eq!(snap.kind(), QemuReleaseKind::Snapshot);
    assert!(snap.is_snapshot());
    assert!(!snap.is_prerelease());
    assert_eq!(snap.commit_hash(), Some("e470268ff4"));

    let min_policy = QemuVersion::parse("8.0.0").unwrap();
    assert!(
        snap > min_policy,
        "11.1.0 snapshot must be greater than 8.0.0 baseline"
    );
}

#[test]
fn test_distribution_package_suffixes_classification() {
    let debian = QemuVersion::parse("8.2.2 (Debian 1:8.2.2+ds-0ubuntu1)").unwrap();
    assert_eq!(debian.kind(), QemuReleaseKind::DistributionPackage);
    assert!(!debian.is_snapshot(), "Debian package is not a snapshot");
    assert!(
        !debian.is_prerelease(),
        "Debian package is not a prerelease"
    );

    let pure_stable = QemuVersion::parse("8.2.2").unwrap();
    assert_eq!(
        debian, pure_stable,
        "Distribution package metadata must not alter stable SemVer precedence"
    );

    // Policy with allow_snapshots = false must accept Debian packages
    let policy = QemuVersionPolicy::new((8, 0, 0), false, true);
    assert!(
        policy.validate(&debian).is_ok(),
        "allow_snapshots = false must NOT reject stable distribution packages"
    );
}

#[test]
fn test_snapshot_classification_all_four_categories() {
    let stable = QemuVersion::parse("9.2.0").unwrap();
    assert_eq!(stable.kind(), QemuReleaseKind::Stable);

    let prerelease = QemuVersion::parse("9.0.0-rc1").unwrap();
    assert_eq!(prerelease.kind(), QemuReleaseKind::PreRelease);

    let snapshot = QemuVersion::parse("11.1.0-12130-ge470268ff4").unwrap();
    assert_eq!(snapshot.kind(), QemuReleaseKind::Snapshot);

    let distro = QemuVersion::parse("8.2.2 (Debian 1:8.2.2+ds-0ubuntu1)").unwrap();
    assert_eq!(distro.kind(), QemuReleaseKind::DistributionPackage);
}

// ============================================================================
// FINDING B2-R6 TESTS: WHPX Capability Distinction
// ============================================================================

#[test]
fn test_whpx_compiled_capability_vs_host_usability() {
    let output = "Accelerators supported in QEMU binary:\ntcg\nwhpx\n";
    let accels = parse_accelerators(output).expect("parse accels");
    let caps = QemuCapabilities::new(QemuVersion::new(9, 2, 0), accels);

    // Compiled support in binary
    assert!(caps.supports_accelerator(QemuAccelerator::Whpx));

    // Host usability is explicitly unprobed at B2
    assert_eq!(
        caps.host_whpx_status,
        AcceleratorHostStatus::NotProbed,
        "Host WHPX usability must be represented as NotProbed at Stage B2"
    );
}

// ============================================================================
// GENERAL DISCOVERY & PROBING INTEGRATION TESTS
// ============================================================================

#[test]
fn test_discovery_explicit_path_success() {
    let temp_dir = tempfile::tempdir().expect("create tempdir");
    let fake_name = if cfg!(windows) {
        "fake-qemu.exe"
    } else {
        "fake-qemu"
    };
    let fake_path = temp_dir.path().join(fake_name);
    File::create(&fake_path).expect("create fake executable");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake_path, std::fs::Permissions::from_mode(0o755))
            .expect("set mode");
    }

    let found = QemuDiscovery::find_executable(Some(&fake_path));
    assert_eq!(found.expect("explicit path succeeds"), fake_path);
}

#[test]
fn test_discovery_explicit_path_missing() {
    let missing_path = if cfg!(windows) {
        PathBuf::from("C:\\definitely_nonexistent_directory\\qemu-system-x86_64.exe")
    } else {
        PathBuf::from("/definitely/nonexistent/qemu-system-x86_64")
    };

    let res = QemuDiscovery::find_executable(Some(&missing_path));
    assert_eq!(res, Err(QemuError::BinaryNotFound(missing_path)));
}

#[test]
fn test_discovery_path_env_var() {
    let temp_dir = tempfile::tempdir().expect("create tempdir");
    let fake_name = if cfg!(windows) {
        "env-qemu.exe"
    } else {
        "env-qemu"
    };
    let fake_path = temp_dir.path().join(fake_name);
    File::create(&fake_path).expect("create fake executable");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake_path, std::fs::Permissions::from_mode(0o755))
            .expect("set mode");
    }

    std::env::set_var("TERMINEST_QEMU_PATH", &fake_path);
    let found = QemuDiscovery::find_executable(None);
    std::env::remove_var("TERMINEST_QEMU_PATH");

    assert_eq!(found.expect("env var succeeds"), fake_path);
}

#[test]
fn test_capabilities_semver_parsing() {
    let standard = "QEMU emulator version 9.2.0\nCopyright (c) 2003-2024 Fabrice Bellard";
    let version = parse_version(standard).expect("parse standard 9.2.0");
    assert_eq!(version.major, 9);
    assert_eq!(version.minor, 2);
    assert_eq!(version.patch, 0);
    assert_eq!(version.suffix, None);
    assert_eq!(version.commit_hash(), None);
    assert_eq!(version.to_string(), "9.2.0");
}

#[test]
fn test_capabilities_snapshot_parsing() {
    let snapshot = "QEMU emulator version 11.1.0 (v11.1.0-12130-ge470268ff4)\nCopyright (c) 2003-2026 Fabrice Bellard";
    let version = parse_version(snapshot).expect("parse snapshot 11.1.0");
    assert_eq!(version.major, 11);
    assert_eq!(version.minor, 1);
    assert_eq!(version.patch, 0);
    assert_eq!(version.suffix.as_deref(), Some("v11.1.0-12130-ge470268ff4"));
    assert_eq!(version.commit_hash(), Some("e470268ff4"));

    let hyphenated = "11.1.0-12130-ge470268ff4";
    let v2 = parse_version(hyphenated).expect("parse hyphenated snapshot");
    assert_eq!(v2.major, 11);
    assert_eq!(v2.minor, 1);
    assert_eq!(v2.patch, 0);
    assert_eq!(v2.suffix.as_deref(), Some("12130-ge470268ff4"));
    assert_eq!(v2.commit_hash(), Some("e470268ff4"));
}

#[test]
fn test_capabilities_malformed_version() {
    assert!(matches!(
        parse_version(""),
        Err(QemuError::VersionParseError(_))
    ));
    assert!(matches!(
        parse_version("random garbage without version"),
        Err(QemuError::VersionParseError(_))
    ));
    assert!(matches!(
        parse_version("QEMU emulator version incomplete.9"),
        Err(QemuError::VersionParseError(_))
    ));
    assert!(matches!(
        parse_version("QEMU emulator version -1.0.0"),
        Err(QemuError::VersionParseError(_))
    ));
}

#[test]
fn test_capabilities_accel_whpx_detected() {
    let output = "Accelerators supported in QEMU binary:\ntcg\nwhpx\n";
    let accels = parse_accelerators(output).expect("parse accelerators");
    assert_eq!(accels, vec![QemuAccelerator::Tcg, QemuAccelerator::Whpx]);

    let caps = QemuCapabilities::new(QemuVersion::new(9, 2, 0), accels);
    assert!(caps.supports_accelerator(QemuAccelerator::Whpx));
    assert!(caps.supports_accelerator(QemuAccelerator::Tcg));
    assert!(!caps.supports_accelerator(QemuAccelerator::Kvm));
}

#[test]
fn test_capabilities_accel_missing_rejected() {
    let output = "Accelerators supported in QEMU binary:\ntcg\n";
    let accels = parse_accelerators(output).expect("parse tcg only");
    let caps = QemuCapabilities::new(QemuVersion::new(9, 2, 0), accels);

    let res = caps.validate_accelerator_support(QemuAccelerator::Whpx);
    assert!(matches!(res, Err(QemuError::AcceleratorUnavailable(_))));
}

#[test]
fn test_version_policy_rejection() {
    let policy = QemuVersionPolicy::default();
    let old_version = QemuVersion::new(7, 2, 0);
    let res = policy.validate(&old_version);
    assert!(matches!(res, Err(QemuError::VersionPolicyViolation(_))));
}

#[test]
fn test_version_policy_override_allowed() {
    let policy = QemuVersionPolicy::new((8, 0, 0), true, false);
    let old_version = QemuVersion::new(7, 2, 0);
    assert!(policy.validate(&old_version).is_ok());
}

#[test]
fn test_version_policy_boundary_and_ordering() {
    let policy = QemuVersionPolicy::default();

    let v800 = QemuVersion::new(8, 0, 0);
    assert!(policy.validate(&v800).is_ok());

    let v801 = QemuVersion::new(8, 0, 1);
    assert!(policy.validate(&v801).is_ok());

    let v799 = QemuVersion::new(7, 99, 99);
    assert!(policy.validate(&v799).is_err());

    let v9 = QemuVersion::new(9, 0, 0);
    let v8 = QemuVersion::new(8, 0, 0);
    assert!(v9 > v8);
    assert_eq!(v8, QemuVersion::new(8, 0, 0));
}

#[test]
fn test_capabilities_display_and_formatting() {
    let v1 = QemuVersion::new(9, 2, 0);
    assert_eq!(format!("{v1}"), "9.2.0");

    let v2 = QemuVersion::with_suffix(11, 1, 0, "snapshot");
    assert_eq!(format!("{v2}"), "11.1.0-snapshot");
}

#[test]
fn test_live_qemu_discovery_if_available() {
    let spike_path =
        PathBuf::from(r"E:\Development\Projects\terminest-m2-spike\qemu\qemu-system-x86_64.exe");
    let test_path = if is_executable(&spike_path) {
        Some(spike_path)
    } else if let Ok(env_path) = std::env::var("TERMINEST_QEMU_PATH") {
        let p = PathBuf::from(env_path);
        if is_executable(&p) {
            Some(p)
        } else {
            None
        }
    } else {
        None
    };

    if let Some(path) = test_path {
        println!("Testing live QEMU discovery with: {}", path.display());
        let caps = QemuDiscovery::probe_executable(&path).expect("probe live QEMU");
        assert!(
            caps.version.major >= 8,
            "Expected QEMU >= 8.0, got {}",
            caps.version
        );

        #[cfg(windows)]
        {
            assert!(
                caps.supports_accelerator(QemuAccelerator::Whpx),
                "Live Windows QEMU must support WHPX"
            );
        }

        let (discovered_path, discovered_caps) =
            QemuDiscovery::discover(Some(&path), None).expect("discover with explicit path");
        assert_eq!(discovered_path, path);
        assert_eq!(discovered_caps, caps);
    }
}
