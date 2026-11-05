# TermiNest — Milestone 1 (M1) Correction Report

**Project:** TermiNest  
**Milestone:** Milestone 1 (M1): Core Host Orchestrator & CLI Foundation  
**Phase:** Audit Findings Correction & Preparation for Re-Audit  
**Date:** November 2026  
**Lead Engineer:** Senior Rust Systems Engineer & Application Security Engineer  
**Host Platform:** Windows 11 (64-bit, x86_64), Rust 1.99.1 (`stable-x86_64-pc-windows-gnu`)  
**Audit Baseline:** [`architecture/implementation/M1-verification-audit.md`](M1-verification-audit.md)  
**M0 Baseline Integrity:** 100% Intact & Verified (44/44 M0 tests passing)  

---

## A. Executive Summary

Following the formal independent verification audit of TermiNest Milestone 1, this controlled correction phase addressed all five actionable audit findings (`CONC-M1-01`, `AUDIT-M1-01`, `LIFE-M1-01`, `CLI-M1-01`, `CLI-M1-02`). 

The corrections were designed and implemented while strictly maintaining the existing domain models, provider-independent `HypervisorBackend` SPI, deep module boundaries in `terminest-core`, and bit-for-bit regression safety across all Milestone 0 protocol and transport components.

### Findings Resolution Ledger

| Finding ID | Original Severity | Category | Status | Corrective Action Summary |
|---|:---:|---|:---:|---|
| **`CONC-M1-01`** | **High** | Concurrency / Multi-Process | **FIXED** | Implemented OS-level advisory file locks (`FileLock`) across processes for repository inventory (`.repo.lock`), per-machine lifecycle (`.machine.lock`), and mock hypervisor state (`mock_backend.lock`). Enforced strict lock ordering and atomic disk reload/replacement. |
| **`AUDIT-M1-01`** | **Medium** | Compliance / STRIDE-R1/R2 | **FIXED** | Designed typed `AuditRecord` schema (v1.0.0) and `AuditLogger` append-only engine writing flush-synced JSON-Lines records to `<machine_dir>/audit.jsonl` on every lifecycle transition and failure. |
| **`LIFE-M1-01`** | **Medium** | Domain Invariants | **FIXED** | Enforced `transition_to(MachineState::Deleting)` in `Orchestrator::delete_machine` when `!force`. Intermediate transitional states (`Stopping`, `Pausing`, `Starting`, `Paused`) strictly require `--force`, which forces an active backend stop before deletion. |
| **`CLI-M1-01`** | **Low** | CLI Protocol / POSIX Codes | **FIXED** | Normalized exit codes so that all operational, backend, persistence, and domain errors return exit code `1`, reserving exit code `2` exclusively for invalid CLI flags and syntax parsing errors. |
| **`CLI-M1-02`** | **Low** | User Safety / CLI Ergonomics | **FIXED** | Enforced destructive delete confirmation: interactive TTY prompts `[y/N]` (defaulting to abort); non-interactive environments require `--confirm`. Confirmed independence of `--force` and `--confirm`. |

**Total Findings Remaining:** 0  
**Quality Gates:** All 4 gates passing (fmt, clippy `-D warnings`, 83 tests passing across workspace, 0 doc-test failures).  
**Recommendation:** **READY FOR INDEPENDENT RE-AUDIT**

---

## B. Original Audit Findings & Root Cause Analysis

### 1. `CONC-M1-01` (High) — Cross-Process Concurrency Protection
- **Audit Finding:** In-memory `tokio::sync::Mutex` in `Orchestrator` and `FilesystemMachineRepository` provided zero mutual exclusion across independent CLI processes. Separate CLI invocations could race on `machine.json` and `mock_backend.json`, risking Windows sharing violations (`ERROR_ACCESS_DENIED`), duplicate name collisions, and state clobbering.
- **Root Cause:** In M1 initial implementation, mutual exclusion was designed around async in-process mutexes without OS-level kernel file locking primitives.
- **Correction Implemented:**
  1. Created `crates/terminest-core/src/persistence/lock.rs` implementing cross-platform advisory file locking via Windows `kernel32::LockFileEx` / `UnlockFileEx` (and `libc::flock` on Unix).
  2. Implemented hierarchical lock ordering:
     - Level 1: Repository inventory lock (`<machines_dir>/.repo.lock`)
     - Level 2: Per-machine lock (`<machines_dir>/<id>/.machine.lock`)
     - Level 3: Mock backend state lock (`<run_dir>/mock_backend.lock`)
  3. Updated `MockHypervisorBackend::with_state_file` to reload the freshest disk state under `mock_backend.lock` prior to every mutation and use atomic file replacement (`tmp.<uuid>` + `sync_all` + `rename`).
