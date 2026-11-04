# Architecture Decision Records (ADRs)

This directory documents the foundational architectural decisions for **TermiNest**.

Every record captures the context, problem statement, alternatives considered, decision made, rationale, and consequences (both positive and negative) for a major system design choice.

## Status Conventions
- **Proposed:** Under active architectural review.
- **Accepted:** Approved and binding for the engineering roadmap.
- **Superseded:** Replaced by a subsequent ADR (referenced in header).
- **Rejected:** Considered and explicitly discarded.

---

## Decision Index

| ADR ID | Title | Status | Impact Area |
|---|---|---|---|
| [ADR-0001](0001-core-implementation-language.md) | Core Implementation Language (Rust) | Accepted | Core, CLI, MCP, Guest Helper |
| [ADR-0002](0002-hypervisor-and-virtualization-backends.md) | Hypervisor & Virtualization Backend Selection | Accepted | Host Orchestrator, Hypervisor SPI |
| [ADR-0003](0003-host-guest-ipc-and-transport.md) | Point-to-Point Vsock Transport over Mailbox | Accepted | Transport, Wire Protocol, IPC |
| [ADR-0004](0004-guest-helper-architecture-and-session-model.md) | Two-Tier Guest Helper Architecture | Accepted | Guest Daemon, Guest Worker |
| [ADR-0005](0005-screen-capture-and-input-injection.md) | Screen Capture & Input Injection Pipelines | Accepted | Guest Graphics, Automation |
| [ADR-0006](0006-mcp-server-architecture.md) | MCP Server Stdio JSON-RPC Architecture | Accepted | Agent Interface, MCP |
| [ADR-0007](0007-desktop-viewer-and-terminal-graphics.md) | Terminal Desktop Viewer & Graphics Protocols | Accepted | CLI Viewer, Terminal Protocols |
| [ADR-0008](0008-security-boundaries-and-network-isolation.md) | Security Boundaries & Network Isolation Tiers | Accepted | Security, Virtual Networking |
| [ADR-0009](0009-vm-storage-format-and-state-management.md) | VM Storage Formats & Ephemeral State Management | Accepted | Storage, Lifecycle, Snapshots |
| [ADR-0010](0010-wire-framing-integrity-and-header-checksum.md) | TNST Wire Framing Integrity & Header Checksum | Accepted | Wire Protocol, Framing, Security |
