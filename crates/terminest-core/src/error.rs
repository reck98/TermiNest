use std::path::PathBuf;
use thiserror::Error;

pub use crate::domain::LifecycleError;
use crate::domain::{MachineId, MachineName};

/// Comprehensive application and orchestration error types for TermiNest Core.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("Configuration validation error: {0}")]
    Validation(#[from] crate::domain::ConfigValidationError),

    #[error("Lifecycle state error: {0}")]
    Lifecycle(#[from] LifecycleError),

    #[error("Hypervisor backend error: {0}")]
    Backend(#[from] HypervisorError),

    #[error("Persistence error: {0}")]
    Persistence(#[from] PersistenceError),

    #[error("Machine not found: {0}")]
    MachineNotFound(String),

    #[error("Machine '{0}' already exists")]
    MachineAlreadyExists(String),

    #[error("Concurrent operation conflict on machine '{0}': {1}")]
    ConcurrencyConflict(String, String),

    #[error("Operation aborted: {0}")]
    OperationAborted(String),

    #[error("Audit log error on machine '{machine}': {error}")]
    AuditFailure { machine: String, error: String },

    #[error("Operation '{operation}' succeeded on machine '{machine}', but recording audit completion failed: {error}")]
    AuditPostMutationFailure {
        operation: String,
        machine: String,
        error: String,
    },

    #[error("File system I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Errors raised by hypervisor backends.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum HypervisorError {
    #[error("Hypervisor backend is unavailable or not installed: {0}")]
    Unavailable(String),

    #[error("Machine '{0}' not found on backend")]
    MachineNotFound(MachineId),

    #[error("Machine '{0}' already exists on backend")]
    MachineAlreadyExists(MachineId),

    #[error("Operation '{0}' is not supported by hypervisor backend '{1}'")]
    UnsupportedOperation(&'static str, &'static str),

    #[error("Operation timed out after {0:?}")]
    Timeout(std::time::Duration),

    #[error("Backend execution failure: {0}")]
    ExecutionFailed(String),
}

/// Errors raised by metadata persistence operations.
#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("Storage directory not initialized or accessible: {0}")]
    DirectoryUnreachable(PathBuf),

    #[error("Machine metadata corrupted at '{0}': {1}")]
    CorruptMetadata(PathBuf, String),

    #[error("Unsupported schema version: '{0}' (expected '{1}')")]
    UnsupportedSchemaVersion(String, String),

    #[error("Atomic write failed for '{0}': {1}")]
    AtomicWriteFailed(PathBuf, String),

    #[error("Machine '{0}' not found in repository")]
    NotFound(MachineId),

    #[error("Machine name '{0}' collision in repository")]
    NameCollision(MachineName),

    #[error("Lock acquisition timed out after {1:?} on '{0}'")]
    LockTimeout(PathBuf, std::time::Duration),

    #[error("I/O error during persistence: {0}")]
    Io(#[from] std::io::Error),
}
