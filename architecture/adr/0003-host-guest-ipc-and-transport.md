# ADR 0003: Host-Guest IPC and Transport Mechanism

## Status
Accepted

## Context
The reference implementation (`terminal-mac`) uses a Virtio-FS shared folder mailbox for host-guest communication. The host and guest exchange JSON commands and responses by writing temporary files to a mounted directory, renaming them atomically, and polling the directory every 2 to 25 milliseconds. 

This approach introduces severe drawbacks:
1. High latency jitter (up to 25-50ms round-trip).
2. Continuous disk and page cache churn.
3. Path traversal vulnerabilities across the shared folder boundary.
4. Total unsuitability for high-bandwidth binary screen streaming (30-80 MB/s).

## Problem Statement
What transport mechanism should TermiNest use for high-speed, secure, and bidirectional communication between the Host Orchestrator and Guest Helper?

## Decision
We replace the shared-folder mailbox entirely with **kernel-level Point-to-Point Vsock streaming**:
- **Windows Host:** Hyper-V Sockets (`AF_HYPERV`, socket family 34).
- **Linux Host:** Linux Vsock (`AF_VSOCK`, socket family 40, `/dev/vhost-vsock`).

Over this Vsock transport, TermiNest implements the **TermiNest Wire Protocol v1**, a multiplexed binary protocol featuring:
- A fixed 16-byte header with a magic prefix (`0x544E5354` = `"TNST"`), protocol version, channel ID, payload length, and CRC-32C checksum.
- **Channel 0 (Control):** Bidirectional, low-latency JSON-RPC 2.0 messages (computer-use commands, lifecycle events, heartbeats).
- **Channel 1 (Display Stream):** High-throughput binary frame streaming (compressed JPEG/LZ4 desktop captures with timing metadata).
- **Channel 2 (File Stream):** Chunked, authenticated file transfer streams.

## Rationale
1. **Sub-Millisecond Latency:** Vsock operates over shared-memory rings managed directly by the hypervisor kernel module. Round-trip command latency drops from 25ms to < 0.2ms.
2. **Zero Disk I/O:** Eliminates all disk writes, temporary files, and filesystem polling loops.
3. **True Network Air-Gap:** Vsock requires zero TCP/IP networking, zero IP addresses, and zero virtual network interface cards (NICs). The VM can be completely air-gapped from local and external networks while maintaining full agent control.
4. **Multiplexed Data & Control:** Allows continuous screen streaming on Channel 1 without blocking or delaying urgent keyboard/mouse commands on Channel 0.

## Consequences

### Positive
- Over 100x improvement in command latency and responsiveness.
- Enables smooth 30 FPS terminal desktop viewing over the same connection.
- Greatly reduced security attack surface (no shared filesystem vulnerabilities).
- Guaranteed packet delivery and backpressure handled by standard socket buffers.

### Negative / Trade-Offs
- Requires host kernel support for Vsock (`vhost_vsock` kernel module on Linux; Hyper-V integration services on Windows).
- Requires guest OS drivers for Virtio-Vsock (standard in all modern Linux kernels >= 4.8; provided by Hyper-V Integration Services in Windows).
