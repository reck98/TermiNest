use crate::channel::ChannelId;
use crate::crc::{crc32c, crc32c_update};
use crate::error::ProtocolError;
use bytes::{Buf, Bytes, BytesMut};

/// Protocol magic identifier: ASCII "TNST" (0x544E5354).
pub const MAGIC: u32 = 0x544E5354;

/// Protocol wire version.
pub const PROTOCOL_VERSION: u8 = 0x01;

/// Fixed wire header size in bytes.
pub const HEADER_SIZE: usize = 16;

/// Number of header bytes preceding the checksum field (Magic + Version + Flags + ChannelId + PayloadLength).
pub const HEADER_PREFIX_SIZE: usize = 12;

/// Maximum payload size allowed by the protocol (16 MB).
pub const MAX_PAYLOAD_SIZE: u32 = 16 * 1024 * 1024; // 16,777,216 bytes

/// Bitfield flags for payload encoding and special treatment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrameFlags(pub u8);

impl FrameFlags {
    pub const PLAIN: Self = Self(0x00);
    pub const LZ4: Self = Self(0x01);
    pub const JPEG: Self = Self(0x02);
    pub const HEARTBEAT: Self = Self(0x04);

    /// Mask of all valid/allocated flag bits. Bits 3..7 are reserved.
    pub const VALID_MASK: u8 = Self::LZ4.0 | Self::JPEG.0 | Self::HEARTBEAT.0;

    /// Validates raw flag bits according to TNST-v1 specification:
    /// - Reserved bits (3..7) must be 0.
    /// - Compression flags (LZ4 and JPEG) are mutually exclusive.
    /// - Heartbeat frames cannot specify compression.
    pub fn from_raw(raw: u8) -> Result<Self, ProtocolError> {
        if (raw & !Self::VALID_MASK) != 0 {
            return Err(ProtocolError::InvalidFlags(raw));
        }

        // Mutual exclusivity of compression formats
        let is_lz4 = (raw & Self::LZ4.0) != 0;
        let is_jpeg = (raw & Self::JPEG.0) != 0;
        if is_lz4 && is_jpeg {
            return Err(ProtocolError::InvalidFlags(raw));
        }

        // Heartbeat frames cannot specify payload compression
        let is_heartbeat = (raw & Self::HEARTBEAT.0) != 0;
        if is_heartbeat && (is_lz4 || is_jpeg) {
            return Err(ProtocolError::InvalidFlags(raw));
        }

        Ok(Self(raw))
    }

    #[inline]
    pub const fn is_plain(self) -> bool {
        self.0 == 0
    }

    #[inline]
    pub const fn is_lz4(self) -> bool {
        (self.0 & Self::LZ4.0) != 0
    }

    #[inline]
    pub const fn is_jpeg(self) -> bool {
        (self.0 & Self::JPEG.0) != 0
    }

    #[inline]
    pub const fn is_heartbeat(self) -> bool {
        (self.0 & Self::HEARTBEAT.0) != 0
    }
}

/// The immutable 16-byte fixed header preceding every TNST-v1 frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameHeader {
    pub magic: u32,
    pub version: u8,
    pub flags: FrameFlags,
    pub channel_id: ChannelId,
    pub payload_len: u32,
    pub checksum: u32,
}

impl FrameHeader {
    /// Encodes the 12-byte header prefix (excluding the checksum field) in Big-Endian format.
    pub fn prefix_bytes(&self) -> [u8; HEADER_PREFIX_SIZE] {
        let mut buf = [0u8; HEADER_PREFIX_SIZE];
        buf[0..4].copy_from_slice(&self.magic.to_be_bytes());
        buf[4] = self.version;
        buf[5] = self.flags.0;
        buf[6..8].copy_from_slice(&self.channel_id.to_raw().to_be_bytes());
        buf[8..12].copy_from_slice(&self.payload_len.to_be_bytes());
        buf
    }

    /// Encodes the full 16-byte header into a fixed-size byte array in Big-Endian format.
    pub fn to_bytes(&self) -> [u8; HEADER_SIZE] {
        let mut buf = [0u8; HEADER_SIZE];
        buf[0..HEADER_PREFIX_SIZE].copy_from_slice(&self.prefix_bytes());
        buf[12..16].copy_from_slice(&self.checksum.to_be_bytes());
        buf
    }

