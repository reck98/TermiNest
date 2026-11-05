//! QEMU capability probing, version parsing, and version policy enforcement.

use std::fmt;
use std::str::FromStr;

use super::config::QemuAccelerator;
use super::error::QemuError;

/// Classification category of a QEMU build/release based on its version string and provenance metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QemuReleaseKind {
    /// Standard stable release without prerelease or development snapshot tags (e.g. `9.2.0`).
    Stable,
    /// Official release candidate or prerelease (e.g. `9.0.0-rc1`, `8.1.0-alpha2`).
    PreRelease,
    /// Development snapshot build (e.g. `11.1.0-12130-ge470268ff4`, containing Git commit hash or snapshot tag).
    Snapshot,
    /// Stable release packaged with vendor/distribution metadata (e.g. `8.2.2 (Debian 1:8.2.2+ds-0ubuntu1)`).
    DistributionPackage,
}

impl fmt::Display for QemuReleaseKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stable => write!(f, "stable"),
            Self::PreRelease => write!(f, "prerelease"),
            Self::Snapshot => write!(f, "snapshot"),
            Self::DistributionPackage => write!(f, "distribution-package"),
        }
    }
}

/// Status of host hardware virtualization usability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AcceleratorHostStatus {
    /// Host hypervisor usability has not been probed (e.g. during initial executable discovery).
    NotProbed,
    /// Host hypervisor is available and usable on this machine.
    Available,
    /// Host hypervisor is unavailable or unsupported on this machine.
    Unavailable,
}

impl fmt::Display for AcceleratorHostStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotProbed => write!(f, "not probed"),
            Self::Available => write!(f, "available"),
            Self::Unavailable => write!(f, "unavailable"),
        }
    }
}

/// Structured semantic version representation of a QEMU binary.
///
/// Implements SemVer 2.0 precedence comparison rules, treating release candidates
/// and development snapshots as lower precedence than final stable releases with the
/// same core `(major, minor, patch)` triplet. Distribution-specific package suffixes
/// are treated as build metadata and do not alter stable version precedence.
#[derive(Debug, Clone)]
pub struct QemuVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    /// Raw unparsed suffix preserved for display and diagnostics.
    pub suffix: Option<String>,
    /// Structured prerelease identifier if applicable (e.g. `rc1`, `rc10`, `12130-ge470268ff4`).
    pub prerelease: Option<String>,
    /// Build or distribution provenance metadata (e.g. `Debian 1:8.2.2+ds-0ubuntu1`, `ge470268ff4`).
    pub build_metadata: Option<String>,
    /// Classification kind of this QEMU release.
    pub kind: QemuReleaseKind,
}

impl QemuVersion {
    /// Creates a new stable `QemuVersion` without a suffix.
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
            suffix: None,
            prerelease: None,
            build_metadata: None,
            kind: QemuReleaseKind::Stable,
        }
    }

    /// Creates a new `QemuVersion` with an explicit suffix, classifying its release kind.
    pub fn with_suffix(major: u32, minor: u32, patch: u32, suffix: impl Into<String>) -> Self {
        let raw_suffix = suffix.into();
        let (kind, prerelease, build_metadata) = classify_suffix(&raw_suffix);
        Self {
            major,
            minor,
            patch,
            suffix: Some(raw_suffix),
            prerelease,
            build_metadata,
            kind,
        }
    }

    /// Parses QEMU version output (such as `qemu-system-x86_64 --version`).
    ///
    /// Accepts standard release strings (e.g. `QEMU emulator version 9.2.0`),
    /// snapshot strings (e.g. `QEMU emulator version 11.1.0 (v11.1.0-12130-ge470268ff4)`
    /// or `11.1.0-12130-ge470268ff4`), distro-modified versions
    /// (e.g. `QEMU emulator version 8.2.2 (Debian 1:8.2.2+ds-0ubuntu1)`),
    /// release candidates (`9.0.0-rc1`), or bare semver triplets (`8.0.0`).
    pub fn parse(output: &str) -> Result<Self, QemuError> {
        parse_version(output)
    }

    /// Returns `true` if this version represents a development snapshot build.
    ///
    /// Release candidates, distribution packaging suffixes, and stable releases return `false`.
    pub fn is_snapshot(&self) -> bool {
        self.kind == QemuReleaseKind::Snapshot
    }

    /// Returns `true` if this version represents an official release candidate or prerelease.
    pub fn is_prerelease(&self) -> bool {
        self.kind == QemuReleaseKind::PreRelease
    }

    /// Returns the release classification category for this version.
    pub fn kind(&self) -> QemuReleaseKind {
        self.kind
    }

    /// Extracts the Git commit hash from the suffix or build metadata if present.
    pub fn commit_hash(&self) -> Option<&str> {
        let s = self.suffix.as_deref()?;
        if let Some(pos) = s.find("-g") {
            let after_g = &s[pos + 2..];
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
        // Step 1: Compare (major, minor, patch) triplets.
        match (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch)) {
            std::cmp::Ordering::Equal => {
                // Step 2: SemVer 2.0 §11: A version without a prerelease has higher
                // precedence than a version with a prerelease.
                match (&self.prerelease, &other.prerelease) {
                    (None, None) => std::cmp::Ordering::Equal,
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (Some(a), Some(b)) => compare_prerelease_identifiers(a, b),
                }
            }
            ord => ord,
        }
    }
}

