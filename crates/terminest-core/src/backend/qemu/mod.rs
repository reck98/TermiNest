//! QEMU Process Management Subsystem for TermiNest (`terminest-core`).
//!
//! Provides executable discovery, capability probing, validated argument
//! construction, asynchronous process supervision, bounded output collection,
//! and platform-specific process containment.

pub mod config;
pub mod error;

pub use config::{QemuAccelerator, QemuCpuConfig, QemuDisplay, QemuMachineType, QemuProcessConfig};
pub use error::QemuError;
