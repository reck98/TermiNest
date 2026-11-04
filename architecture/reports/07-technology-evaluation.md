# Architecture Report 07: Technology Stack & Trade-Off Evaluation

**Status:** Approved Architectural Specification  
**Version:** 1.0.0  
**Date:** November 2026  
**System:** TermiNest Cross-Platform Virtual Computer Infrastructure  
**Author:** Staff Systems Engineer & Infrastructure Architect  

---

## 1. Executive Summary & Evaluation Framework

Building TermiNest requires choosing foundational technologies for six critical subsystems:
1. **Core Orchestrator, CLI, & MCP Server Runtime**
2. **Virtualization Management Interfaces (Windows & Linux)**
3. **Host-Guest Communication Transport**
4. **Guest Helper Agent Implementation Language & Framework**
5. **Frame Compression & Protocol Serialization**
6. **Desktop Viewer & Terminal Graphics Protocols**

### Evaluation Criteria & Weighting

Every candidate technology is evaluated against five engineering dimensions:
- **Portability & Cross-Platform Ergonomics (Weight: 20%):** Does the technology provide clean, first-class abstractions across both Windows (Win32/NT) and Linux (POSIX/Linux kernel) without leaking platform quirks?
- **Performance & Latency (Weight: 25%):** Round-trip command execution time, streaming throughput for 10-30 FPS screen capture, memory footprint, and instantaneous CLI startup (< 50ms).
- **Safety, Reliability, & Concurrency (Weight: 25%):** Memory safety without garbage collection pauses, type-safe IPC framing, deterministic asynchronous I/O, and robust crash recovery.
- **Dependency Footprint & Distribution (Weight: 15%):** Self-contained, zero-dependency static binaries. The tool must run immediately on a clean developer machine without pre-installing runtimes (e.g., Python, Node.js, .NET CLR).
- **Ecosystem & Tooling Maturity (Weight: 15%):** Quality of low-level systems libraries (Vsock, COM/WMI, Win32, X11, Wayland, QMP, LZ4, JPEG, JSON-RPC).

---

## 2. Core Orchestrator, CLI, & MCP Server Language

### 2.1. Candidate Comparison

| Dimension | Rust | Go | C++ (C++20) | C# (.NET 9 Native AOT) |
|---|---|---|---|---|
| **Binary Model** | Single static native binary (`.exe` / ELF). Zero runtime dependencies. | Single static native binary (`.exe` / ELF). Embedded runtime/GC. | Single native binary. Requires MSVC C++ runtime DLLs or static CRT. | Single native binary via Native AOT. Requires platform CRT. |
| **Startup Overhead** | **< 10ms** (Instantaneous) | ~20-30ms (GC init) | **< 10ms** (Instantaneous) | ~30-50ms (AOT init) |
| **Memory Footprint** | **~8-15 MB** RSS idle | ~25-45 MB RSS idle | ~10-20 MB RSS idle | ~35-60 MB RSS idle |
| **Memory Safety** | Compile-time ownership, borrow checker. Zero data races. | Garbage collected. Memory safe, but prone to concurrent race conditions. | Manual memory management. Susceptible to buffer overflows and UAF. | Managed memory / GC. Memory safe in safe blocks. |
| **Windows Systems APIs** | Superb (`windows` and `windows-sys` crates by Microsoft). | Adequate via `golang.org/x/sys/windows`, but COM/WMI is clunky. | Native Win32 / COM / WMI first-class support. | First-class native Win32 / WMI via managed wrappers. |
| **Linux Systems APIs** | Superb (`nix`, `libc`, `tokio-vsock`). | Good (`golang.org/x/sys/unix`). | Native POSIX / Linux libc first-class. | Adequate via P/Invoke. |
| **Async Concurrency** | Zero-cost `async/await` (`tokio`), work-stealing scheduler. | Goroutines (`go func`), runtime M:N scheduler. | Coroutines (C++20), but fragmented async ecosystems. | `Task` / `ValueTask` (`async/await`), thread pool. |
| **Image & Frame Processing**| High-performance SIMD crates (`image`, `turbojpeg`, `lz4_flex`). | Moderate (`image` package in stdlib is CPU-bound; cgo needed for SIMD). | High performance (OpenCV, libjpeg-turbo via C bindings). | Good (`ImageSharp`), but GC pressure on raw frame arrays. |

