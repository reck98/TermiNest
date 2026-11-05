use std::fmt;
use std::str::FromStr;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::domain::ConfigValidationError;

/// Human-readable, validated machine name.
/// Constraints: 1..=64 characters; characters must be alphanumeric, hyphen, or underscore.
/// Explicitly forbids spaces, slashes, backslashes, path traversal sequences, and control characters.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MachineName(String);

impl MachineName {
    /// Validates and constructs a new MachineName.
    pub fn new(name: impl Into<String>) -> Result<Self, ConfigValidationError> {
        let s = name.into();
        Self::validate(&s)?;
        Ok(Self(s))
    }

    /// Returns a string slice of the machine name.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn validate(s: &str) -> Result<(), ConfigValidationError> {
        if s.is_empty() {
            return Err(ConfigValidationError::EmptyName);
        }
        if s.len() > 64 {
            return Err(ConfigValidationError::NameTooLong(s.len()));
        }
        if s.contains('/') || s.contains('\\') || s.contains("..") || s.contains('\0') {
            return Err(ConfigValidationError::PathTraversal(s.to_string()));
        }
        for ch in s.chars() {
            if !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_' {
                return Err(ConfigValidationError::InvalidNameCharacter(ch));
            }
        }
        Ok(())
    }
}

impl AsRef<str> for MachineName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MachineName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for MachineName {
    type Err = ConfigValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl Serialize for MachineName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for MachineName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::new(s).map_err(serde::de::Error::custom)
    }
}
