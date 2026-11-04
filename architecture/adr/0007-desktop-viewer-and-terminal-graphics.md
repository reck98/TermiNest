# ADR 0007: Terminal Desktop Viewer and Graphics Protocols

## Status
Accepted

## Context
When an AI agent controls a virtual computer, a human developer often wants to supervise its progress visually in real time. `terminal-mac` included a terminal-based desktop viewer that leveraged the Kitty graphics protocol, utilizing macOS shared memory (`shm_open`) to stream frames rendered by ScreenCaptureKit at 30 FPS directly into terminals like Ghostty and Kitty.

However, cross-platform terminal graphics support is fragmented:
- Modern terminals on Linux and macOS (Ghostty, Kitty, WezTerm) support the Kitty Graphics Protocol.
- Older terminals (iTerm2, Mintty) support the iTerm2 inline image escape sequence.
- Standard Windows terminals (`cmd.exe`, standard Windows Terminal, PowerShell console) historically had limited support for advanced inline graphics protocols.

## Problem Statement
How should TermiNest provide real-time, low-overhead desktop monitoring for developers directly from the CLI across diverse terminal environments?

## Decision
We architect the `terminest view` subsystem as an **Adaptive Multi-Tier Viewer Engine**:

1. **Tier 1: Kitty Graphics Protocol with Shared Memory (`t=s` / `t=f`):**
   - Active when the terminal identifies as Kitty or Ghostty.
   - The host viewer maps a POSIX shared memory buffer (`/dev/shm` on Linux) or Windows named shared memory section (`CreateFileMappingW`), writes raw or LZ4-compressed frames, and emits Kitty escape sequences referencing the memory handle.
   - Delivers **30 to 60 FPS** live desktop streaming with near-zero CPU and terminal IPC overhead.
2. **Tier 2: Kitty / iTerm2 Base64 Chunked Streaming:**
   - Active in WezTerm, Mintty, or terminals connected over SSH.
   - Emits compressed JPEG frames encoded as base64 chunked escape sequences directly over `stdout`.
   - Tuned for 5 to 15 FPS to prevent terminal buffer overflow.
3. **Tier 3: Universal Native Mini-Window Fallback (`terminest view --window`):**
   - Automatically suggested or triggered when running in an environment without terminal graphics support (such as standard Command Prompt, PowerShell `conhost`, or headless CI).
   - Opens an ultra-lightweight, zero-dependency native desktop window using the Rust `winit` and `softbuffer` crates.
   - Has a minimal memory footprint (~5 MB RSS), requires no heavy GUI framework (no Electron, no Qt), and displays the live guest desktop surface with hardware acceleration.

## Rationale
- **Seamless Developer Experience:** Developers using modern terminals get immediate, smooth visual monitoring without opening external windows or leaving their CLI workflow.
- **Fail-Safe Portability:** The lightweight native mini-window ensures developers on standard Windows consoles or older Linux terminals are never left without a visual monitoring option.
- **Low Resource Overhead:** By decoupling the viewer stream (Channel 1) from the MCP agent control stream (Channel 0), running the viewer does not delay or degrade agent response times.

## Consequences

### Positive
- Delivers the fastest, smoothest terminal desktop experience possible on modern terminals.
- Broadest possible compatibility across Windows and Linux environments.
- Zero external GUI runtime bloat.

### Negative / Trade-Offs
- Supporting three rendering pathways requires maintaining terminal protocol detection logic and a minimal `winit` window pipeline.
- Terminal graphics over high-latency SSH connections can introduce visual stutter if connection bandwidth is constrained.
