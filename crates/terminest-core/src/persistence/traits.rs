use async_trait::async_trait;

use crate::domain::{MachineId, MachineName, MachineState};
use crate::error::PersistenceError;
use crate::persistence::MachineRecord;

/// Interface for durable storage of virtual machine specifications and metadata.
#[async_trait]
pub trait MachineRepository: Send + Sync {
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
