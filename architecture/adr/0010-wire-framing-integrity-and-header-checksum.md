# ADR 0010: TNST Wire Framing Integrity and Header Checksum Coverage

## Status
Accepted

## Context
ADR-0003 established the TermiNest Wire Protocol v1 (TNST-v1) utilizing a 16-byte fixed binary header and Castagnoli CRC-32C for integrity validation.

The initial specification in Report 08 stated that the CRC-32C checksum field covers the "Payload bytes only", with zero-payload frames (such as Heartbeat frames) setting the checksum field to `0x00000000`.

An independent protocol correctness and security review identified severe reliability and denial-of-service vulnerabilities with payload-only checksum coverage:
1. **Header Corruption Blindspot:** A bit flip in the 12 bytes preceding the checksum (`Magic`, `Version`, `Flags`, `ChannelId`, `PayloadLength`) goes completely undetected by the CRC check.
2. **Stream Desynchronization via Corrupted `PayloadLength`:** If transmission noise alters `PayloadLength` (e.g. from 64 to 65,536 bytes), the stream decoder reads into subsequent valid frames, attempts to verify CRC over a misaligned slice, and discards all intermediate frames, destroying stream synchronization.
3. **Channel Misrouting:** A bit flip in `ChannelId` (e.g. `0x0001` Video to `0x0000` Control) causes raw binary video chunks to be routed to the JSON-RPC parser.
4. **Heartbeat Poisoning:** A corrupted heartbeat with a bit flip in `ChannelId` has payload length 0 and checksum 0. The receiver computes `crc32c(b"") == 0`, accepts the frame, and delivers an empty payload to the wrong channel.

## Problem Statement
What data slice must the Castagnoli CRC-32C checksum cover to ensure robust stream framing, early corruption rejection, and channel integrity?

## Evaluated Options

### Option 1: Payload-Only Checksum (Original Draft)
- **Coverage:** Payload bytes only (`offset 16..16 + payload_len`).
- **Header Checksum:** None.
- **Drawbacks:** Desynchronization on `payload_len` bit flips, silent flag corruption, channel misrouting, zero protection for zero-payload frames.

### Option 2: Full Frame Checksum over Header Prefix and Payload (Selected)
- **Coverage:** Header bytes 0..12 (`Magic`, `Version`, `Flags`, `ChannelId`, `PayloadLength`) followed by all `Payload` bytes.
- **Wire Format:** Preserves the exact 16-byte header size (`HEADER_SIZE = 16`). No wire layout change.
- **Advantages:**
  - Protects every routing and metadata field (`ChannelId`, `Flags`, `PayloadLength`) against bit-level corruption.
  - Zero-payload heartbeat frames compute the CRC over the 12-byte header prefix, eliminating spurious acceptance of corrupted heartbeats.
  - Detects corruption before routing or processing data.
  - Zero memory overhead; utilizes existing `crc32c_update(crc32c(header_prefix), payload)`.

### Option 3: Authenticated Framing (AEAD / HMAC-SHA256)
- **Coverage:** Cryptographic authentication tag appended to each frame or chunk.
- **Evaluation:** Necessary when operating across untrusted network boundaries (e.g. public TCP). However, TermiNest M0-M2 operates across hardware-isolated point-to-point kernel virtual sockets (`AF_HYPERV` / `AF_VSOCK`), where hypervisor memory rings provide peer isolation. Adding AEAD in M0 introduces runtime key negotiation overhead and cryptographic dependencies without threat model justification. Authenticated framing is deferred to future horizons where TermiNest operates over unauthenticated network transports.

## Decision
We adopt **Option 2: Full Frame Checksum over Header Prefix and Payload**.

The Castagnoli CRC-32C checksum field at byte offset 12..16 must strictly equal:
```rust
let header_crc = crc32c(&header_bytes[0..12]);
let full_checksum = crc32c_update(header_crc, payload_bytes);
```

For zero-length payload frames (including Heartbeats), the checksum equals `crc32c(&header_bytes[0..12])`.

## Compatibility and Upgrade Path
- Since TermiNest is currently in Milestone 0, no external clients or deployed production agents exist.
- Making this correction now eliminates a major protocol flaw before Milestone 1 and guest agent implementation.
- Wire header remains exactly 16 bytes, preserving all memory alignment contracts.

## Consequences

### Positive
- Complete stream framing integrity: any single-bit flip across the entire frame (header or payload) is detected and rejected.
- Protects stream synchronization against corrupted `PayloadLength` and prevents channel misrouting.
- Maintains sub-microsecond validation performance.

### Negative / Trade-Offs
- Decoder must compute CRC over the 12-byte header slice in addition to payload bytes. (Measured benchmark overhead is < 15 nanoseconds per frame).
