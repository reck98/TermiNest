//! TermiNest Core: Foundational Host Orchestrator Layer.
//!
//! Owns the machine domain model, lifecycle state machine, hypervisor backend
//! SPI, persistence abstraction, and orchestration engine.

pub mod backend;
pub mod domain;
pub mod error;
pub mod persistence;

pub use backend::{
    BackendMachineStatus, HypervisorBackend, HypervisorCapabilities, MockFailure,
    MockHypervisorBackend,
};
pub use domain::{
    Architecture, ConfigValidationError, DiskFormat, DisplaySpec, HardwareSpec, HypervisorSpec,
    HypervisorType, LifecycleError, MachineConfig, MachineId, MachineName, MachineState,
    NetworkSpec, NetworkTier, OsFamily, OsSpec, PortForwardRule, StorageSpec,
};
pub use error::{CoreError, HypervisorError, PersistenceError};
pub use persistence::{FilesystemMachineRepository, MachineRecord, MachineRepository};
