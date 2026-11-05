# TermiNest Milestone 1: Second Independent Re-verification Audit Report

- **Audit Date:** 2026-11-05
- **Auditor:** Independent Principal Systems Architect & Reliability/Security Reviewer
- **Target Repository:** `terminest`
- **Baseline Commit (M0 Closure):** `dc5cf701bd1fdcbd659cd22d10de71be1fd2a147`
- **Evaluated Commit (M1 Correction Head):** `324be04d97c0a7330b441cf1dd24760b464ce1e5`
- **Target Branch:** `main`
- **Working Tree Status:** Clean (0 untracked files prior to this report)
- **Primary Scope:** Verification of the five M1 audit corrections (`CONC-M1-01`, `AUDIT-M1-01`, `LIFE-M1-01`, `CLI-M1-01`, `CLI-M1-02`), in-depth critical race and durability investigations, adversarial security review, quality gate reproduction, and M0 non-regression validation.

---

## 1. Executive Verdict

### **Verdict: FAIL**

While the engineering team successfully implemented robust solutions for **`LIFE-M1-01`** (lifecycle deletion invariants), **`CLI-M1-01`** (exit-code semantics), and **`CLI-M1-02`** (interactive and non-interactive delete confirmation), and correctly replaced external dependencies with native Windows `LockFileEx` / Unix `flock` primitives for **`CONC-M1-01`**, two substantive defects prevent formal milestone acceptance:

1. **`AUDIT-M1-02` (High): Machine Deletion Destroys Entire Audit Trail**
   In Critical Investigation B, live empirical testing and source code inspection confirmed that `audit.jsonl` is stored inside `<data_dir>/machines/<id>/`. When `delete_machine` completes, the orchestrator invokes `fs::remove_dir_all(&dir)`, permanently erasing the machine's directory, the final deletion event, and the machine's entire lifetime audit history. It is neither archived nor recoverable by operators, violating STRIDE-R1 (non-repudiation) requirements.
2. **`RACE-M1-01` (Medium): Machine Deletion Lock Release Interleaving**
   In Critical Investigation A, source code inspection revealed that `delete_machine` explicitly drops `machine_file_lock` before calling `fs::remove_dir_all`. Because mutating operations (`start_machine`, `stop_machine`, etc.) do not acquire the repository-wide lock (`repo_lock`), a concurrent mutating process can acquire `.machine.lock` during this window, recreate the machine folder via `create_dir_all`, or fall back to stale in-memory metadata via `unwrap_or(record)`.

Milestone 1 cannot be certified as production-ready until audit logs survive machine deletion and the deletion concurrency window is closed.

---

## 2. Repository and Environment Details

- **Host Operating System:** Windows 11 Enterprise (x86_64)
- **Host Architecture:** 64-bit AMD64 / Intel x86_64
- **Rust Toolchain:** `rustc 1.83.0` / `cargo 1.83.0`
- **Repository Path:** `E:\Development\Projects\terminest`
- **Evaluated Git Commits (Correction Phase):**
  - `ae5173b` — `fix(core): add cross-process repository locking and serialize mock backend state`
  - `5e41c1f` — `feat(core): enforce lifecycle deletion invariants and record structured audit events`
  - `49ab753` — `fix(cli): normalize operational exit codes and enforce delete confirmation`
  - `324be04` — `docs(m1): author M1 correction report and update implementation records`

---

## 3. Independent Quality Gate Reproduction

All four standard quality gates were independently executed against commit `324be04d97c0a7330b441cf1dd24760b464ce1e5`:

```bash
# 1. Code Formatting
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
cargo fmt --all -- --check
# Result: PASS (Exit code: 0, 0 diffs)

# 2. Strict Compiler Linter
cargo clippy --workspace --all-targets --all-features -- -D warnings
# Result: PASS (Exit code: 0, 0 warnings)

# 3. Comprehensive Test Suite
cargo test --workspace --all-features
# Result: PASS (Exit code: 0, 83 passed, 0 failed, 0 ignored across 10 binaries)

# 4. Documentation Tests
cargo test --workspace --doc
# Result: PASS (Exit code: 0, 0 failed)
```

### Test Count Breakdown by Package

