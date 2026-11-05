use std::fs::File;
use std::path::PathBuf;

use terminest_core::backend::qemu::{
    is_executable, parse_accelerators, parse_version, QemuAccelerator, QemuCapabilities,
    QemuDiscovery, QemuError, QemuVersion, QemuVersionPolicy,
};

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
    // Check if the spike QEMU or system QEMU exists
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
