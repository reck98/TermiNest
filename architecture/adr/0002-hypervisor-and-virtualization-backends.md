# ADR 0002: Hypervisor and Virtualization Backend Selection

## Status
Accepted

## Context
TermiNest must run virtual machines on both Windows and Linux host environments. Different editions of Windows feature different hypervisor capabilities (Windows Pro/Enterprise includes Hyper-V management tools, whereas Windows Home lacks Hyper-V management but supports the Windows Hypervisor Platform kernel API). On Linux, multiple virtualization stacks exist, including direct QEMU/KVM, libvirt, Cloud-Hypervisor, and Firecracker.

The hypervisor backend must support desktop virtual display emulation, Vsock communication, differencing disks, unattended guest provisioning, and non-elevated developer workflows.

## Problem Statement
Which virtualization backends should TermiNest support, and how should they be integrated into the host orchestrator?

## Decision
We establish a clean, statically composed `HypervisorBackend` trait (SPI) in the Core Orchestrator, with platform-specific backend implementations:

1. **Windows Host (Primary Backend):** Native **Hyper-V via WMI v2 (`root\virtualization\v2`)**.
   - Used for Windows 10/11 Pro, Enterprise, and Education.
   - Provides native Hyper-V Sockets (`AF_HYPERV`) and native VHDX management without requiring third-party hypervisor binaries.
   - Operates with least privilege by leveraging the `Hyper-V Administrators` local group (`SID: S-1-5-32-578`).
2. **Windows Host (Fallback Backend):** **QEMU with Windows Hypervisor Platform (WHP via `-accel whpx`)**.
   - Used automatically on Windows Home edition or when Hyper-V management is disabled.
   - Eliminates the hard requirement for Windows Pro.
3. **Linux Host (Primary Backend):** Direct **QEMU with KVM Acceleration (`qemu-system-x86_64 -enable-kvm`)**.
   - Managed directly via QEMU Machine Protocol (QMP) over a local UNIX domain socket.
   - Avoids dependencies on external system daemons like `libvirtd`.
   - Full support for `virtio-vga`, `bochs-display`, and `vhost-vsock-pci`.

## Rationale
- **Hyper-V WMI v2 vs. PowerShell wrappers:** WMI v2 calls execute in ~10-20ms via COM, whereas invoking `powershell.exe` incurs a 300-800ms process startup penalty per command.
- **Direct QEMU vs. libvirt:** Direct QEMU execution provides single-binary orchestration without asking developers to install, enable, or debug system-level `libvirtd` systemd services or socket permissions.
- **Windows Home Accessibility:** Supporting WHP on Windows Home expands TermiNest's reach to developers who do not have enterprise licenses.

## Consequences

### Positive
- Zero external daemon dependencies on Linux (just standard QEMU packages).
- Seamless native experience on Windows Pro via built-in Hyper-V.
- Clear path for Windows Home users via QEMU/WHP.
- Clean abstraction seam (`HypervisorBackend`) isolates virtualization quirks from CLI and MCP layers.

### Negative / Trade-Offs
- Maintaining two backends on Windows (Hyper-V and QEMU/WHP) increases testing complexity.
- Direct QEMU orchestration requires TermiNest to manage the QMP command protocol directly, including socket lifecycle and JSON-RPC parsing.
