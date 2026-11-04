use crate::crc::{crc32c, crc32c_update};
use crate::error::ProtocolError;
use crate::frame::{Frame, FrameHeader, HEADER_PREFIX_SIZE, HEADER_SIZE};
use bytes::{Buf, BytesMut};

/// Default maximum buffer size for the stream decoder (32 MB).
pub const DEFAULT_MAX_BUFFER_SIZE: usize = 32 * 1024 * 1024;

/// Incremental stream decoder for the TNST-v1 wire protocol.
///
/// Designed to parse multiplexed binary frames over stream-oriented transports
/// (Vsock, TCP, Named Pipes, duplex streams) where reads occur at arbitrary
/// byte boundaries.
#[derive(Debug)]
pub struct StreamDecoder {
    buffer: BytesMut,
    max_capacity: usize,
    is_poisoned: bool,
}

impl Default for StreamDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamDecoder {
    /// Creates a new decoder with default capacity bounds (32 MB).
    pub fn new() -> Self {
        Self::with_max_capacity(DEFAULT_MAX_BUFFER_SIZE)
    }

    /// Creates a new decoder with an explicit maximum buffer capacity limit.
    pub fn with_max_capacity(max_capacity: usize) -> Self {
        Self {
            buffer: BytesMut::with_capacity(4096),
            max_capacity,
            is_poisoned: false,
        }
    }

    /// Feeds incoming raw bytes into the internal decoder buffer.
    ///
    /// Rejects input if it causes the buffer to exceed `max_capacity` to prevent memory exhaustion attacks.
    pub fn ingest(&mut self, chunk: &[u8]) -> Result<(), ProtocolError> {
        if self.is_poisoned {
            return Err(ProtocolError::StreamPoisoned);
        }

        if self.buffer.len().saturating_add(chunk.len()) > self.max_capacity {
            self.is_poisoned = true;
            return Err(ProtocolError::BufferOverflow(self.max_capacity));
        }
        self.buffer.extend_from_slice(chunk);
        Ok(())
    }

    /// Returns the expected total byte length of the current incoming frame, if a valid
    /// header is present in the buffer, or `Some(HEADER_SIZE)` if awaiting the header.
    pub fn expected_frame_len(&self) -> Option<usize> {
        if self.buffer.len() < HEADER_SIZE {
            Some(HEADER_SIZE)
        } else {
            let header_bytes = &self.buffer[..HEADER_SIZE];
            if let Ok(header) = FrameHeader::decode(header_bytes) {
                Some(HEADER_SIZE + header.payload_len as usize)
            } else {
                None
            }
        }
    }

    /// Attempts to extract the next complete and verified `Frame` from the internal buffer.
    ///
    /// Returns:
    /// - `Ok(Some(Frame))` when a valid frame with verified CRC-32C is ready.
    /// - `Ok(None)` when more data is needed (partial header or partial payload).
    /// - `Err(ProtocolError)` when an invalid header or corrupted payload is detected.
    pub fn poll_next(&mut self) -> Result<Option<Frame>, ProtocolError> {
        if self.is_poisoned {
            return Err(ProtocolError::StreamPoisoned);
        }

        if self.buffer.len() < HEADER_SIZE {
            // Need at least 16 bytes to decode the header
            return Ok(None);
        }

        // Peek header without consuming buffer
        let header_bytes = &self.buffer[..HEADER_SIZE];
        let header = match FrameHeader::decode(header_bytes) {
            Ok(h) => h,
            Err(err) => {
                // Fatal framing error: poison the stream decoder
                self.is_poisoned = true;
                return Err(err);
            }
        };

        let payload_len = header.payload_len as usize;
        let total_frame_len = HEADER_SIZE + payload_len;

        // Verify that the requested frame fits within the configured maximum decoder capacity
        if total_frame_len > self.max_capacity {
            self.is_poisoned = true;
            return Err(ProtocolError::PayloadTooLarge {
                actual: header.payload_len,
                max: (self.max_capacity.saturating_sub(HEADER_SIZE)) as u32,
            });
        }

        if self.buffer.len() < total_frame_len {
            // Header is valid, but full payload has not arrived yet
            return Ok(None);
        }

        // Complete frame received: verify ADR-0010 checksum (header prefix + payload)
        let prefix_crc = crc32c(&self.buffer[..HEADER_PREFIX_SIZE]);
        let payload_slice = &self.buffer[HEADER_SIZE..total_frame_len];
        let calculated_crc = crc32c_update(prefix_crc, payload_slice);

        if calculated_crc != header.checksum {
            // Discard the corrupted frame to avoid buffer stall
            self.buffer.advance(total_frame_len);
            return Err(ProtocolError::ChecksumMismatch {
                expected: header.checksum,
                calculated: calculated_crc,
            });
        }

        // Consume header
        self.buffer.advance(HEADER_SIZE);
        // Extract payload
        let payload = self.buffer.split_to(payload_len).freeze();

        Ok(Some(Frame { header, payload }))
    }