| Package / Test Binary | Type | Passed | Failed | Ignored | Result |
|---|---|---|---|---|---|
| `apps/terminest-cli` (`cli_tests`) | Integration | 10 | 0 | 0 | **PASS** |
| `crates/terminest-core` (lib unit tests) | Unit | 3 | 0 | 0 | **PASS** |
| `crates/terminest-core` (`backend_tests`) | Integration | 4 | 0 | 0 | **PASS** |
| `crates/terminest-core` (`domain_tests`) | Integration / Prop | 10 | 0 | 0 | **PASS** |
| `crates/terminest-core` (`orchestration_tests`) | Integration | 8 | 0 | 0 | **PASS** |
| `crates/terminest-core` (`persistence_tests`) | Integration | 4 | 0 | 0 | **PASS** |
| `crates/terminest-protocol` (lib unit tests) | Unit | 26 | 0 | 0 | **PASS** |
| `crates/terminest-protocol` (`protocol_fuzz`) | Prop / Fuzz | 4 | 0 | 0 | **PASS** |
| `crates/terminest-transport` (lib unit tests) | Unit | 11 | 0 | 0 | **PASS** |
| `crates/terminest-transport` (`transport_integration`) | Integration | 3 | 0 | 0 | **PASS** |
| **Workspace Total** | | **83** | **0** | **0** | **PASS** |

### Milestone 0 Non-Regression Validation

Comparison between M0 baseline `dc5cf701bd1fdcbd659cd22d10de71be1fd2a147` and HEAD:
```bash
git diff --stat dc5cf70..HEAD -- crates/terminest-protocol crates/terminest-transport
```
**Result:** 0 files modified, 0 insertions, 0 deletions. Milestone 0 wire framing, JSON-RPC codecs, and transport implementations remain completely untouched and verified passing (44/44 tests).

---

## 4. Re-Audit of the Five Original Findings

### 4.1 CONC-M1-01 — Cross-Process Concurrency Protection
- **Original Severity:** High
- **Re-Audit Status:** **PARTIALLY RESOLVED / DEFECT IDENTIFIED**
- **Verification Evidence:**
  1. *Native OS Implementation:* `crates/terminest-core/src/persistence/lock.rs` implements `FileLock` using Windows `kernel32::LockFileEx` (with `LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY`) and `UnlockFileEx` on Windows, and `libc::flock` (`LOCK_EX | LOCK_NB`) on Unix. No third-party dependency collisions occurred.
  2. *OS Kernel Cleanup:* Kernel locks are automatically cleaned up on process exit or abnormal termination (crash, SIGKILL, `std::process::exit`).
  3. *Hierarchical Lock Protocol:* Locking hierarchy is maintained consistently:
     Level 1: Repository inventory lock (`.repo.lock`) $\rightarrow$
     Level 2: Per-machine lock (`.machine.lock`) $\rightarrow$
     Level 3: Backend state lock (`mock_backend.lock`).
  4. *Mock Backend Serialization:* `MockHypervisorBackend` acquires `mock_backend.lock` and reloads latest state from disk via `reload_from_disk()` under lock before mutating in-memory entries, persisting back atomically via temporary files.
  5. *Multi-Process CLI Verification:* Tested with separate OS processes running concurrently:
     - Scenario 1 (Two processes creating the same machine): One process succeeded; the other safely failed with `Error: Machine 'vm-race' already exists`. No corrupted state occurred.
     - Scenario 2 (Start vs Stop race): Both operations serialized safely under the machine lock.
     - Scenario 3 (Concurrent operations on different machines): Executed in parallel without mutual interference.
- **Defects Identified:** See Section 5 (Critical Investigation A: `RACE-M1-01`) and Section 8 (`CONC-M1-02`).

---

### 4.2 AUDIT-M1-01 — Structured Lifecycle Audit Logging
- **Original Severity:** Medium
- **Re-Audit Status:** **PARTIALLY RESOLVED / DEFECT IDENTIFIED**
- **Verification Evidence:**
  1. *Schema and Versioning:* `crates/terminest-core/src/domain/audit.rs` defines a typed `AuditRecord` with schema version `"1.0.0"`, unique UUID `event_id`, RFC3339 `timestamp`, `machine_id`, `machine_name`, `action`, `phase`, `actor`, `state_before`, `state_after`, `result`, and optional `error_message`.
  2. *Durability on Disk:* `crates/terminest-core/src/persistence/audit.rs` (`AuditLogger::append`) opens `audit.jsonl` in append mode and calls `file.sync_all()` after writing every record.
  3. *Orchestrator Integration:* Audit events are emitted across `create`, `start`, `stop`, `pause`, `resume`, `delete`, and `reconcile`.
