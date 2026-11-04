# Milestone 0: Verification & Test Results Report

**Date:** November 2026  
**System:** TermiNest Virtual Computer Infrastructure  
**Target:** Milestone 0 (Protocol and Transport Foundation)  
**Execution Environment:** Windows 11 Home 64-bit (Intel Core i5-12450H), Rust 1.99.1 (`stable-x86_64-pc-windows-gnu`)  
**Audit Status:** Audited and Verified (M0 Closure Audit)  

---

## 1. Executive Summary

Milestone 0 automated test suites executed cleanly across unit, property-based, and transport integration tests:
- **Total Test Cases Executed:** 40 tests
- **Passed:** 40 (100% test pass rate)
- **Failed:** 0
- **Ignored / Filtered:** 0
- **Clippy Status:** 0 warnings (`cargo clippy --workspace --all-targets --all-features -- -D warnings`)
- **Formatting Status:** Clean (`cargo fmt --all -- --check`)
- **Documentation Tests:** 0 failures (`cargo test --workspace --doc`)
- **Coverage Qualification:** 100% test pass rate across 40 defined test cases; line/branch coverage has not been instrumented with `cargo-llvm-cov`.

---

## 2. Test Suite Breakdown

### 2.1. `terminest-protocol` Unit Tests (26 Tests — 100% Pass)
- `channel::tests::test_valid_channel_conversions`: Verified bidirectional mapping for Control (`0x0000`), Video (`0x0001`), File (`0x0002`), and Telemetry (`0x0003`).
- `channel::tests::test_invalid_channel_rejected`: Verified rejection of undefined channel IDs (e.g. `0x0004`, `0xFFFF`).
- `crc::tests::test_standard_vector_123456789`: Evaluated standard ASCII test input `b"123456789"`; confirmed output strictly equals `0xE3069283`.
- `crc::tests::test_empty_slice`: Confirmed empty byte slices yield `0x00000000`.
- `crc::tests::test_incremental_crc_matches_all_at_once`: Validated chunked accumulation (`crc32c_update`) against one-shot calculation.
- `frame::tests::test_header_encode_decode_roundtrip`: Validated 16-byte Big-Endian serialization and field deserialization.
- `frame::tests::test_frame_encode_decode_roundtrip`: End-to-end frame serialization, automatic ADR-0010 CRC-32C calculation, and deserialization.
- `frame::tests::test_heartbeat_frame`: Validated heartbeat frame flags (`0x04`), 0-length payload, and non-zero header prefix CRC.
- `frame::tests::test_invalid_magic_rejected`: Confirmed rejection of frames with corrupted magic prefix (`!= 0x544E5354`).
- `frame::tests::test_unsupported_version_rejected`: Confirmed rejection of non-v1 protocol versions.
- `frame::tests::test_reserved_flags_rejected`: Verified that setting reserved flag bits (3..7) is rejected with `ProtocolError::InvalidFlags`.
- `frame::tests::test_conflicting_compression_flags_rejected`: Verified that simultaneously setting `LZ4` and `JPEG` flags is rejected with `ProtocolError::InvalidFlags`.
- `frame::tests::test_payload_too_large_rejected`: Confirmed strict rejection of payloads exceeding 16 MB ceiling (`16,777,216` bytes).
- `frame::tests::test_corrupt_header_bit_detected_by_checksum`: Flipped a bit in `ChannelId` within the header; confirmed immediate rejection with `ChecksumMismatch` under ADR-0010.
- `frame::tests::test_corrupt_payload_crc_rejected`: Injected single-byte bit flip in payload; confirmed deterministic rejection with `ChecksumMismatch`.
- `decoder::tests::test_decoder_single_complete_frame`: Validated one-shot ingestion and poll extraction.
- `decoder::tests::test_decoder_multiple_frames_in_one_read`: Verified ingestion of 3 contiguous frames in a single buffer, sequentially parsed without re-allocation.
- `decoder::tests::test_decoder_arbitrary_fragmentation`: Fed a multi-byte frame in 1-byte increments; verified perfect reassembly upon arrival of the final byte.
- `decoder::tests::test_decoder_buffer_overflow_protection`: Validated that exceeding maximum decoder buffer capacity rejects ingestion with `BufferOverflow` and poisons the decoder.
- `decoder::tests::test_decoder_poisoning_on_invalid_header`: Verified that encountering an invalid header poisons the decoder, preventing infinite error polling.
- `decoder::tests::test_decoder_corrupt_payload_crc`: Verified corrupt payload in stream is dropped without buffer deadlock.
- `rpc::tests::test_handshake_request_serialization`: Validated JSON-RPC 2.0 `guest.handshake` schema serialization/deserialization.
- `rpc::tests::test_mouse_click_roundtrip`: Validated `desktop.mouse_click` schema and coordinate point response.
- `rpc::tests::test_error_response_serialization`: Validated typed error response mapping (`SESSION_UNAVAILABLE = -32000`).
- `rpc::tests::test_null_id_error_response_serialization`: Validated that error responses with `id: null` serialize correctly per JSON-RPC 2.0 §5.
- `rpc::tests::test_notification_roundtrip`: Validated one-way notifications without an `id` field.

