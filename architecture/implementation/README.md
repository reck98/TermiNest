# TermiNest Implementation Ledger

This directory contains implementation artifacts, specifications, platform feasibility reports, and test execution results generated during the engineering milestones of TermiNest.

## Document Index

- **[Milestone 0 Implementation Report (M0-implementation.md)](M0-implementation.md):** Complete summary of the protocol and transport foundation deliverables, architecture alignment, benchmarks, and Definition of Done compliance.
- **[TermiNest Wire Protocol v1 Specification (protocol-v1.md)](protocol-v1.md):** Authoritative wire specification detailing the 16-byte fixed binary header, Castagnoli CRC-32C verification, channel multiplexing, and JSON-RPC 2.0 schemas.
- **[Transport Feasibility Spike Report (transport-spike.md)](transport-spike.md):** In-depth investigation of `AF_HYPERV`, `AF_VSOCK`, Windows Hypervisor Platform (WHP), and QEMU stream mechanisms across Windows and Linux.
- **[Milestone 0 Test Results (test-results.md)](test-results.md):** Comprehensive ledger of all 33 unit, property-based fuzz, and integration tests with 100% pass rates.
