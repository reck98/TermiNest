use crate::error::TransportError;
use crate::traits::Transport;
use terminest_protocol::{Frame, StreamDecoder};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

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

        let encoded = frame.encode();
        self.io.write_all(&encoded).await?;
        self.io.flush().await?;
        Ok(())
    }

    async fn recv(&mut self) -> Result<Option<Frame>, TransportError> {
        if self.is_closed {
            return Ok(None);
        }

        loop {
            // First check if a frame is already complete in the decoder buffer
            if let Some(frame) = self.decoder.poll_next()? {
                return Ok(Some(frame));
            }

            // Read additional bytes from the underlying transport stream
            let bytes_read = self.io.read(&mut self.read_buf).await?;
            if bytes_read == 0 {
                // EOF reached
                self.is_closed = true;
                if self.decoder.buffered_len() > 0 {
                    return Err(TransportError::Protocol(
                        terminest_protocol::ProtocolError::IncompleteFrame {
                            expected: 16,
                            actual: self.decoder.buffered_len(),
                        },
                    ));
                }
                return Ok(None);
            }

            self.decoder.ingest(&self.read_buf[..bytes_read])?;
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
    use tokio::io::duplex;

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
}
