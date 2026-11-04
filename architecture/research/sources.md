# TermiNest Research Sources & Evidence Ledger

**Document Status:** Complete & Audited  
**Author:** Staff Virtualization & Systems Engineer  
**Date:** 2026-11-05  
**Methodology:** Primary-source technical documentation analysis, kernel specifications, and API contracts.

---

## 1. Primary Documentation Sources

| Source ID | Publisher / Project | Document Title | Canonical URL | Verification Scope | Platform / Version Req. | Confidence |
|---|---|---|---|---|---|---|
| `SRC-MS-01` | Microsoft Learn | *Hyper-V Architecture & Management (WMI v2)* | `https://learn.microsoft.com/en-us/virtualization/hyper-v-on-windows/` | Full VM lifecycle, WMI `root\virtualization\v2`, VHDX differencing disks | Windows 10/11 Pro, Enterprise, Education | **Verified** |
| `SRC-MS-02` | Microsoft Learn | *Hyper-V Sockets (HV_SOCKET / AF_HYPERV)* | `https://learn.microsoft.com/en-us/virtualization/hyper-v-on-windows/user-guide/make-integration-service` | Host-guest socket transport over VMBus without IP network | Windows 10 1709+, Windows Server 2016+, Linux 4.14+ | **Verified** |
| `SRC-MS-03` | Microsoft Learn | *Windows Graphics Capture (WGC)* | `https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture` | Modern WinRT frame capture, display/window picker bypass, cursor inclusion | Windows 10 1803+ (Build 17134+) | **Verified** |
| `SRC-MS-04` | Microsoft Learn | *Desktop Duplication API (DirectX / DXGI)* | `https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/desktop-dup-api` | Low-latency DWM frame acquisition, GPU surface access, dirty rectangles | Windows 8+, Windows Server 2012+ | **Verified** |
| `SRC-MS-05` | Microsoft Learn | *SendInput Win32 API* | `https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput` | Synthetic keyboard/mouse injection, `KEYEVENTF_UNICODE`, mouse point mapping | Windows 2000+ | **Verified** |
| `SRC-MS-06` | Microsoft Learn | *Session 0 Isolation & Interactive Services* | `https://learn.microsoft.com/en-us/windows/win32/services/interactive-services` | Service execution boundaries, `WTSQueryUserToken`, `CreateProcessAsUser` | Windows Vista+, Windows Server 2008+ | **Verified** |
| `SRC-MS-07` | Microsoft Learn | *Windows Hypervisor Platform (WHP)* | `https://learn.microsoft.com/en-us/virtualization/api/hypervisor-platform/hypervisor-platform` | Userspace virtualization acceleration for third-party hypervisors (QEMU) | Windows 10 1803+ (All editions including Home) | **Verified** |
| `SRC-LNX-01` | Linux Kernel Org | *KVM (Kernel-based Virtual Machine) Documentation* | `https://docs.kernel.org/virt/kvm/index.html` | Hardware virtualization, `/dev/kvm` ioctl API, memory virtualization | Linux Kernel 2.6.20+ (x86_64, aarch64) | **Verified** |
| `SRC-LNX-02` | Linux Kernel Org | *vsock (Virtual Sockets) Specification* | `https://docs.kernel.org/networking/dev-drivers/vsock.html` | `AF_VSOCK`, `vhost-vsock`, CID/port addressing, zero-network guest IPC | Linux Kernel 3.9+, QEMU 2.8+ | **Verified** |
| `SRC-QEMU-01`| QEMU Documentation | *QEMU Machine Protocol (QMP) Specification* | `https://www.qemu.org/docs/master/interop/qemu-qmp-ref.html` | JSON-RPC hypervisor control, VM lifecycle, device hotplug, live snapshots | QEMU 2.0+ (Windows & Linux) | **Verified** |
| `SRC-FDO-01` | freedesktop.org | *xdg-desktop-portal ScreenCast & RemoteDesktop API* | `https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html` | Wayland display streaming over PipeWire, session negotiation, portal tokens | Modern Linux (GNOME 40+, KDE 5.24+) | **Verified** |
| `SRC-FDO-02` | freedesktop.org | *libei (Emulated Input for Wayland & X11)* | `https://libei.freedesktop.org/` | Sandboxed input emulation protocol for Wayland compositors and portals | Modern Linux, `libei 1.0+` (2023+) | **Verified** |
| `SRC-LNX-03` | Linux Kernel Org | *uinput (User Level Input Subsystem)* | `https://docs.kernel.org/input/uinput.html` | Synthetic virtual HID input device creation via `/dev/uinput` | Linux Kernel 2.6+ | **Verified** |
| `SRC-KITTY-01`| Kovid Goyal | *Kitty Graphics Protocol Specification* | `https://sw.kovid.goyal.net/kitty/graphics-protocol/` | In-band and shared-memory terminal graphics transmission (`f=32, t=s`) | Kitty 0.20+, Ghostty 1.0+, WezTerm | **Verified** |

