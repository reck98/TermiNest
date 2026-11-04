# TermiNest Engineering Skill Stack Inventory

This document records the curated agent skill stack configured for **TermiNest**—a cross-platform computer-use environment enabling CLI-based AI agents to control isolated virtual machines across Windows and Linux.

The skill stack prepares the environment for the upcoming architectural reconnaissance of `terminal-mac`, domain modelling, systems engineering, security hardening, and test-driven implementation.

---

## 1. Skill Stack Inventory

| Skill | Publisher / Repository | Category | Purpose | Activation Trigger | Installation Scope | Status |
|---|---|---|---|---|---|---|
| `improve-codebase-architecture` | Matt Pocock (`mattpocock/skills`) | Architecture | Scans codebase for deepening opportunities, generates visual HTML reports with before/after diagrams, and guides architectural refactoring. | Explicit architectural audit requests; "improve architecture", "deepen modules", or searching for architectural friction. | Project (`.agents/skills`) | **Installed** |
| `codebase-design` | Matt Pocock (`mattpocock/skills`) | Architecture | Defines shared vocabulary and design principles for deep modules (interfaces, seams, adapters, leverage, locality, deletion test). | Designing or refining module interfaces; determining seam placement; designing for testability. | Project (`.agents/skills`) | **Installed** |
| `domain-modeling` | Matt Pocock (`mattpocock/skills`) | Architecture / Domain | Actively establishes ubiquitous domain vocabulary, maintains `GLOSSARY.md`, and documents irreversible trade-offs in `docs/adr/`. | Discussing domain terminology; resolving naming conflicts; creating or updating architectural decision records. | Project (`.agents/skills`) | **Installed** |
| `grill-me` | Matt Pocock (`mattpocock/skills`) | Architecture / Planning | Conducts a relentless, round-based interview traversing a decision tree frontier to eliminate unstated assumptions before coding. | Stress-testing plans or design proposals; trigger phrases like "grill me", "stress-test this plan". | Project (`.agents/skills`) | **Installed** |
| `grill-with-docs` | Matt Pocock (`mattpocock/skills`) | Architecture / Planning | Combines the relentless grilling interview with real-time documentation generation (`GLOSSARY.md` and ADRs) as decisions crystallize. | Architectural planning sessions where simultaneous formal documentation is required. | Project (`.agents/skills`) | **Installed** |
| `grilling` | Matt Pocock (`mattpocock/skills`) | Architecture / Planning (Core Engine) | Core interview engine executing decision tree traversal in numbered rounds with explicit recommendations and sub-agent fact retrieval. | Invoked directly or via `grill-me` / `grill-with-docs` / `improve-codebase-architecture`. | Project (`.agents/skills`) | **Installed (Dependency)** |
| `research` | Matt Pocock (`mattpocock/skills`) | Research & Docs | Investigates questions against high-trust primary sources (specs, docs, source code) using background sub-agents and repo markdown output. | Gathering API facts, investigating virtualization specifications, or delegating documentation reading to a sub-agent. | Project (`.agents/skills`) | **Installed** |
| `deep-research` | 199 Biotechnologies (`199-biotechnologies/claude-deep-research-skill`) | Research & Docs | Multi-phase deep research pipeline featuring citation tracking, atomic claim verification, evidence logging, and structured report synthesis. | In-depth multi-source investigations, technology trade-off studies, or state-of-the-art competitive analyses. | Project (`.agents/skills`) | **Installed** |
| `writing-for-agents` | Matt Pocock (`mattpocock/skills`) | Agent Engineering | Engineering guide for authoring deterministic agent instructions, skills, and context pointers; balances context load vs. cognitive load. | Creating or modifying agent skills, system instructions, or files like `AGENTS.md` and `CLAUDE.md`. | Project (`.agents/skills`) | **Installed** |
| `security-threat-model` | OpenAI (`openai/skills`) | Security Engineering | Repository-grounded AppSec threat modeling enumerating trust boundaries, assets, attacker capabilities, abuse paths, and mitigations. | Explicit requests to threat-model codebases, subpaths, hypervisor boundaries, or agent execution entrypoints. | Project (`.agents/skills`) | **Installed** |
| `security-and-hardening` | Addy Osmani (`addyosmani/agent-skills`) | Security Engineering | Comprehensive defensive coding controls: boundary validation, query parameterization, SSRF mitigation, destructive path checks, and OWASP defenses. | Implementing authentication, input parsing, IPC protocols, file operations, or handling untrusted user/agent data. | Project (`.agents/skills`) | **Installed** |
| `agent-security-audit` | OWASP (`owasp/secure-agent-playbook`) | Security Engineering | Evaluates AI agent configurations against the OWASP Top 10 for LLM/Agentic Applications (prompt injection, excessive agency, data exfiltration). | Reviewing agent permissions, MCP configs, tool execution boundaries, or agent orchestration code. | Project (`.agents/skills`) | **Installed** |
| `tdd` | Matt Pocock (`mattpocock/skills`) | Software Engineering | Enforces strict test-driven development: red-green-refactor loop, seam identification, vertical tracer-bullet slicing, and anti-pattern prevention. | Implementing logic, fixing bugs, or adding unit and integration tests test-first. | Project (`.agents/skills`) | **Installed** |
| `ponytail` | Dietrich Gebert (`dietrichgebert/ponytail`) | Software Engineering / Lean Dev | "Lazy senior dev mode": enforces minimal code diffs, YAGNI, standard library leverage, and anti-bloat practices (`lite`, `full`, `ultra`). | Minimizing diff size, avoiding speculative abstraction, or when explicitly requested via "be lazy", "simplest solution", "ponytail". | Project (`.agents/skills`) | **Installed** |
| `write-a-skill` | Matt Pocock (`mattpocock/skills`) | Software Engineering / Skills | Original template-based skill authoring tool from Matt Pocock's engineering stack. | — | Project | **Unavailable (Upstream Removed)** |
| `ui-ux-pro` | Candidate Name | UI/UX Design | Candidate name for UI/UX intelligence; canonical package is `ui-ux-pro-max` from `nextlevelbuilder/ui-ux-pro-max-skill`. | — | Project | **Unavailable (Name Mismatch / Skipped)** |
| `taste` / `ui-taste` | Candidate Names | UI/UX Design | Candidate names for anti-slop visual design; canonical package is `design-taste-frontend` from `leonxlnx/taste-skill`. | — | Project | **Unavailable (Name Mismatch / Skipped)** |

