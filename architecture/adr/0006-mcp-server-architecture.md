# ADR 0006: MCP Server Architecture and Agent Interface

## Status
Accepted

## Context
AI agent frameworks (Claude Code, OpenAI Codex, Cursor, OpenCode, Amp) interact with external tools using the Model Context Protocol (MCP). The MCP server must expose virtual computer capabilities (capturing screenshots, clicking coordinates, typing keys, executing guest commands, managing files) to agents over a standard local protocol.

In `terminal-mac`, the MCP server is implemented in Swift and executes as a separate subprocess or mode. It interacts with the hypervisor and virtio-fs mailbox directly.

## Problem Statement
How should the TermiNest MCP server be architected, how should it communicate with AI agents, and how should it interface with the core virtualization engine?

## Decision
1. **Single-Binary MCP Server Integration:**
   - The MCP server is built directly into the main `terminest` binary and invoked via `terminest mcp --machine <name>`.
   - It communicates with the host agent framework over standard input and output (`stdio`) using standard **JSON-RPC 2.0 framing**.
2. **Decoupled Architecture via Rust In-Process API:**
   - The MCP server does not talk directly to hypervisor sockets or guest daemons.
   - It delegates all requests to the **TermiNest Core Orchestrator**, which manages VM lifecycle, Vsock connections, connection pooling, and coordinate bounds checking.
3. **Structured Computer-Use Toolset:**
   The MCP server exposes a standardized, idempotent tool interface:
   - `computer_screenshot`: Captures the current guest desktop, returning a base64 JPEG and resolution metadata.
   - `computer_mouse_click`: Performs left, right, middle, or double clicks at `(x, y)` coordinates.
   - `computer_mouse_move`: Moves the cursor to `(x, y)`.
   - `computer_mouse_drag`: Drags from `(start_x, start_y)` to `(end_x, end_y)`.
   - `computer_key_sequence`: Injects a sequence of keystrokes with modifiers (`["Ctrl", "Alt", "t"]`).
   - `computer_type_text`: Injects a string of Unicode text with realistic inter-key timing.
   - `computer_read_clipboard` / `computer_write_clipboard`: Explicit, scoped clipboard access.
   - `computer_exec`: Runs a guest CLI command non-interactively with stdout/stderr capture and timeout.
   - `computer_push_file` / `computer_pull_file`: Scoped, jailed file transfers between host and guest.
4. **Mandatory Guardrails & Coordinate Validation:**
   - The MCP server validates and clamps all `(x, y)` coordinates against the guest desktop's active viewport before transmitting them to the hypervisor.
   - Operations that would destroy data or modify host settings are strictly excluded from the MCP tool schema.

## Rationale
- **Zero-Dependency Agent Integration:** AI agent tools like Claude Code can configure TermiNest simply by adding `terminest mcp --machine dev` to their MCP configuration (`claude_desktop_config.json` or `.mcp.json`). No Node.js or Python runtime is required.
- **Strict Seam Isolation:** By decoupling the MCP server from hypervisor internals, mock orchestrators can be swapped in during automated unit and integration tests.
- **Predictable Multimodal Context:** Returning compressed JPEG images with explicit pixel width and height ensures multimodal LLMs can accurately compute spatial coordinates for GUI elements.

## Consequences

### Positive
- Universal compatibility with all MCP-compliant AI agent clients.
- Clean separation of concerns between agent protocol formatting and systems orchestration.
- Thread-safe multiplexing: multiple tools can query state concurrently over Tokio async channels.

### Negative / Trade-Offs
- Stdio JSON-RPC debugging requires careful logging: all debug logs must be routed to `stderr` or a log file (`terminest.log`) so as not to corrupt the JSON-RPC stream on `stdout`.
