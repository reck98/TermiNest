# Report 04: Product & System Requirements Specification

**Document Status:** Complete & Audited  
**Author:** Principal Systems Architect & Engineering Team  
**Date:** 2026-11-05  
**Scope:** Functional and Non-Functional Requirements for TermiNest, distinguishing MVP from Future Horizons

---

## 1. System Vision & Objective

**TermiNest** provides an isolated, cross-platform virtual computer execution infrastructure that autonomous CLI-based AI agents can observe and control through standardized computer-use interfaces, without manipulating or exposing the developer's physical host desktop, clipboard, or local credentials.

---

## 2. Requirements Decomposition Matrix (MVP vs. Future)

The requirements are divided into:
- **Phase I (MVP):** Core cross-platform capability focused on immediate developer and agent utility with minimal operational surface.
- **Phase II (Post-MVP / Future Horizons):** Enterprise isolation, multi-tenancy, accessibility-tree introspection, and advanced fleet operations.

| Category | Requirement ID | Requirement Name | Scope | Description & Acceptance Criteria |
|---|---|---|---|---|
| **Installation** | `REQ-INST-01` | Non-Root Host Installation | **MVP** | Single user-level installer (`install.sh` / `install.ps1`) deploying CLI, orchestrator, and default guest assets to user space (`~/.terminest/` or `%LOCALAPPDATA%\TermiNest`). |
| | `REQ-INST-02` | Automated Environment Diagnostics (`doctor`) | **MVP** | CLI command `terminest doctor` verifying hardware virtualization (VT-x/AMD-V), hypervisor availability (Hyper-V / KVM / WHP), kernel permissions, and terminal graphics support. Outputs structured JSON. |
| | `REQ-INST-03` | Clean Uninstallation | **MVP** | Safe uninstall command removing binaries, runtime state, and optionally data directories, without leaving orphaned services or registry entries. |
| **VM Lifecycle** | `REQ-VM-01` | Automated Provisioning (`create`) | **MVP** | Provision a VM from standard cloud images or base ISOs with automated unattended configuration (`cloud-init` on Linux, `autounattend.xml` on Windows). Zero human clicks required for supported base images. |
| | `REQ-VM-02` | Lifecycle Control (`start`, `stop`, `pause`, `resume`) | **MVP** | Start VMs as detached background daemons. Support clean shutdown (`stop`), forced power-off (`stop --force`), pause, and resume. |
| | `REQ-VM-03` | State & Inventory Inspection (`ls`, `status`) | **MVP** | List all configured VMs with CPU, RAM, disk usage, power state, and guest session health in human-readable and `--json` machine formats. |
| | `REQ-VM-04` | Confirmed Deletion (`destroy`) | **MVP** | Explicit `terminest destroy <name> --confirm` deleting virtual disks and metadata, guarded against accidental data loss. |
| | `REQ-VM-05` | Ephemeral / Disposable Mode (`--ephemeral`) | **MVP** | Run a VM backed by a temporary Copy-on-Write disk layer; all changes are discarded automatically on shutdown. |
| | `REQ-VM-06` | Checkpoints & Rollback (`snapshot`) | **Future** | Create named CoW snapshots and roll back state instantaneously in < 3 seconds. |
| **Computer Use** | `REQ-CU-01` | Top-Left Unified Screen Capture (`screenshot`) | **MVP** | Capture current guest session desktop in lossless PNG or compressed JPEG. Coordinate space must equal screenshot pixels. |
| | `REQ-CU-02` | Synthetic Mouse Control (`click`, `move`, `drag`, `scroll`) | **MVP** | Execute single, double, triple, and right-button clicks; smooth dragging; and calibrated wheel scrolling in guest point coordinates. |
| | `REQ-CU-03` | Unicode Keyboard Injection (`type`) | **MVP** | Type arbitrary Unicode text strings directly as keyboard event streams without modifying or reading host/guest clipboards. |
| | `REQ-CU-04` | Virtual Key & Chord Press (`key`) | **MVP** | Execute system key chords (`ctrl+c`, `alt+tab`, `enter`, `escape`, `super/win`) with exact modifier key press and release ordering. |
| | `REQ-CU-05` | Combined Action-Capture Turn (`screenshot: true`) | **MVP** | Action tools optionally return a post-action screenshot after a short animation stabilization interval (default 300–400ms). |
| | `REQ-CU-06` | Session Awareness (`session_status`) | **MVP** | Detect and report active guest session state: `logged_in`, `locked`, `logon_screen`, `display_asleep`, `helper_missing`. |
| | `REQ-CU-07` | Accessibility Tree Snapshot (`accessibility`) | **Future** | Extract structured accessibility trees (UI Automation on Windows, AT-SPI on Linux) returning control roles, names, and bounding boxes. |
| **Agent / MCP** | `REQ-MCP-01` | Stdio MCP Protocol Server (`terminest mcp`) | **MVP** | Standalone JSON-RPC 2.0 MCP server over standard input/output conforming to MCP specification 2025-06-18+. |
| | `REQ-MCP-02` | Autonomous Observe-Act-Verify Skill | **MVP** | Curated agent skill (`SKILL.md`) teaching agents to inspect screenshots, form hypotheses, execute actions, and verify outcomes. |
| | `REQ-MCP-03` | Multi-Client Session Isolation | **Future** | Central broker allowing multiple independent agents to connect to distinct VMs concurrently. |
| **Viewer** | `REQ-VIEW-01` | Terminal Inline Desktop (`terminest view`) | **MVP** | Live terminal streaming for Kitty graphics protocol (via shared memory and direct chunks) and iTerm2 inline protocol. |
| | `REQ-VIEW-02` | Native Window Fallback | **MVP** | Native host desktop window fallback for terminals lacking graphics protocol support. |
| | `REQ-VIEW-03` | Push-Based Zero-Poll Frame Pipeline | **MVP** | Guest captures frames only when desktop changes occur; drops stale frames during client backpressure. |
| **Operations** | `REQ-OPS-01` | Zero-Network Host-Guest Transport (Vsock) | **MVP** | In-memory socket transport over Vsock (`AF_VSOCK` / `AF_HYPERV`) without IP networking, DHCP, or host port exposure. |
| | `REQ-OPS-02` | Network Isolation Tiers | **MVP** | Support `--network offline` (no virtual NIC), `--network isolated` (guest-only private switch), and `--network nat` (outbound-only). |
| | `REQ-OPS-03` | Structured Logging & Correlation IDs | **MVP** | Central structured JSON logs (`runtime.log`) with per-request UUIDs tracing MCP tool calls to guest input events. |
| | `REQ-OPS-04` | Automatic Guest Agent Updates | **MVP** | Host automatically delivers updated guest worker binaries via host-guest transport and executes seamless in-place restart. |

