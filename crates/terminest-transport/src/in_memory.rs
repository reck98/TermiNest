use crate::error::TransportError;
use crate::traits::Transport;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use terminest_protocol::Frame;
use tokio::sync::mpsc;

/// In-memory bidirectional transport channel for deterministic testing.
pub struct InMemoryTransport {
    sender: Option<mpsc::Sender<Frame>>,
    receiver: mpsc::Receiver<Frame>,
    closed: Arc<AtomicBool>,
}

impl InMemoryTransport {
    /// Creates a connected pair of in-memory transport endpoints (e.g., host and guest).
    pub fn pair(buffer_capacity: usize) -> (Self, Self) {
        let (tx_a, rx_b) = mpsc::channel(buffer_capacity);
        let (tx_b, rx_a) = mpsc::channel(buffer_capacity);

        let closed_a = Arc::new(AtomicBool::new(false));
        let closed_b = Arc::new(AtomicBool::new(false));

        let endpoint_a = Self {
            sender: Some(tx_a),
            receiver: rx_a,
            closed: closed_a,
        };

        let endpoint_b = Self {
            sender: Some(tx_b),
            receiver: rx_b,
            closed: closed_b,
        };

        (endpoint_a, endpoint_b)
    }
}

impl Transport for InMemoryTransport {
    async fn send(&mut self, frame: Frame) -> Result<(), TransportError> {
        if self.is_closed() {
            return Err(TransportError::Closed);
        }

        match &self.sender {
            Some(sender) => sender.send(frame).await.map_err(|_| TransportError::Closed),
            None => Err(TransportError::Closed),
        }
    }

    async fn recv(&mut self) -> Result<Option<Frame>, TransportError> {
        if self.is_closed() {
            return Ok(None);
        }

        match self.receiver.recv().await {
            Some(frame) => Ok(Some(frame)),
            None => {
                self.closed.store(true, Ordering::SeqCst);
                Ok(None)
            }
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.closed.store(true, Ordering::SeqCst);
        // Dropping the sender notifies the peer receiver with EOF (None)
        self.sender.take();
        self.receiver.close();
        Ok(())
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[tokio::test]
    async fn test_in_memory_bidirectional_exchange() {
        let (mut host, mut guest) = InMemoryTransport::pair(16);

        let frame_to_guest = Frame::control(Bytes::from_static(b"ping")).unwrap();
        host.send(frame_to_guest.clone()).await.unwrap();

        let received_by_guest = guest.recv().await.unwrap().expect("Guest expected frame");
        assert_eq!(received_by_guest, frame_to_guest);

        let reply_to_host = Frame::control(Bytes::from_static(b"pong")).unwrap();
        guest.send(reply_to_host.clone()).await.unwrap();

        let received_by_host = host.recv().await.unwrap().expect("Host expected frame");
        assert_eq!(received_by_host, reply_to_host);
    }

    #[tokio::test]
    async fn test_in_memory_clean_close() {
        let (mut host, mut guest) = InMemoryTransport::pair(16);

        host.close().await.unwrap();
        assert!(host.is_closed());

        // Guest receiving after host close should see EOF (None)
        let received = guest.recv().await.unwrap();
        assert_eq!(received, None);
        assert!(guest.is_closed());
    }

    #[tokio::test]
    async fn test_send_after_close_returns_error() {
        let (mut host, _guest) = InMemoryTransport::pair(16);
        host.close().await.unwrap();

        let frame = Frame::heartbeat();
        let result = host.send(frame).await;
        assert!(matches!(result, Err(TransportError::Closed)));
    }
}
