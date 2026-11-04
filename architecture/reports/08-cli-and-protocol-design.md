# Architecture Report 08: CLI Specifications & Wire Protocol Design

**Status:** Approved Architectural Specification  
**Version:** 1.0.0  
**Date:** November 2026  
**System:** TermiNest Cross-Platform Virtual Computer Infrastructure  
**Author:** Principal Cross-Platform Infrastructure Architect  

---

## 1. Executive Summary

This specification defines the complete external and internal interface contracts for TermiNest:
1. **The CLI Command Line Interface:** Command hierarchy, flags, outputs, and exit codes.
2. **Filesystem Hierarchy & Machine Directory Layout:** Predictable, cross-platform filesystem structures for configuration, VM images, and runtime artifacts.
3. **Machine Configuration Schema (`machine.json`):** Strict versioned schema governing VM resources, isolation tiers, and hardware definitions.
4. **TermiNest Wire Protocol v1 (TNST-v1):** Binary framing specification for multiplexed Vsock transport, including control JSON-RPC schemas and frame stream headers.

---

## 2. CLI Command Hierarchy & Specifications

The TermiNest CLI (`terminest`) provides an intuitive, POSIX-compliant interface designed for both interactive human use and automated scripting by AI agents.

```
terminest
├── doctor          # Validate host virtualization, permissions, and dependencies
├── create          # Provision a new virtual machine from templates
├── ls              # List configured virtual machines and their statuses
├── start           # Boot a virtual machine (persistent or ephemeral)
├── status          # Query detailed VM runtime state and metrics
├── view            # Stream real-time desktop visuals to terminal or window
├── action          # Direct CLI execution of computer-use primitives
│   ├── screenshot  # Capture current screen to file or stdout
│   ├── click       # Inject mouse click at (x, y)
│   ├── move        # Move mouse cursor to (x, y)
│   ├── drag        # Mouse drag from (x1, y1) to (x2, y2)
│   ├── type        # Type Unicode text string
│   ├── key         # Press key combinations (e.g., Ctrl+C, Enter)
│   ├── exec        # Execute CLI command inside guest OS
│   ├── push        # Transfer file from host to guest
│   └── pull        # Transfer file from guest to host
├── pause           # Pause virtual machine CPU execution
├── resume          # Resume paused virtual machine
├── stop            # Cleanly shut down a virtual machine
├── destroy         # Delete a virtual machine and its disk artifacts
├── mcp             # Launch stdio MCP server for AI agent integration
└── skills          # Install and manage AI agent skills and prompt packs
```

### 2.1. Command Reference

#### `terminest doctor`
Performs comprehensive pre-flight verification of host capabilities.
- **Flags:**
  - `--json`: Emit machine-readable diagnostics JSON.
- **Checks Performed:**
  - CPU hardware virtualization (Intel VT-x / AMD-V) enabled in BIOS/UEFI.
  - Hypervisor status:
    - *Windows:* Hyper-V role presence, `Hyper-V Administrators` group membership, Windows Hypervisor Platform (WHP) availability.
    - *Linux:* `/dev/kvm` presence and read/write permissions, `/dev/vhost-vsock` availability.
  - Terminal graphics support: Queries terminal ID for Kitty, Ghostty, WezTerm, or iTerm2 graphics support.
- **Exit Codes:** `0` (Ready), `1` (Warnings present, can run with limitations), `2` (Critical virtualization failure).

#### `terminest create <name>`
Provisions a new virtual machine instance using automated unattended templates.
- **Arguments:** `<name>` Unique machine identifier (alphanumeric, hyphens).
- **Flags:**
  - `--os <template>`: Guest OS template (`ubuntu-24.04`, `debian-12`, `windows-11`). Default: `ubuntu-24.04`.
  - `--cpus <n>`: Number of virtual CPUs. Default: `2`.
  - `--ram <mb>`: Memory allocation in MB. Default: `4096`.
  - `--disk <gb>`: Virtual disk capacity ceiling in GB. Default: `32`.
  - `--network <isolated|nat|bridged>`: Network profile. Default: `nat`.
  - `--headless`: Mark machine as headless (disables GUI capture pipelines). Default: `false`.
- **Behavior:** Downloads/validates base cloud image, generates cloud-init NoCloud seed ISO or Windows `autounattend.xml`, creates child differencing disk, and writes `machine.json`.

#### `terminest start <name>`
Boots a configured virtual machine.
- **Flags:**
  - `--ephemeral`: Boot in ephemeral mode (temporary differencing overlay; changes discarded on stop).
  - `--mount <host_path>:<guest_path>[:ro|:rw]`: Mount a host directory into the guest. Default: read-only (`ro`).
  - `--wait`: Block until guest helper daemon completes handshake and reports ready.
  - `--timeout <seconds>`: Maximum wait duration. Default: `60`.

