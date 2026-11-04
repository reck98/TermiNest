# ADR 0005: Screen Capture and Input Injection Pipelines

## Status
Accepted

## Context
AI agents interact with virtual environments using computer-use primitives: capturing screenshots to analyze state, and injecting mouse clicks, cursor movements, and keystrokes to take action.

`terminal-mac` relies on macOS-exclusive frameworks: `ScreenCaptureKit` for hardware-accelerated video/screen capture and `CGEvent` for input synthesis. Neither API exists on Windows or Linux.

On Windows and Linux, display architectures vary substantially:
- Windows offers GDI (`BitBlt`), DXGI Desktop Duplication (`IDXGIOutputDuplication`), and Windows Graphics Capture (WGC / `Windows.Graphics.Capture`). Input injection uses `SendInput`.
- Linux offers X11 (XShm / XTest), Headless Xvfb, Wayland compositors (PipeWire via xdg-desktop-portal, `libei` emulated input), or headless micro-compositors (`cage` / `sway --headless`).

## Problem Statement
What APIs and architectures should TermiNest use for high-performance screen capture and reliable input injection across Windows and Linux guest environments?

## Decision
We define an explicit, platform-specific capture and injection strategy encapsulated behind the `GuestWorker` implementation:

### 1. Windows Guest Pipeline
- **Screen Capture:**
  - **Primary:** **DirectX Graphics Infrastructure (DXGI) Desktop Duplication API** (`IDXGIOutputDuplication`). Provides zero-copy GPU-accelerated access to the desktop surface, delivering dirty-rect tracking and sub-5ms capture latencies at 1080p.
  - **Fallback:** **Windows Graphics Capture (WGC)** API (`Windows.Graphics.Capture`) if running under newer DWM composition pipelines where DXGI duplication is constrained.
- **Input Injection:**
  - Native Win32 **`SendInput` API**.
  - Mouse coordinates are mapped to normalized absolute coordinates (`0..65535`) using `MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_MOVE`.
  - Keystrokes are injected using hardware scan codes (`KEYEVENTF_SCANCODE`) to ensure compatibility with games, terminal emulators, and specialized GUI applications.

### 2. Linux Guest Pipeline
- **Headless Automation Mode (Recommended for Server / Agent Workloads):**
  - Uses an isolated X11 virtual framebuffer (**`Xvfb`**) or a headless Wayland compositor (**`cage`** or **`sway --headless`**).
  - Capture is achieved via **XShm** (MIT-SHM shared memory extension) with sub-2ms frame retrieval.
  - Input injection uses the **XTest** extension (`XTestFakeKeyEvent`, `XTestFakeMotionEvent`).
- **Standard Desktop Mode (Ubuntu / Fedora Desktop):**
  - **Capture:** **PipeWire Screencast** via `org.freedesktop.portal.ScreenCast`. Streams frames via DMA-BUF or shared memory buffers.
  - **Input:** Linux kernel **`/dev/uinput`** virtual input device created by the Tier 1 daemon, or **`libei`** (Emulated Input protocol) for compositors supporting Wayland input emulation.

## Rationale
- **Performance:** DXGI Desktop Duplication and XShm bypass GDI and network round-trips, copying framebuffers directly in memory.
- **Reliability:** Hardware scan codes prevent dropped keys in terminal emulators and complex text editors.
- **Broad Compatibility:** Supporting both Xvfb/X11 and modern Wayland/PipeWire ensures TermiNest functions across legacy enterprise Linux distributions and bleeding-edge desktops.

## Consequences

### Positive
- Frame capture latency under 5ms on both Windows and Linux guests.
- Highly accurate input injection that handles modifier keys (Shift, Ctrl, Alt, Meta), dead keys, and Unicode characters.
- Headless execution mode enables massive scalability for automated agent evaluation farms.

### Negative / Trade-Offs
- PipeWire and Wayland portal capture require D-Bus session bus communication inside the Linux guest.
- On Windows, DXGI Desktop Duplication fails when the display enters a Secure Desktop state (UAC prompt), requiring fallback handling or UAC suppression in automation templates.
