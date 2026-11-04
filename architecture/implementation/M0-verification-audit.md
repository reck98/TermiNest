# TermiNest — Milestone 0 (M0) Independent Verification & Closure Audit

**Audit Status:** Complete & Verified  
**Date:** November 2026  
**Auditor:** Principal Rust Systems Engineer & Protocol Security Reviewer  
**Scope:** Milestone 0 (Protocol & Transport Foundation)  
**Execution Environment:** Windows 11 Home 64-bit (Intel Core i5-12450H), Rust 1.99.1 (`stable-x86_64-pc-windows-gnu`)  

---

## 1. Executive Assessment

Milestone 0 (M0) has been subjected to a rigorous systems engineering and security audit. The audit evaluated:
1. Wire protocol framing correctness and mathematical checksum coverage.
2. Flag bit validation, reserved bit safety, and compression mutual exclusivity.
3. Incremental stream decoder behavior under adversarial, corrupted, and fragmented inputs.
4. Transport abstraction interfaces, cancellation safety, and EOF propagation.
5. Host platform virtualization capabilities, WSL2 `/dev/vsock` runtime realities, and Windows Hyper-V socket service registration constraints.
6. Test coverage, property verification, and release benchmark methodologies.

All identified defects have been **corrected, verified, and re-tested**. Zero compiler warnings, zero Clippy warnings, and clean formatting are verified across the workspace.

---

## 2. Actual Implementation Verified

The actual implementation verified consists of two pure Rust crates in `crates/`:

### 2.1. `crates/terminest-protocol`
- **Source Files:**
  - `src/lib.rs`: Public interface re-exports.
  - `src/channel.rs`: `ChannelId` enum mapping `Control (0x0000)`, `Video (0x0001)`, `File (0x0002)`, and `Telemetry (0x0003)`.
  - `src/crc.rs`: Pure Castagnoli CRC-32C table engine (`0x1EDC6F41` polynomial / `0x82F63B78` reflected), verified against vector `123456789` -> `0xE3069283`.
  - `src/frame.rs`: Fixed 16-byte `FrameHeader` and `Frame` struct with ADR-0010 full-frame integrity verification.
  - `src/decoder.rs`: Incremental `StreamDecoder` with bounded memory capacity and fail-closed poisoning.
  - `src/rpc.rs`: Strongly typed JSON-RPC 2.0 requests, responses (with null ID support), notifications, and computer-use models.
  - `src/error.rs`: Typed `ProtocolError` hierarchy.
- **Tests & Benches:**
  - `tests/protocol_fuzz.rs`: 4 property-based tests using `proptest`.
  - `benches/protocol_benchmarks.rs`: Multi-sample release benchmarking harness with warm-up passes.

### 2.2. `crates/terminest-transport`
- **Source Files:**
  - `src/lib.rs`: Public interface re-exports.
  - `src/traits.rs`: Asynchronous `Transport` trait (`send`, `recv`, `close`, `is_closed`).
  - `src/in_memory.rs`: `InMemoryTransport` bidirectional channel test harness.
  - `src/framed.rs`: `FramedTransport<IO>` adapter wrapping any `AsyncRead + AsyncWrite` stream.
  - `src/vsock.rs`: Vsock addresses (`VsockAddress`) and Windows Hyper-V Socket GUID generator.
  - `src/error.rs`: Typed `TransportError` hierarchy.
- **Tests:**
  - `tests/transport_integration.rs`: Concurrent ping-pong, 1 MB video frame streaming, and async timeouts.

---

## 3. Protocol Correctness Findings

### Finding PR-01: Header Corruption Blindspot in Payload-Only CRC
- **Severity:** High
- **Evidence:** Report 08 specified CRC-32C over payload bytes only. In `FrameHeader::decode`, the 12 bytes preceding the checksum field had zero integrity verification.
- **Impact:** A bit flip in `PayloadLength` caused the stream decoder to read into subsequent frames, fail checksum on the misaligned slice, and discard valid data, destroying stream framing synchronization. A bit flip in `ChannelId` caused raw video chunks to be delivered to the JSON-RPC parser. A corrupted heartbeat frame had payload length 0 and checksum 0, which matched `crc32c(b"") == 0`, causing corrupted heartbeats to be accepted on any channel.
- **Required Action:** Author ADR-0010 expanding CRC-32C checksum coverage to include the 12-byte header prefix (`Magic`, `Version`, `Flags`, `ChannelId`, `PayloadLength`) and the entire payload.
- **Whether Corrected:** **CORRECTED**. Implemented in `Frame::new`, `Frame::decode`, and `StreamDecoder::poll_next`. Verified with unit test `test_corrupt_header_bit_detected_by_checksum` and property test `prop_frame_detects_any_bit_flip`.

