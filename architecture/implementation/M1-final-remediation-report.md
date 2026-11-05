# TermiNest Milestone 1: Final Remediation Report — Audit Durability, Locking and Recovery

- **Date:** 2026-11-05
- **Status:** **READY FOR FINAL INDEPENDENT M1 RE-AUDIT**
- **Target Repository:** `terminest`
- **Scope:** Complete resolution of the five findings identified in the Second Independent Re-verification Audit:
  - `AUDIT-M1-02` (High) — Machine deletion destroyed historical audit trail.
  - `RACE-M1-01` (Medium) — Machine lock released before directory removal.
  - `AUDIT-M1-03` (Low) — Silent error suppression on audit append failures.
  - `IDEM-M1-01` (Low) — Non-idempotent backend deletion recovery.
  - `CONC-M1-02` (Low) — Status reconciliation mutated persistent state without machine locking.

---

## 1. Executive Summary & Root Cause Analysis

| Finding ID | Severity | Category | Root Cause | Implemented Remediation |
|---|---|---|---|---|
| **AUDIT-M1-02** | High | Durability / Security | Machine audit logs (`audit.jsonl`) were stored inside the disposable machine folder (`<data_dir>/machines/<id>/`). When `delete_machine` called `repository.delete(&id)`, recursive removal deleted the audit log. | Relocated all machine audit logs to a dedicated, persistent directory (`<data_dir>/audit/<id>.jsonl`). Audit files are never removed during machine deletion. Added automatic, restartable migration for legacy audit logs. |
| **RACE-M1-01** | Medium | Concurrency | Machine lock files were located inside the machine directory (`<data_dir>/machines/<id>/.machine.lock`). To avoid Windows file sharing violations during `remove_dir_all`, the lock was dropped prior to deletion, opening a race window against concurrent mutating commands. | Relocated all machine lock files to an external lock directory (`<data_dir>/locks/machines/<id>.lock`). The machine lock remains held throughout the entire deletion operation without Windows file sharing conflicts. Removed unsafe `unwrap_or(record)` stale fallbacks. |
| **AUDIT-M1-03** | Low | Observability / Reliability | Orchestrator discarded all audit append results via `let _ = append_audit_event(...)` at 13 call sites. | Introduced explicit two-phase audit protocol: fail-closed pre-mutation intent logging (`AuditPhase::Requested`), explicit `CoreError::AuditPostMutationFailure` on post-mutation completion append failures, and prominent `tracing::error!` logging for failure/reconciliation events. Zero silent discards. |
| **IDEM-M1-01** | Low | Robustness | If backend deletion succeeded but repository directory removal failed, a retry invoked `backend.delete_machine(&id)` which returned `HypervisorError::MachineNotFound(id)`. Because `?` was used, retries failed and prevented repository cleanup. | Updated `delete_machine` to treat `HypervisorError::MachineNotFound` as an acceptable already-deleted condition during deletion workflows, enabling safe retry and cleanup. |
| **CONC-M1-02** | Low | Concurrency | `get_machine_status` reconciled state discrepancies and appended audit events without acquiring the machine's lock, permitting races with concurrent lifecycle mutations. | Updated `get_machine_status` to acquire Level 2 machine lock whenever state mutation is required, re-read fresh metadata under lock, and protect in-progress transitional states (`Starting`, `Pausing`, `Resuming`, `Stopping`, `Deleting`). |

---

## 2. Architectural Design Decisions

### 2.1 File & Directory Layout
The repository directory structure was updated to decouple ephemeral machine definitions, durable audit ledgers, and external concurrency locks:

```text
<data_dir>/
├── config.json                     # Global environment configuration
├── machines/                       # Ephemeral VM definitions (deleted upon VM deletion)
│   └── <machine-id>/
│       └── machine.json            # Deserialized MachineRecord specification
├── audit/                          # Durable audit ledgers (PERMANENT across VM deletion)
│   └── <machine-id>.jsonl          # Append-only JSON-Lines audit trail
├── locks/                          # External cross-process advisory lock files
│   ├── repo.lock                   # Level 1: Repository inventory lock
│   └── machines/                   # Level 2: Per-machine external lock files
│       └── <machine-id>.lock
└── run/                            # Runtime PID, socket, and mock backend state files
    ├── mock_backend.json
    └── mock_backend.lock           # Level 3: Hypervisor backend state lock
```

### 2.2 Hierarchical Lock Ordering Protocol
To prevent AB-BA deadlocks across multiple OS processes:
1. **Level 1 — Repository Inventory Lock (`<data_dir>/locks/repo.lock`):**
   Acquired exclusively during machine provisioning (`create_machine`) and deprovisioning (`delete_machine`) to ensure consistent name/ID inventory.
2. **Level 2 — Per-Machine External Lock (`<data_dir>/locks/machines/<id>.lock`):**
   Acquired exclusively by any operation mutating a specific machine (`start`, `stop`, `pause`, `resume`, `delete`, and mutating `status` reconciliation).
3. **Level 3 — Hypervisor Backend State Lock (`<data_dir>/run/mock_backend.lock`):**
   Acquired internally by hypervisor backends during compute instance mutations.

**Lock Duration Invariant for Deletion:**
Because `.machine.lock` is stored externally in `<data_dir>/locks/machines/<id>.lock`, `delete_machine` holds both `repo.lock` and `machine.lock` continuously until the function exits. `repository.delete(&id)` removes `<data_dir>/machines/<id>/` without closing or dropping the lock. No concurrent process can interleave during directory removal.