    /// Decodes and strictly validates a 16-byte header from a byte slice.
    pub fn decode(mut src: &[u8]) -> Result<Self, ProtocolError> {
        if src.len() < HEADER_SIZE {
            return Err(ProtocolError::IncompleteFrame {
                expected: HEADER_SIZE,
                actual: src.len(),
            });
        }

        let magic = src.get_u32();
        if magic != MAGIC {
            return Err(ProtocolError::InvalidMagic(magic));
        }

        let version = src.get_u8();
        if version != PROTOCOL_VERSION {
            return Err(ProtocolError::UnsupportedVersion(version));
        }

        let flags = FrameFlags::from_raw(src.get_u8())?;
        let raw_channel = src.get_u16();
        let channel_id = ChannelId::from_raw(raw_channel)?;

        let payload_len = src.get_u32();
        if payload_len > MAX_PAYLOAD_SIZE {
            return Err(ProtocolError::PayloadTooLarge {
                actual: payload_len,
                max: MAX_PAYLOAD_SIZE,
            });
        }

        if flags.is_heartbeat() && payload_len != 0 {
            return Err(ProtocolError::InvalidHeartbeat(payload_len));
        }

        let checksum = src.get_u32();

        Ok(Self {
            magic,
            version,
            flags,
            channel_id,
            payload_len,
            checksum,
        })
    }
}

/// A complete protocol frame consisting of validated header and payload bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub header: FrameHeader,
    pub payload: Bytes,
}

impl Frame {
    /// Constructs a new frame, automatically calculating the Castagnoli CRC-32C checksum
    /// over the 12-byte header prefix and payload bytes (per ADR-0010).
    pub fn new(
        channel_id: ChannelId,
        flags: FrameFlags,
        payload: Bytes,
    ) -> Result<Self, ProtocolError> {
        let payload_len = payload.len() as u32;
        if payload_len > MAX_PAYLOAD_SIZE {
            return Err(ProtocolError::PayloadTooLarge {
                actual: payload_len,
                max: MAX_PAYLOAD_SIZE,
            });
        }

        if flags.is_heartbeat() && payload_len != 0 {
            return Err(ProtocolError::InvalidHeartbeat(payload_len));
        }

        let mut header = FrameHeader {
            magic: MAGIC,
            version: PROTOCOL_VERSION,
            flags,
            channel_id,
            payload_len,
            checksum: 0,
        };

        // Compute checksum over header prefix (0..12) and payload (ADR-0010)
        let header_prefix = header.prefix_bytes();
        let prefix_crc = crc32c(&header_prefix);
        let checksum = crc32c_update(prefix_crc, &payload);
        header.checksum = checksum;

        Ok(Self { header, payload })
    }

    /// Creates a zero-payload keep-alive heartbeat frame on Channel 0 (Control).
    pub fn heartbeat() -> Self {
        let mut header = FrameHeader {
            magic: MAGIC,
            version: PROTOCOL_VERSION,
            flags: FrameFlags::HEARTBEAT,
            channel_id: ChannelId::Control,
            payload_len: 0,
            checksum: 0,
        };
        let prefix_bytes = header.prefix_bytes();
        header.checksum = crc32c(&prefix_bytes);

        Self {
            header,
            payload: Bytes::new(),
        }
    }

    /// Creates a Control channel frame (Channel 0, plain JSON-RPC).
    pub fn control(payload: Bytes) -> Result<Self, ProtocolError> {
        Self::new(ChannelId::Control, FrameFlags::PLAIN, payload)
    }

    /// Encodes the complete frame (header + payload) into a contiguous `Bytes` buffer.
    pub fn encode(&self) -> Bytes {
        let total_size = HEADER_SIZE + self.payload.len();
        let mut buf = BytesMut::with_capacity(total_size);
        buf.extend_from_slice(&self.header.to_bytes());
        buf.extend_from_slice(&self.payload);
        buf.freeze()
    }