---

## 2. Technical Findings and Constraint Verification

### Finding 1: Windows Edition Segmentation (Hyper-V vs. WHP)
- **Verified Fact:** Microsoft explicitly restricts the **Hyper-V hypervisor role** and `Hyper-V Management Tools` to Windows Pro, Enterprise, and Education editions. Windows Home editions **cannot** enable native Hyper-V virtual machines.
- **WHP Feature:** Microsoft provides the **Windows Hypervisor Platform (WHP)** as an optional feature across **all** Windows editions (including Home). WHP exposes an accelerator API (`WinHvPlatform.dll`) that allows userspace hypervisors (such as **QEMU**) to achieve hardware-assisted virtualization on Windows Home without the full Hyper-V stack.
- **Architectural Impact:** TermiNest must not rely exclusively on native Hyper-V. A QEMU+WHP backend is required to support Windows Home developers.

### Finding 2: Windows Host-Guest Socket Transport (Hyper-V Sockets)
- **Verified Fact:** Windows 10 1709+ supports **Hyper-V Sockets (HV_SOCKET)**.
  - Winsock family: `AF_HYPERV` (numeric value 34).
  - Protocol: Stream-oriented socket over VMBus.
  - Addressing: Service GUID (`GUID_VSOCK_SERVICE`) and VM ID (`GUID_DEVINTERFACE_VMBUS`).
  - Supported Guest OS: Windows 10/11 guests natively support HV_SOCKET; Linux guests with Linux kernel 4.14+ include upstream `hv_sock` drivers.
- **Architectural Impact:** Enables socket-based communication between Windows host and Windows/Linux guests over Hyper-V without network adapters or virtual switches.

### Finding 3: Windows Session 0 Isolation
- **Verified Fact:** Since Windows Vista, all Windows Services run in **Session 0**, which is strictly non-interactive.
  - Session 0 has no interactive window station (`WinSta0`) or desktop display surface.
  - APIs such as `SendInput`, `GetDC`, and Direct3D/DXGI Desktop Duplication fail immediately or return empty frames when called from Session 0.
- **Architectural Impact:** The TermiNest Windows Guest Helper cannot be implemented solely as a Windows Service. It must consist of a **Service Controller** in Session 0 and an **Interactive Session Worker** launched into the active console session via `CreateProcessAsUser`.

### Finding 4: Linux Wayland Display Security & Input Emulation
- **Verified Fact:** Wayland compositors (Mutter/GNOME, KWin/KDE, wlroots) enforce strict client isolation. Unlike X11, clients cannot query global window trees, capture arbitrary screen buffers via `XGetImage`, or inject global keystrokes via `XTest`.
- **Capture Path:** Wayland capture requires **PipeWire** video streams negotiated via **xdg-desktop-portal** (`org.freedesktop.portal.ScreenCast`). In standard desktop configurations, this triggers an interactive permission prompt.
- **Input Path:** Global input injection requires either:
  1. `libei` / `org.freedesktop.portal.RemoteDesktop` (supported on GNOME 45+ and modern compositors).
  2. Linux kernel `/dev/uinput` device node (requires `uinput` group permissions or root).
- **Headless Bypass:** A virtual machine running a dedicated, lightweight, headless compositor (or X11/Xvfb) eliminates portal prompts and guarantees deterministic, 100% automated agent access.

---

*End of Research Sources Ledger. Proceed to Report 03 for the complete platform feasibility analysis.*