### 2.2. Scoring Matrix

| Criteria (Weight) | Rust | Go | C++ | C# (.NET 9 AOT) |
|---|---|---|---|---|
| Portability (20%) | 4.8 / 5 | 4.5 / 5 | 4.0 / 5 | 4.0 / 5 |
| Performance & Latency (25%) | 5.0 / 5 | 4.0 / 5 | 4.8 / 5 | 4.2 / 5 |
| Safety & Concurrency (25%) | 5.0 / 5 | 4.2 / 5 | 3.0 / 5 | 4.4 / 5 |
| Dependency Footprint (15%) | 5.0 / 5 | 4.8 / 5 | 4.0 / 5 | 3.8 / 5 |
| Ecosystem & Maturity (15%) | 4.7 / 5 | 4.6 / 5 | 4.5 / 5 | 4.0 / 5 |
| **Weighted Total** | **4.91 / 5** | **4.37 / 5** | **4.05 / 5** | **4.13 / 5** |

### 2.3. Architectural Decision: Rust
**Recommendation:** Implement the entire TermiNest Host Core Orchestrator, CLI, MCP Server, and Terminal Desktop Viewer in **Rust** (Edition 2021/2024).

**Rationale:**
1. **Zero Runtime Dependency:** A single, self-contained binary (`terminest.exe` on Windows, `terminest` on Linux) with zero dynamic runtime requirements, instant sub-10ms startup, and minimal memory overhead (~12 MB RSS).
2. **Official Systems Bindings:** Direct access to Microsoft's official `windows` crate (for WMI, COM, Hyper-V, Win32 APIs) and Linux `nix`/`libc` crates without fragile wrappers.
3. **High-Throughput Asynchronous Frame Processing:** The `tokio` async runtime combined with SIMD image processing crates (`turbojpeg`, `lz4_flex`) allows multiplexing 30 FPS display streaming and low-latency JSON-RPC control over a single event loop without garbage-collection pauses.
4. **Safety Under Concurrency:** Eliminates data races and memory corruption when managing concurrent Vsock channels, guest state machines, and terminal render loops.

---

## 3. Virtualization Management Interfaces

### 3.1. Windows Host Backend

#### Evaluated Approaches:
1. **Hyper-V WMI v2 (`root\virtualization\v2`):** Direct COM/WMI CIM interface over Win32.
2. **PowerShell Hyper-V Module Wrappers:** Invoking `powershell.exe -Command "New-VM ..."` from CLI.
3. **Direct Hyper-V Compute Services (HCS / `vmcompute.dll`):** Low-level Win32 C API used by WSL2 and Windows Containers.
4. **QEMU with Windows Hypervisor Platform (WHP via `WinHvPlatform.dll`):** Userspace QEMU process accelerated by the Windows hypervisor kernel driver.

| Evaluation Dimension | Hyper-V WMI v2 | PowerShell Wrappers | Direct HCS API | QEMU + WHP |
|---|---|---|---|---|
| **Supported OS Editions** | Windows 10/11 Pro, Enterprise, Server | Windows 10/11 Pro, Enterprise, Server | Windows 10/11 Pro, Enterprise, Server | **All Editions (inc. Windows Home)** |
| **Execution Overhead** | Low (~10-20ms per WMI call via COM) | Extremely High (200-800ms process spawn) | Minimal (< 1ms direct C API) | Low (child process controlled via QMP) |
| **API Stability & Documentation** | **High** (Standardized CIM schema since Win 8) | High (Standard PowerShell cmdlets) | Low (Undocumented private APIs, breaking changes) | High (QEMU QMP JSON specification) |
| **Administrative Privilege** | Can use `Hyper-V Administrators` group | Often requires elevated UAC Administrator | Requires elevated administrative privileges | Standard unprivileged user execution |
| **Vsock Socket Support** | Native `AF_HYPERV` (Hyper-V Sockets) | Inaccessible via PowerShell cmdlets | Supported | Native `vhost-vsock-pci` |

#### Architectural Decision: Dual-Backend Hybrid for Windows
- **Primary Backend (Windows Pro / Enterprise):** **Hyper-V WMI v2 (`root\virtualization\v2`)**.
  - Provides native, battle-tested VM orchestration with zero third-party hypervisor binaries.
  - Interacts seamlessly with Windows Hyper-V Sockets (`AF_HYPERV`).
  - Supports non-elevated operation via the `Hyper-V Administrators` group.
