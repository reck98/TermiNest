use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::domain::{MachineId, MachineState};

/// Hardware and feature capabilities supported by a hypervisor backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HypervisorCapabilities {
    pub hardware_acceleration: bool,
    pub live_snapshots: bool,
    pub disk_differencing: bool,
    pub vsock_transport: bool,
    pub pause_resume: bool,
}

impl Default for HypervisorCapabilities {
    fn default() -> Self {
        Self {
            hardware_acceleration: true,
            live_snapshots: true,
            disk_differencing: true,
            vsock_transport: true,
            pause_resume: true,
        }
    }
}

/// Dynamic runtime state and metrics reported directly by a hypervisor backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendMachineStatus {
    pub id: MachineId,
    pub state: MachineState,
    pub pid: Option<u32>,
    pub uptime_secs: u64,
    pub memory_rss_bytes: u64,
}

/// Deterministic failure injection options for MockHypervisorBackend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MockFailure {
    FailCreate(String),
    FailStart(String),
    FailStop(String),
    FailPause(String),
    FailResume(String),
    FailDelete(String),
    FailStatus(String),
    Timeout(Duration),
    Unavailable(String),
}
