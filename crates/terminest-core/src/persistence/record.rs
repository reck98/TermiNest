use serde::{Deserialize, Serialize};

use crate::domain::{MachineConfig, MachineState};

/// Durable persistent record of a virtual machine definition and its recorded lifecycle state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachineRecord {
    pub config: MachineConfig,
    pub state: MachineState,
}

impl MachineRecord {
    /// Constructs a new record initialized in Defined state.
    pub fn new(config: MachineConfig) -> Self {
        Self {
            config,
            state: MachineState::Defined,
        }
    }

    /// Constructs a record with an explicit state.
    pub fn with_state(config: MachineConfig, state: MachineState) -> Self {
        Self { config, state }
    }
}
