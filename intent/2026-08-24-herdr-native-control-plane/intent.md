# Intent: Herdr-native Agent Factory control plane

Author: Agent Factory engineering · Status: accepted · Date: 2026-08-24
External record: —

This record was written from the current tree. It describes the product
that exists, not a future redesign.

## Problem

Coding agents can already write and run. What we cannot do is run a
repeatable factory around them: define the agent we want, isolate a
draft, apply one Environment, record who authorized each managed
session, and accept a version only when evidence meets criteria we
declared up front. Herdr knows what is running now. Git knows the
files. Neither knows why the work exists or what "done" means.

## Proposed outcome

A macOS desktop application that is a control plane, not a second
agent runtime. A person can:

- define a Target Agent with an objective and measurable success
  criteria;
- work a mutable Draft in one Workspace Binding and worktree;
- start a Factory Run that creates a fresh Orchestrator session in
  Herdr after Rust applies a selected Environment;
- watch the complete live agent tree, intervene in Herdr, and keep a
  durable record of authorized sessions and outcome;
- publish an immutable Version from Git when the work is accepted.

The UI joins three authorities into one honest view: the Factory
ledger (why the work exists), Herdr (what is running now), and Git
(what the repository is now).

## Affected users and systems

- People who build and evaluate target agents on a Mac.
- `apps/web-ui` (static Next.js shell), `apps/native-host` (Zig
  Native-SDK host), `services/runtime` (Rust sidecar).
- Herdr (live Workspaces, agents, panes, processes, terminals).
- Git (worktrees, branches, HEAD, diffs, commits, tags).
- The Orchestrator inside a Herdr pane, driving its Run through
  `agent-factory` / `crates/agent-control`.

## Constraints

- Herdr is the permanent agent runtime. Do not start, stop, install,
  or wrap it. Do not probe `PATH` for harnesses.
- Rust owns the durable Factory ledger and control policy. It does
  not copy Herdr or Git live state into a competing writable cache.
- Git is the authority on checkout, branch, HEAD, dirty state, diff,
  commit, and tag. Herdr creates, opens, and removes Draft worktrees.
- React owns ephemeral view state only. No domain state machine, no
  persistence, no process control.
- Native-SDK owns window, packaging, security policy, sidecar
  lifecycle, and opaque byte transport. It does not interpret domain
  payloads.
- Only the Orchestrator receives the per-Run control token. Run state
  changes only through explicit authenticated commands.
- Production UI is a static Next.js export. No Next.js server, API
  routes, Server Actions, middleware, or ISR.
- Raw credentials live in the platform credential store. They never
  appear in SQLite, logs, IPC, UI projections, or Herdr environment
  variables.
- Greenfield: no compatibility with obsolete schemas. Reset rather
  than migrate.

## Out of scope

- Automations and Tasks.
- Standalone MCP or Skills management outside Environments.
- Hosted gateway infrastructure.
- Bundled, downloaded, or PATH-installed agents.
- A Target Activity feed synthesized from unrelated records.
- Reconstructing tool calls, plans, or approvals from pane text.
- A web terminal renderer (`xterm.js` or similar).
- Starting or managing the Herdr server.

## Success criteria

1. A person can create a Target Agent, Draft, Workspace Binding,
   Environment, and Provider, and see them after a WebView reload.
2. Starting a Run asks Herdr for a fresh Orchestrator in the binding's
   Workspace after Rust resolves the selected Environment.
3. The Orchestrator can `status`, `start coding`, `start evaluation`,
   `escalate`, and `finish` through `agent-factory`; managed agents
   never receive the control token.
4. The Draft view shows the complete live Herdr agent tree, grouped
   by Run where authorized, with remaining agents as other runtime
   activity. Historical managed sessions stay historical.
5. Closing an Agent Factory pane never stops a Herdr agent, cancels a
   Run, or closes a workspace terminal.
6. Publishing a Draft creates an immutable Git commit/tag Version
   inspectable without a persistent Herdr Workspace.
7. `pnpm validate`, `pnpm test`, and `pnpm smoke:web` pass on the
   tree that implements this intent.

## Open questions

Resolved in the current tree:

- Validator and Auditor are responsibilities inside the Run loop, not
  first-class durable Run states. The executable contract is
  Orchestrator, Coding, and Evaluation.
- Target Activity stays an empty state until a real activity domain
  exists.
- One native Herdr client TUI is the Run terminal. Factory-owned
  workspace PTYs exist in Rust but are not a primary UI surface.

Still open for later intents:

- Discard-Draft UI (runtime method exists; no Draft chrome control).
- A primary UI for Factory workspace PTYs.
- A pane body for orchestration-thread Work Contexts (currently
  "Work unavailable").

## Constraints from policy

Skills that bound the spec: `agent-factory-architecture`,
`herdr-authority`, `rust-ledger`.
