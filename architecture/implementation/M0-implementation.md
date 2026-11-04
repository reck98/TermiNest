# Milestone 0 (M0) Implementation Report: Protocol & Transport Foundation

**Project:** TermiNest  
**Phase:** Milestone 0 (M0) Complete  
**Date:** November 2026  
**Lead Engineer:** Principal Systems Infrastructure Architect  
**Implementation Language:** Rust 1.99.1 (`stable-x86_64-pc-windows-gnu`)  
**Baseline Repository Status:** Clean workspace; reference repository `terminal-mac` verified unchanged  

---

## 1. Executive Summary

Milestone 0 (M0) establishes the foundational communication and serialization backbone for TermiNest:
1. **TermiNest Wire Protocol v1 (TNST-v1):** Full implementation of the 16-byte fixed binary framing format with network byte order (Big-Endian), 4 logical multiplexed channels, and Castagnoli CRC-32C payload integrity verification.
2. **Incremental Stream Decoder:** A robust, zero-copy, bounded-memory stream decoder handling arbitrary stream fragmentation, partial headers/payloads, and corrupted frames.
3. **Control Plane RPC:** Strongly typed JSON-RPC 2.0 models for guest handshake, ping/heartbeats, screen capture, mouse/keyboard input synthesis, command execution, and display streaming.
4. **Transport Abstraction:** Clean, decoupled `Transport` trait implemented for both deterministic in-memory testing (`InMemoryTransport`) and stream-based byte transports (`FramedTransport<IO>`).
5. **Platform Vsock Feasibility:** In-depth investigation and architectural mapping for Windows host (`AF_HYPERV` / WHP / QEMU virtio-serial) and Linux guest (`AF_VSOCK` / `hv_sock`).
6. **Rigorously Tested & Benchmarked:** 33 automated tests (20 unit, 3 property fuzz, 7 transport, 3 integration) passing with 100% success rate, 0 Clippy warnings, clean formatting, and sub-microsecond serialization latency.

---

## 2. Workspace & Crate Structure

```text
terminest/
├── Cargo.toml                      # Workspace definition with centralized dependency pinning
├── Cargo.lock                      # Committed dependency lockfile
├── rust-toolchain.toml             # Pinned Rust toolchain configuration
├── .gitignore                      # Git exclusion rules
├── crates/
│   ├── terminest-protocol/         # Wire protocol, framing, CRC-32C, decoder, and JSON-RPC
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs              # Protocol crate root and public re-exports
│   │   │   ├── channel.rs          # Multiplexed ChannelId enum (Control, Video, File, Telemetry)
│   │   │   ├── crc.rs              # Hardware/Software Castagnoli CRC-32C (0xE3069283 verified)
│   │   │   ├── decoder.rs          # Incremental StreamDecoder with bounded buffering
│   │   │   ├── error.rs            # Strongly typed ProtocolError hierarchy
│   │   │   ├── frame.rs            # 16-byte FrameHeader and complete Frame structures
│   │   │   └── rpc.rs              # JSON-RPC 2.0 request/response/error schemas
│   │   ├── tests/
│   │   │   └── protocol_fuzz.rs    # Proptest property-based fuzz tests
│   │   └── benches/
│   │       └── protocol_benchmarks.rs # Release-mode performance benchmarking harness
│   └── terminest-transport/        # Asynchronous transport engines and stream framing
│       ├── Cargo.toml
│       ├── src/
│       │   ├── lib.rs              # Transport crate root and public re-exports
│       │   ├── error.rs            # TransportError definitions
│       │   ├── traits.rs           # Deep Transport trait interface
│       │   ├── in_memory.rs        # InMemoryTransport bidirectional test harness
│       │   ├── framed.rs           # FramedTransport<IO> wrapping AsyncRead + AsyncWrite
│       │   └── vsock.rs            # Vsock address models and AF_HYPERV GUID generators
│       └── tests/
│           └── transport_integration.rs # Concurrent ping-pong and bulk payload integration tests
└── architecture/
    └── implementation/
        ├── README.md               # Implementation document index
        ├── M0-implementation.md    # This report
        ├── protocol-v1.md          # Authoritative TNST-v1 wire protocol specification
        ├── transport-spike.md      # Vsock feasibility and host/guest platform findings
        └── test-results.md         # Comprehensive test matrix and execution results
```

---

## 3. Subsystem Implementation Details

### 3.1. Binary Frame Header (`terminest-protocol::frame`)
- **16-Byte Header Layout:**
  - `0..4`: Magic `0x544E5354` (`"TNST"`).
  - `4`: Protocol Version (`0x01`).
  - `5`: Flags bitfield (`0x00`: Plain, `0x01`: LZ4, `0x02`: JPEG, `0x04`: Heartbeat).
  - `6..8`: Channel ID (`0x0000`: Control, `0x0001`: Video, `0x0002`: File, `0x0003`: Telemetry).
  - `8..12`: Payload Length (`u32_be`, bounds checked to `<= 16,777,216` bytes).
  - `12..16`: Castagnoli CRC-32C Checksum (`u32_be`).