### 2.2. Property-Based Tests (4 Tests — 100% Pass)
*(Note: These are property-based generative tests via `proptest`, distinct from sustained fuzz testing with `cargo-fuzz`)*:
- `prop_frame_encode_decode_roundtrip`: Validated lossless round-trip invariant across random channels, flags, and payloads (up to 64 KB).
- `prop_decoder_no_panic_on_fuzz_input`: Fed arbitrary pseudorandom byte streams of varying chunk sizes; verified the decoder NEVER panics or causes unbounded memory growth.
- `prop_crc32c_detects_single_bit_flip`: Tested random bit inversions across variable payloads; proved Castagnoli CRC-32C detects all single-bit corruptions.
- `prop_frame_detects_any_bit_flip`: Verified that ANY single-bit flip across the entire 16-byte header or payload is deterministically rejected.

### 2.3. `terminest-transport` Unit Tests (7 Tests — 100% Pass)
- `vsock::tests::test_vsock_address_display`: Verified URI format `vsock://<cid>:<port>`.
- `vsock::tests::test_hyperv_port_guid_generation`: Tested port `10001` (`0x2711`) mapping to `{00002711-facb-11e6-bd58-64006a561569}`.
- `in_memory::tests::test_in_memory_bidirectional_exchange`: Verified full-duplex message exchange between paired endpoints.
- `in_memory::tests::test_in_memory_clean_close`: Verified EOF propagation upon close.
- `in_memory::tests::test_send_after_close_returns_error`: Verified send on closed transport returns `TransportError::Closed`.
- `framed::tests::test_framed_duplex_stream_roundtrip`: Verified `FramedTransport` framing over Tokio async duplex stream.
- `framed::tests::test_framed_fragmented_stream`: Verified streaming fragmentation over async byte stream.

### 2.4. `terminest-transport` Integration Tests (3 Tests — 100% Pass)
- `test_in_memory_concurrent_ping_pong`: 100 sequential bidirectional async RPC ping-pongs across concurrent Tokio tasks.
- `test_framed_transport_large_video_frame`: 1 MB binary payload transmission and validation over async framed stream.
- `test_framed_transport_timeout`: Verified asynchronous timeout semantics using `tokio::time::timeout`.

---

## 3. Multi-Run Performance Benchmarks

Measured on host hardware (12th Gen Intel Core i5-12450H, build profile `--release`) using dedicated warm-up passes and 5 timed sample runs:

| Benchmark Target | Payload Size | Samples × Ops | Median Latency | Min / Max Latency | Throughput (at Median) |
|---|---|---|---|---|---|
| **CRC-32C Calculation** | 64 Bytes | 5 × 200,000 | 81.92 ns | 80.87 ns / 82.84 ns | 745.08 MB/s |
| **CRC-32C Calculation** | 64 KB | 5 × 10,000 | 128.93 μs | 128.04 μs / 130.55 μs | 484.76 MB/s |
| **CRC-32C Calculation** | 2 MB (Screenshot) | 5 × 500 | 4.22 ms | 4.18 ms / 4.25 ms | 485.30 MB/s |
| **Frame Encode (ADR-0010)** | 64 Bytes | 5 × 100,000 | 207.81 ns | 203.76 ns / 213.33 ns | ~4.8M frames/s |
| **Frame Decode (ADR-0010)** | 64 Bytes | 5 × 100,000 | 163.50 ns | 154.78 ns / 172.39 ns | ~6.1M frames/s |
| **Frame Encode (ADR-0010)** | 2 MB (Video Frame) | 5 × 200 | 4.88 ms | 4.84 ms / 5.01 ms | ~205 frames/s |
| **Frame Decode (ADR-0010)** | 2 MB (Video Frame) | 5 × 200 | 4.98 ms | 4.82 ms / 5.17 ms | ~200 frames/s |
| **StreamDecoder Ingestion** | 128 Bytes | 5 × 50,000 | 222.66 ns | 221.01 ns / 230.87 ns | ~4.5M frames/s |
| **JSON-RPC Serialization** | `desktop.mouse_click` | 5 × 50,000 | 217.45 ns | 205.53 ns / 242.79 ns | ~4.6M msgs/s |
| **JSON-RPC Deserialization** | `desktop.mouse_click` | 5 × 50,000 | 633.58 ns | 628.43 ns / 657.91 ns | ~1.58M msgs/s |
| **Handshake Deserialization** | `guest.handshake` | 5 × 20,000 | 632.27 ns | 602.61 ns / 689.57 ns | ~1.58M msgs/s |
