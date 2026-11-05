use std::path::PathBuf;
use thiserror::Error;

/// Domain errors produced by the QEMU process management subsystem.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum QemuError {
    #[error("QEMU binary not found: {0}")]
    BinaryNotFound(PathBuf),

    #[error("Failed to parse QEMU version output: {0}")]
    VersionParseError(String),

    #[error("Required QEMU accelerator unavailable: {0}")]
    AcceleratorUnavailable(String),

    #[error("QEMU version policy violation: {0}")]
    VersionPolicyViolation(String),

    #[error("Invalid QEMU process configuration: {0}")]
    ConfigValidation(String),

    #[error("Failed to assign QEMU process to Windows Job Object")]
    JobObjectAssignmentFailed,

    #[error(
        "QEMU process unexpectedly exited ({status}): {stderr_tail}",
        status = match exit_status {
            Some(code) => format!("code {code}"),
            None => "signal".to_string(),
        }
    )]
    ProcessUnexpectedExit {
        exit_status: Option<i32>,
        stderr_tail: String,
    },

    #[error("QEMU process management system error: {0}")]
    System(String),

    #[error("Operation timed out after {0:?}")]
    Timeout(std::time::Duration),
}

impl From<std::io::Error> for QemuError {
    fn from(err: std::io::Error) -> Self {
        Self::System(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qemu_error_display() {
        let err = QemuError::BinaryNotFound(PathBuf::from("/usr/bin/qemu-system-x86_64"));
        assert!(err.to_string().contains("QEMU binary not found"));

        let err = QemuError::ConfigValidation("bad cpu".into());
        assert_eq!(
            err.to_string(),
            "Invalid QEMU process configuration: bad cpu"
        );

        let err = QemuError::JobObjectAssignmentFailed;
        assert_eq!(
            err.to_string(),
            "Failed to assign QEMU process to Windows Job Object"
        );

        let err = QemuError::ProcessUnexpectedExit {
            exit_status: Some(1),
            stderr_tail: "qemu-system-x86_64: fatal initialization error".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "QEMU process unexpectedly exited (code 1): qemu-system-x86_64: fatal initialization error"
        );

        let err = QemuError::ProcessUnexpectedExit {
            exit_status: None,
            stderr_tail: "terminated".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "QEMU process unexpectedly exited (signal): terminated"
        );
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let qemu_err: QemuError = io_err.into();
        assert_eq!(qemu_err, QemuError::System("file not found".to_string()));
    }
}