### 3.2. Castagnoli CRC-32C Engine (`terminest-protocol::crc`)
- Precomputed compile-time table (`[u32; 256]`) using polynomial `0x1EDC6F41` (reflected: `0x82F63B78`).
- Strictly validated against the canonical test vector: `b"123456789"` produces `0xE3069283`.
- Empty slices produce `0x00000000`.
- Supports incremental streaming accumulation via `crc32c_update`.

### 3.3. Incremental Stream Decoder (`terminest-protocol::decoder`)
- Implements `StreamDecoder` to parse streams fragmented across arbitrary read increments.
- Zero-copy buffer slicing via `bytes::BytesMut`.
- Memory bounding: Configurable `max_capacity` (default 32 MB) rejects oversized ingestion with `BufferOverflow` before allocating heap memory.
- Deterministic failure: Corrupted checksums or invalid headers trigger immediate typed errors and discard the invalid chunk, preventing cascading buffer stall.

### 3.4. JSON-RPC 2.0 Control Plane (`terminest-protocol::rpc`)
- Implements standard specification 2.0 schemas (`JsonRpcRequest`, `JsonRpcResponse`, `JsonRpcError`).
- Provides strongly typed request/response structs for:
  - `guest.handshake` (distro, kernel, session ID, username, desktop environment, display resolution, DPI)
  - `guest.ping`
  - `desktop.mouse_click`, `desktop.mouse_move`, `desktop.mouse_drag`
  - `desktop.key_sequence`, `desktop.type_text`
  - `system.exec`
  - `stream.start`, `stream.stop`
  - `clipboard.read`, `clipboard.write`

### 3.5. Transport Abstraction & In-Memory Duplex (`terminest-transport`)
- `Transport` trait defines asynchronous frame exchange: `send()`, `recv()`, `close()`, and `is_closed()`.
- `InMemoryTransport::pair(capacity)` creates a connected bidirectional test pair using Tokio bounded channels. Closing an endpoint safely drops the sender, notifying the peer receiver with clean EOF (`None`).
- `FramedTransport<IO>` wraps any `AsyncRead + AsyncWrite + Unpin + Send + Sync` stream (such as `tokio::io::DuplexStream`, Unix domain sockets, TCP streams, or Hyper-V sockets), utilizing `StreamDecoder` to assemble complete frames.

---

## 4. Platform Reconnaissance & Vsock Findings

- **Host Machine:** Windows 11 Home Single Language (Version 10.0.26300, 64-bit), Intel Core i5-12450H.
- **Virtualization Capabilities:**
  - `VirtualMachinePlatform` (VMP): Enabled.
  - `HypervisorPlatform` (WHP): Enabled.
  - Native Hyper-V VMMS role: Unavailable on Windows Home (expected per ADR-0002).
  - WSL2: Default Ubuntu-24.04 available.
- **Transport Realities:**
  - On Windows Pro/Enterprise: Native `AF_HYPERV` (Winsock family 34) provides direct kernel Vsock to Linux guest `hv_sock`.
  - On Windows Home (with QEMU/WHP): Native `/dev/vhost-vsock` is unavailable on Windows hosts. Communication will be routed through `FramedTransport` over a local stream (virtio-serial connected to a Windows Named Pipe, or private host-only loopback port `127.0.0.1:10001`).
  - Because `terminest-transport` abstracts over `AsyncRead + AsyncWrite`, the protocol engine remains 100% identical and reusable across all host editions.

---

## 5. Security & Threat Hardening Review

- **Input Validation:** All integer lengths, channel identifiers, protocol versions, and magic bytes are strictly validated before allocating memory.
- **Denial of Service (DoS) Prevention:** Payloads are capped at 16 MB. The decoder rejects inputs that exceed buffer capacity with `BufferOverflow`.
- **Integrity vs. Authentication:** As documented in `protocol-v1.md`, CRC-32C provides data integrity detection against transport bit corruption, not cryptographic authentication. Security relies on hardware hypervisor ring buffer isolation.
- **Zero Unsafe Rust:** Zero `unsafe` blocks are introduced in `terminest-protocol` or `terminest-transport`.
- **Memory Safety:** The implementation relies exclusively on safe Rust borrow mechanics and the `bytes` crate.

---

## 6. Verification Results

