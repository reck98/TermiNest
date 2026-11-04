use proptest::prelude::*;
use terminest_protocol::{crc32c, ChannelId, Frame, FrameFlags, StreamDecoder};

proptest! {
    /// Invariant 1: For any valid channel, flags, and payload within bounds,
    /// encoding then decoding must produce the exact original frame.
    #[test]
    fn prop_frame_encode_decode_roundtrip(
        channel_raw in 0u16..=3u16,
        flag_byte in 0u8..=3u8, // plain, lz4, jpeg (non-heartbeat)
        payload in proptest::collection::vec(any::<u8>(), 0..65536)
    ) {
        let channel = ChannelId::from_raw(channel_raw).unwrap();
        let flags = FrameFlags(flag_byte);
        let bytes = bytes::Bytes::from(payload);

        let frame = Frame::new(channel, flags, bytes.clone()).unwrap();
        let encoded = frame.encode();

        let decoded = Frame::decode(&encoded).expect("Valid frame must decode");
        prop_assert_eq!(decoded.header, frame.header);
        prop_assert_eq!(decoded.payload, bytes);
    }

    /// Invariant 2: Arbitrary random byte streams fed into StreamDecoder must
    /// NEVER panic or abort. It must deterministically return Ok(None), Ok(Some),
    /// or Err(ProtocolError).
    #[test]
    fn prop_decoder_no_panic_on_fuzz_input(
        chunks in proptest::collection::vec(
            proptest::collection::vec(any::<u8>(), 0..1024),
            1..20
        )
    ) {
        let mut decoder = StreamDecoder::with_max_capacity(128 * 1024);
        for chunk in chunks {
            if decoder.ingest(&chunk).is_ok() {
                loop {
                    match decoder.poll_next() {
                        Ok(Some(_)) => continue,
                        Ok(None) => break,
                        Err(_) => {
                            decoder.clear();
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Invariant 3: Castagnoli CRC-32C must detect any single-bit flip in the payload.
    #[test]
    fn prop_crc32c_detects_single_bit_flip(
        payload in proptest::collection::vec(any::<u8>(), 1..1024),
        bit_index in 0usize..8192usize
    ) {
        let original_crc = crc32c(&payload);
        let mut corrupted = payload.clone();

        let target_byte = (bit_index / 8) % corrupted.len();
        let target_bit = (bit_index % 8) as u8;
        corrupted[target_byte] ^= 1 << target_bit;

        let corrupted_crc = crc32c(&corrupted);
        prop_assert_ne!(original_crc, corrupted_crc, "Single bit flip was not detected by CRC-32C");
    }
}
