//! QEMU capability probing, version parsing, and version policy enforcement.

use std::fmt;
use std::str::FromStr;

use super::config::QemuAccelerator;
use super::error::QemuError;

/// Parsed semantic version representation of a QEMU binary.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QemuVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub suffix: Option<String>,
}

impl QemuVersion {
    /// Creates a new `QemuVersion` without a suffix.
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
            suffix: None,
        }
    }

    /// Creates a new `QemuVersion` with an explicit suffix (e.g. snapshot, prerelease, or build tag).
    pub fn with_suffix(major: u32, minor: u32, patch: u32, suffix: impl Into<String>) -> Self {
        Self {
            major,
            minor,
            patch,
            suffix: Some(suffix.into()),
        }
    }

    /// Parses QEMU version output (such as `qemu-system-x86_64 --version`).
    ///
    /// Accepts standard release strings (e.g. `QEMU emulator version 9.2.0`),
    /// snapshot strings (e.g. `QEMU emulator version 11.1.0 (v11.1.0-12130-ge470268ff4)`
    /// or `11.1.0-12130-ge470268ff4`), distro-modified versions
    /// (e.g. `QEMU emulator version 8.2.2 (Debian 1:8.2.2+ds-0ubuntu1)`),
    /// or bare semver triplets (`8.0.0`).
    pub fn parse(output: &str) -> Result<Self, QemuError> {
        parse_version(output)
    }

    /// Returns `true` if this version represents a development snapshot or prerelease.
    pub fn is_snapshot(&self) -> bool {
        self.suffix.is_some()
    }

    /// Extracts the Git commit hash from the suffix if present (e.g. from `-g<hash>` or `(v...-g<hash>)`).
    pub fn commit_hash(&self) -> Option<&str> {
        let suffix = self.suffix.as_deref()?;
        if let Some(pos) = suffix.find("-g") {
            let after_g = &suffix[pos + 2..];
            let hash_len = after_g
                .chars()
                .take_while(|c| c.is_ascii_hexdigit())
                .count();
            if hash_len >= 7 {
                return Some(&after_g[..hash_len]);
            }
        }
        None
    }
}

impl Ord for QemuVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch)) {
            std::cmp::Ordering::Equal => match (&self.suffix, &other.suffix) {
                (None, None) => std::cmp::Ordering::Equal,
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (Some(a), Some(b)) => a.cmp(b),
            },
            ord => ord,
        }
    }
}

impl PartialOrd for QemuVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for QemuVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(ref suffix) = self.suffix {
            write!(f, "-{suffix}")?;
        }
        Ok(())
    }
}

impl FromStr for QemuVersion {
    type Err = QemuError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

/// Parses a QEMU version string into a structured [`QemuVersion`].
pub fn parse_version(output: &str) -> Result<QemuVersion, QemuError> {
    if output.trim().is_empty() {
        return Err(QemuError::VersionParseError(
            "Version output is empty".to_string(),
        ));
    }

    // Cap scanning to 64 KiB to prevent DoS from runaway output
    let truncated = if output.len() > 65_536 {
        &output[..65_536]
    } else {
        output
    };

    let version_line = truncated
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| {
            QemuError::VersionParseError("No non-empty line in version output".to_string())
        })?;

    const PREFIX: &str = "QEMU emulator version ";
    let target = if let Some(idx) = version_line.find(PREFIX) {
        &version_line[idx + PREFIX.len()..]
    } else if let Some(idx) = version_line.to_ascii_lowercase().find("version ") {
        &version_line[idx + "version ".len()..]
    } else {
        version_line
    };

    let target = target.trim();
    if target.is_empty() {
        return Err(QemuError::VersionParseError(
            "Version token is missing after prefix".to_string(),
        ));
    }

    let (first_token, rest_str) = match target.split_once(char::is_whitespace) {
        Some((first, rest)) => (first, Some(rest.trim())),
        None => (target, None),
    };

    let (version_core, inline_suffix) = if let Some(hyphen_idx) = first_token.find('-') {
        (
            &first_token[..hyphen_idx],
            Some(&first_token[hyphen_idx + 1..]),
        )
    } else {
        (first_token, None)
    };

    let semver_parts: Vec<&str> = version_core.split('.').collect();
    if semver_parts.len() != 3 {
        return Err(QemuError::VersionParseError(format!(
            "Expected 3 version components (major.minor.patch), found {}: {:?}",
            semver_parts.len(),
            version_core
        )));
    }

