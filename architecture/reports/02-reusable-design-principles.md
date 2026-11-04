# Report 02: Architectural Extraction & Reusable Design Principles

**Document Status:** Complete & Verified  
**Author:** Principal Systems Architect  
**Date:** 2026-11-05  
**Subject:** Extraction of platform-independent principles, replaceable implementations, and architectural gaps from `terminal-mac` for TermiNest

---

## 1. Architectural Classification Matrix

To design a robust cross-platform system for Windows and Linux hosts, we systematically categorize all subsystems of `terminal-mac` across five architectural categories:

| Subsystem | Reference Implementation (`terminal-mac`) | Categorization | Architectural Disposition for TermiNest |
|---|---|---|---|
| **Agent Interface (MCP)** | Stdio JSON-RPC 2.0 with tool set (`screenshot`, `click`, `drag`, `scroll`, `type`, `key`, `status`) | **Reusable Architecture** | Preserve and standardize across all platforms. Decouple entirely from virtualization backends. |
| **Observe-Act-Verify Loop** | Agent skill (`SKILL.md`) guiding agents to inspect screenshots before and after actions | **Reusable Architecture** | Preserve as the canonical agent interaction pattern. |
| **Combined Act-and-Screenshot** | Parameter `screenshot: true` returning tool result + fresh post-action JPEG in one turn | **Reusable Architecture** | Preserve. Significantly reduces LLM context round-trips and latency. |
| **Top-Left Coordinate System** | Display coordinates mapped 1:1 with screenshot pixels (points) | **Reusable Architecture** | Preserve as universal coordinate space. |
| **Out-of-Band Input Stream** | Typing via direct Unicode events rather than host/guest clipboard | **Reusable Architecture** | Preserve. Clipboard pollution must be strictly avoided on Windows and Linux. |
| **VM Hypervisor Backend** | Apple `Virtualization.framework` (`VZVirtualMachine`, `VZMacOSInstaller`) | **Replaceable Implementation** | Replace with **Hyper-V** and **QEMU/KVM** backends via a unified `Hypervisor` abstraction. |
| **Host-Guest IPC Transport** | virtio-fs atomic file exchange (`Mailbox.swift`) | **Replaceable Implementation** | Replace with **Vsock** (`AF_VSOCK` on Linux, Hyper-V Sockets on Windows). File-based transport remains a fallback only. |
| **Screen Capture Engine** | Apple `ScreenCaptureKit` (`SCStream`, `SCScreenshotManager`) | **Replaceable Implementation** | Replace with **Windows Graphics Capture (WGC)** / **Desktop Duplication API** on Windows, and **PipeWire / Portal** on Linux. |
| **Input Injection Engine** | `CGEvent.post(tap: .cghidEventTap)` via Quartz Event Services | **Replaceable Implementation** | Replace with Win32 `SendInput` on Windows, and `libei` / `uinput` on Linux. |
| **Terminal Graphics Pipeline** | Kitty shared memory (`/tmac.<pid>.<seq>`), Kitty direct PNG, iTerm2 inline | **Replaceable Implementation** | Retain Kitty protocol for supported terminals, add Sixel / native fallback windows for Windows Terminal & legacy Linux terminals. |
| **Guest Attestation Guard** | `sysctl hw.model` string prefix check (`VirtualMac*`) | **Replaceable Implementation** | Replace with platform hypervisor checks (e.g. `CPUID` hypervisor leaf, DMI system vendor check). |
| **Guest Session Management** | macOS GUI login item (`SMAppService`) | **Platform-Dependent Behaviour** | Windows requires Service + Interactive Session worker; Linux requires systemd user units. |
| **Display Scaling (HiDPI)** | `pixelsPerInch = 220` forcing macOS 2x Retina mode | **Platform-Dependent Behaviour** | Windows uses Per-Monitor DPI scaling; Linux Wayland uses fractional scaling protocols. |
| **Headless Operation** | Not supported (macOS WindowServer requires active virtual display) | **Missing Capability** | Must be architected as a core capability for Linux (Xvfb / headless Wayland compositor). |
| **Automated Guest Provisioning**| Manual (Human must complete Apple Setup Assistant and click TCC buttons) | **Missing Capability** | Implement unattended installation via `unattend.xml` (Windows) and `cloud-init` / `kickstart` (Linux). |
| **Snapshot & Rollback** | Not implemented (explicitly documented as missing) | **Missing Capability** | Core requirement for TermiNest: QCOW2 internal/external snapshots, VHDX checkpoints. |
| **Destructive Lifecycle Controls**| No `delete` or `clean` command; manual directory removal required | **Missing Capability** | Add explicit, confirmed `terminest destroy <vm>` commands with audit logging. |
| **Multi-Client Concurrency** | Single client assumed; race conditions on mailbox reads | **Missing Capability** | Introduce centralized connection broker supporting multiple agent sessions. |