---

## 2. Category Breakdown and Skill Capabilities

### A. Architecture and Codebase Analysis

#### 1. `improve-codebase-architecture`
- **Source:** `mattpocock/skills/skills/engineering/improve-codebase-architecture/SKILL.md`
- **Capabilities:**
  - Identifies codebase hotspots via git history and friction points (e.g. shallow modules, leaked seams, scattered logic).
  - Generates an interactive visual HTML report formatted with Tailwind CSS and Mermaid diagrams, saved to the OS temporary directory and opened in the default browser.
  - Features before/after visual cards with recommendation ratings (`Strong`, `Worth exploring`, `Speculative`).
  - Seamlessly initiates a grilling session on the chosen refactoring target.

#### 2. `codebase-design`
- **Source:** `mattpocock/skills/skills/engineering/codebase-design/SKILL.md`
- **Capabilities:**
  - Establishes a rigorous design taxonomy: **Module**, **Interface**, **Implementation**, **Depth**, **Seam**, **Adapter**, **Leverage**, and **Locality**.
  - Enforces John Ousterhout’s deep module philosophy: maximum behavior encapsulated behind a minimal, clean interface.
  - Implements the "Deletion Test" to eliminate pass-through/shallow abstractions.
  - Provides patterns for "Design It Twice" (parallel sub-agent interface exploration) and dependency deepening.

#### 3. `domain-modeling`
- **Source:** `mattpocock/skills/skills/engineering/domain-modeling/SKILL.md`
- **Capabilities:**
  - Enforces active domain modeling: flags terminology contradictions against `GLOSSARY.md` in real time.
  - Clarifies ambiguous abstractions into canonical terms (e.g., distinguishing VM instance, guest session, and agent executor).
  - Lazily creates and maintains `GLOSSARY.md` and records high-impact, hard-to-reverse architectural decisions in `docs/adr/`.

#### 4. `grill-me` & `grilling`
- **Source:** `mattpocock/skills/skills/productivity/grill-me/SKILL.md` & `grilling/SKILL.md`
- **Capabilities:**
  - Treats design spaces as branching decision trees.
  - Identifies the unblocked "frontier" of decisions and presents numbered question rounds with recommended answers.
  - Dispatches background sub-agents to discover empirical facts from the environment rather than burdening the user.
  - Halts only when the decision frontier is completely resolved.

