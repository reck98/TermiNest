use crate::error::TransportError;
use terminest_protocol::Frame;

/// Asynchronous bidirectional frame transport abstraction.
///
/// Hides the underlying physical or virtual medium (In-Memory Duplex, Vsock,
/// Named Pipe, or TCP stream) behind a clean, testable interface.
#[allow(async_fn_in_trait)]
pub trait Transport: Send + Sync {
    /// Sends a validated protocol frame across the transport.
    async fn send(&mut self, frame: Frame) -> Result<(), TransportError>;

    /// Receives the next available protocol frame from the transport.
    ///
    /// Returns:
    /// - `Ok(Some(frame))` if a valid frame was received.
    /// - `Ok(None)` if the remote endpoint closed cleanly (EOF).
    /// - `Err(TransportError)` on network error, framing corruption, or timeout.
    async fn recv(&mut self) -> Result<Option<Frame>, TransportError>;

    /// Gracefully closes the transport connection.
    async fn close(&mut self) -> Result<(), TransportError>;

    /// Returns whether the transport connection has been closed.
    fn is_closed(&self) -> bool;
}