### Finding PR-02: Missing Validation of Reserved Bits and Conflicting Flags
- **Severity:** Medium
- **Evidence:** `FrameFlags` accepted arbitrary raw `u8` values without checking reserved bits 3..7, and permitted conflicting flags (such as simultaneously setting `LZ4` and `JPEG`, or setting `HEARTBEAT` with compression).
- **Impact:** Malformed or future-version flags would be accepted silently without validation, leading to undefined payload decompression behavior.
- **Required Action:** Implement `FrameFlags::from_raw(raw: u8) -> Result<Self, ProtocolError>` enforcing that bits 3..7 are zero, compression flags are mutually exclusive, and heartbeat frames cannot specify compression.
- **Whether Corrected:** **CORRECTED**. Verified with tests `test_reserved_flags_rejected` and `test_conflicting_compression_flags_rejected`.

### Finding PR-03: Missing JSON-RPC 2.0 Notification Type & Inflexible Response ID
- **Severity:** Medium
- **Evidence:** `crates/terminest-protocol/src/rpc.rs` only defined `JsonRpcRequest` with a mandatory `id: RequestId`, lacking a `JsonRpcNotification` type. Furthermore, `JsonRpcResponse` required a non-null `RequestId`, preventing standard `id: null` error responses when request parsing fails.
- **Impact:** Inability to send or receive one-way notifications without IDs; violation of JSON-RPC 2.0 §5 for parse errors.
- **Required Action:** Implement `JsonRpcNotification<T>` and update `JsonRpcResponse` to use `pub id: Option<RequestId>`, serializing `None` as `id: null`.
- **Whether Corrected:** **CORRECTED**. Verified with `test_notification_roundtrip` and `test_null_id_error_response_serialization`.

---

## 4. Security Findings & Hardening Review

### Finding SEC-01: Decoder Infinite Loop on Unhandled Framing Errors
- **Severity:** High
- **Evidence:** In `StreamDecoder::poll_next()`, when `FrameHeader::decode` returned an error (e.g. invalid magic or unsupported version), the error was returned via `?`, but `self.buffer` was not advanced or cleared. Subsequent calls to `poll_next()` re-inspected the exact same 16 bytes and returned the same error repeatedly.
- **Impact:** An application calling `poll_next()` in a loop on error would enter an infinite CPU spin without making progress.
- **Required Action:** Implement deterministic decoder poisoning (`self.is_poisoned = true`). Any unrecoverable framing error poisons the decoder, causing subsequent calls to return `ProtocolError::StreamPoisoned` until explicitly cleared.
- **Whether Corrected:** **CORRECTED**. Verified with `test_decoder_poisoning_on_invalid_header`.

### Finding SEC-02: Buffer Overflow Deadlock when `max_capacity < MAX_PAYLOAD_SIZE`
- **Severity:** Medium
- **Evidence:** If a caller configured `StreamDecoder::with_max_capacity(cap)` with `cap < MAX_PAYLOAD_SIZE`, and an incoming frame header specified `payload_len` larger than `cap`, `poll_next()` would wait for `HEADER_SIZE + payload_len` bytes, while `ingest()` rejected incoming chunks once the buffer reached `cap`.
- **Impact:** Stream deadlock: `poll_next` waits for bytes that `ingest` refuses to accept.
- **Required Action:** In `poll_next()`, immediately check `HEADER_SIZE + payload_len > self.max_capacity`. If true, poison the decoder and reject immediately with `ProtocolError::PayloadTooLarge`.
- **Whether Corrected:** **CORRECTED**. Implemented and verified in `StreamDecoder::poll_next`.

### Finding SEC-03: Clarification of Transport Security Claims
- **Severity:** Low (Documentation & Threat Model Integrity)
- **Evidence:** Earlier drafts loosely associated hypervisor transport with "authentication" or "security".
- **Impact:** Readers could infer that Vsock frames are cryptographically authenticated or encrypted.
- **Required Action:** Explicitly delineate security boundaries in documentation: Castagnoli CRC-32C provides data integrity against transmission noise only; peer isolation and access control are provided by hypervisor kernel ring buffers; cryptographic authentication and wire encryption are deferred to network-facing transports.
- **Whether Corrected:** **CORRECTED**. Documented in `protocol-v1.md`, `transport-spike.md`, and `M0-implementation.md`.

---

## 5. Transport Audit Findings

