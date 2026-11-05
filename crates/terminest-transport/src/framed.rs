use crate::error::TransportError;
use crate::traits::Transport;
use terminest_protocol::{Frame, StreamDecoder};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// RAII guard that ensures `FramedTransport` is fail-closed.
///
/// If a send operation is interrupted or cancelled at any yield point (e.g.,
/// due to `tokio::time::timeout`, task abort, or cancellation in `tokio::select!`),
/// the guard drops without being committed, permanently marking the transport
/// as closed. This prevents subsequent writes from interleaving with an uncompleted,
/// partially transmitted frame and corrupting the stream framing.
struct SendCancellationGuard<'a> {
    is_closed: &'a mut bool,
    committed: bool,
}

impl<'a> SendCancellationGuard<'a> {
    #[inline]
    fn new(is_closed: &'a mut bool) -> Self {
        *is_closed = true;
        Self {
            is_closed,
            committed: false,
        }
    }

    #[inline]
    fn commit(mut self) {
        self.committed = true;
        *self.is_closed = false;
    }
}

impl<'a> Drop for SendCancellationGuard<'a> {
    fn drop(&mut self) {
        if !self.committed {
            *self.is_closed = true;
        }
    }
}

/// Framed transport layer wrapping any asynchronous byte stream.
///
/// Encodes frames using TNST-v1 binary format and decodes incoming chunks
/// using the incremental `StreamDecoder`.
pub struct FramedTransport<IO> {
    io: IO,
    decoder: StreamDecoder,
    read_buf: [u8; 8192],
    is_closed: bool,
}

impl<IO> FramedTransport<IO>
where
    IO: AsyncRead + AsyncWrite + Unpin + Send + Sync,
{
    /// Creates a new framed transport wrapping the given asynchronous I/O stream.
    pub fn new(io: IO) -> Self {
        Self {
            io,
            decoder: StreamDecoder::new(),
            read_buf: [0u8; 8192],
            is_closed: false,
        }
    }

    /// Creates a new framed transport with a custom maximum buffer capacity.
    pub fn with_max_capacity(io: IO, max_capacity: usize) -> Self {
        Self {
            io,
            decoder: StreamDecoder::with_max_capacity(max_capacity),
            read_buf: [0u8; 8192],
            is_closed: false,
        }
    }
}

