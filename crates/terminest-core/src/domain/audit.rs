use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::{MachineId, MachineName, MachineState};

/// High-level lifecycle action recorded in the security and operational audit trail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditAction {
    Create,
    Start,
    Stop,
    Pause,
    Resume,
    Delete,
    Reconcile,
}

impl std::fmt::Display for AuditAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Create => write!(f, "create"),
            Self::Start => write!(f, "start"),
            Self::Stop => write!(f, "stop"),
            Self::Pause => write!(f, "pause"),
            Self::Resume => write!(f, "resume"),
            Self::Delete => write!(f, "delete"),
            Self::Reconcile => write!(f, "reconcile"),
        }
    }
}

/// Operational phase of the audited event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditPhase {
    Requested,
    Completed,
    Failed,
}

impl std::fmt::Display for AuditPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Requested => write!(f, "requested"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

/// Outcome status of the audited operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditResult {
    Success,
    Failure,
}

impl std::fmt::Display for AuditResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Success => write!(f, "success"),
            Self::Failure => write!(f, "failure"),
        }
    }
}

/// A structured, versioned audit log event adhering to STRIDE-R1/R2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditRecord {
    pub schema_version: String,
    pub event_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub machine_id: MachineId,
    pub machine_name: MachineName,
    pub action: AuditAction,
    pub phase: AuditPhase,
    pub actor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_before: Option<MachineState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_after: Option<MachineState>,
    pub result: AuditResult,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl AuditRecord {
    /// Creates a new operation intent audit event.
    pub fn requested(
        machine_id: MachineId,
        machine_name: MachineName,
        action: AuditAction,
        state_before: Option<MachineState>,
        state_after: Option<MachineState>,
        actor: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: "1.0.0".to_string(),
            event_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            machine_id,
            machine_name,
            action,
            phase: AuditPhase::Requested,
            actor: actor.into(),
            state_before,
            state_after,
            result: AuditResult::Success,
            error_message: None,
            details: None,
        }
    }

    /// Creates a new successful completion audit event.
    pub fn completed(
        machine_id: MachineId,
        machine_name: MachineName,
        action: AuditAction,
        state_before: Option<MachineState>,
        state_after: Option<MachineState>,
        actor: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: "1.0.0".to_string(),
            event_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            machine_id,
            machine_name,
            action,
            phase: AuditPhase::Completed,
            actor: actor.into(),
            state_before,
            state_after,
            result: AuditResult::Success,
            error_message: None,
            details: None,
        }
    }

    /// Creates a new failed operation audit event.
    pub fn failed(
        machine_id: MachineId,
        machine_name: MachineName,
        action: AuditAction,
        state_before: Option<MachineState>,
        state_after: Option<MachineState>,
        actor: impl Into<String>,
        error_message: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: "1.0.0".to_string(),
            event_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            machine_id,
            machine_name,
            action,
            phase: AuditPhase::Failed,
            actor: actor.into(),
            state_before,
            state_after,
            result: AuditResult::Failure,
            error_message: Some(error_message.into()),
            details: None,
        }
    }
}