    /// Decodes a complete frame from a slice, verifying header integrity, payload CRC-32C,
    /// and strict bounds per ADR-0010.
    pub fn decode(src: &[u8]) -> Result<Self, ProtocolError> {
        if src.len() < HEADER_SIZE {
            return Err(ProtocolError::IncompleteFrame {
                expected: HEADER_SIZE,
                actual: src.len(),
            });
        }

        let header = FrameHeader::decode(&src[..HEADER_SIZE])?;
        let expected_total = HEADER_SIZE + header.payload_len as usize;

        if src.len() < expected_total {
            return Err(ProtocolError::IncompleteFrame {
                expected: expected_total,
                actual: src.len(),
            });
        }

        // Validate full frame CRC: header prefix (0..12) + payload (ADR-0010)
        let prefix_crc = crc32c(&src[..HEADER_PREFIX_SIZE]);
        let payload_slice = &src[HEADER_SIZE..expected_total];
        let calculated_crc = crc32c_update(prefix_crc, payload_slice);

        if calculated_crc != header.checksum {
            return Err(ProtocolError::ChecksumMismatch {
                expected: header.checksum,
                calculated: calculated_crc,
            });
        }

        let payload = Bytes::copy_from_slice(payload_slice);
        Ok(Self { header, payload })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_encode_decode_roundtrip() {
        let header = FrameHeader {
            magic: MAGIC,
            version: PROTOCOL_VERSION,
            flags: FrameFlags::PLAIN,
            channel_id: ChannelId::Control,
            payload_len: 42,
            checksum: 0x12345678,
        };

        let bytes = header.to_bytes();
        assert_eq!(bytes.len(), 16);

        let decoded = FrameHeader::decode(&bytes).expect("Header decode failed");
        assert_eq!(decoded, header);
    }

    #[test]
    fn test_frame_encode_decode_roundtrip() {
        let payload = Bytes::from_static(b"{\"jsonrpc\":\"2.0\",\"method\":\"ping\",\"id\":1}");
        let frame = Frame::control(payload.clone()).expect("Failed to build frame");

        assert_eq!(frame.header.payload_len, payload.len() as u32);
        assert_ne!(frame.header.checksum, 0);

        let encoded = frame.encode();
        assert_eq!(encoded.len(), HEADER_SIZE + payload.len());

        let decoded = Frame::decode(&encoded).expect("Failed to decode frame");
        assert_eq!(decoded.header, frame.header);
        assert_eq!(decoded.payload, payload);
    }

    #[test]
    fn test_heartbeat_frame() {
        let hb = Frame::heartbeat();
        assert!(hb.header.flags.is_heartbeat());
        assert_eq!(hb.header.payload_len, 0);
        // Under ADR-0010, heartbeat checksum is the CRC of the 12-byte header prefix
        assert_ne!(hb.header.checksum, 0);

        let encoded = hb.encode();
        let decoded = Frame::decode(&encoded).expect("Failed to decode heartbeat");
        assert_eq!(decoded, hb);
    }

    #[test]
    fn test_invalid_magic_rejected() {
        let mut header_bytes = Frame::heartbeat().header.to_bytes();
        header_bytes[0] = 0x00; // corrupt magic

        let err = FrameHeader::decode(&header_bytes).unwrap_err();
        assert!(matches!(err, ProtocolError::InvalidMagic(_)));
    }

    #[test]
    fn test_unsupported_version_rejected() {
        let mut header_bytes = Frame::heartbeat().header.to_bytes();
        header_bytes[4] = 0x99; // corrupt version

        let err = FrameHeader::decode(&header_bytes).unwrap_err();
        assert!(matches!(err, ProtocolError::UnsupportedVersion(0x99)));
    }

    #[test]
    fn test_reserved_flags_rejected() {
        let mut header_bytes = Frame::heartbeat().header.to_bytes();
        header_bytes[5] = 0xF0; // set reserved bits 4..7

        let err = FrameHeader::decode(&header_bytes).unwrap_err();
        assert!(matches!(err, ProtocolError::InvalidFlags(0xF0)));
    }

    #[test]
    fn test_conflicting_compression_flags_rejected() {
        let mut header_bytes = Frame::heartbeat().header.to_bytes();
        header_bytes[5] = FrameFlags::LZ4.0 | FrameFlags::JPEG.0;

        let err = FrameHeader::decode(&header_bytes).unwrap_err();
        assert!(matches!(err, ProtocolError::InvalidFlags(_)));
    }

    #[test]
    fn test_payload_too_large_rejected() {
        let mut header_bytes = Frame::heartbeat().header.to_bytes();
        let excessive_len = MAX_PAYLOAD_SIZE + 1;
        header_bytes[8..12].copy_from_slice(&excessive_len.to_be_bytes());

        let err = FrameHeader::decode(&header_bytes).unwrap_err();
        assert!(matches!(err, ProtocolError::PayloadTooLarge { .. }));
    }

    #[test]
    fn test_corrupt_header_bit_detected_by_checksum() {
        let payload = Bytes::from_static(b"secure data");
        let frame = Frame::control(payload).unwrap();
        let mut encoded = frame.encode().to_vec();

        // Flip a bit in ChannelId (byte 7 of header)
        encoded[7] ^= 0x01;

        let err = Frame::decode(&encoded).unwrap_err();
        // Under ADR-0010, header bit flip MUST fail checksum verification!
        assert!(matches!(err, ProtocolError::ChecksumMismatch { .. }));
    }

    #[test]
    fn test_corrupt_payload_crc_rejected() {
        let payload = Bytes::from_static(b"secure data");
        let frame = Frame::control(payload).unwrap();
        let mut encoded = frame.encode().to_vec();

        // Corrupt one byte of payload
        let last = encoded.len() - 1;
        encoded[last] ^= 0xFF;

        let err = Frame::decode(&encoded).unwrap_err();
        assert!(matches!(err, ProtocolError::ChecksumMismatch { .. }));
    }
}