    let major = semver_parts[0]
        .parse::<u32>()
        .map_err(|e| QemuError::VersionParseError(format!("Invalid major version: {e}")))?;
    let minor = semver_parts[1]
        .parse::<u32>()
        .map_err(|e| QemuError::VersionParseError(format!("Invalid minor version: {e}")))?;
    let patch = semver_parts[2]
        .parse::<u32>()
        .map_err(|e| QemuError::VersionParseError(format!("Invalid patch version: {e}")))?;

    let suffix = if let Some(s) = inline_suffix {
        Some(s.to_string())
    } else if let Some(rest) = rest_str {
        let cleaned = rest.trim_start_matches('(').trim_end_matches(')');
        if cleaned.is_empty() {
            None
        } else {
            Some(cleaned.to_string())
        }
    } else {
        None
    };

    Ok(QemuVersion {
        major,
        minor,
        patch,
        suffix,
    })
}

/// Parses the output of `qemu-system-x86_64 -accel help` to extract supported accelerators.
pub fn parse_accelerators(output: &str) -> Result<Vec<QemuAccelerator>, QemuError> {
    if output.trim().is_empty() {
        return Err(QemuError::AcceleratorUnavailable(
            "Accelerator output is empty".to_string(),
        ));
    }

    let mut accelerators = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.to_ascii_lowercase().starts_with("accelerator") {
            continue;
        }

        for token in trimmed.split_whitespace() {
            let lower = token.to_ascii_lowercase();
            let parsed = match lower.as_str() {
                "whpx" => Some(QemuAccelerator::Whpx),
                "kvm" => Some(QemuAccelerator::Kvm),
                "tcg" => Some(QemuAccelerator::Tcg),
                _ => None,
            };

            if let Some(accel) = parsed {
                if !accelerators.contains(&accel) {
                    accelerators.push(accel);
                }
            }
        }
    }

    if accelerators.is_empty() {
        return Err(QemuError::AcceleratorUnavailable(
            "No known accelerators found in QEMU -accel help output".to_string(),
        ));
    }

    Ok(accelerators)
}

/// Configurable policy for QEMU version and release provenance validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QemuVersionPolicy {
    /// Minimum required (major, minor, patch) version. Provisional baseline: `(8, 0, 0)`.
    pub min_version: (u32, u32, u32),
    /// Whether development snapshots and pre-releases are permitted.
    pub allow_snapshots: bool,
    /// Whether version enforcement is active (can be disabled for explicit overrides/testing).
    pub enforce_version: bool,
}

impl Default for QemuVersionPolicy {
    fn default() -> Self {
        Self {
            min_version: (8, 0, 0),
            allow_snapshots: true,
            enforce_version: true,
        }
    }
}

impl QemuVersionPolicy {
    /// Creates a new custom version policy.
    pub const fn new(
        min_version: (u32, u32, u32),
        allow_snapshots: bool,
        enforce_version: bool,
    ) -> Self {
        Self {
            min_version,
            allow_snapshots,
            enforce_version,
        }
    }

    /// Validates a parsed [`QemuVersion`] against this policy.
    pub fn validate(&self, version: &QemuVersion) -> Result<(), QemuError> {
        if !self.enforce_version {
            return Ok(());
        }

        let min = QemuVersion::new(self.min_version.0, self.min_version.1, self.min_version.2);

        if version < &min {
            return Err(QemuError::VersionPolicyViolation(format!(
                "QEMU version {version} is below minimum required version {}.{}.{}",
                self.min_version.0, self.min_version.1, self.min_version.2
            )));
        }

        if version.is_snapshot() {
            if !self.allow_snapshots {
                return Err(QemuError::VersionPolicyViolation(format!(
                    "QEMU snapshot release {version} is disallowed by version policy"
                )));
            }
            tracing::warn!(
                version = %version,
                "Using provisional QEMU snapshot release; full multi-version qualification is pending"
            );
        }

        Ok(())
    }
}

/// Probed capabilities and metadata of a QEMU binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QemuCapabilities {
    /// Parsed version of the QEMU binary.
    pub version: QemuVersion,
    /// List of accelerators compiled into the QEMU binary.
    pub accelerators: Vec<QemuAccelerator>,
}

impl QemuCapabilities {
    /// Creates a new `QemuCapabilities` instance.
    pub const fn new(version: QemuVersion, accelerators: Vec<QemuAccelerator>) -> Self {
        Self {
            version,
            accelerators,
        }
    }

