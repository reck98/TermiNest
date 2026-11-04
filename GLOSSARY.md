# TermiNest Ubiquitous Domain Glossary

This glossary defines the authoritative domain model and ubiquitous vocabulary for **TermiNest**. Every specification, code symbol, documentation artifact, and API schema within TermiNest must use these terms exactly as defined here. Do not use terms like "VM", "Guest", and "Session" interchangeably.

---

## 1. Physical and Host Infrastructure

### Host Machine
The physical or bare-metal computer hardware executing the TermiNest host orchestration stack. Owns the physical CPU, physical RAM, host storage controllers, network interfaces, physical display monitors, and human input devices (physical mouse and keyboard).

### Host Operating System (Host OS)
The native operating system installed directly on the Host Machine (e.g. Windows 11 Pro 64-bit, Ubuntu 24.04 LTS x86_64). Manages physical drivers, user sessions, host security boundaries, and host access control lists.

### Hypervisor Backend (Hypervisor)
The virtualization layer responsible for partitioning hardware, scheduling virtual CPUs (vCPUs), managing guest physical address spaces (GPA to HPA translations), and emulating virtual platform devices.
- On Windows: **Microsoft Hyper-V** (via WMI v2) or **Windows Hypervisor Platform (WHP)** via QEMU.
- On Linux: **KVM (Kernel-based Virtual Machine)** via QEMU.

### Host Orchestrator (`TermiNest Core`)
The central, unprivileged userspace service or runtime executing on the Host OS. Manages VM lifecycles, configuration validation, capability discovery, host-guest transport arbitration, and MCP service routing.

---

## 2. Virtual Infrastructure

### Virtual Machine (VM)
A software-defined execution container instantiated and scheduled by the Hypervisor. Encompasses virtual hardware definitions: virtual CPUs, virtual memory, virtual storage controllers, virtual network interfaces, virtual display adapters, and virtual platform devices.
*Distinction:* A Virtual Machine is a compute instance; it is distinct from the operating system running inside it (the Guest OS) and distinct from user logins inside that OS (Guest Sessions).

### VM Image
A packaged, bootable template or base virtual disk containing an installed operating system or installer payload.
- Formats: QCOW2 (QEMU Copy-on-Write v2/v3), VHDX (Hyper-V Virtual Hard Disk v2), raw ISO, or cloud-init disk image.

### VM Snapshot (Checkpoint)
A point-in-time capture of a Virtual Machine's disk state and optionally its memory/device state. Enables instantaneous state rollbacks.
- Differencing disk checkpoint: Parent read-only base disk with an active read/write child overlay.

### Ephemeral VM
A Virtual Machine configured with disposable execution semantics. Discards all virtual disk modifications upon shutdown, ensuring a clean-slate state for subsequent agent runs.

---

## 3. Guest Environment

### Guest Operating System (Guest OS)
The operating system running *inside* the Virtual Machine on virtual hardware (e.g., Windows 11 Enterprise guest, Ubuntu 24.04 desktop guest). Manages guest processes, virtual display drivers, internal security tokens, and user credentials independently from the Host OS.

### Guest Session (Interactive Desktop Session)
A logged-in interactive user environment inside the Guest OS associated with an active graphical desktop window station (e.g., Windows Session 1 `WinSta0\Default`, Linux X11 display `:0`, or Wayland seat `seat0`).
*Distinction:* A Virtual Machine may be running while having **zero** active Guest Sessions (e.g., system is halted at the Winlogon/GDM logon screen, or user has logged out).

### Guest Helper (`TermiNest Guest Agent`)
The native software suite installed inside the Guest OS responsible for bridging TermiNest host commands to guest OS APIs. Operates across two architectural tiers:
1. **Guest System Daemon:** A background service executing with elevated privileges (Windows Session 0 Service or Linux systemd system unit). Maintains host transport connections, monitors user login events, and reports machine health.
2. **Guest Session Worker:** A process spawned inside the active interactive Guest Session. Performs screen capture, monitors frame updates, and injects synthetic mouse and keyboard input.

---

## 4. Communication and Transports

### Host-Guest Transport
The physical or virtual communication medium bridging the Host Orchestrator and the Guest Helper without relying on an external IP network or physical cabling.
- Primary: **Virtual Sockets (Vsock)** — `AF_VSOCK` on Linux KVM; Hyper-V Sockets (`AF_HYPERV`) on Windows.
- Fallback: Virtio-serial or memory-mapped virtio-fs file exchange.

### Control Channel
A bidirectional, message-oriented stream within the Host-Guest Transport dedicated to low-latency RPC commands, execution requests, tool invocations, and status health checks.

### Data Channel
A dedicated high-bandwidth, streaming channel within the Host-Guest Transport optimized for transmitting bulk binary payloads, specifically compressed video frames, lossless screenshots, and file transfers, without blocking Control Channel RPCs.

---

## 5. Agent Integration & Tool Interfaces

### Autonomous AI Agent (Agent Runtime)
An external autonomous LLM or CLI-based agent system (e.g., Claude Code, OpenAI Codex, Cursor, OpenCode, Amp) that plans and issues commands to achieve a defined technical goal.

### Model Context Protocol Client (MCP Client)
The subsystem inside an Agent Runtime that discovers, negotiates, and invokes tools exposed by an MCP Server over standard JSON-RPC 2.0 protocols.

### TermiNest MCP Server
The stdio or stream-based Model Context Protocol service exposed by TermiNest. Translates standardized agent computer-use requests (`screenshot`, `click`, `type`, etc.) into Host Orchestrator actions directed at a specific Virtual Machine.

### Computer-Use Tool (Action Tool)
A discrete, structured operational primitive exposed to an Agent through MCP or the CLI:
- `screenshot`: Acquires current desktop image in unified coordinate space.
- `click`: Performs single, double, triple, or right-button mouse click at `(x, y)`.
- `move`: Displaces virtual mouse cursor to `(x, y)`.
- `drag`: Executes smooth mouse button depression, trajectory interpolation, and release.
- `scroll`: Emits vertical and horizontal scroll wheel notches at `(x, y)`.
- `type`: Inputs literal text using Unicode character events (no clipboard alteration).
- `key`: Dispatches hardware/virtual key combinations with modifier flags (e.g. `ctrl+c`, `alt+tab`).
- `status`: Reports VM lifecycle state, active session status, and guest helper readiness.

### Unified Coordinate Space
A 2D coordinate system where `(0, 0)` represents the top-left pixel of the Guest Session desktop. Coordinates passed by an Agent to any computer-use tool directly correspond to pixel coordinates in the most recently delivered screenshot, normalized for HiDPI scaling factors.

---

## 6. Visualization & Operations

### Desktop Viewer (`TermiNest Viewer`)
A terminal-based or native graphical window utility running on the Host OS that displays live visual frames of the Guest Session desktop and transmits interactive developer input.
- Terminal Inline Mode: Kitty graphics protocol (shared memory or direct chunks) and iTerm2 inline protocol.
- Native Fallback Window: OS-native desktop window surface rendered directly from the hypervisor framebuffer or frame stream.

### Agent Skill
A structured instruction set and workflow specification (packaged as `SKILL.md`) that guides an AI Agent through the **Observe → Interpret → Act → Verify → Recover** cycle when operating a TermiNest virtual computer.