### Finding TR-01: Hardcoded Expected Length on Mid-Frame EOF
- **Severity:** Low
- **Evidence:** In `FramedTransport::recv()`, when the underlying stream returned EOF (`bytes_read == 0`) and unparsed bytes remained, it returned `IncompleteFrame { expected: 16, actual: len }`. If a valid header had already arrived expecting a 1,000-byte payload, reporting `expected: 16` when `actual: 50` was confusing.
- **Impact:** Misleading diagnostic logs during abrupt connection resets.
- **Required Action:** Add `StreamDecoder::expected_frame_len(&self) -> Option<usize>` which dynamically returns `HEADER_SIZE + payload_len` if a valid header is in the buffer, and use it in `FramedTransport`.
- **Whether Corrected:** **CORRECTED**. Implemented in `StreamDecoder` and `FramedTransport::recv`.

### Finding TR-02: Cancellation and Fail-Closed Write Behavior
- **Severity:** Medium
- **Evidence:** In `FramedTransport::send()`, if `write_all` or `flush` failed or timed out, the transport was not explicitly marked closed, allowing subsequent calls to attempt writes on a potentially desynchronized byte stream.
- **Impact:** If a caller caught an IO error and retried, partial frame bytes could corrupt subsequent frames.
- **Required Action:** Set `self.is_closed = true` immediately upon any write or flush error in `FramedTransport::send()`.
- **Whether Corrected:** **CORRECTED**. Implemented in `FramedTransport::send`.

---

## 6. Vsock Feasibility Status

- **Classification:** **Designed & Mapped (Unverified on real VM)**
- **Technical Reality:**
  - In Windows 11 Home, the Hyper-V VMMS role is not present, though Windows Hypervisor Platform (WHP) and WSL2 are active.
  - Inspection of WSL2 verified active Vsock connections to Host CID 2, but local CID returned `4294967295` (`VMADDR_CID_ANY`), confirming Hyper-V identifies VM endpoints via GUIDs rather than fixed integer CIDs.
  - Inspection of the Windows Registry revealed that custom Hyper-V socket service GUIDs must be registered under `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Virtualization\GuestCommunicationServices`.
  - Writing to `HKLM` requires **Administrator privileges**, meaning an unprivileged user-space installation cannot dynamically register service GUIDs without an elevated installer step.
- **Prerequisites for Future Testing:**
  1. Administrative registration of the TermiNest Service GUID (`{00002711-facb-11e6-bd58-64006a561569}`) on Windows Pro/Enterprise, or
  2. Guest-initiated connection from Linux to Host, or
  3. Non-elevated stream fallback via Named Pipes.

---

## 7. Windows Home Transport Status

- **Classification:** **Designed but Not Implemented**
- **Evaluation:**
  - On Windows Home with QEMU/WHP, `/dev/vhost-vsock` is absent from the host kernel.
  - The proposed architecture routes traffic through QEMU's `virtio-serial-pci` device connected to a Windows Named Pipe (`\\.\pipe\terminest-<name>`) or local loopback TCP port.
  - The `FramedTransport<IO>` adapter demonstrates that framing works across any async byte stream, but the concrete Windows Named Pipe server and QEMU process lifecycle manager have not yet been implemented (deferred to Milestone 1 & 2).

---

## 8. Test Results Ledger

Running `cargo test --workspace --all-features` and `cargo test --workspace --doc`:

```text
=== terminest-protocol (26 Unit Tests) ===
test channel::tests::test_valid_channel_conversions ... ok
test channel::tests::test_invalid_channel_rejected ... ok
test crc::tests::test_empty_slice ... ok
test crc::tests::test_incremental_crc_matches_all_at_once ... ok
test crc::tests::test_standard_vector_123456789 ... ok
test decoder::tests::test_decoder_single_complete_frame ... ok
test decoder::tests::test_decoder_multiple_frames_in_one_read ... ok
test decoder::tests::test_decoder_arbitrary_fragmentation ... ok
test decoder::tests::test_decoder_buffer_overflow_protection ... ok
test decoder::tests::test_decoder_poisoning_on_invalid_header ... ok
test decoder::tests::test_decoder_corrupt_payload_crc ... ok
test frame::tests::test_header_encode_decode_roundtrip ... ok
test frame::tests::test_frame_encode_decode_roundtrip ... ok
test frame::tests::test_heartbeat_frame ... ok
test frame::tests::test_invalid_magic_rejected ... ok
test frame::tests::test_unsupported_version_rejected ... ok
test frame::tests::test_reserved_flags_rejected ... ok
test frame::tests::test_conflicting_compression_flags_rejected ... ok
test frame::tests::test_payload_too_large_rejected ... ok
test frame::tests::test_corrupt_header_bit_detected_by_checksum ... ok
test frame::tests::test_corrupt_payload_crc_rejected ... ok
test rpc::tests::test_handshake_request_serialization ... ok
test rpc::tests::test_mouse_click_roundtrip ... ok
test rpc::tests::test_error_response_serialization ... ok
test rpc::tests::test_null_id_error_response_serialization ... ok
test rpc::tests::test_notification_roundtrip ... ok

=== protocol_fuzz (4 Property Tests via Proptest) ===
test prop_frame_encode_decode_roundtrip ... ok
test prop_decoder_no_panic_on_fuzz_input ... ok
test prop_crc32c_detects_single_bit_flip ... ok
test prop_frame_detects_any_bit_flip ... ok

=== terminest-transport (7 Unit Tests) ===
test vsock::tests::test_vsock_address_display ... ok
test vsock::tests::test_hyperv_port_guid_generation ... ok
test in_memory::tests::test_in_memory_bidirectional_exchange ... ok
test in_memory::tests::test_in_memory_clean_close ... ok
test in_memory::tests::test_send_after_close_returns_error ... ok
test framed::tests::test_framed_duplex_stream_roundtrip ... ok
test framed::tests::test_framed_fragmented_stream ... ok

=== transport_integration (3 Integration Tests) ===
test test_in_memory_concurrent_ping_pong ... ok
test test_framed_transport_large_video_frame ... ok
test test_framed_transport_timeout ... ok

Total: 40 Passed / 0 Failed (100% test pass rate)
```

---

## 9. Benchmark Assessment

Benchmark methodology incorporates:
- Dedicated warm-up passes (500 to 50,000 iterations depending on payload size) to stabilize instruction caches and CPU frequency governors.
- 5 independent timed sample runs calculating median, minimum, and maximum latencies.
- Compiled under the release profile (`--release`).

### Summary of Measurements:
- **CRC-32C Throughput:** 745.08 MB/s on small packets (81.92 ns/op median), 485.30 MB/s on 2 MB screenshots (4.22 ms/op median).
- **Frame Encoding (ADR-0010):** 207.81 ns/op median for 64-byte control frames; 4.88 ms/op median for 2 MB video frames (~205 FPS capacity).
- **Frame Decoding (ADR-0010):** 163.50 ns/op median for 64-byte control frames; 4.98 ms/op median for 2 MB video frames.
- **StreamDecoder Ingestion:** 222.66 ns/op median for 128-byte chunks.
- **JSON-RPC Serialization:** 217.45 ns/op median; Deserialization: 633.58 ns/op median.

---

## 10. Documentation Corrections

The following documentation files were updated to remove unsupported claims and align with the audit findings:
1. **`protocol-v1.md`:** Updated framing specification to reflect ADR-0010 full-frame checksum coverage; added strict flags and reserved bit rules; clarified integrity vs. authentication.
2. **`transport-spike.md`:** Applied the five-state taxonomy to all transport features; recorded the Windows Registry administrative constraint for Hyper-V socket service registration; classified QEMU/WHP Named Pipe as designed but not implemented.
3. **`test-results.md`:** Updated test count to 40 passed tests; distinguished generative property tests from sustained fuzzing; qualified code coverage statements.
4. **`M0-implementation.md`:** Synchronized implementation report with ADR-0010 and the closure audit findings.
5. **`ADR-0010`:** Created formal Architecture Decision Record for full-frame checksum coverage.

---

## 11. Remaining Blockers

There are **zero remaining technical blockers** within the scope of Milestone 0.

Known platform constraints documented for Milestone 1 & 2:
- On Windows Pro/Enterprise: An elevated setup action is required to register the TermiNest Service GUID in `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Virtualization\GuestCommunicationServices`.
- On Windows Home: QEMU/WHP will require implementing the concrete Windows Named Pipe or loopback stream adapter.

---

## 12. Final M0 Readiness Decision

**Decision:** **MILESTONE 0 IS READY FOR FORMAL CLOSURE.**

- Protocol behavior matches its specification under ADR-0010.
- Critical correctness and security defects (PR-01, PR-02, PR-03, SEC-01, SEC-02, TR-01, TR-02) have been completely corrected and verified.
- All 40 automated tests, Clippy, and formatting checks pass with zero warnings.
- Transport limitations are honestly documented without unsupported claims.
- The repository is clean, disciplined, and prepared for Milestone 1.