impl PartialOrd for QemuVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for QemuVersion {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
    }
}

impl Eq for QemuVersion {}

impl std::hash::Hash for QemuVersion {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.major.hash(state);
        self.minor.hash(state);
        self.patch.hash(state);
        self.prerelease.hash(state);
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

/// Compares two SemVer 2.0 prerelease identifier strings.
///
/// - Dot-separated identifiers are compared in sequence.
/// - Identifiers consisting of digits only are compared numerically (`rc2` vs `rc10`).
/// - Numeric identifiers have lower precedence than alphanumeric identifiers.
/// - Alphanumeric identifiers are compared lexicographically in ASCII sort order.
fn compare_prerelease_identifiers(a: &str, b: &str) -> std::cmp::Ordering {
    let parts_a: Vec<&str> = a.split('.').collect();
    let parts_b: Vec<&str> = b.split('.').collect();

    for (pa, pb) in parts_a.iter().zip(parts_b.iter()) {
        if pa == pb {
            continue;
        }

        let num_a = pa.parse::<u64>().ok();
        let num_b = pb.parse::<u64>().ok();

        match (num_a, num_b) {
            (Some(na), Some(nb)) => {
                if na != nb {
                    return na.cmp(&nb);
                }
            }
            (Some(_), None) => {
                // SemVer §11: Numeric identifiers always have lower precedence than non-numeric identifiers.
                return std::cmp::Ordering::Less;
            }
            (None, Some(_)) => {
                return std::cmp::Ordering::Greater;
            }
            (None, None) => {
                // If both are alphanumeric, inspect whether they share an alphabetical prefix
                // and a numeric suffix (e.g. "rc2" vs "rc10").
                let (prefix_a, suffix_a) = split_alphanumeric_prefix_suffix(pa);
                let (prefix_b, suffix_b) = split_alphanumeric_prefix_suffix(pb);

                if prefix_a == prefix_b {
                    if let (Some(sa), Some(sb)) = (suffix_a, suffix_b) {
                        let ord = sa.cmp(&sb);
                        if ord != std::cmp::Ordering::Equal {
                            return ord;
                        }
                    }
                }

                let ord = pa.cmp(pb);
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
            }
        }
    }

    parts_a.len().cmp(&parts_b.len())
}

/// Splits an alphanumeric string like `"rc10"` into `("rc", Some(10))`.
fn split_alphanumeric_prefix_suffix(s: &str) -> (&str, Option<u64>) {
    let non_digit_end = s.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    if non_digit_end < s.len() && non_digit_end > 0 {
        let prefix = &s[..non_digit_end];
        let num = s[non_digit_end..].parse::<u64>().ok();
        (prefix, num)
    } else {
        (s, None)
    }
}

/// Classifies a raw suffix into release kind, optional prerelease identifier, and build metadata.
fn classify_suffix(raw_suffix: &str) -> (QemuReleaseKind, Option<String>, Option<String>) {
    let s = raw_suffix.trim();

    // Check for development snapshot: contains a Git commit hash ("-g<hex>") or explicit "snapshot"/"dev" tag.
    if s.contains("-g") || s.contains("snapshot") || s.contains("dev") || s.contains("git") {
        let prerelease = Some(s.to_string());
        let build_metadata = s.find("-g").map(|pos| s[pos + 1..].to_string());
        return (QemuReleaseKind::Snapshot, prerelease, build_metadata);
    }

    // Check for release candidate or prerelease: "rc1", "rc.1", "alpha", "beta", "pre".
    let lower = s.to_ascii_lowercase();
    if lower.starts_with("rc")
        || lower.starts_with("alpha")
        || lower.starts_with("beta")
        || lower.starts_with("pre")
    {
        return (QemuReleaseKind::PreRelease, Some(s.to_string()), None);
    }

    // Check for distribution package metadata: "Debian", "Ubuntu", "Fedora", "+ds", packaging revisions.
    if lower.contains("debian")
        || lower.contains("ubuntu")
        || lower.contains("fedora")
        || lower.contains("red hat")
        || lower.contains("arch")
        || lower.contains("alpine")
        || s.contains("+ds")
        || s.contains(':')
    {
        return (
            QemuReleaseKind::DistributionPackage,
            None,
            Some(s.to_string()),
        );
    }

    // Default fallback: treated as prerelease if it starts with hyphen or alphanumeric token.
    (QemuReleaseKind::PreRelease, Some(s.to_string()), None)
}

/// Safely truncates a UTF-8 string slice to at most `max_bytes` without panicking
/// at a multibyte character boundary.
pub fn safe_truncate_str(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut boundary = max_bytes;
    while boundary > 0 && !s.is_char_boundary(boundary) {
        boundary -= 1;
    }
    &s[..boundary]
}

/// Parses a QEMU version string into a structured [`QemuVersion`].
pub fn parse_version(output: &str) -> Result<QemuVersion, QemuError> {
    if output.trim().is_empty() {
        return Err(QemuError::VersionParseError(
            "Version output is empty".to_string(),
        ));
    }

    // UTF-8 safe scanning truncation to 64 KiB ceiling
    let truncated = safe_truncate_str(output, 65_536);

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

    let raw_suffix = if let Some(s) = inline_suffix {
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

    let (kind, prerelease, build_metadata) = match raw_suffix.as_deref() {
        Some(s) => classify_suffix(s),
        None => (QemuReleaseKind::Stable, None, None),
    };

    Ok(QemuVersion {
        major,
        minor,
        patch,
        suffix: raw_suffix,
        prerelease,
        build_metadata,
        kind,
    })
}

/// Parses the output of `qemu-system-x86_64 -accel help` to extract compiled accelerators.
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

        // Only development snapshots are guarded by allow_snapshots.
        // Stable distribution packages and release candidates are not rejected as snapshots.
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
///
/// **Important Distinction:** `accelerators` records accelerators compiled into the
/// QEMU binary as reported by `qemu-system-x86_64 -accel help`. It does **not** guarantee
/// that the host hypervisor (e.g. WHPX or KVM) is enabled or usable in the current OS environment.
/// Host hypervisor usability is represented separately via [`AcceleratorHostStatus`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QemuCapabilities {
    /// Parsed version of the QEMU binary.
    pub version: QemuVersion,
    /// List of accelerators compiled into the QEMU binary.
    pub accelerators: Vec<QemuAccelerator>,
    /// Status of host WHPX hypervisor usability.
    pub host_whpx_status: AcceleratorHostStatus,
    /// Status of host KVM hypervisor usability.
    pub host_kvm_status: AcceleratorHostStatus,
}

impl QemuCapabilities {
    /// Creates a new `QemuCapabilities` instance with unprobed host hypervisor status.
    pub const fn new(version: QemuVersion, accelerators: Vec<QemuAccelerator>) -> Self {
        Self {
            version,
            accelerators,
            host_whpx_status: AcceleratorHostStatus::NotProbed,
            host_kvm_status: AcceleratorHostStatus::NotProbed,
        }
    }