- **Defects Identified:**
  - Audit history is stored inside `<data_dir>/machines/<id>/` and is deleted when the machine is deleted. See Section 6 (Critical Investigation B: `AUDIT-M1-02`).
  - All 13 call sites in `service.rs` silently discard append errors (`let _ = self.repository.append_audit_event(...)`). See Section 8 (`AUDIT-M1-03`).

---

### 4.3 LIFE-M1-01 — Enforce Deletion Lifecycle Invariants
- **Original Severity:** Medium
- **Re-Audit Status:** **RESOLVED**
- **Verification Evidence:**
  1. *State Validation:* In `Orchestrator::delete_machine` (`crates/terminest-core/src/orchestrator/service.rs`), when `force = false`, the method explicitly invokes `record.state.transition_to(MachineState::Deleting)?`.
  2. *Refusal Matrix:* Deletion without `--force` was verified to reject:
     - `Running` $\rightarrow$ `CannotDeleteWhileRunning`
     - `Starting`, `Pausing`, `Paused`, `Resuming`, `Stopping` $\rightarrow$ `InvalidTransition`
  3. *Allowed Normal Deletion:* Normal deletion is permitted only for `Defined`, `Created`, `Stopped`, `Failed`, and `Unknown`.
  4. *Force Deletion:* When `force = true`, active or transitional states (`Running`, `Paused`, `Pausing`, `Stopping`, `Starting`, `Resuming`) trigger an explicit forced shutdown on the backend:
     ```rust
     backend.stop_machine(&id, Duration::from_secs(3), true).await?;
     ```
     Only after the backend acknowledges the stop does deletion proceed.

---

### 4.4 CLI-M1-01 — Correct Exit-Code Mapping
- **Original Severity:** Low
- **Re-Audit Status:** **RESOLVED**
- **Verification Evidence:**
  1. *Clap Syntax / Argument Parsing Errors:*
     - Command: `terminest --invalid-flag` $\rightarrow$ **Exit Code: 2**
     - Command: `terminest create --unknown-flag` $\rightarrow$ **Exit Code: 2**
  2. *Operational / Domain / Persistence Errors:*
     - Command: `terminest start nonexistent-vm` $\rightarrow$ **Exit Code: 1** (`Error: Machine not found: nonexistent-vm` printed to stderr)
     - Command: `terminest delete nonexistent-vm --confirm` $\rightarrow$ **Exit Code: 1**
     - Command: `terminest delete active-vm --confirm` (without force) $\rightarrow$ **Exit Code: 1** (`Error: Lifecycle state error: Cannot delete virtual machine while it is running`)
  3. *Successful Operations:*
     - Command: `terminest create --name test-vm --cpus 2 --ram 2048 --disk 10` $\rightarrow$ **Exit Code: 0**
     - Command: `terminest inspect test-vm` $\rightarrow$ **Exit Code: 0**
  4. *Stream Routing:* All error outputs are directed to `stderr` (`eprintln!`), while normal output and JSON payloads are directed to `stdout` (`println!`).

---

### 4.5 CLI-M1-02 — Enforce Delete Confirmation
- **Original Severity:** Low
- **Re-Audit Status:** **RESOLVED**
- **Verification Evidence:**
  1. *Non-Interactive Invocations:*
     - Executing `terminest delete <machine>` in non-interactive mode without `--confirm` immediately aborts:
       ```text
       Error: Operation aborted: Deletion in non-interactive mode requires the '--confirm' flag
       ```
       Exit code: `1`. The machine is not deleted.
  2. *Orthogonality of Flags:*
     - Executing `terminest delete <machine> --force` in non-interactive mode without `--confirm` also aborts with exit code `1`. `--force` cannot bypass confirmation.
  3. *Piped / Scripted Input:*
     - Piped input (`"no" | terminest delete <machine>` or `"" | terminest delete <machine>`) detects `!is_terminal()`, fails closed, and requires `--confirm`.
  4. *Interactive TTY Semantics:*
     - Code inspection of `apps/terminest-cli/src/handlers.rs` lines 277-306 shows that on interactive terminals (`io::stdin().is_terminal()`), the prompt explicitly defaults to refusal: `[y/N]`. Empty input, EOF, or anything other than `y` / `yes` returns `CoreError::OperationAborted`.

