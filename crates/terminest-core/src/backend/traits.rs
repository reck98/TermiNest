use async_trait::async_trait;
use std::time::Duration;

use crate::backend::{BackendMachineStatus, HypervisorCapabilities};
use crate::domain::{HypervisorType, MachineConfig, MachineId};
use crate::error::HypervisorError;

/// Service Provider Interface (SPI) for hypervisor virtualization backends.
///
/// Decouples higher-level orchestration, state persistence, and CLI commands
/// from platform-specific virtualization mechanisms (Hyper-V, QEMU/WHP, QEMU/KVM, Mock).
#[async_trait]
pub trait HypervisorBackend: Send + Sync {
    /// Returns the discriminator type of this backend.
    fn backend_type(&self) -> HypervisorType;

    /// Returns static capabilities supported by this hypervisor provider.
    fn capabilities(&self) -> HypervisorCapabilities;

    /// Probes the host system to determine hypervisor health, kernel drivers, and active support.
    async fn probe(&self) -> Result<HypervisorCapabilities, HypervisorError>;

    /// Creates virtual machine hardware definitions, disk overlays, and firmware config.
    async fn create_machine(&self, config: &MachineConfig) -> Result<(), HypervisorError>;

    /// Powers on the virtual machine in detached background execution.
    async fn start_machine(&self, id: &MachineId) -> Result<BackendMachineStatus, HypervisorError>;

    /// Requests shutdown of the virtual machine.
    /// If `force` is true, immediately terminates execution (equivalent to cutting virtual power).
    async fn stop_machine(
        &self,
        id: &MachineId,
        timeout: Duration,
        force: bool,
    ) -> Result<(), HypervisorError>;

    /// Freezes vCPU execution and guest timers.
    async fn pause_machine(&self, id: &MachineId) -> Result<(), HypervisorError>;

    /// Unfreezes vCPU execution.
    async fn resume_machine(&self, id: &MachineId) -> Result<(), HypervisorError>;

    /// Deletes the virtual machine instance and associated runtime resources.
    async fn delete_machine(&self, id: &MachineId) -> Result<(), HypervisorError>;

    /// Queries the live runtime state of the virtual machine directly from the backend.
    async fn get_machine_status(
        &self,
        id: &MachineId,
    ) -> Result<BackendMachineStatus, HypervisorError>;
}
