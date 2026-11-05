# TermiNest Implementation Ledger

This directory contains authoritative implementation records, specifications, platform feasibility reports, verification audits, and closure ledgers generated during the engineering milestones of TermiNest.

---

## Milestone Status Ledger

| Milestone | Title | Status | Accepted Baseline Commit | Acceptance Audit | Verdict |
|---|---|---|---|---|---|
| **M0** | Protocol & Transport Foundation | **Formally Accepted & Closed** | `dc5cf701bd1fdcbd659cd22d10de71be1fd2a147` | [M0-verification-audit.md](M0-verification-audit.md) | **PASS** |
| **M1** | Core Host Orchestrator & CLI Foundation | **Formally Accepted & Closed** | `bd0e9c89c12a1cbee2bd11a8507f6e278dd87d76` | [M1-final-acceptance-audit.md](M1-final-acceptance-audit.md) | **PASS WITH CONDITIONS** |
| **M2** | Windows Host -> Linux Guest (MVP) | Ready to Begin | Pending M2 Implementation | — | — |

### Milestone 1 Acceptance & Closure Details
- **Accepted Baseline Commit Hash:** `bd0e9c89c12a1cbee2bd11a8507f6e278dd87d76`
- **Closure Date:** 2026-11-05
- **Audit Verdict:** `PASS WITH CONDITIONS` (Zero compiler warnings, zero Clippy warnings, clean formatting, 88/88 passing tests, M0 integrity 100% verified).
- **Tracked Non-Blocking Conditions:**
  1. *Audit log retention/archival policy:* Historical audit logs are retained permanently at `<data_dir>/audit/<id>.jsonl`. Automated archival/pruning policy remains M2 or later work.
  2. *Legacy audit migration validation:* Future data migration utilities should explicitly verify byte size and checksum equality before deleting legacy source files (`MIGR-M1-01`).
  3. *Live Linux multi-process verification:* POSIX `flock` implementation in `lock.rs` is verified correct by static code inspection and standard API alignment; live multi-process verification remains pending Linux CI runners.
- **Architectural Boundary Clarification:** The real hypervisor backend (Hyper-V WMI v2, QEMU/WHP) is explicitly not part of Milestone 1 and remains deferred to Milestone 2. M1 provides the `HypervisorBackend` SPI trait, backend registry, and deterministic `MockHypervisorBackend`.

---

## Document Index

- **[Milestone 1 Final Acceptance Audit (M1-final-acceptance-audit.md)](M1-final-acceptance-audit.md):** Authoritative final independent acceptance audit report confirming all 5 findings resolved, multi-process concurrency verified, quality gates satisfied, and rendering the `PASS WITH CONDITIONS` acceptance decision.
- **[Milestone 1 Final Remediation Report (M1-final-remediation-report.md)](M1-final-remediation-report.md):** Detailed report documenting the remediation of AUDIT-M1-02, RACE-M1-01, AUDIT-M1-03, IDEM-M1-01, and CONC-M1-02, including persistent audit storage, external locks, two-phase audit protocol, and idempotent recovery.
- **[Milestone 1 Second Independent Re-verification Audit (M1-reverification-audit.md)](M1-reverification-audit.md):** Formal second independent audit report analyzing deletion races, audit log erasure, and outstanding findings.
- **[Milestone 1 Implementation Report (M1-implementation.md)](M1-implementation.md):** Complete implementation report for the core host orchestrator and CLI foundation, including domain model, lifecycle state machine, hypervisor SPI, mock backend, atomic persistence, and CLI command suite.
- **[Milestone 1 Independent Verification Audit (M1-verification-audit.md)](M1-verification-audit.md):** Formal audit report detailing defects found (`CONC-M1-01`, `AUDIT-M1-01`, `LIFE-M1-01`, `CLI-M1-01`, `CLI-M1-02`) and conditional acceptance decision.
- **[Milestone 1 Correction Report (M1-correction-report.md)](M1-correction-report.md):** Comprehensive report detailing the resolution, implementation, and test verification of all M1 audit findings, preparing the workspace for re-audit.
- **[Milestone 0 Implementation Report (M0-implementation.md)](M0-implementation.md):** Complete summary of the protocol and transport foundation deliverables, architecture alignment, benchmarks, and Definition of Done compliance.
- **[Milestone 0 Independent Verification & Closure Audit (M0-verification-audit.md)](M0-verification-audit.md):** Formal audit report detailing defects found, corrective actions taken (ADR-0010, flags validation, decoder poisoning), and readiness verification.
- **[TermiNest Wire Protocol v1 Specification (protocol-v1.md)](protocol-v1.md):** Authoritative wire specification detailing the 16-byte fixed binary header, Castagnoli CRC-32C verification, channel multiplexing, and JSON-RPC 2.0 schemas.
- **[Transport Feasibility Spike Report (transport-spike.md)](transport-spike.md):** In-depth investigation of `AF_HYPERV`, `AF_VSOCK`, Windows Hypervisor Platform (WHP), and QEMU stream mechanisms across Windows and Linux.
- **[Milestone 0 Test Results (test-results.md)](test-results.md):** Comprehensive ledger of all 40 automated test cases, property tests, and multi-sample release benchmarks.
