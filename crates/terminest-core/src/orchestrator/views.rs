use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::{HypervisorType, MachineId, MachineName, MachineState};

/// Comprehensive runtime status view reconciled between persistent metadata and backend telemetry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachineStatusView {
    pub id: MachineId,
    pub name: MachineName,
    pub state: MachineState,
    pub backend: HypervisorType,
    pub pid: Option<u32>,
    pub uptime_secs: u64,
    pub memory_rss_bytes: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Concise inventory summary view for machine listing (`terminest ls` / `terminest machine list`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachineSummaryView {
    pub id: MachineId,
    pub name: MachineName,
    pub state: MachineState,
    pub backend: HypervisorType,
    pub cpus: u32,
    pub memory_mb: u64,
    pub storage_gb: u64,
}
