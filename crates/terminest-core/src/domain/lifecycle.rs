use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Discrete lifecycle states of a TermiNest virtual machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MachineState {
    /// Configuration defined in repository, but compute instance not yet created on backend.
    Defined,
    /// Virtual machine created on hypervisor backend; powered off.
    Created,
    /// Boot sequence initiated; powering on.
    Starting,
    /// Virtual machine active and running.
    Running,
    /// Transitioning CPU execution to paused.
    Pausing,
    /// Virtual machine vCPU execution paused.
    Paused,
    /// Transitioning vCPU execution back to running.
    Resuming,
    /// Shutdown sequence initiated; powering off.
    Stopping,
    /// Virtual machine cleanly stopped and powered off.
    Stopped,
    /// Deprovisioning / deletion in progress.
    Deleting,
    /// Unrecoverable failure state.
    Failed,
    /// Runtime state not reconciled with hypervisor backend.
    Unknown,
}

impl std::fmt::Display for MachineState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Defined => write!(f, "defined"),
            Self::Created => write!(f, "created"),
            Self::Starting => write!(f, "starting"),
            Self::Running => write!(f, "running"),
            Self::Pausing => write!(f, "pausing"),
            Self::Paused => write!(f, "paused"),
            Self::Resuming => write!(f, "resuming"),
            Self::Stopping => write!(f, "stopping"),
            Self::Stopped => write!(f, "stopped"),
            Self::Deleting => write!(f, "deleting"),
            Self::Failed => write!(f, "failed"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

impl MachineState {
    /// Checks whether transitioning from `self` to `target` is allowed.
    pub fn transition_to(&self, target: MachineState) -> Result<(), LifecycleError> {
        if *self == target {
            return Ok(());
        }

        if *self == MachineState::Running && target == MachineState::Deleting {
            return Err(LifecycleError::CannotDeleteWhileRunning);
        }

        let allowed = match (*self, target) {
            // Defined -> Created, Deleting, Failed
            (MachineState::Defined, MachineState::Created)
            | (MachineState::Defined, MachineState::Deleting)
            | (MachineState::Defined, MachineState::Failed) => true,

            // Created -> Starting, Deleting, Failed
            (MachineState::Created, MachineState::Starting)
            | (MachineState::Created, MachineState::Deleting)
            | (MachineState::Created, MachineState::Failed) => true,

            // Starting -> Running, Failed
            (MachineState::Starting, MachineState::Running)
            | (MachineState::Starting, MachineState::Failed) => true,

            // Running -> Stopping, Pausing, Failed
            (MachineState::Running, MachineState::Stopping)
            | (MachineState::Running, MachineState::Pausing)
            | (MachineState::Running, MachineState::Failed) => true,

            // Pausing -> Paused, Failed
            (MachineState::Pausing, MachineState::Paused)
            | (MachineState::Pausing, MachineState::Failed) => true,

            // Paused -> Resuming, Stopping (force stop), Failed
            (MachineState::Paused, MachineState::Resuming)
            | (MachineState::Paused, MachineState::Stopping)
            | (MachineState::Paused, MachineState::Failed) => true,

            // Resuming -> Running, Failed
            (MachineState::Resuming, MachineState::Running)
            | (MachineState::Resuming, MachineState::Failed) => true,

            // Stopping -> Stopped, Failed
            (MachineState::Stopping, MachineState::Stopped)
            | (MachineState::Stopping, MachineState::Failed) => true,

            // Stopped -> Starting, Deleting, Failed
            (MachineState::Stopped, MachineState::Starting)
            | (MachineState::Stopped, MachineState::Deleting)
            | (MachineState::Stopped, MachineState::Failed) => true,

            // Failed -> Starting (retry), Deleting
            (MachineState::Failed, MachineState::Starting)
            | (MachineState::Failed, MachineState::Deleting) => true,

            // Unknown -> Any valid reconciled state or deletion
            (MachineState::Unknown, MachineState::Stopped)
            | (MachineState::Unknown, MachineState::Running)
            | (MachineState::Unknown, MachineState::Paused)
            | (MachineState::Unknown, MachineState::Created)
            | (MachineState::Unknown, MachineState::Failed)
            | (MachineState::Unknown, MachineState::Deleting) => true,

            _ => false,
        };

        if allowed {
            Ok(())
        } else {
            Err(LifecycleError::InvalidTransition {
                current: *self,
                target,
            })
        }
    }

    /// Whether this machine is in a running or executing state.
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running | Self::Starting | Self::Resuming)
    }

    /// Whether this machine can be started.
    pub fn can_start(&self) -> bool {
        matches!(self, Self::Created | Self::Stopped | Self::Failed)
    }

    /// Whether this machine can be stopped.
    pub fn can_stop(&self) -> bool {
        matches!(self, Self::Running | Self::Paused | Self::Starting)
    }

    /// Whether this machine can be paused.
    pub fn can_pause(&self) -> bool {
        matches!(self, Self::Running)
    }

    /// Whether this machine can be resumed.
    pub fn can_resume(&self) -> bool {
        matches!(self, Self::Paused)
    }

    /// Whether this machine can be safely deleted.
    pub fn can_delete(&self) -> bool {
        matches!(
            self,
            Self::Defined | Self::Created | Self::Stopped | Self::Failed | Self::Unknown
        )
    }
}

/// Errors occurring during VM lifecycle state transitions.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum LifecycleError {
    #[error("Invalid lifecycle transition from '{current}' to '{target}'")]
    InvalidTransition {
        current: MachineState,
        target: MachineState,
    },

    #[error("Cannot delete machine while it is running. Stop the machine before deleting or pass --force.")]
    CannotDeleteWhileRunning,

    #[error("Machine is already running")]
    AlreadyRunning,

    #[error("Machine is not running (current state: '{0}')")]
    NotRunning(MachineState),

    #[error("Machine is not paused (current state: '{0}')")]
    NotPaused(MachineState),

    #[error("Machine is already paused")]
    AlreadyPaused,
}