    /// Checks if a specific accelerator is compiled into the QEMU binary.
    pub fn supports_accelerator(&self, accel: QemuAccelerator) -> bool {
        self.accelerators.contains(&accel)
    }

    /// Verifies that a specific accelerator is compiled into the binary.
    pub fn validate_accelerator_support(&self, accel: QemuAccelerator) -> Result<(), QemuError> {
        if !self.supports_accelerator(accel) {
            return Err(QemuError::AcceleratorUnavailable(format!(
                "Required accelerator '{}' is not supported by QEMU binary",
                accel.as_str()
            )));
        }
        Ok(())
    }

    /// Validates that the host platform's required hardware accelerator is supported by the binary.
    ///
    /// - On Windows: requires `QemuAccelerator::Whpx` compiled in the binary.
    ///   *(Note: host WHPX kernel hypervisor enablement is verified upon launch supervision in B4).*
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
        assert_eq!(v.kind, QemuReleaseKind::Stable);
        assert!(!v.is_snapshot());
        assert!(!v.is_prerelease());
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
        assert_eq!(v.kind, QemuReleaseKind::Snapshot);
        assert!(v.is_snapshot());
        assert_eq!(v.commit_hash(), Some("e470268ff4"));

        let direct = "11.1.0-12130-ge470268ff4";
        let v2 = QemuVersion::parse(direct).expect("parse direct hyphenated snapshot");
        assert_eq!(v2.major, 11);
        assert_eq!(v2.minor, 1);
        assert_eq!(v2.patch, 0);
        assert_eq!(v2.suffix.as_deref(), Some("12130-ge470268ff4"));
        assert_eq!(v2.kind, QemuReleaseKind::Snapshot);
        assert!(v2.is_snapshot());
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
        assert_eq!(v.kind, QemuReleaseKind::DistributionPackage);
        assert!(!v.is_snapshot());
        assert!(!v.is_prerelease());