- **Secondary / Fallback Backend (Windows Home):** **QEMU with Windows Hypervisor Platform (`-accel whpx`)**.
  - Windows Home users cannot enable the full Hyper-V management stack, but WHP is available across all Windows 10/11 editions.
  - Provides a frictionless path for developers on consumer-grade hardware.

---

### 3.2. Linux Host Backend

#### Evaluated Approaches:
1. **Direct QEMU Process + QMP (QEMU Machine Protocol):** Orchestrator spawns `qemu-system-x86_64` / `qemu-system-aarch64` with KVM acceleration (`-enable-kvm`) and interacts via local UNIX domain socket using JSON QMP.
2. **libvirt (`libvirtd` + `virsh`):** Interacting with the system virtualization daemon via C bindings (`libvirt-sys`).
3. **Cloud-Hypervisor:** Modern Rust-based KVM virtual machine monitor designed for cloud workloads.
4. **Firecracker:** Minimalist microVM hypervisor by AWS.

| Evaluation Dimension | Direct QEMU + QMP | libvirt | Cloud-Hypervisor | Firecracker |
|---|---|---|---|---|
| **Desktop / Display Emulation** | **Full** (Virtio-GPU, Bochs, RAMFB) | Full (via QEMU driver) | Minimal (headless/console focus) | None (Serial console only; no GUI) |
| **External Daemon Dependency** | **Zero** (Self-contained process) | Heavy (`libvirtd` service must run) | Zero (Static binary) | Zero (Static binary) |
| **Guest OS Support** | Windows & Linux (All versions) | Windows & Linux | Modern Linux only; Windows experimental | Linux kernels only; no Windows |
| **Vsock Support** | Native (`vhost-vsock-pci`) | Supported | Supported | Supported |
| **Architecture Fit** | Perfect fit for desktop automation | High operational friction for CLI | Excellent for headless, poor for GUI | Unsuitable for desktop computer-use |

#### Architectural Decision: Direct QEMU with KVM Acceleration
**Recommendation:** Manage **QEMU directly via QMP over UNIX domain sockets** (`qemu-system-x86_64 -enable-kvm`).

**Rationale:**
1. Eliminates dependency on system-level `libvirtd` daemons that may not be configured or running.
2. Complete support for both modern Linux and Windows guests, including virtual display adapters (`virtio-vga`, `bochs-display`).
3. High-speed programmatic control via standard QMP JSON commands over local sockets.

---

## 4. Host-Guest Communication Transport

The communication transport connects the Host Orchestrator with the Guest Helper. This is the single most critical performance seam in the architecture.

### 4.1. Evaluated Transports

| Evaluation Dimension | Vsock (`AF_VSOCK` / `AF_HYPERV`) | Virtio-FS Mailbox (`terminal-mac`) | TCP/IP over Virtual NIC | Named Pipes over Serial |
|---|---|---|---|---|
| **Latency (Round-Trip)** | **< 0.2 ms** | 5.0 – 25.0 ms (polling jitter) | 1.0 – 3.0 ms | 10.0 – 50.0 ms |
| **Throughput** | **> 1.2 GB/s** (zero-copy shared memory) | Limited by host/guest disk I/O | ~200-400 MB/s (TCP stack overhead) | < 5 MB/s (Baud rate/buffer bottleneck) |
| **Network Stack Dependency** | **Zero** (Operates with zero network NICs) | Zero (Filesystem driver dependent) | High (Requires DHCP, IP allocation, routing) | Zero |
| **Security Surface** | Minimal (Point-to-point hardware channel) | High (Host/guest directory traversal risks) | Moderate (Network port scanning, spoofing) | Minimal |
| **Framing Protocol** | Standard stream socket (`read`/`write`) | File lock, write temp, atomic rename, poll | Standard stream socket | Character stream |
| **Host Resource Impact** | Zero idle disk writes | Continuous disk writes/reads every 5ms | Network driver packet handling | Serial buffer polling |

### 4.2. Architectural Decision: Vsock Point-to-Point Streaming
**Recommendation:** Standardize on **Vsock** across all platforms:
- **Windows Host:** Hyper-V Sockets (`AF_HYPERV`, socket family 34).
- **Linux Host:** Linux Vsock (`AF_VSOCK`, socket family 40, `/dev/vhost-vsock`).

