# TermiNest Wire Protocol v1 (TNST-v1) Specification

**Status:** Authoritative Specification (Amended via ADR-0010)  
**Version:** 1.0.0  
**Date:** November 2026  
**Target Systems:** Windows (Host/Guest), Linux (Host/Guest)  
**Byte Ordering:** Network Byte Order (Big-Endian)  

---

## 1. Overview & Protocol Architecture

TermiNest Wire Protocol v1 (`TNST-v1`) provides low-latency, multiplexed, binary-framed communication over point-to-point stream transports (specifically kernel-level Virtual Sockets: `AF_VSOCK` on Linux and `AF_HYPERV` on Windows, or framed stream streams).

It multiplexes four logical channels over a single bidirectional stream:
- **Channel 0 (`0x0000`):** Control Channel (JSON-RPC 2.0 commands, responses, and events).
- **Channel 1 (`0x0001`):** Video Stream Channel (Desktop frame streaming and screenshots).
- **Channel 2 (`0x0002`):** File Transfer Channel (Chunked binary file transfers).
- **Channel 3 (`0x0003`):** Telemetry & Metrics Channel (Guest CPU, RAM, and session health).

---

## 2. Binary Framing Specification

Every frame transmitted across the transport begins with an immutable, fixed-size **16-byte header**, followed immediately by the variable-length payload.