        let rc = "QEMU emulator version 9.0.0-rc1";
        let v_rc = QemuVersion::parse(rc).expect("parse rc version");
        assert_eq!(v_rc.major, 9);
        assert_eq!(v_rc.minor, 0);
        assert_eq!(v_rc.patch, 0);
        assert_eq!(v_rc.suffix.as_deref(), Some("rc1"));
        assert_eq!(v_rc.kind, QemuReleaseKind::PreRelease);
        assert!(v_rc.is_prerelease());
        assert!(!v_rc.is_snapshot());
    }

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
    fn test_distribution_package_precedence_equals_stable() {
        let debian = QemuVersion::parse("8.2.2 (Debian 1:8.2.2+ds-0ubuntu1)").unwrap();
        let pure_stable = QemuVersion::parse("8.2.2").unwrap();
        assert_eq!(
            debian, pure_stable,
            "Distribution packaging build metadata must not affect stable SemVer precedence"
        );
    }

    #[test]
    fn test_version_policy_allow_snapshots_false_with_debian_package() {
        let policy = QemuVersionPolicy::new((8, 0, 0), false, true);
        let debian = QemuVersion::parse("8.2.2 (Debian 1:8.2.2+ds-0ubuntu1)").unwrap();
        assert!(
            policy.validate(&debian).is_ok(),
            "allow_snapshots = false must NOT reject stable distribution packages"
        );

        let snap = QemuVersion::parse("11.1.0-12130-ge470268ff4").unwrap();
        assert!(
            policy.validate(&snap).is_err(),
            "allow_snapshots = false must reject development snapshots"
        );
    }

    #[test]
    fn test_utf8_safe_truncation() {
        let short = "hello";
        assert_eq!(safe_truncate_str(short, 10), "hello");

        // "a🦀b" -> 'a' (1 byte), '🦀' (4 bytes: F0 9F A6 80), 'b' (1 byte)
        let s = "a\u{1F980}b";
        assert_eq!(s.len(), 6);

        // Truncate at byte 2, 3, or 4 (inside 🦀)
        assert_eq!(safe_truncate_str(s, 1), "a");
        assert_eq!(safe_truncate_str(s, 2), "a");
        assert_eq!(safe_truncate_str(s, 3), "a");
        assert_eq!(safe_truncate_str(s, 4), "a");
        assert_eq!(safe_truncate_str(s, 5), "a\u{1F980}");
        assert_eq!(safe_truncate_str(s, 6), "a\u{1F980}b");

        // Input exceeding 65,536 bytes with multibyte character crossing 65,536
        let mut long_str = "a".repeat(65_534);
        long_str.push('\u{1F980}'); // 4 bytes, crossing byte 65,536
        long_str.push_str("QEMU emulator version 9.2.0\n");

        let truncated = safe_truncate_str(&long_str, 65_536);
        assert_eq!(truncated.len(), 65_534); // stepped back before the crab
        assert!(truncated.is_char_boundary(truncated.len()));
    }

    #[test]
    fn test_parse_version_token_before_and_beyond_boundary() {
        // Token before boundary: parses fine even with 70 KiB trailing data
        let mut input = "QEMU emulator version 9.2.0\n".to_string();
        input.push_str(&"x".repeat(70_000));
        let v = parse_version(&input).expect("parse version before boundary");
        assert_eq!(v.major, 9);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 0);

        // Token beyond boundary (prefixed with 70 KiB of padding): fails cleanly without panic
        let mut padded = "x".repeat(70_000);
        padded.push_str("\nQEMU emulator version 9.2.0\n");
        let res = parse_version(&padded);
        assert!(
            matches!(res, Err(QemuError::VersionParseError(_))),
            "Token beyond 64 KiB boundary must return controlled VersionParseError without panic"
        );
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
        let v11_snap = QemuVersion::with_suffix(11, 1, 0, "12130-ge470268ff4");
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
        let snap = QemuVersion::with_suffix(11, 1, 0, "12130-ge470268ff4");
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
        assert_eq!(caps.host_whpx_status, AcceleratorHostStatus::NotProbed);
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
