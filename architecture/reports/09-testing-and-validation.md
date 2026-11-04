# Architecture Report 09: Testing, Verification, & Quality Strategy

**Status:** Approved Architectural Specification  
**Version:** 1.0.0  
**Date:** November 2026  
**System:** TermiNest Cross-Platform Virtual Computer Infrastructure  
**Author:** Staff Systems Engineer & QA/Reliability Architect  

---

## 1. Executive Summary & Quality Charter

TermiNest interfaces with bare-metal CPU virtualization extensions, OS kernel drivers, display pipelines, and autonomous AI agents. A defect in this system could corrupt virtual disk images, freeze developer workstations, or leak credentials across isolation boundaries.

The TermiNest Testing & Verification Strategy adheres to three core mandates:
1. **Deterministic Testability Without Bare-Metal Virtualization:** 90% of the codebase—including protocol multiplexing, JSON-RPC handling, coordinate clamping, state machines, and file streaming—must be testable in standard unprivileged GitHub Actions / GitLab CI environments without requiring hardware VT-x / AMD-V.
2. **Strict Seam Isolation:** Every major subsystem (Hypervisor, Transport, Display Capture, Input Injection) is isolated behind clean Rust traits, allowing automated substitution with high-fidelity mock harnesses.
3. **Automated Fuzzing & Security Hardening Verification:** The binary protocol framing, Vsock decoders, path resolvers, and network firewalls are continuously verified against adversarial test cases.

---

## 2. Test Pyramid & Architectural Seams

```mermaid
flowchart TD
    subgraph E2ETier ["Tier 3: End-to-End Bare-Metal Matrix (10%)"]
        E2E_Win_Linux["Win Host -> Linux Guest (Hyper-V / WHP)"]
        E2E_Linux_Linux["Linux Host -> Linux Guest (QEMU / KVM)"]
        E2E_Agent_MCP["Agent Multimodal Autonomous Loop Test"]
    end

    subgraph IntegrationTier ["Tier 2: Subsystem Integration Harnesses (30%)"]
        Vsock_Loopback["Loopback Vsock Transport Harness"]
        QMP_Mock["QEMU QMP Engine Harness"]
        WMI_Mock["Hyper-V CIM/WMI Mock"]
        Guest_IPC["Tier 1 Daemon <-> Tier 2 Worker IPC"]
    end

    subgraph UnitTier ["Tier 1: Deterministic Unit & Property Tests (60%)"]
        Wire_Framing["TNST-v1 Binary Header & CRC-32 Fuzzing"]
        RPC_Validation["JSON-RPC 2.0 Schemas & Error Mapping"]
        Path_Jail["Path Traversal & Symlink Jail Verification"]
        Coord_Bounds["Coordinate Clamping & Display Viewport"]
        Config_Migration["machine.json Schema Migrations"]
    end

    UnitTier --> IntegrationTier
    IntegrationTier --> E2ETier
```

---

## 3. Tier 1: Unit & Property-Based Testing

Unit tests run in milliseconds and validate core algorithmic logic without starting virtual machines or touching OS hypervisor drivers.

### 3.1. Architectural Test Seams

#### 1. `MockHypervisorBackend`
The `HypervisorBackend` trait is decoupled from concrete platform hypervisors. The test harness implements an in-memory mock:

```rust
#[cfg(test)]
pub struct MockHypervisorBackend {
    pub state: std::sync::Arc<tokio::sync::Mutex<MachineState>>,
    pub launched_vms: std::sync::Arc<tokio::sync::Mutex<Vec<String>>>,
}

#[async_trait]
impl HypervisorBackend for MockHypervisorBackend {
    async fn create_vm(&self, config: &MachineConfig) -> Result<(), HypervisorError> {
        self.launched_vms.lock().await.push(config.name.clone());
        Ok(())
    }
    // ... complete state transition assertions
}
```

This allows full unit testing of `terminest start`, `terminest stop`, and state machine lifecycle transitions without launching real processes.

#### 2. In-Memory Vsock Duplex Channel
Using `tokio::io::duplex`, the protocol multiplexer and decoder are tested with zero network or hypervisor sockets:
- Validates 16-byte header parsing (`Magic`, `Version`, `Flags`, `Channel ID`, `Length`, `CRC-32C`).
- Tests packet reassembly when packets arrive fragmented across multiple arbitrary byte chunks (1-byte chunks, 3-byte chunks).
- Validates backpressure when Channel 1 (Display) is saturated, asserting Channel 0 (Control) commands are prioritized.

### 3.2. Property-Based Fuzzing & Security Tests

