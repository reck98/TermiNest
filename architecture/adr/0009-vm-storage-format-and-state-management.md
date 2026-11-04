# ADR 0009: VM Storage Formats and Ephemeral State Management

## Status
Accepted

## Context
Virtual machines require persistent virtual disk storage for OS installation, application binaries, and user data. AI agent evaluation workflows frequently require running tests from a known, clean state and discarding all modifications upon completion. Conversely, developer development environments require preserving modifications across VM reboots.

In `terminal-mac`, VM disks are managed as single disk images on APFS with coarse manual handling.

On Windows and Linux:
- Windows Hyper-V uses **VHDX** (Virtual Hard Disk v2), supporting dynamic expansion, differencing disks, and 64 TB capacities.
- Linux QEMU/KVM uses **QCOW2** (QEMU Copy-On-Write v2/v3), supporting sparse allocation, backing files, internal snapshots, and zstd compression.

## Problem Statement
What storage formats and state management strategies should TermiNest employ to support both persistent development workflows and disposable, ephemeral evaluation runs?

## Decision
We standardize on native hypervisor storage formats combined with **Copy-On-Write (CoW) Differencing Disks**:

### 1. Storage Formats by Platform
- **Windows Host (Hyper-V):** **VHDX** (Virtual Hard Disk v2).
  - Sparse dynamic allocation with a default ceiling of 32 GB.
  - Parent/child differencing VHDX trees for branching and ephemeral execution.
- **Linux Host (QEMU/KVM) & Windows Fallback (QEMU/WHP):** **QCOW2** (QEMU Copy-On-Write v3).
  - Sparse dynamic allocation with cluster size 64 KB and optional zstd compression for base distribution templates.
  - Backing file hierarchies (`qemu-img create -f qcow2 -b base.qcow2 -F qcow2 diff.qcow2`).

### 2. Ephemeral Execution Mode (`--ephemeral`)
When a machine is started with `--ephemeral`:
1. The orchestrator leaves the base parent disk image completely unopened for write operations (`read_only: true`).
2. A temporary differencing disk overlay is spawned in a designated temp directory:
   - Windows: Child VHDX (`temp_diff_<uuid>.vhdx`) pointing to parent VHDX.
   - Linux: Child QCOW2 (`temp_diff_<uuid>.qcow2`) with backing file pointing to parent QCOW2.
3. The VM boots and executes against the temporary differencing overlay. All OS updates, downloaded files, and test artifacts accumulate exclusively in the child disk.
4. When the machine stops (`terminest stop` or CLI exit), the child differencing disk is **immediately deleted and securely unlinked**.
5. The base golden image remains completely unaltered, guaranteeing bit-for-bit repeatability for subsequent runs.

### 3. Persistent Execution Mode (Default)
When run without `--ephemeral`, changes write directly to the machine's primary disk image, preserving state across sessions.

## Rationale
- **Instantaneous Reset:** In ephemeral mode, resetting a virtual machine to a clean state takes under 50 milliseconds (deleting a small temporary file and creating a new empty differencing overlay), compared to minutes spent re-copying gigabyte-sized disk images.
- **Minimal Host Disk Usage:** Multiple concurrent VMs can share the same read-only parent base image while consuming only a few megabytes of physical host disk space for their active differencing deltas.
- **Zero Base Image Corruption:** Base golden images are protected from filesystem corruption or malware contamination.

## Consequences

### Positive
- Sub-second VM provisioning and teardown for CI/CD and agent benchmark test suites.
- Significant disk storage savings through shared base image deduplication.
- Safe exploration: AI agents can execute hazardous operations without permanently ruining the base development environment.

### Negative / Trade-Offs
- Deep differencing disk trees (more than 3-4 levels) can introduce minor read latency; TermiNest limits ephemeral runs to a single differencing layer over the golden base.
- Deleting a parent base disk while child differencing disks are active will corrupt child state; TermiNest locks base images while child instances are active.