- **Evidence:** Verified by `test_file_lock_mutual_exclusion_same_process`, `test_file_lock_timeout`, `test_mock_backend_concurrent_state_file_synchronization`, and real multi-process CLI test `test_cli_cross_process_create_collision_race`.

### 2. `AUDIT-M1-01` (Medium) — Structured Lifecycle Audit Logging
- **Audit Finding:** `fs.rs` created an empty `audit.jsonl` file upon machine creation, but no events were ever written to it across any lifecycle operation.
- **Root Cause:** The M1 implementation established directory scaffolding for the audit ledger but deferred audit event emission logic.
- **Correction Implemented:**
  1. Authored `crates/terminest-core/src/domain/audit.rs` defining versioned, typed `AuditRecord`, `AuditAction`, `AuditPhase`, and `AuditResult`.
  2. Implemented `AuditLogger::append` in `crates/terminest-core/src/persistence/audit.rs` writing UTF-8 JSON-Lines with `sync_all` durability.
  3. Integrated audit emission into `Orchestrator` methods: `create_machine`, `start_machine`, `stop_machine`, `pause_machine`, `resume_machine`, `delete_machine`, and reconciliation updates in `get_machine_status`.
- **Evidence:** Verified by `test_audit_logger_append_and_read`, `test_orchestrator_audit_trail_recorded_across_lifecycle`, and `test_cli_audit_log_written_to_disk`.

### 3. `LIFE-M1-01` (Medium) — Enforce Deletion Lifecycle Invariants
- **Audit Finding:** `delete_machine` relied solely on `if record.state.is_running()`. Machines in `Stopping`, `Pausing`, or `Paused` returned `false` for `is_running()`, bypassing lifecycle invariant checks and allowing deletion without `--force`.
- **Root Cause:** Incomplete predicate in `service.rs:281` which failed to check `record.state.transition_to(MachineState::Deleting)`.
- **Correction Implemented:**
  1. In `Orchestrator::delete_machine`, replaced `is_running()` check with:
     ```rust
     if !force {
         record.state.transition_to(MachineState::Deleting)?;
     } else if record.state.is_running()
         || record.state == MachineState::Paused
         || record.state == MachineState::Pausing
         || record.state == MachineState::Stopping
     {
         backend.stop_machine(&id, Duration::from_secs(3), true).await?;
     }
     ```
  2. For non-force deletion, all non-deletable states (`Starting`, `Running`, `Pausing`, `Paused`, `Resuming`, `Stopping`) are rejected with typed `LifecycleError`.
  3. For force deletion, active and transitional machines are actively stopped on the backend and confirmed stopped before unlinking.
- **Evidence:** Verified by `test_orchestrator_rejects_deleting_transitional_states_without_force`, `test_orchestrator_deletion_all_eligibility_states`, and `test_cli_delete_confirmation_semantics`.

### 4. `CLI-M1-01` (Low) — Correct Exit-Code Mapping
- **Audit Finding:** `main.rs` returned exit code `2` for `CoreError::Backend`, `CoreError::Persistence`, and `CoreError::Io`, overlapping with Clap's argument parsing exit code `2`.
- **Root Cause:** Erroneous exit code grouping in `main.rs` match statement.
- **Correction Implemented:**
  1. Updated `main.rs` to map all operational, domain, backend, and persistence errors to exit code `1`.
  2. Reserved exit code `2` strictly for invalid CLI usage or argument parsing errors generated by Clap.
- **Evidence:** Verified by `test_cli_error_exit_codes` asserting exit code 1 for runtime failures and exit code 2 for `--bad-flag` and missing required arguments.

### 5. `CLI-M1-02` (Low) — Enforce Delete Confirmation
- **Audit Finding:** The `--confirm` flag was defined in `DeleteArgs`, but `handle_delete` never inspected it and deleted machines immediately without confirmation.
- **Root Cause:** Handler omitted confirmation evaluation.
- **Correction Implemented:**
  1. In `apps/terminest-cli/src/handlers.rs::handle_delete`, checked `!args.confirm`.
  2. Interactive detection via `std::io::stdin().is_terminal()`: prompts `[y/N]`, defaulting to refusal on empty input, EOF, or anything other than `y`/`yes`.
  3. Non-interactive detection: rejects deletion with exit code 1 unless `--confirm` is explicitly passed.
  4. Preserved independence: `--force` does not bypass `--confirm`, and `--confirm` does not bypass lifecycle invariants.
