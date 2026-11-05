//! QEMU Process Management Subsystem for TermiNest (`terminest-core`).
//!
//! Provides executable discovery, capability probing, validated argument
//! construction, asynchronous process supervision, bounded output collection,
//! and platform-specific process containment.

pub mod capabilities;
pub mod config;
pub mod discovery;
pub mod error;

pub use capabilities::{
    parse_accelerators, parse_version, QemuCapabilities, QemuVersion, QemuVersionPolicy,
};
pub use config::{QemuAccelerator, QemuCpuConfig, QemuDisplay, QemuMachineType, QemuProcessConfig};
pub use discovery::{is_executable, QemuDiscovery};
pub use error::QemuError;