**Rationale:**
1. **100x Latency Reduction:** Drops RPC round-trip latency from 25ms to < 0.2ms.
2. **Zero Disk I/O:** Eliminates the thousands of temp file writes and polling loops present in `terminal-mac`.
3. **Air-Gapped Operation:** Functions flawlessly even when the guest VM has zero network interfaces attached (`networking: "isolated"`).
4. **Multiplexing:** Supports bidirectional multiplexing of both low-latency JSON-RPC control and high-bandwidth binary screen frames.

---

## 5. Guest Helper Architecture & Language

The Guest Helper must execute inside the guest virtual machine, capture the display surface, inject mouse/keyboard inputs, manage clipboard, and execute commands.

### 5.1. Evaluated Languages for Guest Helper

| Language | Static Binary Size | Native Win32 / Linux API Access | Runtime Requirement | Startup Time |
|---|---|---|---|---|
| **Rust** | **~3-6 MB** | Direct Win32 / Direct3D / PipeWire / X11 | **Zero** | **< 5ms** |
| **C# (.NET 9 Native AOT)** | ~12-18 MB | Native P/Invoke on Windows; clunky on Linux | Minimal (Platform CRT) | ~25ms |
| **C++ (C++20)** | ~2-4 MB | Native headers (`windows.h`, `X11/Xlib.h`) | Requires static runtime linkage | **< 5ms** |
| **Go** | ~8-12 MB | Poor Direct3D COM integration; cgo required | Embedded runtime/GC | ~15ms |
| **Python** | N/A (Script) | Requires pywin32, ctypes | Massive (Requires Python installed in VM) | ~200-400ms |

### 5.2. Architectural Decision: Rust for Guest Daemons
**Recommendation:** Implement both the Tier 1 System Daemon and Tier 2 Session Worker in **Rust**.

**Rationale:**
1. **Zero Pre-Installed Prerequisites:** The compiled binaries (`terminest-guest-service.exe` on Windows, `terminest-guest-daemon` on Linux) have zero runtime dependencies and can be pre-baked into images or injected during unattended provisioning.
2. **Unified Codebase:** Common protocol parsing, CRC validation, and serialization logic can be shared directly between host and guest via a shared Rust crate (`terminest-protocol`).
3. **High-Performance Screen Capture:** Direct native bindings to Windows Graphics Capture / DXGI and Linux PipeWire / XShm without foreign-function interface overhead.

---

## 6. Frame Compression & Image Encoding

Agents require frequent visual updates (1-10 FPS during active interaction, up to 30 FPS for live viewer monitoring). Transmitting uncompressed 1080p raw BGRA frames (1920x1080x4 = 8.29 MB per frame) would consume ~250 MB/s of bandwidth.

### 6.1. Evaluated Codecs

| Codec | Encode Latency (1080p) | Decode Latency (Host) | Compression Ratio | Bandwidth at 10 FPS | CPU Utilization |
|---|---|---|---|---|---|
| **Raw BGRA (Uncompressed)** | 0 ms | 0 ms | 1:1 | ~83 MB/s | Near 0% |
| **LZ4 (Block Compression)** | **~1.5 ms** | **~0.8 ms** | 1.8:1 – 2.5:1 | ~35 MB/s | Low (< 5% CPU) |
| **JPEG (libjpeg-turbo / SIMD)** | **~4.0 ms** | **~2.5 ms** | **15:1 – 25:1** | **~3-5 MB/s** | Moderate (~8% CPU) |
| **PNG (libpng / zlib)** | ~45-80 ms | ~15 ms | 5:1 – 8:1 | ~12 MB/s | Very High (Bottleneck) |
| **H.264 / WebRTC (Hardware NVENC/VAAPI)** | ~2.0 ms (HW) | ~2.0 ms (HW) | 50:1 – 100:1 | ~1 MB/s | High complexity; requires GPU pass-through |

### 6.2. Architectural Decision: Hybrid JPEG + LZ4 Encoding Strategy
**Recommendation:** Standardize on a two-mode frame encoding pipeline:

1. **Agent MCP Mode (Default: JPEG @ 80% Quality):**
   - Transmits frames compressed via SIMD-accelerated JPEG (`turbojpeg` crate).
   - Delivers crisp, high-fidelity visual context for multimodal LLMs at a lightweight ~300-500 KB per frame.
   - Encode time is sub-5ms on standard virtual vCPUs.