#### `terminest status <name>`
Queries live runtime status of a virtual machine.
- **Output Fields:** State (`stopped`, `starting`, `running`, `paused`, `stopping`, `error`), PID, Hypervisor Backend, Vsock Port/CID, Active Session User, Display Resolution, Egress IP, Uptime, Memory RSS.

#### `terminest view <name>`
Streams the guest virtual desktop in real time.
- **Flags:**
  - `--fps <n>`: Target frame rate (1 to 60). Default: `30` for Kitty shm, `10` for chunked.
  - `--window`: Force launch of the lightweight native mini-window instead of terminal graphics.
  - `--scale <float>`: Downscale factor for terminal rendering (e.g., `0.5`, `1.0`).

#### `terminest action <name> <subcommand> [args]`
Enables human operators or shell scripts to execute single computer-use actions without running an MCP agent.
- `action screenshot [--output <path>]` (Outputs JPEG; writes to stdout if path omitted).
- `action click <x> <y> [--button left|right|middle] [--double]`
- `action move <x> <y>`
- `action drag <x1> <y1> <x2> <y2>`
- `action type "<text>" [--delay-ms <n>]`
- `action key "<key_combo>"` (e.g., `"Ctrl+Alt+t"`, `"Return"`, `"Escape"`)
- `action exec "<cmd>" [--timeout <sec>]`
- `action push <host_src> <guest_dst>`
- `action pull <guest_src> <host_dst>`

#### `terminest stop <name>`
Cleanly shuts down the virtual machine.
- **Flags:**
  - `--force`: Force immediate hypervisor termination (ACPI power-off) without waiting for graceful guest OS shutdown.
  - `--timeout <seconds>`: Grace period before forced termination. Default: `15`.

#### `terminest destroy <name>`
Deletes a virtual machine and unlinks its disk images.
- **Flags:**
  - `--force`: Skip interactive confirmation prompt.
  - `--keep-disk`: Delete VM registration but preserve virtual disk files.

#### `terminest mcp --machine <name>`
Launches the Model Context Protocol stdio server targeting a specific virtual machine.
- Communicates strictly via `stdin`/`stdout` JSON-RPC 2.0.
- Automatically boots the VM if not already running (configurable via `--no-auto-start`).

---

## 3. Filesystem Hierarchy & Layout

TermiNest adheres to native OS directory standards.

### 3.1. Windows Layout
Base Root: `%LOCALAPPDATA%\TermiNest` (e.g., `C:\Users\<username>\AppData\Local\TermiNest`)

```
%LOCALAPPDATA%\TermiNest\
├── config.json                     # Global settings, default hypervisor, telemetry
├── templates\                      # Cached golden base images and seed assets
│   ├── ubuntu-24.04-base.qcow2
│   ├── ubuntu-24.04-base.vhdx
│   └── win11-base.vhdx
├── machines\                       # Individual VM instances
│   ├── dev-box\
│   │   ├── machine.json            # Machine configuration contract
│   │   ├── disk.vhdx               # Primary persistent virtual disk
│   │   ├── nvram.bin               # UEFI NVRAM variables
│   │   ├── audit.jsonl             # Append-only security audit log
│   │   └── logs\
│   │       ├── console.log         # Hypervisor serial console output
│   │       └── terminest.log       # Orchestrator session debug logs
│   └── test-worker\
│       ├── machine.json
│       └── disk.vhdx
└── run\                            # Ephemeral runtime state
    ├── dev-box.pid                 # Running process PID
    ├── dev-box.sock                # Named pipe / control socket
    └── diff_7f3a9e.vhdx            # Active ephemeral differencing overlay (auto-deleted)
```

### 3.2. Linux Layout
- **User Config:** `~/.config/terminest/config.json`
- **Data & Disks:** `~/.local/share/terminest/`
- **Runtime Sockets & PIDs:** `/run/user/<uid>/terminest/` (or `~/.local/state/terminest/run/`)

---

## 4. Machine Configuration Schema (`machine.json`)

The `machine.json` file resides in every VM directory and serves as the single source of truth for the virtual machine.

