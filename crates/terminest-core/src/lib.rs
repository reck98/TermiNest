//! TermiNest Core: Foundational Host Orchestrator Layer.
//!
//! Owns the machine domain model, lifecycle state machine, hypervisor backend
//! SPI, persistence abstraction, and orchestration engine.

pub mod domain;
pub mod error;

pub use domain::{
    Architecture, ConfigValidationError, DiskFormat, DisplaySpec, HardwareSpec, HypervisorSpec,
    HypervisorType, LifecycleError, MachineConfig, MachineId, MachineName, MachineState,
    NetworkSpec, NetworkTier, OsFamily, OsSpec, PortForwardRule, StorageSpec,
};
pub use error::{CoreError, HypervisorError, PersistenceError};