impl<IO> Transport for FramedTransport<IO>
where
    IO: AsyncRead + AsyncWrite + Unpin + Send + Sync,
{
    async fn send(&mut self, frame: Frame) -> Result<(), TransportError> {
        if self.is_closed {
            return Err(TransportError::Closed);
        }

        // Arm fail-closed cancellation guard before starting any async I/O.
        // If write_all or flush is cancelled at an await point, is_closed remains true.
        let guard = SendCancellationGuard::new(&mut self.is_closed);

        let encoded = frame.encode();
        self.io.write_all(&encoded).await?;
        self.io.flush().await?;

        // Both write_all and flush succeeded completely: disarm guard and restore open state.
        guard.commit();
        Ok(())
    }

    async fn recv(&mut self) -> Result<Option<Frame>, TransportError> {
        if self.is_closed {
            return Ok(None);
        }

        loop {
            // First check if a frame is already complete in the decoder buffer
            match self.decoder.poll_next() {
                Ok(Some(frame)) => return Ok(Some(frame)),
                Ok(None) => {}
                Err(e) => {
                    self.is_closed = true;
                    return Err(TransportError::Protocol(e));
                }
            }

            // Read additional bytes from the underlying transport stream
            let bytes_read = match self.io.read(&mut self.read_buf).await {
                Ok(n) => n,
                Err(e) => {
                    self.is_closed = true;
                    return Err(TransportError::Io(e));
                }
            };
            if bytes_read == 0 {
                // EOF reached
                self.is_closed = true;
                if self.decoder.buffered_len() > 0 {
                    let expected = self.decoder.expected_frame_len().unwrap_or(16);
                    return Err(TransportError::Protocol(
                        terminest_protocol::ProtocolError::IncompleteFrame {
                            expected,
                            actual: self.decoder.buffered_len(),
                        },
                    ));
                }
                return Ok(None);
            }

            if let Err(e) = self.decoder.ingest(&self.read_buf[..bytes_read]) {
                self.is_closed = true;
                return Err(TransportError::Protocol(e));
            }
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.is_closed = true;
        self.io.shutdown().await?;
        Ok(())
    }

    fn is_closed(&self) -> bool {
        self.is_closed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::io::duplex;
    use tokio::sync::Mutex;

    #[tokio::test]
    async fn test_framed_duplex_stream_roundtrip() {
        let (client_io, server_io) = duplex(1024);
        let mut client = FramedTransport::new(client_io);
        let mut server = FramedTransport::new(server_io);

        let test_frame = Frame::control(Bytes::from_static(b"{\"hello\":\"world\"}")).unwrap();
        client.send(test_frame.clone()).await.unwrap();

        let received = server
            .recv()
            .await
            .unwrap()
            .expect("Server did not receive frame");
        assert_eq!(received.header, test_frame.header);
        assert_eq!(received.payload, test_frame.payload);
    }

    #[tokio::test]
    async fn test_framed_fragmented_stream() {
        let (mut client_io, server_io) = duplex(1024);
        let mut server = FramedTransport::new(server_io);

        let test_frame = Frame::control(Bytes::from_static(b"stream fragmentation test")).unwrap();
        let encoded = test_frame.encode();

        // Write to client_io in tiny 3-byte chunks
        tokio::spawn(async move {
            for chunk in encoded.chunks(3) {
                client_io.write_all(chunk).await.unwrap();
                client_io.flush().await.unwrap();
                tokio::task::yield_now().await;
            }
        });

        let received = server
            .recv()
            .await
            .unwrap()
            .expect("Failed to reconstruct fragmented frame");
        assert_eq!(received.payload, test_frame.payload);
    }

    #[tokio::test]
    async fn test_framed_send_cancellation_marks_transport_closed() {
        // Create a duplex stream with a tiny 32-byte internal buffer.
        let (client_io, _server_io) = duplex(32);
        let mut client = FramedTransport::new(client_io);

        // Frame of 500 bytes will not fit in 32 bytes and will block in write_all
        let large_frame = Frame::control(Bytes::from(vec![0xAA; 500])).unwrap();

        // Attempt send with a 10ms timeout so the future is dropped during partial write
        let res = tokio::time::timeout(Duration::from_millis(10), client.send(large_frame)).await;
        assert!(res.is_err(), "Send should have timed out");

        // The cancellation guard must have tripped and marked the transport closed
        assert!(
            client.is_closed(),
            "Transport must be fail-closed after cancelled send"
        );

        // Any subsequent send attempt MUST be rejected immediately as Closed,
        // preventing partial-frame interleaving and stream desynchronization.
        let next_frame = Frame::control(Bytes::from_static(b"subsequent")).unwrap();
        let next_res = client.send(next_frame).await;
        match next_res {
            Err(TransportError::Closed) => {}
            other => panic!("Expected Err(TransportError::Closed), got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_framed_write_error_marks_transport_closed() {
        let (client_io, server_io) = duplex(64);
        let mut client = FramedTransport::new(client_io);

        // Drop the server end so writing results in a broken pipe error
        drop(server_io);

        let test_frame = Frame::control(Bytes::from(vec![0xBB; 200])).unwrap();
        let res = client.send(test_frame).await;
        assert!(res.is_err(), "Send should fail on dropped receiver");

        // Transport must be marked closed
        assert!(
            client.is_closed(),
            "Transport must be marked closed after write error"
        );

        // Subsequent send fails closed
        let subsequent = Frame::control(Bytes::from_static(b"retry")).unwrap();
        match client.send(subsequent).await {
            Err(TransportError::Closed) => {}
            other => panic!("Expected Err(TransportError::Closed), got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_framed_task_abort_marks_transport_closed() {
        let (client_io, _server_io) = duplex(32);
        let client = Arc::new(Mutex::new(FramedTransport::new(client_io)));

        let client_clone = Arc::clone(&client);
        let handle = tokio::spawn(async move {
            let large_frame = Frame::control(Bytes::from(vec![0xCC; 500])).unwrap();
            let mut guard = client_clone.lock().await;
            guard.send(large_frame).await
        });

        // Yield to allow the background task to start writing and block on the 32-byte buffer
        tokio::time::sleep(Duration::from_millis(15)).await;

        // Abort the task while write_all is pending
        handle.abort();
        let _ = handle.await;

        // Verify the transport inside the Arc/Mutex is fail-closed
        let locked = client.lock().await;
        assert!(
            locked.is_closed(),
            "Transport must be marked closed when task was aborted during write"
        );
    }

    #[tokio::test]
    async fn test_framed_recv_incomplete_frame_marks_closed() {
        let (mut client_io, server_io) = duplex(64);
        let mut server = FramedTransport::new(server_io);

        // Write partial header (8 bytes instead of 16) and drop client_io (EOF)
        client_io.write_all(b"TNST\x01\x00\x00\x00").await.unwrap();
        client_io.flush().await.unwrap();
        drop(client_io);

        let res = server.recv().await;
        assert!(matches!(
            res,
            Err(TransportError::Protocol(
                terminest_protocol::ProtocolError::IncompleteFrame { .. }
            ))
        ));

        // Transport must be marked closed
        assert!(server.is_closed());

        // Subsequent recv returns Ok(None) because transport is closed
        assert_eq!(server.recv().await.unwrap(), None);
    }
}