### 4.1. JSON Schema Definition (v1.0.0)

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "TermiNestMachineConfig",
  "type": "object",
  "required": ["version", "id", "name", "os", "hardware", "storage", "networking", "created_at"],
  "properties": {
    "version": {
      "type": "string",
      "enum": ["1.0.0"]
    },
    "id": {
      "type": "string",
      "pattern": "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    },
    "name": {
      "type": "string",
      "pattern": "^[a-zA-Z0-9_-]{1,64}$"
    },
    "os": {
      "type": "object",
      "required": ["family", "distro", "version", "arch"],
      "properties": {
        "family": { "type": "string", "enum": ["linux", "windows"] },
        "distro": { "type": "string" },
        "version": { "type": "string" },
        "arch": { "type": "string", "enum": ["x86_64", "aarch64"] },
        "guest_agent_version": { "type": "string" }
      }
    },
    "hardware": {
      "type": "object",
      "required": ["cpus", "memory_mb", "display"],
      "properties": {
        "cpus": { "type": "integer", "minimum": 1, "maximum": 64 },
        "memory_mb": { "type": "integer", "minimum": 512, "maximum": 131072 },
        "nested_virtualization": { "type": "boolean", "default": false },
        "display": {
          "type": "object",
          "required": ["width", "height", "dpi"],
          "properties": {
            "width": { "type": "integer", "default": 1920 },
            "height": { "type": "integer", "default": 1080 },
            "dpi": { "type": "integer", "default": 96 }
          }
        }
      }
    },
    "storage": {
      "type": "object",
      "required": ["format", "disk_path", "capacity_gb"],
      "properties": {
        "format": { "type": "string", "enum": ["vhdx", "qcow2", "raw"] },
        "disk_path": { "type": "string" },
        "base_image": { "type": "string" },
        "capacity_gb": { "type": "integer", "default": 32 },
        "ephemeral": { "type": "boolean", "default": false }
      }
    },
    "networking": {
      "type": "object",
      "required": ["tier"],
      "properties": {
        "tier": { "type": "string", "enum": ["isolated", "nat", "bridged"], "default": "nat" },
        "mac_address": { "type": "string" },
        "port_forwarding": {
          "type": "array",
          "items": {
            "type": "object",
            "properties": {
              "host_port": { "type": "integer" },
              "guest_port": { "type": "integer" },
              "protocol": { "type": "string", "enum": ["tcp", "udp"] }
            }
          }
        }
      }
    },
    "hypervisor": {
      "type": "object",
      "required": ["backend"],
      "properties": {
        "backend": { "type": "string", "enum": ["hyperv", "qemu-whp", "qemu-kvm"] },
        "vsock_port": { "type": "integer", "default": 10001 },
        "vsock_cid": { "type": "integer", "default": 3 }
      }
    },
    "created_at": { "type": "string", "format": "date-time" },
    "updated_at": { "type": "string", "format": "date-time" }
  }
}
```

---

## 4. TermiNest Wire Protocol v1 (TNST-v1)

The Wire Protocol defines how the Host Orchestrator and Guest Helper exchange control, display frames, and file chunks over kernel-level Vsock streams.

### 4.1. Binary Frame Header Specification

Every packet transmitted across the Vsock socket begins with an immutable, fixed-size **16-byte header** encoded in network byte order (Big-Endian):

```
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                     Magic (0x544E5354 "TNST")                 |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|    Version    |     Flags     |          Channel ID           |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                         Payload Length                        |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                        CRC-32C Checksum                       |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                     Payload Data (Variable) ...               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

#### Header Field Breakdown:
1. **Magic Number (4 bytes):** Always `0x544E5354` (ASCII `"TNST"`). Rejects spurious data or mismatched socket connections immediately.
2. **Version (1 byte):** Protocol version. Currently `0x01`.
3. **Flags (1 byte):** Bitfield indicating payload treatment:
   - `0x00`: Plain uncompressed payload.
   - `0x01`: Payload compressed via LZ4.
   - `0x02`: Payload is JPEG image stream.
   - `0x04`: Heartbeat / Keep-alive frame (Payload Length = 0).
4. **Channel ID (2 bytes):** Multiplexed logical channel:
   - `0x0000`: **Control Channel (JSON-RPC 2.0)** - Commands, replies, errors.
   - `0x0001`: **Video Stream Channel** - Real-time screen capture frames.
   - `0x0002`: **File Transfer Channel** - Chunked binary file streams.
   - `0x0003`: **Telemetry & Metrics Channel** - Guest resource utilization (CPU, RAM).
5. **Payload Length (4 bytes, uint32_be):** Exact length of payload bytes following the header. Maximum payload size: `16,777,216` bytes (16 MB).
6. **CRC-32C Checksum (4 bytes, uint32_be):** Castagnoli CRC-32 of the Payload bytes. Hardware-accelerated on modern Intel/AMD processors via `SSE4.2` / `ARMv8 CRC32`. Packets with mismatched checksums are dropped with a protocol error.

---

### 4.2. Channel 0: Control Channel (JSON-RPC 2.0)