```text
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                     Magic (0x544E5354 "TNST")                 |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|    Version    |     Flags     |          Channel ID           |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                         Payload Length                        |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                        CRC-32C Checksum                       |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                     Payload Data (Variable) ...               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

### 2.1. Header Field Layout

| Field Name | Offset (Bytes) | Size (Bytes) | Type / Endianness | Allowed Values / Validation Rules |
|---|---|---|---|---|
| **Magic** | `0` | `4` | `u32` (Big-Endian) | Must equal `0x544E5354` (ASCII `"TNST"`). Rejects spurious streams immediately. |
| **Version** | `4` | `1` | `u8` | Must equal `0x01`. Future revisions negotiate via handshake or version bump. |
| **Flags** | `5` | `1` | `u8` bitfield | `0x00` = Plain uncompressed, `0x01` = LZ4, `0x02` = JPEG, `0x04` = Heartbeat. Bits 3..7 are reserved (must be 0). `LZ4` and `JPEG` are mutually exclusive. `HEARTBEAT` cannot specify compression. |
| **Channel ID** | `6` | `2` | `u16` (Big-Endian) | `0x0000` (Control), `0x0001` (Video), `0x0002` (File), `0x0003` (Telemetry). Unknown channels trigger `ProtocolError::UnknownChannel`. |
| **Payload Length** | `8` | `4` | `u32` (Big-Endian) | `0` to `16,777,216` (16 MB maximum). For Heartbeat frames (`0x04`), must be strictly `0`. |
| **CRC-32C Checksum** | `12` | `4` | `u32` (Big-Endian) | Castagnoli CRC-32C of **Header Prefix (bytes 0..12) + Payload bytes** (per ADR-0010). For 0-length payload frames (Heartbeats), equals `crc32c(&header[0..12])`. |

### 2.2. Checksum Algorithm (Castagnoli CRC-32C)
- **Polynomial:** `0x1EDC6F41` (reflected: `0x82F63B78`).
- **Initial value:** `0xFFFFFFFF`.
- **Final XOR:** `0xFFFFFFFF`.
- **Standard verification vector:** ASCII string `123456789` (`0x313233343536373839`) evaluates to `0xE3069283`.
- **Coverage (ADR-0010):** Covers the 12-byte header prefix (`Magic`, `Version`, `Flags`, `ChannelId`, `PayloadLength`) and the entire payload. This protects the frame against header corruption, channel misrouting, and stream desynchronization.

> [!IMPORTANT]
> **Integrity vs. Authentication & Confidentiality:**
> - **Accidental Corruption Detection:** Castagnoli CRC-32C provides mathematical integrity detection against accidental transmission bit flips, buffer drops, and framing misalignment. It is **not** a cryptographic hash or digital signature and provides zero security against an active in-path adversary.
> - **Transport Boundary Isolation:** Hardware hypervisor boundary isolation (`AF_VSOCK` / `AF_HYPERV` kernel memory rings) separates guest memory from host memory, but does **not** provide cryptographic authentication or encryption.
> - **No Inherent Cryptographic Authentication:** Raw Vsock and Hyper-V sockets identify VM containers (via CID / VmId GUID), not the user or process identity inside the VM.
> - **No Wire Encryption:** Payloads travel as unencrypted plaintext through hypervisor shared memory. Cryptographic peer authentication and wire confidentiality (e.g. TLS or AEAD) are deferred to future network-facing transports.

---

## 3. Logical Channels

### 3.1. Channel 0: Control Channel (JSON-RPC 2.0)
All control commands, agent tool actions, and lifecycle signals execute over Channel 0 as UTF-8 encoded JSON-RPC 2.0 objects.

#### Standard Messages:
- `JsonRpcRequest<T>`: Standard request with `jsonrpc: "2.0"`, `method`, `params`, and `id` (`RequestId::String` or `RequestId::Number`).
- `JsonRpcResponse<R>`: Standard response with `jsonrpc: "2.0"`, optional `result`, optional `error`, and optional `id` (`id: null` supported for parse errors).
- `JsonRpcNotification<T>`: One-way notification without `id`.
- `guest.handshake`: Announce OS distro, kernel, desktop environment, and active user session.
- `guest.ping`: Low-latency keep-alive round-trip.
- `desktop.capture_frame`: Request a single desktop screenshot.
- `desktop.mouse_click`: Inject left/right/middle click at `(x, y)`.
- `desktop.mouse_move`: Move virtual cursor to `(x, y)`.
- `desktop.mouse_drag`: Drag cursor with button depressed from `(start_x, start_y)` to `(end_x, end_y)`.
- `desktop.key_sequence`: Inject key combinations (e.g. `ctrl+c`, `alt+tab`).
- `desktop.type_text`: Type Unicode text string directly without modifying clipboard.
- `system.exec`: Execute guest CLI process and stream stdout/stderr.
- `stream.start`: Begin continuous video streaming on Channel 1 at target FPS.
- `stream.stop`: Stop continuous video streaming.
- `clipboard.read`: Read guest clipboard text.
- `clipboard.write`: Write guest clipboard text.

### 3.2. Channel 1: Video Stream Channel
High-bandwidth streaming of guest desktop frames.
Each payload begins with a 16-byte Video Stream Header:
```text
Offset 0..4: Sequence Number (u32, Big-Endian)
Offset 4..12: Timestamp Unix Microseconds (u64, Big-Endian)
Offset 12..14: Width in pixels (u16, Big-Endian)
Offset 14..16: Height in pixels (u16, Big-Endian)
Followed by: Encoded image bytes (JPEG / LZ4 BGRA / Raw BGRA per flags)
```

### 3.3. Channel 2: File Transfer Channel
Chunked binary streams for pushing and pulling files between host and guest.
Transmitted in 64 KB chunks:
```text
Offset 0..16: Transfer UUID (16 bytes)
Offset 16..24: Chunk Offset (u64, Big-Endian)
Offset 24..28: Chunk Length (u32, Big-Endian)
Offset 28: Flags (0x01 = EOF / final chunk)
Followed by: Raw file chunk bytes
```

### 3.4. Channel 3: Telemetry & Metrics Channel
Lightweight metrics emitted periodically by the guest daemon:
- CPU utilization percentage
- RAM utilization (used / total MB)
- Active session ID and window manager state

---

## 4. Connection Lifecycle & Error Recovery

1. **Transport Connect:** Host initiates transport connection to guest CID 3 on port 10001 (or local stream endpoint).
2. **Initial Handshake:** Immediately upon connection, guest transmits `guest.handshake` on Channel 0.
3. **Handshake Verification:** Host validates `version == "1.0.0"` and returns `HandshakeResult`.
4. **Heartbeats:** Either party emits Heartbeat frames (`Flags = 0x04`, `PayloadLen = 0`) periodically.
5. **Framing Errors & Decoder Poisoning:** Any framing violation (invalid magic, version, flags, or payload exceeding buffer capacity) marks the incremental decoder as poisoned (`ProtocolError::StreamPoisoned`), enforcing fail-closed connection reset rather than risky speculative re-synchronization.
6. **Cancellation Safety & Fail-Closed Write:** In `FramedTransport`, any interrupted, cancelled, or timed-out `send()` operation immediately trips the fail-closed `SendCancellationGuard`, permanently marking the transport as closed (`is_closed = true`). This prevents subsequent writes from interleaving with an uncompleted, partially transmitted frame and corrupting the stream framing.
7. **Clean Termination:** Graceful close via `Transport::close()` drops the transmitter, notifying the remote receiver with EOF (`None`).
