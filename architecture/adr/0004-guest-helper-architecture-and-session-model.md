# ADR 0004: Guest Helper Architecture and Session Model

## Status
Accepted

## Context
Operating systems isolate system-level background processes from interactive graphical user sessions for security and stability:
- **Windows:** Since Windows Vista, all Windows Services execute in **Session 0**, which has no display surface, no active desktop window station (`WinSta0`), and is forbidden from injecting inputs via `SendInput` or capturing screens via Direct3D/DXGI. The interactive logged-in user resides in **Session 1** (or higher).
- **Linux:** System daemons (managed by systemd PID 1) run as `root` without an X11 `$DISPLAY` or Wayland `$WAYLAND_DISPLAY` environment variable or seat access. Furthermore, Wayland security models explicitly prevent background daemons from capturing other applications or injecting synthetic inputs.

A monolithic single-process guest agent running as a service cannot capture or control the user's desktop. Conversely, a monolithic agent running only in the user session cannot survive user logouts, manage OS-level network/disk reconfiguration, or initialize early in the boot cycle.

## Problem Statement
How should the TermiNest Guest Helper be structured to provide reliable early-boot connectivity while maintaining full capability to observe and control the interactive user desktop?

## Decision
We architect the Guest Helper as a **Two-Tier Privilege-Separated System**:

```
+-------------------------------------------------------------------------+
| Guest Operating System                                                  |
|                                                                         |
|  [Tier 1: System Daemon] (Session 0 / root)                             |
|  - Runs as NT AUTHORITY\SYSTEM (Win) or root systemd service (Linux)   |
|  - Binds the guest-side Vsock endpoint early at boot                   |
|  - Monitors active user login sessions via WTS / systemd-logind         |
|  - Spawns and supervises Tier 2 Session Worker                          |
|  - Handles system-level lifecycle (reboot, shutdown, IP config)         |
|                                                                         |
|         | Authenticated Local IPC (Local Named Pipe / Unix Socket)      |
|         v                                                               |
|                                                                         |
|  [Tier 2: Session Worker] (Session 1+ / Active User Desktop)            |
|  - Spawned into user session via CreateProcessAsUserW / systemd user    |
|  - Runs under the identity of the logged-in desktop user                |
|  - Captures display via DXGI / WGC (Win) or PipeWire / XShm (Linux)     |
|  - Injects inputs via SendInput (Win) or /dev/uinput / libei (Linux)   |
|  - Manages session clipboard and active window metadata                 |
+-------------------------------------------------------------------------+
```

1. **Tier 1 (System Daemon - `terminest-guest-service`):**
   - Starts automatically on OS boot.
   - Binds to the hypervisor Vsock device and handles connection handshakes with the host orchestrator.
   - Detects when an interactive user logs in (using `WTSRegisterSessionNotification` on Windows; `sd_login_monitor` on Linux).
   - Once a session is active, spawns the Tier 2 Worker inside that session using `CreateProcessAsUserW` (Windows) or activates a user systemd service (Linux).
2. **Tier 2 (Session Worker - `terminest-guest-worker`):**
   - Connects to Tier 1 via a private local pipe/socket.
   - Performs all graphical desktop operations: screen frame capture, coordinate-bounded mouse clicks, synthetic keystrokes, and clipboard synchronization.
   - Automatically shuts down when the user session logs off or locks.

## Rationale
- **Bypasses Session 0 Isolation:** Adheres strictly to the Windows architecture instead of attempting fragile hacks (e.g. interactive service flags, which were completely removed by Microsoft).
- **Early-Boot Connectivity:** The host orchestrator can detect VM readiness, query OS boot progress, and receive guest diagnostics long before a user desktop finishes logging in.
- **Graceful Session Transitions:** If the user logs out or the desktop restarts, the Tier 1 daemon remains alive, reconnects to the host, and automatically launches a new Tier 2 worker when a session reappears.

## Consequences

### Positive
- Robust, crash-resilient guest supervision.
- Compliant with Windows and Linux security and desktop isolation boundaries.
- Clean separation between privileged system management and unprivileged GUI interaction.

### Negative / Trade-Offs
- Increased complexity: two distinct binaries must be built, packaged, and coordinated inside the guest.
- Local inter-tier IPC protocol must be maintained and secured against local privilege escalation.