---

## 5. Critical Investigation A — Machine Deletion Concurrency Race

### Investigation Scope
The M1 correction report states:
> *"The machine file lock handle is explicitly dropped prior to calling `repository.delete(&id)` to prevent Windows access violations (`ERROR_ACCESS_DENIED`) during recursive directory removal."*

This audit evaluated whether another process can interleave between lock release and directory removal.

### Source Analysis
In `crates/terminest-core/src/orchestrator/service.rs`:
```rust
460: pub async fn delete_machine(&self, selector: &str, force: bool) -> Result<(), CoreError> {
461:     // Level 1: Acquire repository inventory lock
462:     let _repo_lock = self.repository.acquire_repo_lock(Duration::from_secs(5)).await?;
...
471:     let machine_file_lock = self.repository.acquire_machine_lock(&id, Duration::from_secs(5)).await?;
...
518:     drop(machine_file_lock); // <-- MACHINE LOCK DROPPED HERE
519:
520:     // Delete from repository
521:     self.repository.delete(&id).await?; // <-- DIRECTORY REMOVAL EXECUTES HERE
522:     Ok(())
523: }
```

Now inspect `start_machine` (lines 140-156) and `stop_machine` (lines 224-245):
```rust
140: pub async fn start_machine(&self, selector: &str) -> Result<MachineStatusView, CoreError> {
141:     let record = self.resolve_machine(selector).await?;
142:     let id = record.config.id;
143:
144:     // Level 2: Acquire per-machine lock
145:     let _machine_file_lock = self.repository.acquire_machine_lock(&id, Duration::from_secs(5)).await?;
...
153:     let record = self.repository.get_by_id(&id).await?.unwrap_or(record);
```

### Architectural Vulnerability (`RACE-M1-01`)
1. **`repo_lock` Asymmetry:** While `delete_machine` and `create_machine` acquire `repo_lock` (Level 1), `start_machine`, `stop_machine`, `pause_machine`, and `resume_machine` **only** acquire `machine_file_lock` (Level 2). They do not check or wait on `repo_lock`.
2. **The Interleaving Window:**
   - At line 518, `delete_machine` drops `machine_file_lock`.
   - Before line 521 calls `fs::remove_dir_all(&dir)`, Process B (calling `start_machine` or `stop_machine`) executes `acquire_machine_lock(&id)`.
   - Because `machine_file_lock` is unlocked, Process B successfully acquires the lock.
   - Process B now has an open handle to `machines/<id>/.machine.lock`.
   - When Process A proceeds to `fs::remove_dir_all(&dir)` on Windows, if Process B created or opened handles, file deletion or directory unlinking can return `ERROR_ACCESS_DENIED` or `ERROR_SHARING_VIOLATION`.
3. **Directory Recreation by Stale Operations:**
   - In `crates/terminest-core/src/persistence/lock.rs` line 85:
     ```rust
     if let Some(parent) = path.parent() {
         fs::create_dir_all(parent).map_err(PersistenceError::Io)?;
     }
     ```
   - If Process B acquires `acquire_machine_lock(&id)` right as the directory is removed, `FileLock` recreates the directory `<data_dir>/machines/<id>/` and creates `.machine.lock`.
   - But `machine.json` has already been deleted!
   - This leaves an orphaned, empty machine directory containing only `.machine.lock`.
4. **Stale Metadata Fallback:**
   - At line 153 of `start_machine`:
     ```rust
     let record = self.repository.get_by_id(&id).await?.unwrap_or(record);
     ```
     If `get_by_id` returns `None` (because the directory was deleted), `start_machine` falls back to `record`—the pre-resolved record loaded before acquiring the lock! It then proceeds to call `record.state.transition_to(MachineState::Starting)?` and `update_state`, which eventually fails with `PersistenceError::NotFound`.

---

## 6. Critical Investigation B — Audit History After Deletion

### Investigation Scope
Evaluate the exact persistence, location, and post-deletion durability of `audit.jsonl`.