| Test Category | Target Module / Crate | Tests | Result |
|---|---|---|---|
| Wire Header & Magic | `terminest-protocol::frame` | 6 | **PASS (100%)** |
| CRC-32C Algorithm | `terminest-protocol::crc` | 3 | **PASS (100%)** |
| Channel Multiplexing | `terminest-protocol::channel` | 2 | **PASS (100%)** |
| Stream Decoder & Bounded Buffering | `terminest-protocol::decoder` | 5 | **PASS (100%)** |
| JSON-RPC 2.0 Serialization | `terminest-protocol::rpc` | 4 | **PASS (100%)** |
| Property-Based Fuzzing | `terminest-protocol::tests::protocol_fuzz` | 3 | **PASS (100%)** |
| Vsock Addressing & GUIDs | `terminest-transport::vsock` | 2 | **PASS (100%)** |
| In-Memory Transport | `terminest-transport::in_memory` | 3 | **PASS (100%)** |
| Stream Framing | `terminest-transport::framed` | 2 | **PASS (100%)** |
| Transport Integration & Timeouts | `terminest-transport::tests::transport_integration` | 3 | **PASS (100%)** |
| **Total Automated Tests** | | **33** | **33 PASSED / 0 FAILED** |

---

## 7. Performance Benchmarks

Measured on host hardware (12th Gen Intel Core i5-12450H, build profile `--release`):

| Operation | Payload Size | Iterations | Latency (ns / op) | Throughput |
|---|---|---|---|---|
| **CRC-32C Calculation** | 64 Bytes | 1,000,000 | 83.95 ns | 727.06 MB/s |
| **CRC-32C Calculation** | 64 KB | 50,000 | 127.39 μs | 490.61 MB/s |
| **CRC-32C Calculation** | 2 MB (Screenshot) | 2,000 | 4.11 ms | 486.42 MB/s |
| **Frame Encoding** | 64 Bytes | 500,000 | 194.25 ns | ~5.1M frames/s |
| **Frame Decoding + CRC** | 64 Bytes | 500,000 | 146.67 ns | ~6.8M frames/s |
| **Frame Encoding** | 2 MB (Video Frame) | 1,000 | 4.69 ms | ~213 frames/s |
| **Frame Decoding + CRC** | 2 MB (Video Frame) | 1,000 | 4.64 ms | ~215 frames/s |
| **StreamDecoder Ingest + Poll** | 128 Bytes | 200,000 | 234.05 ns | ~4.2M frames/s |
| **JSON-RPC Serialization** | `desktop.mouse_click` | 200,000 | 179.06 ns | ~5.5M msgs/s |
| **JSON-RPC Deserialization** | `desktop.mouse_click` | 200,000 | 607.80 ns | ~1.6M msgs/s |
| **Handshake Deserialization** | `guest.handshake` | 100,000 | 592.37 ns | ~1.6M msgs/s |

---

## 8. Definition of Done Compliance Audit

| Requirement | Audit Status | Evidence |
|---|---|---|
| Clean Rust Workspace | **COMPLETE** | Workspace configured with `terminest-protocol` and `terminest-transport`. Builds cleanly. |
| No Unnecessary Dependencies | **COMPLETE** | Only minimal, established crates (`bytes`, `serde`, `serde_json`, `thiserror`, `tokio`). |
| Reference Repository Unchanged | **COMPLETE** | `govijr/terminal-mac` external reference analyzed; local workspace is clean. |
| Exact Frame Format Documented | **COMPLETE** | Documented in `protocol-v1.md` and Report 08. |
| Header Encoding & Decoding | **COMPLETE** | 16-byte Big-Endian header verified by unit and property tests. |
| Castagnoli CRC-32C Validation | **COMPLETE** | Test vector `b"123456789"` evaluated to `0xE3069283`. |
| Payload Limits Enforced | **COMPLETE** | Payloads `> 16 MB` rejected with `ProtocolError::PayloadTooLarge`. |
| Version and Channel Validation | **COMPLETE** | Unknown channels and non-0x01 versions rejected. |
| Typed JSON-RPC Messages | **COMPLETE** | Implemented in `terminest_protocol::rpc`. |
| Incremental Stream Decoder | **COMPLETE** | Tested against arbitrary 1-byte chunk fragmentation and multi-frame bursts. |
| In-Memory Duplex Transport | **COMPLETE** | `InMemoryTransport` validated for bidirectional messaging, clean close, and errors after close. |
| Stream Framing Layer | **COMPLETE** | `FramedTransport<IO>` validated over Tokio async duplex stream. |
| Vsock Feasibility Investigated | **COMPLETE** | Documented in `transport-spike.md`. |
| Unit & Integration Tests Pass | **COMPLETE** | 33 tests pass in `< 3s`. |
| Formatting & Clippy Clean | **COMPLETE** | `cargo fmt --check` and `cargo clippy --workspace --all-targets` pass with 0 warnings. |
| Documentation Complete | **COMPLETE** | `protocol-v1.md`, `transport-spike.md`, `test-results.md`, and `M0-implementation.md` authored. |

---

## 9. Readiness Assessment for Milestone 1

Milestone 0 is **fully achieved** with zero blockers. The communication foundation is robust, secure, and measured.

The repository is now in an ideal state to proceed to **Milestone 1 (Core Host Orchestrator & CLI Foundation)** upon user directive.
