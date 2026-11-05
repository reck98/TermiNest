use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

use crate::domain::ConfigValidationError;

/// Strongly typed, immutable identifier for a TermiNest virtual machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MachineId(Uuid);

impl MachineId {
    /// Generates a new random UUIDv4 MachineId.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Creates a MachineId from an existing Uuid.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Access the underlying UUID.
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }

    /// Parses a string into a MachineId, returning a typed validation error on failure.
    pub fn parse(s: &str) -> Result<Self, ConfigValidationError> {
        Uuid::parse_str(s)
            .map(Self)
            .map_err(|_| ConfigValidationError::InvalidMachineId(s.to_string()))
    }
}

impl Default for MachineId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for MachineId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for MachineId {
    type Err = ConfigValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl From<Uuid> for MachineId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}