#### 5. `grill-with-docs`
- **Source:** `mattpocock/skills/skills/engineering/grill-with-docs/SKILL.md`
- **Capabilities:**
  - Orchestrates `grilling` and `domain-modeling` concurrently.
  - Ensures that every agreement reached during an interview is instantly codified into `GLOSSARY.md` and `docs/adr/`.

---

### B. Research and Technical Documentation

#### 1. `deep-research`
- **Source:** `199-biotechnologies/claude-deep-research-skill/SKILL.md`
- **Capabilities:**
  - Operates across 4 execution depths: Quick (3 phases), Standard (6 phases), Deep (8 phases), and Ultradeep (8+ phases).
  - Main pipeline: `SCOPE` → `PLAN` → `RETRIEVE` → `TRIANGULATE` → `OUTLINE REFINEMENT` → `SYNTHESIZE` → `CRITIQUE` → `REFINE` → `PACKAGE`.
  - Persists atomic evidence artifacts: `sources.jsonl`, `evidence.jsonl`, `claims.jsonl`, and `run_manifest.json`.
  - Enforces strict verification: minimum 10+ sources, 3+ per major claim, zero uncited factual claims.
  - Includes validation scripts: `validate_report.py`, `verify_citations.py`, and `md_to_html.py`.

#### 2. `research`
- **Source:** `mattpocock/skills/skills/engineering/research/SKILL.md`
- **Capabilities:**
  - Fast, focused investigation targeting primary sources (official hypervisor specs, OS virtualization documentation, source code repositories).
  - Spawns a background agent with a fresh context window to prevent polluting the primary conversation.
  - Writes structured markdown notes directly into the repository documentation hierarchy.

#### 3. `writing-for-agents`
- **Source:** `mattpocock/skills/skills/productivity/writing-for-agents/SKILL.md`
- **Capabilities:**
  - Explains the dual budgets: Context Load (tokens consumed every turn) vs. Cognitive Load (mental effort required by human operators).
  - Guides progressive disclosure (keeping primary instructions lean, pushing reference behind context pointers).
  - Teaches the use of "Leading Words" (recruiting pretrained model behaviors with high-signal anchor tokens).
  - Enforces checkable and exhaustive completion criteria to prevent premature task completion.

---

### C. Security Engineering

#### 1. `security-threat-model`
- **Source:** `openai/skills/skills/security-threat-model/SKILL.md`
- **Capabilities:**
  - Grounded repository threat modeling mapping trust boundaries, assets, entry points, and attacker capabilities.
  - Evaluates critical virtualization and isolation risks: VM escape, cross-tenant leakage, host compromise, unauthorized IPC, credential exfiltration.
  - Generates standardized AppSec threat models saved as `<component>-threat-model.md`.

#### 2. `security-and-hardening`
- **Source:** `addyosmani/agent-skills/skills/security-and-hardening/SKILL.md`
- **Capabilities:**
  - Three-tier security boundary system: Always Do, Ask First, and Never Do.
  - Hardens input boundaries, parameterizes database and shell calls, and defends against SSRF and command injection.
  - Strict filesystem guardrails: verifies allowlisted roots, path depths, and pre-execution ownership before destructive actions.
  - Secure handling of LLM outputs (treating model completions as untrusted input).

#### 3. `agent-security-audit`
- **Source:** `owasp/secure-agent-playbook/skills/agent-security-audit/SKILL.md`
- **Capabilities:**
  - Comprehensive audit aligned with OWASP Top 10 for LLM Applications:
    - LLM01: Prompt Injection
    - LLM02: Sensitive Information Disclosure
    - LLM05: Improper Output Handling
    - LLM06: Excessive Agency (escalation of privileges, irreversible actions without confirmation)
    - LLM07: System Prompt Leakage
    - LLM08: Vector & Embedding Weaknesses
  - Audits agent tool permissions, MCP configs, and sandbox boundaries.

---

### D. Software Engineering and Testing

#### 1. `tdd`
- **Source:** `mattpocock/skills/skills/engineering/tdd/SKILL.md`
- **Capabilities:**
  - Strict Red-Green-Refactor loop.
  - Public interface testing at pre-agreed seams; prevents implementation coupling and tautological assertions.
  - Vertical slicing: builds features tracer bullet by tracer bullet rather than horizontal speculative layers.
  - Separation of concerns: implementation cycles focus on making tests pass; refactoring is handled during structured code reviews.