- **Evidence:** Verified by `test_cli_delete_confirmation_semantics`.

---

## C. Architectural Design Decisions

### 1. Locking Strategy & Monotonic Lock Ordering
To prevent deadlocks across concurrent processes operating on the filesystem, locks must always be acquired in strictly increasing numerical level:

```text
Level 1: Repository Inventory Lock (.repo.lock)
    │
    ▼
Level 2: Per-Machine Lock (.machine.lock)
    │
    ▼
Level 3: Backend Runtime Lock (mock_backend.lock)
```

- **Lock Invariant:** A process holding a lock at Level $N$ may only acquire a lock at Level $> N$. Acquiring a lock at Level $\le N$ is strictly forbidden.
- **Creation Path:** Holds Level 1 (`.repo.lock`) -> Level 2 (`.machine.lock`) -> Level 3 (`mock_backend.lock`).
- **Lifecycle Mutation Path (Start/Stop/Pause/Resume):** Holds Level 2 (`.machine.lock`) -> Level 3 (`mock_backend.lock`). Multiple machines execute in parallel without contending on Level 1.
- **Deletion Path:** Holds Level 1 (`.repo.lock`) -> Level 2 (`.machine.lock`) -> Level 3 (`mock_backend.lock`). Drops Level 2 before unlinking machine directory.

### 2. Lock-File Safety
- **Advisory OS Kernel Locks:** Locks are tied to open file handles via Windows `LockFileEx` and Unix `flock`.
- **Crash Safety:** If a process crashes, aborts, or is terminated by `taskkill`/`SIGKILL`, the OS kernel automatically closes the file handle and immediately releases the lock.
- **No Routine Deletion:** Lock files (`.repo.lock`, `.machine.lock`, `mock_backend.lock`) are never unlinked upon normal unlock, eliminating file recreation race conditions.

### 3. Audit Event Schema & Consistency Model
- **Schema:**
  ```json
  {
    "schema_version": "1.0.0",
    "event_id": "c1f7a0b3-4e89-4a1d-9e62-8f1e9c2b3a4d",
    "timestamp": "2026-11-05T08:45:00.123456Z",
    "machine_id": "a1b2c3d4-e5f6-4a1b-8c2d-3e4f5a6b7c8d",
    "machine_name": "ubuntu-worker",
    "action": "start",
    "phase": "completed",
    "actor": "orchestrator",
    "state_before": "starting",
    "state_after": "running",
    "result": "success"
  }
  ```
- **Consistency Model:** Audit records are written and synced to physical storage immediately upon operation completion or failure. For machine deletion, the completion event is appended and synced before the machine directory is unlinked.

---

## D. Implementation Details

### Files Added
1. `crates/terminest-core/src/persistence/lock.rs`: OS-level advisory file lock primitive (`FileLock`).
2. `crates/terminest-core/src/domain/audit.rs`: Structured audit event data models (`AuditRecord`, `AuditAction`, `AuditPhase`, `AuditResult`).
3. `crates/terminest-core/src/persistence/audit.rs`: JSON-Lines append-only audit persistence adapter (`AuditLogger`).

### Files Modified
1. `crates/terminest-core/src/error.rs`: Added `PersistenceError::LockTimeout` and `CoreError::OperationAborted`.
2. `crates/terminest-core/src/persistence/traits.rs`: Added `acquire_repo_lock`, `acquire_machine_lock`, `append_audit_event`, and `read_audit_events` to `MachineRepository`.
3. `crates/terminest-core/src/persistence/fs.rs`: Implemented cross-process lock paths and audit logging delegation.
4. `crates/terminest-core/src/persistence/mod.rs`: Re-exported `lock`, `audit`, `FileLock`, and `AuditLogger`.
5. `crates/terminest-core/src/domain/mod.rs`: Re-exported audit models.
6. `crates/terminest-core/src/backend/mock.rs`: Implemented cross-process serialization (`mock_backend.lock`), atomic reload, and atomic replacement for mock state.
7. `crates/terminest-core/src/orchestrator/service.rs`: Integrated hierarchical locking, strict lifecycle invariant checks for deletion, and audit trail emission.
8. `apps/terminest-cli/src/main.rs`: Realigned exit codes (1 for operational/runtime, 2 for argument parsing).
9. `apps/terminest-cli/src/handlers.rs`: Implemented interactive/non-interactive delete confirmation via `is_terminal()`.
10. `crates/terminest-core/tests/backend_tests.rs`: Added concurrent state file synchronization test.
11. `crates/terminest-core/tests/orchestration_tests.rs`: Added transitional deletion rejection, all-eligibility-states, and audit trail integration tests.
12. `apps/terminest-cli/tests/cli_tests.rs`: Added cross-process creation collision race, exit-code contract, delete confirmation, and audit disk tests.
13. `architecture/implementation/README.md` & `M1-implementation.md`: Updated capability ledger and documentation.

