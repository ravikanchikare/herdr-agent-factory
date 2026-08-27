# Spec: Herdr-native Agent Factory control plane

Derived from: `intent.md` (2026-08-24) · Status: accepted
Author: Agent Factory engineering
Skills applied: `agent-factory-architecture`, `herdr-authority`,
`rust-ledger`
Prompt: Reconstruct requirements and design from the current tree so
this spec matches shipped code, not future work. Flag gaps instead of
inventing product.

## Requirements

1. The application is a macOS desktop control plane. Herdr is the
   permanent agent runtime; Git is the repository authority; Rust is
   the durable Factory ledger and control-policy writer.
2. Durable product records are Projects, Target Agents, Drafts,
   Versions, Workspace Bindings, managed-session history, Factory
   Runs, Work Contexts, workspace-terminal descriptors, Environments,
   providers, secrets metadata, plugins, and settings.
3. A Target Agent is the primary navigation identity. A Workspace
   Binding joins it to one Project or worktree and is the execution
   context for Drafts, sessions, and Runs.
4. A Draft is mutable and tied to one binding and worktree. A Version
   is an immutable Git commit/tag plus Factory metadata.
5. A Factory Run records one Orchestrator session and the Coding or
   Evaluation sessions that Orchestrator explicitly requests. It does
   not claim every agent in the Workspace.
6. Starting a Run accepts the user's conversational objective, resolves
   one selected Environment, then asks Herdr to create a fresh Orchestrator
   in the binding's Workspace.
7. Only the Orchestrator holds `AGENT_FACTORY_CONTROL_TOKEN`. Its
   verbs are `status`, `start coding`, `start evaluation`, `escalate`,
   and `finish` (`pass` | `needs-review`).
8. Run semantic state changes only through those commands or an
   authorized cancel. Herdr `idle` / `working` / `blocked` / `done` /
   `unknown` never finish a Run.
9. The Draft view joins a fresh Herdr snapshot with the ledger: managed
   agents grouped by Run, role, and parent; remaining agents as other
   runtime activity; missing Herdr objects as historical.
10. Live commands require a fresh Herdr observation. Stale views are
    labelled last-observed and cannot authorize side effects.
11. Side-effecting control requests carry stable intent identity and
    fresh Run, Herdr, and Git preconditions. Retries must not create a
    second agent, worktree, publication, or cleanup.
12. The production frontend is a statically exported Next.js app.
    `crates/runtime-contract` is the sole runtime message authority;
    TypeScript bindings are generated, never hand-edited.
13. Native-SDK owns window, drag, folder picker, notifications, Draft
    windows, sidecar lifecycle, and the Herdr client TUI surface. It
    does not parse domain payloads.
14. Providers are app-level SQLite records. Environments are independent
    descriptors that reference providers by id. Raw credentials stay in
    Keychain. Each new managed session gets its own loopback gateway.
15. Plugins distribute Skills and MCP tools into Environments. Local
    stdio requires explicit trust. Remote artifacts fail closed on
    HTTPS, size, digest, and signature rules.
16. Closing an Agent Factory pane is presentation-only.

## Design

### Authority

Six writers. No dual writes.

| Authority | Owns | Must not |
|---|---|---|
| `apps/web-ui` | Ephemeral view state, typed intents | Domain machines, persistence, process control |
| `apps/native-host` | Window, packaging, policy, sidecar, opaque transport, Herdr TUI | Domain payload interpretation |
| Rust | Ledger, Environments, Run authorization, evidence, IPC, recovery | Live Herdr/Git as SQLite truth |
| Herdr | Workspaces, tabs, panes, agents, processes, terminals, lifecycle | Factory semantics |
| Git | Path, branch, HEAD, dirty, diff, commit, tag | Factory policy |
| Orchestrator | Delegate, iterate, evaluate, escalate, finish | Holding a control token on a sub-agent |

`services/runtime/src/herdr_sessions.rs` is the only runtime module that
uses the Herdr client. `crates/herdr-client` is the only crate on the
Herdr socket.

### Domain

- **Target Agent** — portable identity in
  `.agent-factory/target-agent.json`. Machine-local roots stay in the
  ledger.
- **Workspace Binding** — durable context. Zero or one current
  Factory-managed Herdr Workspace, located by Factory metadata and
  revalidated. Sequential Runs reuse that Workspace and create fresh
  sessions.
- **Draft** — objective, criteria, branch, worktree path, Git head
  (observed), Environment preference, lifecycle
  `active | publishing | archived | cleanup_required`.
- **Version** — git commit + tag; inspected with
  `version.files.list` / `version.file.read`. No persistent Workspace.
- **Factory Run** — objective, criteria, Environment, starting/final
  Git anchors, changed files, test evidence, evaluation, escalation,
  state `draft | orchestrating | coding | evaluating | escalated |
  passed | failed | needs_review | cancelled`.
- **Managed session** — binding, Run, role, parent, Environment,
  stable Herdr identity, prompt-delivery receipt, durable outcome.
  Lifecycle and placement stay live Herdr facts.
- **Work Context** — at most one of Draft, managed session, or Factory
  Run; all-null is Target Activity (empty). At most three panes; a
  context appears once. Focus derives active target, binding, and
  work item.
- **Environment** — schema v1 descriptor: provider reference, model
  policy, variables, plugins, harnesses, permissions. Unready
  Environments cannot start sessions. No global active Environment.
- **Harness** — allowlisted Herdr agent kind from manifests.

Repository policy `.agent-factory/config.json` may set a relative
`worktreesDirectory`. Rust validates it, then Herdr creates the
worktree. Existing worktrees are never silently moved.