2. **High-Speed Viewer Mode (Local LZ4 or Shared Memory):**
   - When the host runs a local viewer on the same physical machine, frames are compressed using lightweight **LZ4** or written directly to a shared memory buffer (`shm`).
   - Yields near-zero visual artifacting and sub-2ms end-to-end latency for terminal graphics rendering.

---

## 7. Desktop Viewer & Terminal Graphics Protocols

CLI agents run inside terminal emulators. Developers monitoring agent activity require real-time visual feedback directly within their terminal or via a lightweight external window.

### 7.1. Evaluated Terminal Graphics Protocols

| Protocol | Supported Terminals | Mechanism | Max Performance | Fallback Capability |
|---|---|---|---|---|
| **Kitty Graphics Protocol** | Kitty, Ghostty, WezTerm | Escape sequences (`ESC_G...`) supporting direct base64 chunks or shared memory (`t=s`, `t=f`) | **30-60 FPS** (zero-copy shared memory) | Native terminal window |
| **iTerm2 Inline Images** | iTerm2, WezTerm, Mintty | Base64-encoded file stream (`ESC]1337;File=...`) | 10-15 FPS | Drops frames under load |
| **Sixel** | Alacritty (patched), Foot, XTerm, Mintty | Paletted raster graphics escape codes | 2-5 FPS (High CPU encode) | Retro terminals |
| **Lightweight GUI Window** | Any platform (Win32 / Wayland / X11) | Native mini-window via `winit` / `softbuffer` | **60 FPS** | Universal fallback when terminal lacks graphics |

### 7.2. Architectural Decision: Tiered Terminal Graphics with Native Window Fallback
**Recommendation:** Implement a three-tiered terminal viewer engine in `terminest view`:

1. **Tier 1 (Kitty Protocol with Shared Memory):** Active when `$TERM` or terminal identification matches Kitty or Ghostty. Uses shared memory mapping for zero-overhead, 30 FPS terminal streaming.
2. **Tier 2 (Standard Kitty / iTerm2 Base64 Chunks):** Active in WezTerm, Mintty, or remote SSH terminal sessions. Streams compressed JPEG frames directly over the terminal stream.
3. **Tier 3 (Universal Native Window Fallback):** When the terminal emulator lacks graphical capabilities (e.g., standard Windows Command Prompt, standard `conhost`, or vanilla Linux virtual consoles), `terminest view --window` opens an ultra-lightweight, zero-dependency native desktop window (using `winit` and `softbuffer`, ~5 MB RSS) to display the live virtual desktop.

---

## 8. Summary of Technology Stack Selections

| Layer / Component | Selected Technology | Primary Alternative Rejected | Justification |
|---|---|---|---|
| **Core Orchestrator & CLI** | **Rust** (Tokio, Clap) | Go / C++ | Single native binary, zero runtime dependency, sub-10ms startup, memory safety. |
| **MCP Server** | **Rust** (In-process Tokio) | TypeScript / Python | Embeds directly inside the CLI binary without spawning Node or Python runtimes. |
| **Windows Hypervisor** | **Hyper-V WMI v2** (+ QEMU/WHP for Home) | PowerShell cmdlets | WMI is 50x faster than spawning PowerShell; supports non-admin users via Hyper-V group. |
| **Linux Hypervisor** | **Direct QEMU + QMP** | libvirt | Eliminates external daemon requirements; full desktop and Virtio-GPU support. |
| **Host-Guest Transport** | **Vsock (`AF_VSOCK` / `AF_HYPERV`)** | Virtio-FS Mailbox | 100x lower latency (< 0.2ms vs 25ms), zero disk churn, air-gapped security. |
| **Guest Helper** | **Rust** (Two-Tier Daemon) | C# (.NET) / C++ | Zero VM runtime prerequisites, high-performance capture, shared protocol definitions. |
| **Frame Compression** | **SIMD JPEG (`turbojpeg`) + LZ4** | PNG / WebP | Sub-5ms encode time, high compression ratio, optimal for multimodal LLM context. |
| **Desktop Viewer** | **Kitty shm + Native Window fallback** | Sixel only | High-frame-rate rendering on modern terminals; robust universal fallback. |
