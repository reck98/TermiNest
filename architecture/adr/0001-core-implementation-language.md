# ADR 0001: Core Implementation Language

## Status
Accepted

## Context
TermiNest is a cross-platform systems infrastructure project designed to manage virtual machines, stream high-frame-rate desktop buffers, parse binary protocol frames over kernel sockets, and provide low-latency JSON-RPC interfaces for AI agents. The software must run as a single, self-contained binary on both Windows and Linux hosts, with sub-10ms CLI startup latency, zero external runtime dependencies (no requiring Python, Node.js, or .NET CLR on developer workstations), and rock-solid memory safety.

The candidate languages evaluated were Rust, Go, C++ (C++20), and C# (.NET 9 Native AOT).

## Problem Statement
What primary programming language should be selected for implementing the TermiNest Core Orchestrator, CLI, MCP Server, and Guest Helper daemons?

## Decision
We will implement the TermiNest Host Core Orchestrator, CLI, MCP Server, and Guest Helper daemons entirely in **Rust** (Edition 2021/2024).

## Rationale
1. **Single Static Binary Distribution:** Rust compiles directly to static, standalone native machine code (`terminest.exe` / `terminest`). It has zero runtime requirements, allowing seamless installation via `cargo install`, package managers (`winget`, `brew`, `apt`), or direct binary downloads.
2. **Sub-10ms Startup Time:** Unlike Go (GC initialization) or .NET (runtime spin-up), Rust binaries start up and exit in under 5ms, which is critical for interactive CLI responsiveness and quick tool invocation by agents.
3. **Low-Level Native Systems Access:** Rust provides first-class bindings to low-level Windows APIs (`windows` and `windows-sys` crates by Microsoft) for COM, WMI v2, Hyper-V Sockets, and Win32, as well as POSIX/Linux syscalls (`nix`, `libc`, `tokio-vsock`) without foreign-function interface (FFI) impedance mismatch.
4. **Memory Safety Without Garbage Collection:** Processing uncompressed 1080p display frames (8.3 MB per frame) generates substantial memory throughput. A garbage-collected runtime introduces non-deterministic GC pauses and frame dropping. Rust's compile-time ownership and borrow checker guarantee memory safety without pauses.
5. **Code Sharing Across Host and Guest:** The wire protocol definitions (`terminest-protocol`), serialization logic, CRC-32 validation, and computer-use schemas are shared as a pure Rust crate between host and guest.

## Consequences

### Positive
- Unified language ecosystem across host orchestrator, CLI, MCP server, and guest agents.
- Superior performance, predictability, and minimal memory footprint (~12 MB RSS idle).
- Type-safe async I/O via `tokio` for handling multiplexed Vsock sockets and terminal rendering.
- Complete memory and concurrency safety at compile time.

### Negative / Trade-Offs
- Steeper learning curve for contributors unfamiliar with Rust borrow semantics and lifetime management.
- Slower initial compilation times compared to Go or C#.
- FFI bindings to platform-specific C libraries (e.g. `turbojpeg`, `x11`) require careful unsafe wrapping, although pure-Rust alternatives (`image`, `lz4_flex`) exist for most subsystems.
