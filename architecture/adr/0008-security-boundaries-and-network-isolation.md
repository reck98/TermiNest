# ADR 0008: Security Boundaries and Network Isolation Tiers

## Status
Accepted

## Context
Virtual machines operated by AI agents represent an active execution boundary. Agents may browse the web, clone third-party repositories, or execute untrusted code. If the virtual machine is bridged to the developer's local network without restrictions, a rogue payload could probe local IoT devices, internal microservices, or cloud metadata endpoints (`169.254.169.254`). Furthermore, accidental data leakage between the host desktop (e.g. developer clipboard) and the guest must be strictly prevented.

## Problem Statement
How should TermiNest isolate guest virtual machines from the host operating system, local network, and sensitive developer data?

## Decision
We enforce five non-negotiable security controls:

### 1. Three Configurable Network Isolation Tiers
Every VM is created with an explicit network profile:
- **`isolated` (Air-Gapped / Tier A):** The VM is connected to an internal-only virtual switch with zero host routing and no default gateway. The VM has no network egress whatsoever. Host-guest communication occurs strictly via Vsock.
- **`nat` (Default / Tier B):** The VM is attached to an isolated host-only NAT network. Egress to the public Internet is permitted for downloading dependencies (`apt`, `npm`, `cargo`), but firewall rules drop all traffic to:
  - Host loopback (`127.0.0.0/8`).
  - RFC 1918 private subnets (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`).
  - Cloud instance metadata services (`169.254.169.254`).
- **`bridged` (Opt-in / Tier C):** The VM is attached directly to the host's physical network adapter. Permitted only when explicitly launched with `--allow-bridged`.

### 2. Zero Ambient Authority for File Access & Clipboard
- **Zero Host Clipboard Bleed:** The developer's host clipboard is never automatically synchronized to the guest. Clipboard data is read or written exclusively via deliberate, discrete MCP tool invocations (`read_guest_clipboard`, `write_guest_clipboard`).
- **Strict Path Canonicalization:** Host folder sharing (`--mount`) mandates path canonicalization. Symlinks inside the shared tree that resolve outside the designated mount directory are rejected with access denied.

### 3. Least Privilege Execution on Host
- On Windows hosts, the orchestrator avoids requiring full local Administrator privileges by leveraging the `Hyper-V Administrators` group.
- On Linux hosts, QEMU executes as an unprivileged user with restricted Linux capabilities and seccomp filters.

## Rationale
- Defends against lateral movement if a guest VM is compromised by untrusted software.
- Prevents cloud credential exfiltration via AWS/GCP/Azure metadata services.
- Protects the developer's private corporate network from accidental automated scanning.

## Consequences

### Positive
- Robust defense-in-depth architecture suitable for enterprise development environments.
- Protects developers from prompt-injected agents attempting to exfiltrate tokens or scan local networks.
- Full compliance with modern security and audit standards.

### Negative / Trade-Offs
- Guests in `isolated` mode cannot download new software packages unless pre-installed into the base image or pushed via Vsock file transfers.
- Blocking RFC 1918 traffic means guests cannot access local development servers running on other machines in the developer's office without switching to `bridged` mode.