#### 2. `ponytail`
- **Source:** `dietrichgebert/ponytail/skills/ponytail/SKILL.md`
- **Capabilities:**
  - Enforces senior developer pragmatism: writes the absolute minimum code necessary to solve the problem completely.
  - Three operational tiers:
    - `lite`: Generates requested code while noting smaller alternatives.
    - `full` (default): Prefers standard libraries and existing patterns; bans unnecessary wrappers, speculative abstractions, and superfluous dependencies.
    - `ultra`: Actively pushes back against unneeded requirements before implementing.
  - Flags deliberate technical trade-offs with explicit `shortcut: <limit>, <when to upgrade>` comments.

---

## 3. Operational Dependencies and Internal Composition

Several skills in the Matt Pocock engineering suite operate as composite workflows that depend on sibling skills:

```
improve-codebase-architecture
  ├── calls: codebase-design (shared vocabulary & principles)
  ├── calls: grilling (interactive refactoring interview)
  └── calls: domain-modeling (updates GLOSSARY.md & docs/adr/)

grill-me
  └── calls: grilling (core decision-tree interview engine)

grill-with-docs
  ├── calls: grilling (core decision-tree interview engine)
  └── calls: domain-modeling (in-flight ADR and glossary updates)

tdd
  └── consults: codebase-design (seam and interface definition)
```

**Resolution:**
The core skill `grilling` was explicitly installed from `mattpocock/skills` to satisfy the invocation requirements of `grill-me`, `grill-with-docs`, and `improve-codebase-architecture`.

---

## 4. Capability Overlaps and Selection Guidelines

### Research: `research` vs. `deep-research`
- **Use `research` (Matt Pocock):** For immediate, single-agent code-level research, checking API contracts, reviewing repository architecture, or gathering facts during active design sessions. Outputs lightweight markdown directly into the workspace.
- **Use `deep-research` (199 Biotechnologies):** When conducting extensive multi-source investigations (e.g., evaluating hypervisor backends: QEMU vs. Cloud Hypervisor vs. Apple Virtualization Framework vs. Firecracker), where citation ledgers (`sources.jsonl`, `evidence.jsonl`) and formal reports are mandatory.

### Planning: `grill-me` vs. `grill-with-docs`
- **Use `grill-me`:** When you need a focused, interactive interview to stress-test an implementation idea, algorithm, or test plan without immediately altering repository documentation.
- **Use `grill-with-docs`:** During major architectural sessions where decisions must be permanently captured in `GLOSSARY.md` and `docs/adr/` as the interview progresses.

### Security: `security-threat-model` vs. `security-and-hardening` vs. `agent-security-audit`
- **Use `security-threat-model` (OpenAI):** At design time to identify trust boundaries (host vs. VM, agent vs. sandbox), external attack surfaces, and threat matrices before writing code.
- **Use `security-and-hardening` (Addy Osmani):** At implementation time to ensure code touching IPC, inputs, filesystem paths, and network calls adheres to defensive programming rules.
- **Use `agent-security-audit` (OWASP):** When reviewing the agent runtime, tool execution harness, and MCP configurations to guard against prompt injection and excessive agency.

---

## 5. Candidate Discovery & Gap Analysis (Unavailable / Skipped Skills)

In accordance with strict discovery rules ("Do not assume a search result with a similar name is the intended skill; if a candidate cannot be found, report it as unavailable; do not silently substitute"), the following candidate skills were evaluated and not installed:

### 1. `write-a-skill`
- **Discovery Result:** Searched `mattpocock/skills` and global registry.
- **Finding:** Originally authored by Matt Pocock in `skills/productivity/write-a-skill`, this skill was **deprecated and removed upstream** in commit `bc4cf903ff4855ce23199a8dd3bf98b3dbd7ad71`. The current repository does not expose `write-a-skill`, and running `npx skills add mattpocock/skills --skill write-a-skill` returns: `x No matching skills found for: write-a-skill`.
- **Disposition:** Marked as **Unavailable**.
- **Alternative:** Authoring agent documentation and skills is fully supported by the installed `writing-for-agents` skill.