---

## 2. Preserved vs. Redesigned Decisions

### Decisions to Preserve in TermiNest

1. **Strict Decoupling of Agent Desktop from Host Desktop:**
   - The agent never controls the developer’s mouse pointer, keyboard focus, or clipboard.
   - Screen capture and input injection take place exclusively within the guest environment.
2. **Unified Computer-Use Coordinate Space:**
   - Coordinates provided to `click`, `move`, `drag`, and `scroll` must always match the pixel space of the returned screenshot.
   - HiDPI scaling transformations must be handled transparently by the guest helper before injection.
3. **Unicode Keyboard Emulation (No Clipboard Dependency):**
   - Text entry must never overwrite the host or guest system clipboard. Key events must be delivered as discrete character inputs or virtual scan codes.
4. **Resilient Stdio MCP Architecture:**
   - The MCP server must remain lightweight, decoupled from hypervisor drivers, and capable of operating across standard stdio streams for Codex, Claude Code, Cursor, and OpenCode.
5. **Zero-Poll Frame Transmission (Push over Pull):**
   - Screen frames must only be delivered when the guest screen actually changes, with configurable max FPS (default 30).

---

### Decisions to Redesign in TermiNest

#### 1. Replace virtio-fs Mailbox with Socket Transport (Vsock)
- **Problem in `terminal-mac`:** The shared-folder mailbox writes every request, reply, and screenshot to disk (or memory-backed virtio-fs). Polling every 2–25ms wastes host/guest CPU cycles, generates disk churn, and introduces unacceptable jitter for interactive agents.
- **TermiNest Solution:** Use **Vsock** (`AF_VSOCK` on Linux/KVM, Hyper-V Sockets on Windows).
  - Vsock provides direct, in-memory, high-bandwidth streaming between host and guest without network stack configuration, IP allocation, or filesystem disk writes.
  - Latency drops from ~15ms to sub-millisecond (<0.5ms).
  - Enables separate multiplexed channels for low-latency command RPC (control channel) and binary screenshot streaming (data channel).

#### 2. Replace Manual Setup with Unattended Guest Provisioning
- **Problem in `terminal-mac`:** Users must manually click through Setup Assistant, create an account, open Finder, launch the helper, and manually click security approval dialogs. This breaks autonomous agent automation and CI/CD pipelines.
- **TermiNest Solution:**
  - **Windows Guests:** Automated unattended setup using `autounattend.xml` injecting an admin user, enabling auto-logon, and installing the TermiNest Guest Agent via FirstLogonCommands.
  - **Linux Guests:** Automated provisioning using `cloud-init` (NoCloud datasource) or preseed/kickstart configurations.
  - Enables true `terminest create <name> --unattended` execution from clean ISOs or cloud base images.

#### 3. Redesign Guest Agent Architecture for Session Isolation
- **Problem in `terminal-mac`:** The helper runs only as an interactive user-space application. If the guest locks, sleeps, or reboots to the login screen, all communication breaks with a `MailboxTimeout`.
- **TermiNest Solution:** Two-tier guest architecture:
  1. **System Daemon / Service (Session 0 / systemd):** Starts automatically on boot, manages Vsock communication with the host, monitors guest health, handles machine reboot/shutdown, and detects active user sessions.
  2. **Session Worker (Interactive User Session):** Launched inside the active logged-in graphical desktop, handles screen capture and input injection, and communicates locally with the system daemon via named pipes or local domain sockets.
  - If the user logs out or the screen locks, the system daemon remains alive and explicitly reports the exact session state back to the agent ("session locked", "at logon screen", "display asleep").

#### 4. Add Snapshot, Checkpoint, and Disposable VM Capabilities
- **Problem in `terminal-mac`:** No rollback or snapshot mechanisms exist. If an agent ruins guest configuration, the entire multi-gigabyte VM must be manually deleted and reinstalled.
- **TermiNest Solution:**
  - CoW (Copy-on-Write) disk images: QCOW2 backing chains for QEMU/KVM, and VHDX Differencing Disks for Hyper-V.
  - Commands: `terminest snapshot <name> <snap>`, `terminest rollback <name> <snap>`, and `--ephemeral` flag to discard all changes upon VM stop.

---

*End of Report 02. Proceed to Report 03 for platform feasibility analysis across Windows and Linux.*