Using `proptest` and `cargo-fuzz`, the test suite verifies security boundaries:

| Test Harness | Target Module | Invariant Verified |
|---|---|---|
| `fuzz_wire_header` | `terminest-protocol::codec` | Any corrupted 16-byte header must result in an immediate `ProtocolError::InvalidHeader` without panic or out-of-bounds memory read. |
| `fuzz_path_jail` | `terminest-core::fs::sandbox` | Given an arbitrary attacker path string (including `..`, `\`, `/`, null bytes, symlinks), the resolved path must strictly satisfy `resolved.starts_with(allowed_root)` or return `PermissionDenied`. |
| `test_coord_clamping` | `terminest-core::action` | For any `(x, y)` coordinate where `x < 0`, `x >= width`, `y < 0`, or `y >= height`, the action must be rejected with `OutOfBounds` before dispatch. |
| `test_crc32_corruption` | `terminest-protocol::crc` | Modifying a single bit in a 1 MB payload must cause CRC verification failure. |

---

## 4. Tier 2: Subsystem Integration Testing

Integration tests verify interactions between components using lightweight mock servers or loopback sockets.

### 4.1. Vsock Loopback Transport Test Harness
A standalone binary harness executes both the Host Orchestrator transport layer and the Guest Helper Tier 1 daemon connected over a local loopback TCP or UNIX domain socket (simulating Vsock).
- Verifies the `guest.handshake` negotiation sequence.
- Tests heartbeat timeouts: asserts that when the guest drops connection, the host marks the machine state as `unresponsive` within 3 heartbeats (6 seconds).
- Verifies file transfer streaming over Channel 2: transmits a 50 MB randomized binary file, validates SHA-256 hash match on arrival, and verifies file descriptor cleanup.

### 4.2. Guest Inter-Tier IPC Verification
Inside the guest agent crate:
- Starts `terminest-guest-service` in a test process.
- Starts `terminest-guest-worker` in a child process.
- Tests session notification events: simulates user login, desktop lock, and user logout, verifying that the worker is gracefully signaled to terminate and respawn.

---

## 5. Tier 3: End-to-End (E2E) Bare-Metal Validation

End-to-End tests execute on dedicated bare-metal runners equipped with hardware virtualization (Windows Server / Desktop with Hyper-V; Ubuntu Linux with `/dev/kvm`).

### 5.1. Automated Headless E2E Test Workflow

Every nightly build executes the full automated validation cycle:

```mermaid
sequenceDiagram
    autonumber
    participant TestRunner as E2E Test Runner (Host)
    participant CLI as TermiNest CLI / Orchestrator
    participant VM as Virtual Machine (Hyper-V / KVM)
    participant Guest as Guest Daemon & Worker

    TestRunner->>CLI: terminest create e2e-test --os ubuntu-24.04 --ephemeral
    CLI->>VM: Provision base QCOW2/VHDX + Cloud-Init NoCloud ISO
    TestRunner->>CLI: terminest start e2e-test --wait --timeout 90
    CLI->>VM: Power on VM
    VM->>Guest: Boot kernel & systemd
    Guest->>CLI: Vsock handshake (guest.handshake)
    CLI-->>TestRunner: VM Ready (Exit 0)

    TestRunner->>CLI: terminest action e2e-test exec "echo 'TermiNest E2E' > /tmp/marker.txt"
    CLI->>Guest: Channel 0: system.exec
    Guest-->>CLI: Exit 0, stdout: ""
    
    TestRunner->>CLI: terminest action e2e-test pull /tmp/marker.txt marker_local.txt
    CLI->>Guest: Channel 2: File Transfer Stream
    Guest-->>CLI: File bytes
    TestRunner->>TestRunner: Assert marker_local.txt == "TermiNest E2E\n"

    TestRunner->>CLI: terminest action e2e-test screenshot --output screen.jpg
    CLI->>Guest: Channel 0: desktop.capture_frame
    Guest-->>CLI: Return JPEG frame (1920x1080)
    TestRunner->>TestRunner: Verify valid JPEG header & dimensions

    TestRunner->>CLI: terminest stop e2e-test
    CLI->>VM: ACPI Shutdown
    CLI->>CLI: Unlink ephemeral differencing disk
    TestRunner->>TestRunner: Verify base image SHA-256 untouched
```

### 5.2. Platform Compatibility Test Matrix

| Test Suite ID | Host OS | Hypervisor | Guest OS | Critical Verification Focus |
|---|---|---|---|---|
| **E2E-WIN-LNX** | Windows 11 Pro | Hyper-V WMI | Ubuntu 24.04 | Vsock (`AF_HYPERV`), Cloud-init seed, headless Xvfb capture |
| **E2E-WIN-WIN** | Windows 11 Pro | Hyper-V WMI | Windows 11 Pro | Session 0 vs Session 1 worker launch, DXGI capture, SendInput |
| **E2E-LNX-LNX** | Ubuntu 24.04 | QEMU / KVM | Ubuntu 24.04 | QMP socket control, `/dev/vhost-vsock`, PipeWire capture |
| **E2E-LNX-WIN** | Ubuntu 24.04 | QEMU / KVM | Windows 11 Pro | QEMU Windows guest tools, virtio-win drivers, WGC capture |
| **E2E-WHP-LNX** | Windows 11 Home | QEMU / WHP | Ubuntu 24.04 | WHP acceleration stability, userspace Vsock emulation |

---

## 6. Performance Benchmarks & Latency Budgets

TermiNest defines hard performance budgets enforced via automated regression benchmarks (`cargo bench` using `criterion`):

| Metric | Target Budget | Upper Bound (Failure) | Measurement Method |
|---|---|---|---|
| **CLI Startup Latency** (`terminest --version`) | **< 8 ms** | > 20 ms | `hyperfine` over 50 runs |
| **Config Load & Parse** (`machine.json`) | **< 1 ms** | > 5 ms | Criterion benchmark |
| **Vsock RPC Round-Trip** (`guest.ping`) | **< 0.2 ms** | > 1.0 ms | 1,000 sequential pings over Vsock |
| **Display Capture Latency** (1080p DXGI / XShm) | **< 4 ms** | > 12 ms | Timestamp delta from capture call to buffer in RAM |
| **JPEG SIMD Compression** (1080p frame) | **< 5 ms** | > 15 ms | `turbojpeg` encode benchmark |
| **End-to-End Screenshot MCP Tool** | **< 15 ms** | > 40 ms | Client MCP request to base64 response |
| **Memory Footprint (Idle)** | **< 15 MB** RSS | > 30 MB RSS | Working set monitor on Host Orchestrator |
| **Memory Footprint (30 FPS Viewer Streaming)** | **< 35 MB** RSS | > 70 MB RSS | Working set monitor during live Kitty shm stream |

---

## 7. Security Verification & Hardening Tests

Automated security integration tests run against every release candidate:

### 1. Symlink Traversal Attack Test (`SEC-TEST-01`)
- Attacker script inside guest creates a symlink pointing to `/etc/shadow` or `C:\Windows\System32\config\SAM`.
- Agent invokes `pull_file` on the symlink.
- **Assertion:** Host orchestrator verifies realpath resolves within permitted sandbox; operation is aborted with `PermissionDenied`.

### 2. Network Isolation Verification (`SEC-TEST-02`)
- VM created with `networking: "isolated"`.
- Guest script attempts UDP DNS query to `8.8.8.8:53` and TCP connection to `1.1.1.1:80`.
- **Assertion:** Socket fails immediately with `ENETUNREACH` or times out; zero packets captured on host network interfaces.

### 3. Cloud Metadata Block Verification (`SEC-TEST-03`)
- VM created with `networking: "nat"`.
- Guest script executes HTTP GET to `http://169.254.169.254/latest/meta-data/`.
- **Assertion:** Host NAT firewall drops packet; HTTP request times out with zero bytes received.

### 4. Malformed Vsock Fuzzing (`SEC-TEST-04`)
- Host test engine blasts 50,000 randomized packets with corrupt lengths, wrong magic headers, and invalid CRC checksums at the guest daemon listener.
- **Assertion:** Daemon process memory remains flat, zero segfaults or panics occur, and valid subsequent packets are processed normally.

---

## 8. Continuous Integration & Release Gates

```mermaid
flowchart LR
    subgraph PRGate ["Pull Request Gate (Standard Cloud Runner)"]
        A["cargo fmt & clippy"] --> B["cargo test (Unit & Mock Tier)"]
        B --> C["cargo bench (Criterion Latency)"]
        C --> D["Fuzzing Sanity Check (1 min)"]
    end

    subgraph NightlyGate ["Nightly Gate (Bare-Metal Self-Hosted Runner)"]
        E["Bare-Metal Hyper-V E2E"] --> F["Bare-Metal KVM E2E"]
        F --> G["Security Penetration Suite"]
        G --> H["Long-Running 4-Hour Agent Soak Test"]
    end

    PRGate -->|Merge to Main| NightlyGate
```

A release binary is only generated when both the Pull Request Gate and the Nightly Bare-Metal Gate achieve 100% pass rates across all targeted platforms.