All administrative and computer-use commands transmit on Channel 0 as UTF-8 encoded JSON-RPC 2.0 objects.

#### RPC Command Set:

| Method | Initiator | Description |
|---|---|---|
| `guest.handshake` | Guest -> Host | Initial greeting upon Vsock connection; announces OS details, versions, active sessions. |
| `guest.ping` | Host <-> Guest | Low-overhead keep-alive round-trip. |
| `desktop.capture_frame` | Host -> Guest | Requests an immediate single display screenshot. |
| `desktop.mouse_click` | Host -> Guest | Injects mouse click event. |
| `desktop.mouse_move` | Host -> Guest | Injects cursor position update. |
| `desktop.mouse_drag` | Host -> Guest | Drags from start to end coordinate with button held down. |
| `desktop.key_sequence` | Host -> Guest | Injects key combination (modifiers + virtual key code). |
| `desktop.type_text` | Host -> Guest | Types literal Unicode text string. |
| `clipboard.read` | Host -> Guest | Retrieves current text/image on guest session clipboard. |
| `clipboard.write` | Host -> Guest | Sets content on guest session clipboard. |
| `system.exec` | Host -> Guest | Executes a shell command inside guest and captures stdout/stderr. |
| `stream.start` | Host -> Guest | Requests continuous video frame streaming on Channel 1 at specified FPS. |
| `stream.stop` | Host -> Guest | Terminates continuous video frame streaming on Channel 1. |

#### JSON-RPC Payload Examples:

##### 1. Handshake (`guest.handshake`)
```json
{
  "jsonrpc": "2.0",
  "method": "guest.handshake",
  "params": {
    "version": "1.0.0",
    "os_family": "linux",
    "os_distro": "Ubuntu 24.04 LTS",
    "kernel": "6.8.0-40-generic",
    "session": {
      "session_id": 1,
      "username": "terminest",
      "desktop_env": "wayland",
      "display_width": 1920,
      "display_height": 1080,
      "dpi": 96
    }
  },
  "id": "init-001"
}
```

##### 2. Computer Mouse Click (`desktop.mouse_click`)
```json
{
  "jsonrpc": "2.0",
  "method": "desktop.mouse_click",
  "params": {
    "x": 450,
    "y": 280,
    "button": "left",
    "clicks": 1,
    "modifiers": []
  },
  "id": "req-042"
}
```

##### Response:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "status": "success",
    "cursor_position": { "x": 450, "y": 280 }
  },
  "id": "req-042"
}
```

##### 3. Guest Shell Execution (`system.exec`)
```json
{
  "jsonrpc": "2.0",
  "method": "system.exec",
  "params": {
    "command": "git status --porcelain",
    "cwd": "/home/terminest/workspace",
    "timeout_ms": 5000,
    "env": {
      "GIT_TERMINAL_PROMPT": "0"
    }
  },
  "id": "req-043"
}
```

##### Response:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "exit_code": 0,
    "stdout": "M src/main.rs\n?? tests/new_test.rs\n",
    "stderr": "",
    "duration_ms": 42
  },
  "id": "req-043"
}
```

---

### 4.3. Channel 1: Video Stream Channel

Frames transmitted during live desktop viewing or continuous streaming travel over Channel 1.

#### Frame Data Layout (Preceding Payload Image Bytes):
```
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                      Sequence Number (uint32)                 |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                     Timestamp Unix Microseconds               |
|                                                               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|         Width (uint16)        |        Height (uint16)        |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
| Format (uint8)|  Reserved (3) |     Encoded Image Bytes ...   |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

- **Format Code:**
  - `0x01`: Raw BGRA (uncompressed, for local shared memory transfers).
  - `0x02`: JPEG (compressed, standard over Vsock).
  - `0x03`: LZ4-compressed raw BGRA.
- The viewer receives this frame on Channel 1, decodes the image if necessary, and renders it to the terminal or native window surface without interfering with JSON-RPC traffic on Channel 0.

---

### 4.4. Channel 2: File Transfer Channel

Files transferred via `push` and `pull` travel over Channel 2 in 64 KB binary chunks to avoid buffering entire gigabyte files in memory.

#### Chunk Payload Format:
1. **Transfer ID (16 bytes):** UUID identifying the file transfer session.
2. **Offset (8 bytes, uint64_be):** Byte offset in the destination file.
3. **Flags (1 byte):** `0x01` = First chunk (creates file), `0x02` = Final chunk (closes file, triggers flush).
4. **Data (Variable, up to 65,536 bytes):** Raw file bytes.

This ensures zero-allocation streaming file transfers with automatic flow control and backpressure governed by Vsock socket buffers.