### Source Analysis
1. In `crates/terminest-core/src/persistence/fs.rs`:
   ```rust
   pub fn machine_dir(&self, id: &MachineId) -> PathBuf {
       self.machines_dir.join(id.to_string())
   }
   ```
2. In `crates/terminest-core/src/persistence/audit.rs`:
   ```rust
   pub fn append(machine_dir: &Path, record: &AuditRecord) -> Result<(), PersistenceError> {
       let audit_path = machine_dir.join("audit.jsonl");
       ...
   }
   ```
3. In `crates/terminest-core/src/orchestrator/service.rs`:
   ```rust
   // Record deletion audit event before removing directory
   let delete_event = AuditRecord::completed(
       id,
       record.config.name.clone(),
       AuditAction::Delete,
       Some(record.state),
       Some(MachineState::Deleting),
       "orchestrator",
   );
   let _ = self.repository.append_audit_event(&id, &delete_event).await;

   drop(machine_file_lock);

   // Delete from repository
   self.repository.delete(&id).await?;
   ```
4. In `crates/terminest-core/src/persistence/fs.rs`:
   ```rust
   async fn delete(&self, id: &MachineId) -> Result<(), PersistenceError> {
       let _guard = self.lock.lock().await;
       let dir = self.machine_dir(id);
       if dir.exists() {
           fs::remove_dir_all(&dir).map_err(PersistenceError::Io)?;
       }
       Ok(())
   }
   ```

### Live Empirical Reproduction (`AUDIT-M1-02`)
An isolated disposable test repository was created using `terminest` CLI commands:
```powershell
# 1. Create a machine
terminest --data-dir $tmp create --name audit-vm --cpus 2 --ram 2048 --disk 10
# Observed: File created at: $tmp\machines\<id>\audit.jsonl
# Content:
# {"schema_version":"1.0.0","event_id":"...","timestamp":"...","machine_id":"...","machine_name":"audit-vm","action":"create","phase":"completed","actor":"orchestrator","state_before":"defined","state_after":"created","result":"success"}

# 2. Delete the machine
terminest --data-dir $tmp delete audit-vm --confirm
# Observed: "Machine 'audit-vm' successfully deleted." (Exit code: 0)

# 3. Search for any audit.jsonl anywhere in repository
Get-ChildItem -Recurse $tmp -Filter "*audit*"
# Observed: 0 files found.
```

### Audit Findings for Specific Questions
1. **Does the deletion event survive?**
   **NO.** The deletion completion event is appended to `<data_dir>/machines/<id>/audit.jsonl` and immediately destroyed when `remove_dir_all(&dir)` runs milliseconds later.
2. **Does previous machine audit history survive?**
   **NO.** The entire historical audit trail of the machine (creation, boot, shutdowns, failures) is permanently deleted.
3. **Is it archived elsewhere?**
   **NO.** There is no central audit repository, archive directory, or host-wide audit log.
4. **Can an operator retrieve the history after deletion?**
   **NO.** `repository.read_audit_events(&id)` references `self.machine_dir(id)`, which no longer exists.
5. **Can a crash occur after recording deletion but before removing the directory?**
   **YES.** If the system halts after `append_audit_event` and before `remove_dir_all`, the directory remains with `machine.json` in state `deleting` and `audit.jsonl` containing the deletion event.
6. **Can a crash occur after directory removal but before the operation is reported?**
   **YES.** The directory is purged from disk, but the caller receives an abnormal exit or interruption.
7. **Verdict on Durability:**
   Storing audit trails inside the container being destroyed violates the fundamental definition of audit durability and non-repudiation (STRIDE-R1/R2).

---

## 7. Lifecycle and Deletion Verification Matrix

