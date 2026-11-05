use std::path::{Path, PathBuf};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{MachineId, MachineName};

/// Supported guest operating system families.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OsFamily {
    Linux,
    Windows,
}

/// Target CPU architectures for virtual machines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Architecture {
    #[serde(rename = "x86_64")]
    X86_64,
    #[serde(rename = "aarch64")]
    Aarch64,
}

/// Virtual disk formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiskFormat {
    Vhdx,
    Qcow2,
    Raw,
}

/// Network isolation tiers adhering to ADR-0008.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum NetworkTier {
    Isolated,
    #[default]
    Nat,
    Bridged,
}

/// Hypervisor backend provider choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HypervisorType {
    Mock,
    #[serde(rename = "hyperv")]
    HyperV,
    #[serde(rename = "qemu-whp")]
    QemuWhp,
    #[serde(rename = "qemu-kvm")]
    QemuKvm,
}

impl std::fmt::Display for HypervisorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mock => write!(f, "mock"),
            Self::HyperV => write!(f, "hyperv"),
            Self::QemuWhp => write!(f, "qemu-whp"),
            Self::QemuKvm => write!(f, "qemu-kvm"),
        }
    }
}

/// Operating system specification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OsSpec {
    pub family: OsFamily,
    pub distro: String,
    pub version: String,
    pub arch: Architecture,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guest_agent_version: Option<String>,
}

/// Display configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplaySpec {
    pub width: u32,
    pub height: u32,
    pub dpi: u32,
}

impl Default for DisplaySpec {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            dpi: 96,
        }
    }
}

/// Hardware resource allocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardwareSpec {
    pub cpus: u32,
    pub memory_mb: u64,
    #[serde(default)]
    pub nested_virtualization: bool,
    #[serde(default)]
    pub display: DisplaySpec,
}

/// Storage allocation and virtual disk configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageSpec {
    pub format: DiskFormat,
    pub disk_path: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_image: Option<PathBuf>,
    #[serde(default = "default_capacity_gb")]
    pub capacity_gb: u64,
    #[serde(default)]
    pub ephemeral: bool,
}

fn default_capacity_gb() -> u64 {
    32
}

/// Port forwarding configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortForwardRule {
    pub host_port: u16,
    pub guest_port: u16,
    pub protocol: String,
}

/// Network configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NetworkSpec {
    pub tier: NetworkTier,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mac_address: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub port_forwarding: Vec<PortForwardRule>,
}

/// Hypervisor backend binding details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HypervisorSpec {
    pub backend: HypervisorType,
    #[serde(default = "default_vsock_port")]
    pub vsock_port: u32,
    #[serde(default = "default_vsock_cid")]
    pub vsock_cid: u32,
}

fn default_vsock_port() -> u32 {
    10001
}

fn default_vsock_cid() -> u32 {
    3
}

/// Authoritative, versioned configuration contract for a TermiNest virtual machine (`machine.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachineConfig {
    pub version: String,
    pub id: MachineId,
    pub name: MachineName,
    pub os: OsSpec,
    pub hardware: HardwareSpec,
    pub storage: StorageSpec,
    pub networking: NetworkSpec,
    pub hypervisor: HypervisorSpec,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl MachineConfig {
    /// Validates configuration parameters and invariants according to system specifications.
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        if self.version != "1.0.0" {
            return Err(ConfigValidationError::UnsupportedSchemaVersion(
                self.version.clone(),
            ));
        }

        // Validate CPU
        if self.hardware.cpus == 0 || self.hardware.cpus > 64 {
            return Err(ConfigValidationError::InvalidCpu(self.hardware.cpus));
        }

        // Validate Memory (512 MiB to 128 GiB)
        if self.hardware.memory_mb < 512 || self.hardware.memory_mb > 131_072 {
            return Err(ConfigValidationError::InvalidMemory(self.hardware.memory_mb));
        }

        // Validate Display
        if self.hardware.display.width < 640
            || self.hardware.display.width > 7680
            || self.hardware.display.height < 480
            || self.hardware.display.height > 4320
        {
            return Err(ConfigValidationError::InvalidDisplayResolution(
                self.hardware.display.width,
                self.hardware.display.height,
            ));
        }

        // Validate Storage Capacity
        if self.storage.capacity_gb == 0 || self.storage.capacity_gb > 2048 {
            return Err(ConfigValidationError::InvalidStorageCapacity(
                self.storage.capacity_gb,
            ));
        }

        // Validate storage path against path traversal
        Self::validate_path(&self.storage.disk_path)?;
        if let Some(ref base) = self.storage.base_image {
            Self::validate_path(base)?;
        }

        // Validate Vsock port
        if self.hypervisor.vsock_port < 1024 || self.hypervisor.vsock_port > 65535 {
            return Err(ConfigValidationError::InvalidVsockPort(
                self.hypervisor.vsock_port,
            ));
        }

        Ok(())
    }

    fn validate_path(path: &Path) -> Result<(), ConfigValidationError> {
        let s = path.to_string_lossy();
        if s.contains("..") || s.contains('\0') {
            return Err(ConfigValidationError::PathTraversal(s.to_string()));
        }
        Ok(())
    }
}

/// Errors occurring during configuration validation.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ConfigValidationError {
    #[error("Machine name is empty")]
    EmptyName,

    #[error("Machine name is too long ({0} chars, max 64)")]
    NameTooLong(usize),

    #[error("Machine name contains invalid character '{0}' (only alphanumeric, hyphen, underscore allowed)")]
    InvalidNameCharacter(char),

    #[error("Path traversal or illegal path detected: '{0}'")]
    PathTraversal(String),

    #[error("Invalid machine ID: '{0}'")]
    InvalidMachineId(String),

    #[error("Unsupported schema version: '{0}', expected '1.0.0'")]
    UnsupportedSchemaVersion(String),

    #[error("Invalid CPU allocation: {0} (must be 1..=64)")]
    InvalidCpu(u32),

    #[error("Invalid memory allocation: {0} MiB (must be 512..=131072 MiB)")]
    InvalidMemory(u64),

    #[error("Invalid display resolution: {0}x{1}")]
    InvalidDisplayResolution(u32, u32),

    #[error("Invalid storage capacity: {0} GiB (must be 1..=2048 GiB)")]
    InvalidStorageCapacity(u64),

    #[error("Invalid Vsock port: {0} (must be 1024..=65535)")]
    InvalidVsockPort(u32),
}