    /// Returns whether the stream decoder is in a poisoned state.
    pub fn is_poisoned(&self) -> bool {
        self.is_poisoned
    }

    /// Returns the number of unparsed bytes currently in the buffer.
    pub fn buffered_len(&self) -> usize {
        self.buffer.len()
    }

    /// Clears the internal buffer and resets the poisoned state.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.is_poisoned = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[test]
    fn test_decoder_single_complete_frame() {
        let mut decoder = StreamDecoder::new();
        let payload = Bytes::from_static(b"{\"test\":true}");
        let frame = Frame::control(payload.clone()).unwrap();
        let encoded = frame.encode();

        decoder.ingest(&encoded).unwrap();
        let decoded = decoder.poll_next().unwrap().expect("Expected frame");

        assert_eq!(decoded.header, frame.header);
        assert_eq!(decoded.payload, payload);
        assert_eq!(decoder.poll_next().unwrap(), None);
    }

    #[test]
    fn test_decoder_multiple_frames_in_one_read() {
        let mut decoder = StreamDecoder::new();
        let frame1 = Frame::heartbeat();
        let frame2 = Frame::control(Bytes::from_static(b"req1")).unwrap();
        let frame3 = Frame::control(Bytes::from_static(b"req2")).unwrap();

        let mut batch = BytesMut::new();
        batch.extend_from_slice(&frame1.encode());
        batch.extend_from_slice(&frame2.encode());
        batch.extend_from_slice(&frame3.encode());

        decoder.ingest(&batch).unwrap();

        let d1 = decoder.poll_next().unwrap().expect("Expected frame 1");
        assert_eq!(d1.header, frame1.header);

        let d2 = decoder.poll_next().unwrap().expect("Expected frame 2");
        assert_eq!(d2.payload, frame2.payload);

        let d3 = decoder.poll_next().unwrap().expect("Expected frame 3");
        assert_eq!(d3.payload, frame3.payload);

        assert_eq!(decoder.poll_next().unwrap(), None);
    }

    #[test]
    fn test_decoder_arbitrary_fragmentation() {
        let mut decoder = StreamDecoder::new();
        let payload = Bytes::from_static(b"abcdefghijklmnopqrstuvwxyz0123456789");
        let frame = Frame::control(payload.clone()).unwrap();
        let encoded = frame.encode();

        // Feed 1 byte at a time
        let mut extracted = None;
        for &byte in encoded.as_ref() {
            decoder.ingest(&[byte]).unwrap();
            if let Some(f) = decoder.poll_next().unwrap() {
                assert!(extracted.is_none(), "Should only extract one frame");
                extracted = Some(f);
            }
        }

        let extracted = extracted.expect("Frame was not reconstructed");
        assert_eq!(extracted.payload, payload);
        assert_eq!(decoder.buffered_len(), 0);
    }

    #[test]
    fn test_decoder_buffer_overflow_protection() {
        let mut decoder = StreamDecoder::with_max_capacity(100);
        let large_chunk = vec![0u8; 101];

        let err = decoder.ingest(&large_chunk).unwrap_err();
        assert!(matches!(err, ProtocolError::BufferOverflow(100)));
        assert!(decoder.is_poisoned());
    }

    #[test]
    fn test_decoder_poisoning_on_invalid_header() {
        let mut decoder = StreamDecoder::new();
        let bad_header = [0x00u8; 16]; // bad magic
        decoder.ingest(&bad_header).unwrap();

        let err = decoder.poll_next().unwrap_err();
        assert!(matches!(err, ProtocolError::InvalidMagic(_)));
        assert!(decoder.is_poisoned());

        // Subsequent poll must return StreamPoisoned
        let err2 = decoder.poll_next().unwrap_err();
        assert_eq!(err2, ProtocolError::StreamPoisoned);
    }

    #[test]
    fn test_decoder_corrupt_payload_crc() {
        let mut decoder = StreamDecoder::new();
        let frame = Frame::control(Bytes::from_static(b"critical command")).unwrap();
        let mut encoded = frame.encode().to_vec();

        // Corrupt payload byte
        let last = encoded.len() - 1;
        encoded[last] ^= 0xAA;

        decoder.ingest(&encoded).unwrap();
        let err = decoder.poll_next().unwrap_err();
        assert!(matches!(err, ProtocolError::ChecksumMismatch { .. }));
    }
}