### 2. `ui-ux-pro`
- **Discovery Result:** Searched `ui-ux-pro` on skills registry.
- **Finding:** The candidate name `ui-ux-pro` does not exist as a canonical package. The widely recognized, canonical skill is `ui-ux-pro-max` from repository `nextlevelbuilder/ui-ux-pro-max-skill` (434K+ installs). While an unverified fork (`yuniorglez/gemini-elite-core@ui-ux-pro`) exists with only 30 installs, installing unvetted forks violates supply-chain security guidelines.
- **Disposition:** Marked as **Unavailable (Name Mismatch / Skipped)**.
- **Recommendation:** If UI design intelligence is desired, the user can explicitly install `nextlevelbuilder/ui-ux-pro-max-skill --skill ui-ux-pro-max`.

### 3. `taste` / `ui-taste`
- **Discovery Result:** Searched `taste` and `ui-taste`.
- **Findings:**
  - The canonical frontend taste repository `leonxlnx/taste-skill` (729K+ installs) publishes its anti-slop art direction skill under the identifier **`design-taste-frontend`**, not `taste` or `ui-taste`.
  - The exact package identifier `uizze.sh@ui-taste` (listed on skills.sh) is invalid git syntax and fails during clone (`fatal: repository 'uizze.sh' does not exist`).
  - The exact match `affaan-m/ecc@taste` is a specialized music video and short-form video editing skill (angelcore/cloud-trance aesthetic), completely unrelated to software UI/UX.
- **Disposition:** Marked as **Unavailable (Name Mismatch / Repository Error / Skipped)**.
- **Recommendation:** If web art direction is required, the user can install the canonical `leonxlnx/taste-skill --skill design-taste-frontend`.

### 4. `pony-tail` vs. `ponytail`
- **Discovery Result:** Candidate name was listed as `pony-tail` / `ponytail` under UI/UX.
- **Finding:** The canonical skill is `dietrichgebert/ponytail@ponytail` (118K+ installs). Its domain is lean software engineering and anti-bloat implementation rather than visual/UI design.
- **Disposition:** Installed as `ponytail`, with its category accurately classified under Software Engineering / Lean Development.

---

## 6. Installation Commands Log

All skills were installed at the project level into `.agents/skills` using the official `skills` CLI:

```bash
# Architecture & Design (Matt Pocock)
npx skills add mattpocock/skills --skill improve-codebase-architecture -y
npx skills add mattpocock/skills --skill codebase-design domain-modeling grill-me grill-with-docs research writing-for-agents tdd -y
npx skills add mattpocock/skills --skill grilling -y

# Security Engineering (OpenAI, Addy Osmani, OWASP)
npx skills add openai/skills --skill security-threat-model -y
npx skills add addyosmani/agent-skills --skill security-and-hardening -y
npx skills add owasp/secure-agent-playbook --skill agent-security-audit -y

# Deep Research (199 Biotechnologies)
npx skills add 199-biotechnologies/claude-deep-research-skill --skill deep-research -y

# Lean Software Engineering (Dietrich Gebert)
npx skills add dietrichgebert/ponytail --skill ponytail -y
```

---

## 7. Agent Compatibility and Integration Notes

### Project Directory Structure
All installed skills reside locally in the project root:
```
terminest/
├── .agents/
│   └── skills/
│       ├── agent-security-audit/
│       ├── codebase-design/
│       ├── deep-research/
│       ├── domain-modeling/
│       ├── grill-me/
│       ├── grill-with-docs/
│       ├── grilling/
│       ├── improve-codebase-architecture/
│       ├── ponytail/
│       ├── research/
│       ├── security-and-hardening/
│       ├── security-threat-model/
│       ├── tdd/
│       └── writing-for-agents/
├── architecture/
│   └── skill-inventory.md
└── skills-lock.json
```

### Agent Integration
- **Discovery:** The `skills` CLI automatically linked these project skills to all detected coding agents, including **Antigravity**, **Antigravity CLI**, **Codex**, **Cursor**, **Gemini CLI**, **Amp**, and **Cline**.
- **Lockfile Integrity:** `skills-lock.json` maintains cryptographic hash verification (`computedHash`) and source git repositories for reproducible reinstallation across development environments via `npx skills experimental_install`.
- **Runtime Prerequisites:**
  - `deep-research`: Includes Python verification scripts (`validate_report.py`, `verify_citations.py`, `md_to_html.py`) requiring a local Python 3 interpreter if automated claim validation scripts are executed.
  - Subagent Spawning: Skills like `improve-codebase-architecture`, `codebase-design` ("Design It Twice"), `grilling`, and `research` expect the host agent to support spawning sub-agents with fresh context windows for deep exploration.