| Machine State | Normal Deletion (`!force`) | Force Deletion (`force = true`) | Verified Code Behavior |
|---|---|---|---|
| **Defined** | **Allowed** | **Allowed** | Transitions to `Deleting`; backend and repository purged. |
| **Created** | **Allowed** | **Allowed** | Transitions to `Deleting`; backend and repository purged. |
| **Starting** | **Rejected** (`InvalidTransition`) | **Allowed** | Triggers forced `backend.stop_machine` (timeout: 3s); then deleted. |
| **Running** | **Rejected** (`CannotDeleteWhileRunning`) | **Allowed** | Triggers forced `backend.stop_machine` (timeout: 3s); then deleted. |
| **Pausing** | **Rejected** (`InvalidTransition`) | **Allowed** | Triggers forced `backend.stop_machine` (timeout: 3s); then deleted. |
| **Paused** | **Rejected** (`InvalidTransition`) | **Allowed** | Triggers forced `backend.stop_machine` (timeout: 3s); then deleted. |
| **Resuming** | **Rejected** (`InvalidTransition`) | **Allowed** | Triggers forced `backend.stop_machine` (timeout: 3s); then deleted. |
| **Stopping** | **Rejected** (`InvalidTransition`) | **Allowed** | Triggers forced `backend.stop_machine` (timeout: 3s); then deleted. |
| **Stopped** | **Allowed** | **Allowed** | Clean deletion without stopping backend. |
| **Failed** | **Allowed** | **Allowed** | Direct deletion from backend and repository. |
| **Unknown** | **Allowed** | **Allowed** | Direct deletion from backend and repository. |
| **Deleting** | **Allowed** (Idempotent) | **Allowed** (Idempotent) | `state == target` returns `Ok(())`; retries cleanup. |

---

## 8. New Audit Findings

### Summary of New Findings

| Finding ID | Severity | Category | Description | Status |
|---|---|---|---|---|
| **AUDIT-M1-02** | **High** | Durability / Security | Machine deletion erases `audit.jsonl`, destroying entire historical audit log. | **Open** |
| **RACE-M1-01** | **Medium** | Concurrency | `delete_machine` drops `.machine.lock` before `remove_dir_all`, permitting concurrent interleaving. | **Open** |
| **AUDIT-M1-03** | **Low** | Observability | Orchestrator silently discards all audit write errors (`let _ = append_audit_event`). | **Open** |
| **IDEM-M1-01** | **Low** | Robustness | `delete_machine` fails on retry if backend machine was already removed (`MachineNotFound`). | **Open** |
| **CONC-M1-02** | **Low** | Concurrency | `get_machine_status` mutates persistent state and audit log without acquiring `.machine.lock`. | **Open** |

---

### Detailed Findings

#### Finding AUDIT-M1-02: Machine Deletion Destroys Entire Historical Audit Trail
- **Severity:** High
- **Category:** Durability / Security
- **File & Lines:** `crates/terminest-core/src/persistence/fs.rs:235-242`, `crates/terminest-core/src/orchestrator/service.rs:506-522`
- **Observed Behavior:** Each machine's audit log is placed at `<data_dir>/machines/<id>/audit.jsonl`. When `delete_machine` executes, it records the deletion event to that file, drops the lock, and calls `repository.delete(&id)`, which executes `fs::remove_dir_all`. All audit records for the machine are permanently erased.
- **Expected Behavior:** Audit trails must be durable across the complete machine lifecycle, including post-deletion. In accordance with STRIDE-R1 (non-repudiation), an operator must be able to inspect what actions an AI agent or user took on a deleted machine.
- **Impact:** Complete loss of auditability and compliance trails upon machine decommission.
- **Recommendation:** Store audit logs outside the ephemeral machine directory (e.g. `<data_dir>/audit/<id>.jsonl` or archive `<data_dir>/machines/<id>/audit.jsonl` to `<data_dir>/archive/audit/<id>.jsonl` before deleting the directory).

---

#### Finding RACE-M1-01: Machine Deletion Releases Machine Lock Before Directory Removal
- **Severity:** Medium
- **Category:** Concurrency / Integrity
- **File & Lines:** `crates/terminest-core/src/orchestrator/service.rs:517-522`
- **Observed Behavior:** `delete_machine` calls `drop(machine_file_lock)` at line 518 before calling `repository.delete(&id)` at line 521. Because `start_machine`, `stop_machine`, `pause_machine`, and `resume_machine` only acquire `machine_file_lock` (and not `repo_lock`), a concurrent operation can acquire the lock in this interval.
- **Impact:**
  - File locking or directory unlinking failure on Windows (`ERROR_ACCESS_DENIED`).
  - Concurrent `start_machine` recreation of an orphaned directory containing only `.machine.lock`.
  - Fallback to stale metadata via `unwrap_or(record)`.
