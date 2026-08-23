---
name: agent-factory-architecture
description: Apply Agent Factory's product architecture and authority boundaries. Use whenever creating or modifying Rust ledger code, Herdr/Git integration, IPC/contracts, worktree/workspace logic, or any change that touches authority boundaries.
---

# Agent Factory Architecture

Institutional knowledge as code. When you create or change code that crosses an authority boundary, apply these rules.

## Authorities — one writer each

1. `apps/web-ui` renders validated Rust projections and emits typed intents. Ephemeral interaction state only; no domain state machines, persistence, process control, or recovery.
2. `apps/native-host` (Zig) owns window lifecycle, platform integration, packaging, security policy, Rust sidecar lifecycle, bounded opaque byte transport. Narrow platform actions only.
3. Rust (`services/runtime`, `crates/*`) owns durable ledger and control policy: Projects, Target Agents, Drafts, Versions, Workspace Bindings, Factory Runs, managed-session lineage, Work Contexts, workspace PTYs, Environments, providers, secrets, plugins, updates, IPC validation, recovery.
4. Herdr owns live Workspaces, tabs, panes, Agents, sessions, processes, terminals, topology, lifecycle states. Agent Factory is a client; never a second runtime.
5. Git owns worktree/checkout existence, branch, HEAD, dirty/diff/commits/tags.
6. Orchestrator owns workflow decisions (start sub-agent, iterate, evaluate, escalate, finish) via authenticated `agent-control` commands only.

## Invariants you must enforce

- Commit durable Factory facts before publishing their revisioned projection. Observed Herdr/Git facts carry freshness + source; they need not be written to SQLite before display.
- A live projection is a join of the durable ledger + fresh Herdr + fresh Git snapshots. Never copy Herdr/Git state into a competing writable machine.
- `services/runtime/src/herdr_sessions.rs` is the only runtime module that uses the Herdr client; `crates/herdr-client` is the only crate that speaks the Herdr socket protocol.
- Harnesses are discovered from Herdr manifests. Never probe `PATH`, bundle/install agents, or launch harness executables outside Herdr. Never start/stop/restart Herdr.
- One Target Agent may have multiple Workspace Bindings; a binding is the durable execution-context identity for Drafts, sessions, Runs.
- A Version is an immutable Git commit/tag + metadata; no persistent Herdr Workspace destination.

## Checks

Run `pnpm contracts:check` when touching `crates/runtime-contract`, and include its output in your summary.