    /// Checks if a specific accelerator is supported by the QEMU binary.
    pub fn supports_accelerator(&self, accel: QemuAccelerator) -> bool {
        self.accelerators.contains(&accel)
    }

    /// Verifies that a specific accelerator is supported, returning [`QemuError::AcceleratorUnavailable`] if not.
    pub fn validate_accelerator_support(&self, accel: QemuAccelerator) -> Result<(), QemuError> {
        if !self.supports_accelerator(accel) {
            return Err(QemuError::AcceleratorUnavailable(format!(
                "Required accelerator '{}' is not supported by QEMU binary",
                accel.as_str()
            )));
        }
        Ok(())
    }

    /// Validates that the host platform's required hardware accelerator is supported.
    ///
    /// - On Windows: requires `QemuAccelerator::Whpx`.
    /// - On Linux: requires `QemuAccelerator::Kvm` and accessible `/dev/kvm`.
    pub fn validate_platform_support(&self) -> Result<(), QemuError> {
        #[cfg(windows)]
        {
            if !self.supports_accelerator(QemuAccelerator::Whpx) {
                return Err(QemuError::AcceleratorUnavailable(
                    "WHPX acceleration is required on Windows, but QEMU binary does not support it"
                        .to_string(),
                ));
            }
        }
        #[cfg(target_os = "linux")]
        {
            if !self.supports_accelerator(QemuAccelerator::Kvm) {
                return Err(QemuError::AcceleratorUnavailable(
                    "KVM acceleration is required on Linux, but QEMU binary does not support it"
                        .to_string(),
                ));
            }
            let kvm_path = std::path::Path::new("/dev/kvm");
            if !kvm_path.exists() {
                return Err(QemuError::AcceleratorUnavailable(
                    "KVM device /dev/kvm does not exist on host".to_string(),
                ));
            }
            match std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(kvm_path)
            {
                Ok(_) => {}
                Err(e) => {
                    return Err(QemuError::AcceleratorUnavailable(format!(
                        "/dev/kvm is not accessible (read/write): {e}"
                    )));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capabilities_semver_parsing() {
        let output = "QEMU emulator version 9.2.0\nCopyright (c) 2003-2024 Fabrice Bellard";
        let v = QemuVersion::parse(output).expect("parse standard 9.2.0");
        assert_eq!(v.major, 9);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 0);
        assert_eq!(v.suffix, None);
        assert_eq!(v.to_string(), "9.2.0");
        assert!(!v.is_snapshot());
        assert_eq!(v.commit_hash(), None);
    }

    #[test]
    fn test_capabilities_snapshot_parsing() {
        let output = "QEMU emulator version 11.1.0 (v11.1.0-12130-ge470268ff4)\nCopyright (c) 2003-2026 Fabrice Bellard";
        let v = QemuVersion::parse(output).expect("parse snapshot with parens");
        assert_eq!(v.major, 11);
        assert_eq!(v.minor, 1);
        assert_eq!(v.patch, 0);
        assert_eq!(v.suffix.as_deref(), Some("v11.1.0-12130-ge470268ff4"));
        assert!(v.is_snapshot());
        assert_eq!(v.commit_hash(), Some("e470268ff4"));

        // Direct hyphenated snapshot token
        let direct = "11.1.0-12130-ge470268ff4";
        let v2 = QemuVersion::parse(direct).expect("parse direct hyphenated snapshot");
        assert_eq!(v2.major, 11);
        assert_eq!(v2.minor, 1);
        assert_eq!(v2.patch, 0);
        assert_eq!(v2.suffix.as_deref(), Some("12130-ge470268ff4"));
        assert_eq!(v2.commit_hash(), Some("e470268ff4"));
    }

    #[test]
    fn test_capabilities_debian_and_rc_parsing() {
        let debian = "QEMU emulator version 8.2.2 (Debian 1:8.2.2+ds-0ubuntu1)";
        let v = QemuVersion::parse(debian).expect("parse debian version");
        assert_eq!(v.major, 8);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 2);
        assert_eq!(v.suffix.as_deref(), Some("Debian 1:8.2.2+ds-0ubuntu1"));

        let rc = "QEMU emulator version 9.0.0-rc1";
        let v_rc = QemuVersion::parse(rc).expect("parse rc version");
        assert_eq!(v_rc.major, 9);
        assert_eq!(v_rc.minor, 0);
        assert_eq!(v_rc.patch, 0);
        assert_eq!(v_rc.suffix.as_deref(), Some("rc1"));
    }

    #[test]
    fn test_capabilities_malformed_version() {
        assert!(matches!(
            QemuVersion::parse(""),
            Err(QemuError::VersionParseError(_))
        ));
        assert!(matches!(
            QemuVersion::parse("   \n\t  "),
            Err(QemuError::VersionParseError(_))
        ));
        assert!(matches!(
            QemuVersion::parse("not a version string"),
            Err(QemuError::VersionParseError(_))
        ));
        assert!(matches!(
            QemuVersion::parse("QEMU emulator version 9.2"),
            Err(QemuError::VersionParseError(_))
        ));
        assert!(matches!(
            QemuVersion::parse("QEMU emulator version -9.2.0"),
            Err(QemuError::VersionParseError(_))
        ));
        assert!(matches!(
            QemuVersion::parse("QEMU emulator version 99999999999999999999.0.0"),
            Err(QemuError::VersionParseError(_))
        ));
        assert!(matches!(
            QemuVersion::parse("QEMU emulator version abc.def.ghi"),
            Err(QemuError::VersionParseError(_))
        ));
    }

    #[test]
    fn test_version_ordering() {
        let v800 = QemuVersion::new(8, 0, 0);
        let v800_b = QemuVersion::new(8, 0, 0);
        let v810 = QemuVersion::new(8, 1, 0);
        let v720 = QemuVersion::new(7, 2, 0);
        let v11_snap = QemuVersion::with_suffix(11, 1, 0, "snapshot");
        let v900_rc1 = QemuVersion::with_suffix(9, 0, 0, "rc1");
        let v900 = QemuVersion::new(9, 0, 0);

        assert_eq!(v800, v800_b);
        assert!(v810 > v800);
        assert!(v720 < v800);
        assert!(v11_snap > v800);
        assert!(v900_rc1 < v900);
    }

    #[test]
    fn test_version_policy_rejection() {
        let policy = QemuVersionPolicy::default();
        let old = QemuVersion::new(7, 2, 0);
        let res = policy.validate(&old);
        assert!(matches!(res, Err(QemuError::VersionPolicyViolation(_))));
        if let Err(QemuError::VersionPolicyViolation(msg)) = res {
            assert!(msg.contains("below minimum required version 8.0.0"));
        }

        let valid = QemuVersion::new(8, 0, 0);
        assert!(policy.validate(&valid).is_ok());

        let newer = QemuVersion::new(9, 2, 0);
        assert!(policy.validate(&newer).is_ok());
    }

    #[test]
    fn test_version_policy_override_allowed() {
        let policy = QemuVersionPolicy {
            enforce_version: false,
            ..Default::default()
        };

        let old = QemuVersion::new(7, 2, 0);
        assert!(policy.validate(&old).is_ok());
    }

    #[test]
    fn test_version_policy_disallow_snapshots() {
        let policy = QemuVersionPolicy::new((8, 0, 0), false, true);
        let snap = QemuVersion::with_suffix(11, 1, 0, "snapshot");
        let res = policy.validate(&snap);
        assert!(matches!(res, Err(QemuError::VersionPolicyViolation(_))));
    }

    #[test]
    fn test_capabilities_accel_whpx_detected() {
        let output = "Accelerators supported in QEMU binary:\ntcg\nwhpx\n";
        let accels = parse_accelerators(output).expect("parse accels");
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
        if let Err(QemuError::AcceleratorUnavailable(msg)) = res {
            assert!(msg.contains("Required accelerator 'whpx' is not supported"));
        }
    }

    #[test]
    fn test_capabilities_parse_accelerators_whitespace_and_casing() {
        let output = "  WHPX  \n  TCG  \n  KVM  \n";
        let accels = parse_accelerators(output).expect("parse accels with whitespace");
        assert_eq!(
            accels,
            vec![
                QemuAccelerator::Whpx,
                QemuAccelerator::Tcg,
                QemuAccelerator::Kvm
            ]
        );
    }

    #[test]
    fn test_capabilities_parse_accelerators_empty_or_unsupported() {
        assert!(matches!(
            parse_accelerators(""),
            Err(QemuError::AcceleratorUnavailable(_))
        ));
        assert!(matches!(
            parse_accelerators("Accelerators supported:\nhax\nhvf\n"),
            Err(QemuError::AcceleratorUnavailable(_))
        ));
    }
}