### 2.3 Two-Phase Audit Protocol & Error Policy
1. **Pre-Mutation Intent (`AuditPhase::Requested`):**
   Before executing side-effects on the hypervisor backend, the orchestrator durably records an intent event. If appending the intent event fails (e.g., read-only filesystem, disk full), the operation **fails closed immediately** with `CoreError::AuditFailure`. The hypervisor backend is never called.
2. **Post-Mutation Completion (`AuditPhase::Completed`):**
   After the hypervisor backend operation and repository state update succeed, the completion event is appended. If appending completion fails, the operation returns `CoreError::AuditPostMutationFailure`, explicitly alerting the operator and CLI that the compute state was mutated successfully, but audit recording failed.
3. **Reconciliation & Failure Logging:**
   Reconciliation and failure audit events log errors with `tracing::error!` rather than masking root errors or query results.

---

## 3. Files Modified

| File Path | Description of Changes |
|---|---|
| `crates/terminest-core/src/domain/audit.rs` | Added `AuditRecord::requested(...)` constructor for recording pre-mutation intent events. |
| `crates/terminest-core/src/error.rs` | Added `CoreError::AuditFailure` and `CoreError::AuditPostMutationFailure` error variants. |
| `crates/terminest-core/src/persistence/audit.rs` | Added `AuditLogger::append_to_file` and `read_from_file`; added resilient handling for partial/corrupt trailing lines. |
| `crates/terminest-core/src/persistence/fs.rs` | Relocated audit paths to `audit/<id>.jsonl` and lock paths to `locks/machines/<id>.lock`; added automatic legacy migration `migrate_legacy_audit_logs`; ensured `delete(&id)` purges only `machines/<id>/`. |
| `crates/terminest-core/src/orchestrator/service.rs` | Integrated two-phase audit protocol; eliminated all `let _ = append_audit_event`; eliminated unsafe `unwrap_or(record)` fallbacks; enforced continuous lock hold in `delete_machine`; added idempotent backend deletion; added locking and transitional protection to `get_machine_status`. |
| `apps/terminest-cli/src/config.rs` | Added `audit_dir` and `locks_dir` to `TermiNestPaths` and created them during `initialize()`. |
| `apps/terminest-cli/tests/cli_tests.rs` | Updated `test_cli_audit_log_written_to_disk` to verify external audit persistence and post-deletion durability. |
| `crates/terminest-core/tests/persistence_tests.rs` | Added `test_persistence_legacy_audit_migration` verifying discovery, copy, and cleanup of legacy audit files. |
| `crates/terminest-core/tests/orchestration_tests.rs` | Updated audit assertions for 2-phase lifecycle; added `test_orchestrator_idempotent_deletion_retry_after_backend_removal`, `test_orchestrator_status_reconciliation_transitional_protection`, and `test_orchestrator_post_deletion_lock_safety_and_no_directory_recreation`. |

---

## 4. Verification & Testing Evidence

### 4.1 Quality Gates
All workspace quality gates pass with zero warnings and zero failures:

```bash
cargo fmt --all -- --check
# Result: PASS (0 diffs)

cargo clippy --workspace --all-targets --all-features -- -D warnings
# Result: PASS (0 warnings)

cargo test --workspace --doc
# Result: PASS (0 failures)

cargo test --workspace --all-features
# Result: PASS (88 passed, 0 failed across 10 test suites)
```

### 4.2 Test Breakdown
- `apps/terminest-cli`: 10 integration tests passed.
- `crates/terminest-core`: 4 unit tests, 4 backend tests, 10 domain tests, 11 orchestration tests, 5 persistence tests passed (34 total).
- `crates/terminest-protocol`: 26 unit tests, 4 property/fuzz tests passed (30 total, untouched M0).
- `crates/terminest-transport`: 11 unit tests, 3 integration tests passed (14 total, untouched M0).

### 4.3 Empirical Live Verification
Executed isolated multi-step verification using `terminest.exe` in temporary storage:
1. Created machine `race-vm` $\rightarrow$ verified `<data_dir>/audit/<id>.jsonl` created with `create (requested)` and `create (completed)` records.
2. Started machine $\rightarrow$ verified `start (requested)` and `start (completed)` records.
3. Deleted machine with `--confirm --force` $\rightarrow$ verified exit code 0.
4. Verified `<data_dir>/machines/<id>/` was deleted (count: 0).
5. Verified `<data_dir>/audit/<id>.jsonl` **survived completely** and contains 6 durable audit records, ending with `delete (completed)`.
6. Attempted `start` on deleted machine $\rightarrow$ failed with exit code 1 (`Error: Machine not found: race-vm`).
7. Verified `<data_dir>/machines/<id>/` was **not recreated** by lock acquisition (count: 0).

---

## 5. Known Limitations & Remaining Risks

1. **Simulated Virtualization:** Real Hyper-V and QEMU hypervisor drivers remain deliberately unbuilt (scheduled for Milestone 2). Current tests operate against `MockHypervisorBackend`.
2. **Platform Verification Scope:** Windows file locking and TTY interaction were verified live on Windows 11. Unix `flock` implementation was verified through code inspection and syntax checking; live Unix verification remains pending execution in a Linux CI runner.

---

## 6. Final Recommendation

All five findings from the Second Independent Re-verification Audit have been implemented, tested, and empirically proven resolved.

**Milestone 1 is:** **READY FOR FINAL INDEPENDENT M1 RE-AUDIT**.
