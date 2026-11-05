# TermiNest Implementation Ledger

This directory contains implementation artifacts, specifications, platform feasibility reports, and test execution results generated during the engineering milestones of TermiNest.

## Document Index

- **[Milestone 1 Implementation Report (M1-implementation.md)](M1-implementation.md):** Complete implementation report for the core host orchestrator and CLI foundation, including domain model, lifecycle state machine, hypervisor SPI, mock backend, atomic persistence, and CLI command suite.
- **[Milestone 1 Independent Verification Audit (M1-verification-audit.md)](M1-verification-audit.md):** Formal audit report detailing defects found (`CONC-M1-01`, `AUDIT-M1-01`, `LIFE-M1-01`, `CLI-M1-01`, `CLI-M1-02`) and conditional acceptance decision.
- **[Milestone 1 Correction Report (M1-correction-report.md)](M1-correction-report.md):** Comprehensive report detailing the resolution, implementation, and test verification of all M1 audit findings, preparing the workspace for re-audit.
- **[Milestone 0 Implementation Report (M0-implementation.md)](M0-implementation.md):** Complete summary of the protocol and transport foundation deliverables, architecture alignment, benchmarks, and Definition of Done compliance.
- **[Milestone 0 Independent Verification & Closure Audit (M0-verification-audit.md)](M0-verification-audit.md):** Formal audit report detailing defects found, corrective actions taken (ADR-0010, flags validation, decoder poisoning), and readiness verification.
- **[TermiNest Wire Protocol v1 Specification (protocol-v1.md)](protocol-v1.md):** Authoritative wire specification detailing the 16-byte fixed binary header, Castagnoli CRC-32C verification, channel multiplexing, and JSON-RPC 2.0 schemas.
- **[Transport Feasibility Spike Report (transport-spike.md)](transport-spike.md):** In-depth investigation of `AF_HYPERV`, `AF_VSOCK`, Windows Hypervisor Platform (WHP), and QEMU stream mechanisms across Windows and Linux.
- **[Milestone 0 Test Results (test-results.md)](test-results.md):** Comprehensive ledger of all 40 automated test cases, property tests, and multi-sample release benchmarks.