- **Recommendation:**
  - Either acquire a shared read-lock on `repo_lock` during `start`/`stop`/`pause`/`resume` so `delete_machine` (holding exclusive `repo_lock`) blocks all lifecycle operations on any machine until directory deletion completes;
  - OR maintain machine locks in a dedicated directory outside the machine tree (e.g. `<data_dir>/run/locks/<id>.lock`), allowing the lock to remain held across the `fs::remove_dir_all` call.

---

#### Finding AUDIT-M1-03: Silent Error Suppression on Audit Append Failures
- **Severity:** Low
- **Category:** Reliability / Observability
- **File & Lines:** `crates/terminest-core/src/orchestrator/service.rs:106, 131, 184, 216, 271, 302, 346, 372, 415, 452, 515, 554, 579`
- **Observed Behavior:** At all 13 audit append call sites, the returned `Result<(), PersistenceError>` is discarded with `let _ = ...`.
- **Expected Behavior:** If writing an audit event fails (e.g., disk full, write error), the failure should at least log a `tracing::error!` warning, or fail closed if audit compliance is mandatory.
- **Impact:** Operators receive no indication when audit logging fails silently.
- **Recommendation:** Replace `let _ = ...` with explicit error logging:
  ```rust
  if let Err(e) = self.repository.append_audit_event(&id, &event).await {
      tracing::error!("Failed to record audit event for machine {}: {}", id, e);
  }
  ```

---

#### Finding IDEM-M1-01: Non-Idempotent Backend Deletion Recovery
- **Severity:** Low
- **Category:** Robustness / Error Recovery
- **File & Lines:** `crates/terminest-core/src/orchestrator/service.rs:501-504`, `crates/terminest-core/src/backend/mock.rs:407`
- **Observed Behavior:** If backend deletion succeeds but repository directory deletion fails (e.g., temporary I/O lock), a retry of `delete_machine` invokes `backend.delete_machine(&id).await?`. The backend returns `Err(HypervisorError::MachineNotFound(id))`. Because `delete_machine` uses `?`, the entire retry fails with `CoreError::Backend(...)`, preventing the operator from completing the deletion of the repository directory.
- **Impact:** Retrying deletion on an incompletely deleted machine gets stuck.
- **Recommendation:** Ignore `HypervisorError::MachineNotFound` during backend deletion:
  ```rust
  if let Err(e) = backend.delete_machine(&id).await {
      if !matches!(e, HypervisorError::MachineNotFound(_)) {
          return Err(CoreError::Backend(e));
      }
  }
  ```

---

#### Finding CONC-M1-02: Status Reconciliation Mutates State and Audit Log Without Machine Lock
- **Severity:** Low
- **Category:** Concurrency
- **File & Lines:** `crates/terminest-core/src/orchestrator/service.rs:526-600`
- **Observed Behavior:** `get_machine_status` queries the hypervisor backend and updates repository state and audit logs when a discrepancy is detected, but does not acquire `machine_file_lock`.
- **Impact:** If `status` is queried concurrently with `start` or `stop`, both processes can attempt to update `machine.json` and append to `audit.jsonl` simultaneously.
- **Recommendation:** Acquire `machine_file_lock` in `get_machine_status` before calling `update_state` and `append_audit_event`.

---

## 9. Remaining Limitations

1. **Simulated Virtualization:** Real Hyper-V and QEMU hypervisor drivers remain deliberately unbuilt (scheduled for Milestone 2). Current tests operate against `MockHypervisorBackend`.
2. **Platform Verification Scope:** Windows file locking and TTY interaction were verified live on Windows 11. Unix `flock` implementation was verified through rigorous source inspection; live Unix verification remains pending execution in a Linux/macOS CI environment.

---

## 10. Final Recommendation

**Milestone 1 is:** `NOT READY FOR FORMAL ACCEPTANCE (CORRECTIONS REQUIRED)`.

### Required Actions Before Milestone Acceptance:
1. **Resolve `AUDIT-M1-02`:** Move or archive audit logs so they persist permanently after machine deletion.
2. **Resolve `RACE-M1-01`:** Close the concurrency window between `.machine.lock` release and directory deletion by relocating lock files outside the machine directory or utilizing repository-level synchronization.
3. **Resolve `AUDIT-M1-03` & `IDEM-M1-01`:** Add error logging for audit append failures and treat `MachineNotFound` as idempotent success during backend deletion.

Once these specific corrections are implemented and verified, Milestone 1 will be ready for formal sign-off.
