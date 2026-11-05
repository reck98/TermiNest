use async_trait::async_trait;
use std::time::Duration;

use crate::domain::{AuditRecord, MachineId, MachineName, MachineState};
use crate::error::PersistenceError;
use crate::persistence::{FileLock, MachineRecord};

/// Interface for durable storage of virtual machine specifications and metadata.
#[async_trait]
pub trait MachineRepository: Send + Sync {
    /// Acquires an exclusive repository-wide lock for inventory mutations.
    async fn acquire_repo_lock(&self, timeout: Duration) -> Result<FileLock, PersistenceError>;

    /// Acquires an exclusive per-machine lock for lifecycle mutations.
    async fn acquire_machine_lock(
        &self,
        id: &MachineId,
        timeout: Duration,
    ) -> Result<FileLock, PersistenceError>;

    /// Appends a structured audit event to the machine's audit ledger.
    async fn append_audit_event(
        &self,
        id: &MachineId,
        record: &AuditRecord,
    ) -> Result<(), PersistenceError>;

    /// Reads all structured audit events from the machine's audit ledger.
    async fn read_audit_events(&self, id: &MachineId)
        -> Result<Vec<AuditRecord>, PersistenceError>;

    /// Saves a machine record. Fails with NameCollision if another machine shares the same name.
    async fn save(&self, record: &MachineRecord) -> Result<(), PersistenceError>;

    /// Retrieves a machine record by its immutable identifier.
    async fn get_by_id(&self, id: &MachineId) -> Result<Option<MachineRecord>, PersistenceError>;

    /// Retrieves a machine record by its human-readable machine name.
    async fn get_by_name(
        &self,
        name: &MachineName,
    ) -> Result<Option<MachineRecord>, PersistenceError>;

    /// Lists all discoverable machine records in the repository.
    async fn list(&self) -> Result<Vec<MachineRecord>, PersistenceError>;

    /// Deletes a machine's metadata and directory from the repository.
    async fn delete(&self, id: &MachineId) -> Result<(), PersistenceError>;

    /// Atomically updates the recorded lifecycle state of a machine.
    async fn update_state(
        &self,
        id: &MachineId,
        state: MachineState,
    ) -> Result<MachineRecord, PersistenceError>;
}
