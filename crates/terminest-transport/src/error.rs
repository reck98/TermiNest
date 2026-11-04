use terminest_protocol::ProtocolError;
use thiserror::Error;

/// Transport-level errors.
#[derive(Debug, Error)]
pub enum TransportError {
    #[error("Transport connection is closed")]
    Closed,

    #[error("Operation timed out")]
    Timeout,

    #[error("Protocol error: {0}")]
    Protocol(#[from] ProtocolError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid transport state: {0}")]
    InvalidState(String),
}
