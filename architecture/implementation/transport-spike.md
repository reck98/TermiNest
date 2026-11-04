# Milestone 0: Transport Feasibility & Platform Audit Report

**Date:** November 2026  
**Subject:** Host-Guest Transport Investigation (Windows Host -> Ubuntu 24.04 LTS Guest)  
**Authors:** TermiNest Systems Architecture & Engineering Team  
**Audit Status:** Audited and Qualified (M0 Closure Audit)  

---

## 1. Executive Summary & Capability Classification

To ensure rigorous systems engineering without unsupported claims, all transport capabilities are formally classified according to the five-state taxonomy:

| Transport Component / Capability | Implementation & Verification Status | Scope in M0 |
|---|---|---|
| **`Transport` Trait Abstraction** | **Implemented & Tested** | Core bidirectional async interface in `terminest-transport`. |
| **`InMemoryTransport` Duplex Channel** | **Implemented & Tested** | In-memory Tokio channel pair for deterministic unit/integration testing. |
| **`FramedTransport<IO>` Async Adapter** | **Implemented & Tested** | Stream framing adapter over generic `AsyncRead + AsyncWrite` streams. |
| **Vsock Address & Hyper-V GUID Generator** | **Implemented & Tested** | Addressing models and GUID converters in `terminest-transport::vsock`. |
| **Host-to-Guest `AF_HYPERV` Transport** | **Designed & Mapped (Unverified on real VM)** | Winsock API architecture mapped; real VM integration requires elevated registry setup in M2. |
| **QEMU/WHP + Virtio-Serial + Named Pipe** | **Designed but Not Implemented** | Proposed fallback architecture for Windows Home; concrete adapter not yet built. |
| **Cryptographic Authentication (AEAD)** | **Deferred** | Out of scope for M0; deferred to future network transport milestones. |

---

## 2. Host Platform Reconnaissance Findings

Environment reconnaissance was conducted directly on the target host workstation:
- **Host Operating System:** Microsoft Windows 11 Home Single Language (Build 26300, 64-bit).
- **Host CPU:** 12th Gen Intel Core i5-12450H (x86_64, 8 cores / 12 logical processors).
- **Hardware Virtualization Flags:**
  - `VirtualMachinePlatform` (VMP): **Enabled**.
  - `HypervisorPlatform` (WHP): **Enabled**.
  - `Microsoft-Hyper-V-Hypervisor` (VMMS): **Not supported on Windows Home edition**.
- **WSL2 Runtime:** Active, default distribution Ubuntu-24.04 LTS.

---

## 3. Real Platform Vsock Investigation & Discoveries

### 3.1. WSL2 `/dev/vsock` Inspection
Direct inspection of the Ubuntu 24.04 LTS environment inside WSL2 confirmed:
1. `/dev/vsock` is present as a character device (`crw-rw-rw- 1 root root 10, 259`).
2. Active Vsock streams (`ss -a --vsock`) exist between the guest and Windows host CID `2` (`VMADDR_CID_HOST`).
3. Calling `IOCTL_VM_SOCKETS_GET_LOCAL_CID` on `/dev/vsock` returns `4294967295` (`VMADDR_CID_ANY`).
4. **Root Cause:** On Windows Hyper-V, the Linux `hv_sock` driver does not assign fixed integer CIDs to guests. Instead, Hyper-V identifies guest endpoints by 128-bit VM GUIDs and routes services via Service GUIDs.

### 3.2. Critical Windows Security Finding: Service GUID Registry Requirement
Inspection of the Windows Host Registry revealed a foundational constraint for native Hyper-V Sockets:
- Windows Hyper-V Sockets (`AF_HYPERV`, socket family 34) require that any custom listening service GUID be registered under:
  `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Virtualization\GuestCommunicationServices\<Service-GUID>`
- Existing registered services on the host include:
  - `{999E53D4-3D5C-4C3E-8779-BED06EC056E1}` (VM Session Service)
  - `{A5201C21-2770-4C11-A68E-F182EDB29220}` (VM Session Service-2)
- **Security & Privilege Implication:** Writing to `HKLM` requires **Administrator privileges**. Under `REQ-INST-01` (Non-Root Host Installation), an unprivileged TermiNest orchestrator running in user space (`%LOCALAPPDATA%`) **cannot dynamically register its own custom Service GUID** without prior administrative setup.
- **Architectural Mitigation:**
  1. For Windows Pro/Enterprise with Hyper-V: The one-time setup installer (`install.ps1`) must register the TermiNest Service GUID (`{00002711-facb-11e6-bd58-64006a561569}`) with administrative rights, OR
  2. The orchestrator must connect from Host to Guest (where the Guest daemon initiates the listener), OR
  3. The unprivileged fallback transport (Windows Named Pipes or loopback stream) must be used.

---

## 4. Windows Home Transport Evaluation (QEMU/WHP)

On Windows Home, Hyper-V management is absent, requiring QEMU with Windows Hypervisor Platform (`-accel whpx`):
- **Vsock Limitation:** The Linux kernel driver `/dev/vhost-vsock` does not exist on Windows. QEMU on Windows cannot attach a host-side `/dev/vhost-vsock` backend.
- **Proposed Architecture (Designed):**
  - QEMU attaches a `virtio-serial-pci` device to the guest VM (exposed in Linux as `/dev/virtio-ports/terminest`).
  - On the Windows host, QEMU attaches the serial port to a Windows Named Pipe (`\\.\pipe\terminest-<name>`) or local loopback TCP socket (`127.0.0.1:10001`).
- **Audit Qualification:** This architecture is **designed but not implemented** in M0. The `FramedTransport<IO>` adapter proves that any asynchronous byte stream can frame TNST-v1 packets, but the concrete Windows Named Pipe adapter and QEMU process manager belong to Milestone 1 and Milestone 2.

---

## 5. Security Property Distinction

The implementation enforces the following distinct security boundaries:

| Security Property | Mechanism in M0 | Status |
|---|---|---|
| **Accidental Corruption Detection** | Castagnoli CRC-32C over header prefix (0..12) and payload (ADR-0010) | **Implemented & Tested** |
| **Stream Framing Integrity** | `StreamDecoder` poisoning on malformed headers, bounds checking | **Implemented & Tested** |
| **DoS Memory Protection** | 16 MB maximum payload limit, 32 MB decoder capacity cap | **Implemented & Tested** |
| **Transport Isolation** | Hardware hypervisor memory rings (`AF_VSOCK` / `AF_HYPERV`) | **Architecture Dependency (M2)** |
| **Peer Authentication** | Cryptographic identity verification (mutual TLS / HMAC tokens) | **Deferred to Network Transports** |
| **Confidentiality / Encryption** | In-transit stream encryption (TLS / WireGuard) | **Deferred to Network Transports** |

---

## 6. Reproducible Experiment for Future Host-Guest Validation

To validate host-guest connectivity in Milestone 2 when guest provisioning is ready:
1. **Linux Guest Daemon:** Bind `AF_VSOCK` port `10001` inside Ubuntu 24.04 LTS guest.
2. **Windows Host Listener:** Open `AF_HYPERV` socket, bind to `{00002711-facb-11e6-bd58-64006a561569}`.
3. **Framed Transport Ping:** Exchange 1,000 TNST-v1 control frames over `FramedTransport`.
4. **Measure Round-Trip Latency:** Record latency using high-resolution hardware counters.