---

## 3. Non-Functional Requirements (NFRs)

### Performance & Latency
- **NFR-PERF-01 (Command Round-Trip):** Single-action execution (`click`, `key`, `type`) over Vsock must complete with round-trip latency < 15ms (excluding artificial animation sleeps).
- **NFR-PERF-02 (Screenshot Acquisition):** Full desktop capture (1920×1080) and encoding must complete in < 50ms inside the guest.
- **NFR-PERF-03 (Viewer Streaming):** Live viewer must sustain 30 fps under active UI changes with < 80ms glass-to-glass latency on supported terminals.
- **NFR-PERF-04 (Idle Resource Consumption):** When no agent or viewer is active, the host orchestrator and guest helper must consume < 1% CPU utilization combined.

### Reliability & Fault Tolerance
- **NFR-REL-01 (Process Crash Isolation):** A crash of the TermiNest Viewer or an MCP client must never terminate the running Virtual Machine.
- **NFR-REL-02 (Worker Recovery):** If the interactive Guest Session Worker crashes, the Guest System Daemon must detect worker termination and automatically restart it within 2 seconds.
- **NFR-REL-03 (Lockout Grace Period):** Timeouts must explicitly identify whether an unanswered request was delivered to an inactive session or lost in transit (`pickedUp: false`).

### Security & Isolation
- **NFR-SEC-01 (Host Desktop Protection):** Under no circumstance shall TermiNest inject input events into the host OS desktop or capture host display monitors.
- **NFR-SEC-02 (Credential Boundary):** Host credentials, environment variables, SSH keys, and host file paths must never be injected into guest memory or filesystems unless explicitly attached by the user.
- **NFR-SEC-03 (File Exchange Hardening):** Any fallback file-sharing transport must reject symlinks, directory links, hardlinks (`st_nlink == 1`), and path traversals (`O_NOFOLLOW`, `openat`).

---

*End of Report 04. Proceed to Target System Architecture & Component Design.*
