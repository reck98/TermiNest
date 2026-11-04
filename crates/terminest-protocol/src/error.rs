use thiserror::Error;

/// Protocol-level error encountered during serialization, framing, or decoding.
#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum ProtocolError {
    #[error("Invalid magic number: expected 0x544E5354 ('TNST'), got 0x{0:08X}")]
    InvalidMagic(u32),

    #[error("Unsupported protocol version: expected 0x01, got 0x{0:02X}")]
    UnsupportedVersion(u8),

    #[error("Unknown channel ID: 0x{0:04X}")]
    UnknownChannel(u16),

    #[error("Invalid frame flags or reserved bits set: 0x{0:02X}")]
    InvalidFlags(u8),

    #[error("Stream decoder is poisoned due to a previous unrecoverable framing error")]
    StreamPoisoned,

    #[error("Payload length {actual} exceeds maximum allowed limit of {max} bytes")]
    PayloadTooLarge { actual: u32, max: u32 },

    #[error("Checksum mismatch: expected 0x{expected:08X}, calculated 0x{calculated:08X}")]
    ChecksumMismatch { expected: u32, calculated: u32 },

    #[error("Stream buffer capacity exceeded ({0} bytes)")]
    BufferOverflow(usize),

    #[error("Heartbeat frame must have payload length 0, got {0}")]
    InvalidHeartbeat(u32),

    #[error("Incomplete frame: expected at least {expected} bytes, got {actual}")]
    IncompleteFrame { expected: usize, actual: usize },

    #[error("JSON-RPC serialization/deserialization error: {0}")]
    JsonRpc(String),

    #[error("IO error: {0}")]
    Io(String),
}