### Dependency Changes
Zero new third-party crates were added. The `FileLock` implementation leverages native standard library platform FFI (`kernel32` on Windows, `libc` on Unix) without introducing dependency bloat or toolchain incompatibilities.

---

## E. Test Evidence

### Workspace Quality Gate Execution
```bash
# 1. Code Formatting
cargo fmt --all -- --check
# Result: PASS (exit code 0, 0 diffs)

# 2. Static Analysis
cargo clippy --workspace --all-targets --all-features -- -D warnings
# Result: PASS (exit code 0, 0 warnings)

# 3. Documentation Tests
cargo test --workspace --doc
# Result: PASS (exit code 0, 0 failures)

# 4. Workspace Unit and Integration Tests
cargo test --workspace --all-features
# Result: PASS (83 passed; 0 failed; 0 ignored; 100% pass rate)
```

### Test Count Breakdown
- **`apps/terminest-cli`:** 10 passed (including cross-process race, confirmation semantics, exit codes, audit on disk).
- **`crates/terminest-core`:** 29 passed (3 lock/audit unittests, 4 backend, 10 domain/fuzz, 8 orchestration, 4 persistence).
- **`crates/terminest-protocol` (M0):** 30 passed (26 unit + 4 fuzz).
- **`crates/terminest-transport` (M0):** 14 passed (11 unit + 3 integration).
- **Total Tests:** **83 passed; 0 failed.**

### Milestone 0 Regression Status
- **Files Modified in M0:** **Zero.** (`crates/terminest-protocol` and `crates/terminest-transport` unchanged).
- **M0 Tests Passing:** **44 / 44** (100% passing).

---

## F. Known Limitations

1. **Simulated Hypervisor Backend:** All operations execute against `MockHypervisorBackend`. Real Windows Hyper-V WMI v2 and WHP QEMU backends remain scheduled for Milestone 2.
2. **Advisory Locking Nature:** File locks are OS-level advisory locks; processes that do not use the TermiNest locking protocol could theoretically modify files directly.
3. **Platform Coverage:** Verified live on Windows 11 x86_64; Unix FFI implementation uses standard `libc::flock`.

---

## G. Git Record

- **Branch:** `main`
- **Initial Baseline Commit:** `a5ee0ca docs(m1): author M1 independent verification audit report`
- **Working Tree:** Clean (all changes tracked and verified).

---

## H. Re-Audit Checklist

An independent auditor can reproduce all findings and verify their resolution using these steps:

1. **Verify Concurrency (Race on Create):**
   ```bash
   cargo test --package terminest-cli --test cli_tests test_cli_cross_process_create_collision_race -- --nocapture
   ```
2. **Verify Delete Invariants & Transitional Rejection:**
   ```bash
   cargo test --package terminest-core --test orchestration_tests test_orchestrator_rejects_deleting_transitional_states_without_force -- --nocapture
   ```
3. **Verify Delete Confirmation & Force Independence:**
   ```bash
   cargo test --package terminest-cli --test cli_tests test_cli_delete_confirmation_semantics -- --nocapture
   ```
4. **Verify Exit Codes (0, 1, 2):**
   ```bash
   cargo test --package terminest-cli --test cli_tests test_cli_error_exit_codes -- --nocapture
   ```
5. **Verify Structured Audit Logging on Disk:**
   ```bash
   cargo test --package terminest-cli --test cli_tests test_cli_audit_log_written_to_disk -- --nocapture
   ```
6. **Verify Mock Backend Concurrent State Synchronization:**
   ```bash
   cargo test --package terminest-core --test backend_tests test_mock_backend_concurrent_state_file_synchronization -- --nocapture
   ```

---

## I. Final Recommendation

### **READY FOR INDEPENDENT RE-AUDIT**

All identified defects from the initial M1 verification audit have been formally corrected, backed by comprehensive automated test coverage, and documented. The codebase is ready for independent verification re-audit.
