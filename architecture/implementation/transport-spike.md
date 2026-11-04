# Milestone 0: Transport Feasibility Spike Report

**Date:** November 2026  
**Subject:** Host-Guest Transport Investigation (Windows Host -> Ubuntu 24.04 LTS Guest)  
**Authors:** TermiNest Systems Architecture & Engineering Team  

---

## 1. Objective

ADR-0003 establishes **Point-to-Point Virtual Sockets (Vsock)** as the target host-guest IPC mechanism for TermiNest, replacing the file-based shared folder mailbox of `terminal-mac`.

The objective of this feasibility spike is to validate:
1. Windows host socket APIs for virtualization (`AF_HYPERV` vs alternatives).
2. Linux guest socket support (`AF_VSOCK`, `hv_sock`).
3. Interoperability between Windows host and Linux guest across different Windows editions (Windows 11 Pro vs. Windows 11 Home).
4. Fallback architectures when native hypervisor Vsock is unavailable or restricted.

---

## 2. Platform Environment Findings

During environment reconnaissance on the target machine:
- **Host OS:** Microsoft Windows 11 Home Single Language (Build 26300, 64-bit).
- **CPU:** 12th Gen Intel Core i5-12450H (x86_64, 8 cores / 12 logical processors).
- **Virtualization Features:**
  - `VirtualMachinePlatform` (VMP): **Enabled**.
  - `HypervisorPlatform` (WHP): **Enabled**.
  - `Microsoft-Hyper-V-Hypervisor` (VMMS): **Not supported on Windows Home edition**.
- **WSL2:** Active, running Ubuntu-24.04 LTS default.

---

## 3. Deep-Dive Analysis of Virtualization Socket Transports

### 3.1. Windows Hyper-V Sockets (`AF_HYPERV`, Socket Family 34)
- **Mechanism:** Windows Hyper-V Sockets communicate directly over the VMBus hypervisor transport using Winsock address family `AF_HYPERV` (`34`).
- **Addressing:**
  - Uses `SOCKADDR_HV` struct containing two GUIDs:
    - `VmId`: The virtual machine GUID, or `HV_GUID_PARENT` (`a42e7cda-d03f-480c-9cc2-a4de20abb878`) from guest to host.
    - `ServiceId`: The service GUID. For port-based routing, the port is mapped using `{PORT-facb-11e6-bd58-64006a561569}` (e.g. port `10001` -> `{00002711-facb-11e6-bd58-64006a561569}`).
- **Linux Guest Support:** The Linux kernel includes `hv_sock` drivers in mainline kernels since 4.8. When Ubuntu 24.04 LTS runs inside Hyper-V, `AF_VSOCK` calls are seamlessly bridged to `hv_sock` over VMBus.
- **Limitation on Windows Home:** Native Hyper-V VMMS service is unavailable on Windows Home. `AF_HYPERV` requires the Hyper-V virtualization infrastructure.

### 3.2. QEMU with Windows Hypervisor Platform (WHP)
- **Mechanism:** On Windows Home, ADR-0002 selects QEMU with `-accel whpx` as the hypervisor backend.
- **Vsock Reality on Windows Host:**
  - Standard Linux QEMU uses `/dev/vhost-vsock`, which is a Linux-kernel-only device node.
  - Windows host OS does not possess a `/dev/vhost-vsock` kernel driver.
  - Therefore, running QEMU on Windows cannot use native Linux `vhost-vsock`.
- **Architectural Solution for Windows Home:**
  1. **Virtio-Serial over Local Named Pipe / Socket:** QEMU emulates `virtio-serial-pci` inside the guest (which appears in Linux as `/dev/virtio-ports/terminest`), connected on the Windows host to a local Windows Named Pipe (`\\.\pipe\terminest-<vmid>`) or local loopback TCP socket (`127.0.0.1:<port>`).
  2. **Isolated Private Host-Only Virtual Switch:** QEMU forwards port 10001 over an internal, air-gapped user network (`-netdev user,id=net0,hostfwd=tcp:127.0.0.1:10001-:10001`).
  3. **Abstraction Seam:** Because the `Transport` and `FramedTransport` traits in `terminest-transport` are agnostic of the underlying stream type (`AsyncRead + AsyncWrite`), the exact same `StreamDecoder` and TNST-v1 wire protocol execute identically over Hyper-V Vsock, Unix domain sockets, Windows named pipes, and local stream sockets without changing a single line of protocol code.

---

## 4. Reconnection and Fault Tolerance

1. **Abrupt Guest Reset:** When a guest VM resets or reboots, the socket stream terminates with EOF or reset. The host `FramedTransport::recv()` detects EOF and safely marks the connection closed without panic.
2. **Heartbeat Liveness:** If 3 consecutive heartbeat intervals (6 seconds) pass without frames, the host detects a dead connection and triggers reconnection.
3. **Session Worker Reconnect:** If the interactive session worker restarts, the system daemon maintains the host socket and relays commands to the newly spawned worker.

---

## 5. Conclusions & M0 Validation

1. `terminest-transport` successfully implements `InMemoryTransport` and `FramedTransport<IO>` satisfying all requirements for deterministic testing and multi-backend deployment.
2. The protocol layer is decoupled from kernel-specific socket headers.
3. Windows Pro deployments will leverage native `AF_HYPERV`, while Windows Home (QEMU/WHP) will use stream-backed virtio-serial or private loopback framing.