### UX

Single static page (`apps/web-ui/app/page.tsx`) rendering
`RuntimeApplication`.

- Sidebar: Target Agents as disclosure folders; Draft rows by binding
  name; Create Agent; Settings. Versions are not sidebar identities.
- Work creation: Target Agent, project root, Environment, objective.
- Draft workspace: conversational Run composer, visible Project and
  Environment context, Cancel Run, live Herdr agent tree, Run history,
  code-changes inspector, Open/Close Herdr terminal in the title bar.
- Draft Overview: width-driven inline column or popover.
- Versions: tab surface and read-only Git file inspector.
- Session panes: coding/evaluation transcript and agent-native input.
- Settings: General, Providers, Environments, Secrets, Harnesses,
  Plugins.
- Target Activity: intentional empty state. Do not synthesize a feed.

Herdr lifecycle is shown as Herdr reports it. `done` is unseen
background work returned to ready, not Run completion.

The native shell starts with the WebView filling the window. Opening
the Run terminal asks Rust to focus the Factory-managed Workspace,
then shows Native-SDK `<terminal pty>` running the Herdr client TUI
(WebView ~30%, terminal ~70%). Reopening another active Run focuses
that Workspace and reuses the client. No `xterm.js`.

Actions that create configuration use **Add**. Actions that author
target-agent artifacts use **Create**.

### API / Contracts

Runtime IPC (`crates/ipc-contract`): length-prefixed JSON frames,
`MAX_FRAME_BYTES = 1 MiB`, `PROTOCOL_VERSION: u16 = 1`. Stdout is
frames only; logs go to stderr. Bridge command is
`window.zero.invoke("runtime.invoke", request)`.

`crates/runtime-contract` (`CONTRACT_VERSION: u32 = 1`) owns methods.
The UI snapshot is `ApplicationProjection`: revision, settings,
derived active ids from focus, projects, providers, environments,
Herdr status, harnesses, agent sessions, live agents, factory runs,
and `target_workspace` (groups, work contexts, panes, terminals,
focused pane).

Orchestrator control (`crates/agent-control`): newline-delimited JSON
on a Unix socket, one request per connection. CLI:
`services/agent-cli` (`agent-factory`). Endpoint and token injected
into the Orchestrator pane only.

Events are invalidations (`harness.changed`,
`notification.requested`), not a transition log. After an event, Rust
re-reads. Reload, reconnect, revision gap, or invalid payload → full
snapshot.

### Data / Persistence

SQLite (`crates/project-store`, schema user_version 27) stores durable
Factory facts. Environment descriptors live on disk under
`environments/<id>/environment.json`. Provider records are
`provider_json`. Secret values are Keychain-only.

Do not persist current Herdr lifecycle, topology, process IDs, focus,
current Git status/diff, transcripts, or unassociated-agent history as
authoritative state. A last-observed cache must include freshness and
must never authorize a command.

Commit durable facts before publishing their revisioned projection.
Observed Herdr and Git facts may appear in the projection without a
prior SQLite write if they carry source and freshness.

Filesystem operations require an explicitly trusted project and stay
inside canonical roots.

## Flagged concerns

- [x] Validator/Auditor as durable Run states — owner: product —
  status: resolved. First-class roles are Orchestrator, Coding,
  Evaluation. Other passes are managed-agent responsibilities.
- [x] OpenCode / TypeScript runtime — owner: architecture —
  status: resolved. Parked; see `docs/spec/adr-0012-park-opencode-research.md`.
- [x] Target Activity — owner: product — status: resolved as empty
  until a real activity domain exists.
- [ ] Discard Draft has a runtime method and no Draft chrome control
  — owner: product — status: open, later intent.
- [ ] Factory workspace PTYs (`workspaceTerminal.*`) have no primary
  UI — owner: product — status: open, later intent.
- [ ] Orchestration-thread pane body renders "Work unavailable" —
  owner: product — status: open, later intent.

## Out of scope

Matches `intent.md`: Automations, Tasks, standalone MCP/Skills
product area, hosted gateways, bundled agents, web terminals,
starting Herdr, transcript-as-semantics.

## Acceptance criteria

Maps 1-to-1 to intent success criteria.

1. Durable Target Agent, Draft, Binding, Environment, and Provider
   records survive WebView reload via `snapshot.get`.
2. `factoryRun.create` applies the selected Environment and asks
   Herdr for a new Orchestrator in the binding Workspace.
3. `agent-factory` verbs match `crates/agent-control`; the token is
   injected only into the Orchestrator pane.
4. Draft workspace renders managed vs other live agents from a Herdr
   snapshot; historical sessions are not live.
5. `workspacePane.close` does not stop agents, cancel Runs, or kill
   PTYs.
6. `agentDraft.publish` writes a Version from Git; inspector reads
   the commit ephemerally.
7. `pnpm validate`, `pnpm test:web`, `cargo test --workspace`, and
   `pnpm smoke:web` are green.

## Risks & mitigations

- **Stale Herdr observations authorizing commands.** Mitigate: freshness
  on every live observation; disable commands when Herdr is unavailable
  or the snapshot is last-observed.
- **Duplicate launches on retry.** Mitigate: stable intent identity and
  precondition checks; reject stale commands.
- **Credential leakage through pane env or logs.** Mitigate: Keychain
  only; loopback sentinel; neutralize inherited provider variables; no
  `Debug` of secrets.
- **Competing runtimes.** Mitigate: one Herdr client module; generated
  contracts only; no PATH probing; no web terminal.
- **Schema drift.** Mitigate: greenfield reset; `pnpm contracts:check`
  before commit when `runtime-contract` changes.
