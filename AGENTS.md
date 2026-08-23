# Repository Guidelines

## Product Direction

Agent Factory is a native desktop control plane and presentation layer for
building and evaluating target agents on Herdr. Herdr is the permanent agent
runtime: its Workspace, Agent, pane, process, and native session concepts are
first-class runtime concepts in Agent Factory rather than an interchangeable
backend hidden behind parallel domain types.

The application joins three authorities into one honest view:

- Agent Factory's durable product ledger explains why work exists, who
  authorized it, which Environment applies, and what durable result it
  produced.
- Herdr reports what Workspaces, agents, sessions, panes, processes, terminal
  text, and lifecycle state exist now.
- Git reports what worktrees, branches, commits, tags, and changes exist now.

Do not copy Herdr or Git state into a competing writable state machine. A live
projection is a join of the durable Factory ledger with fresh Herdr and Git
snapshots. Historical records describe what Agent Factory authorized and
observed; they never overrule current runtime or repository truth.

A Factory Run is a durable semantic record, not a runtime container. It records
one Orchestrator session and the managed Coding, Evaluation, or other sub-agent
sessions that the Orchestrator requests. The Orchestrator owns workflow
decisions and judges the work. Agent Factory applies Environments, authorizes
domain transitions, captures durable evidence, and asks Herdr to perform
runtime operations. Herdr owns the resulting processes and lifecycle.

Use the domain that exists today. The durable product concepts are Projects,
Target Agents, Drafts, Versions, Workspace Bindings, managed session history,
Factory Runs, Work Contexts, workspace terminals, Environments, providers,
secrets, plugins, and updates. Do not introduce execution profiles, target
executions, standalone evaluation runs, or other speculative entities before a
real user behavior requires them.

Target Agents are the primary navigation identity. A Workspace Binding joins a
Target Agent to one Project or worktree and is the durable execution-context
identity. Drafts, managed sessions, and Factory Runs reference the binding and
derive target/project identity from it. Do not duplicate those identities in
new state or contracts. Project records are metadata and local-container
boundaries, not a second primary navigation model.

A Target Agent is a product definition, not a Herdr Agent process. A Draft is a
mutable definition associated with one Workspace Binding and worktree. A
Version is an immutable Git commit/tag plus Agent Factory metadata. A managed
session record preserves role, parentage, Run association, Environment, and
durable outcome after its Herdr object disappears; it is not a shadow copy of
live Herdr state.

Treat this repository as greenfield. There are no backward-compatibility
requirements for obsolete schemas, contracts, naming, or persistence. Prefer
one explicit current model; reset incompatible development state instead of
adding legacy aliases, inference, dual reads, or migrations.

## Non-Negotiable Authority

The architecture has one writer for each class of truth:

1. `apps/web-ui` renders validated Rust projections and emits typed intents.
   React may own ephemeral interaction state only; it must not implement domain
   state machines, persistence, process control, retries, or recovery.
2. `apps/native-host` owns Native-SDK window lifecycle, platform integration,
   packaging, security policy, the Rust sidecar lifecycle, and bounded opaque
   byte transport. It may implement narrow platform actions such as window
   dragging, folder selection, and notifications. It must not interpret domain
   payloads or own application behavior.
3. Rust owns Agent Factory's product ledger and control policy: durable domain
   transitions, Environment application, authenticated Run commands, evidence,
   cleanup authorization, workspace PTYs, providers, gateways, plugins,
   updates, IPC validation, and recovery coordination.
4. Herdr owns live Workspaces, tabs, panes, Agents, native agent sessions,
   processes, terminals, topology, and the lifecycle states it reports. Agent
   Factory connects as a client; it never implements a second agent runtime.
5. Git owns repository and worktree facts: checkout existence, branch, HEAD,
   dirty state, diff, commits, and tags. Agent Factory and Herdr may request Git
   operations but may not replace Git observations with cached claims.
6. The Orchestrator owns the workflow decision to start a managed sub-agent,
   iterate, evaluate, escalate, or finish. Rust validates and records explicit
   commands; it never infers those decisions from Herdr lifecycle or terminal
   text.

Commit durable Factory facts before publishing their revisioned projection.
Observed Herdr and Git facts do not need to be written to SQLite before display;
they must carry freshness and source identity. The UI requests a full snapshot
after reload, reconnect, revision gaps, or invalid data. Never create competing
writable state in React, native code, Rust persistence, or cached projections.

A Harness is a Herdr agent kind discovered from Herdr manifests. Never probe
`PATH` for agents, bundle or install agents, or launch harness executables
outside Herdr. Agent Factory observes and reconnects to Herdr; it does not
start, stop, restart, update, or kill the Herdr server. Explain unavailable
prerequisites to the user.

Herdr exposes runtime topology, process information, lifecycle state, events,
and terminal text. It does not expose a structured semantic record of an
agent's turns. Do not reconstruct tool calls, plans, usage, or approval
protocols from pane text; approvals remain inside the agent's own interface.
Read recent unwrapped transcript text on demand rather than streaming a
duplicate transcript into the main projection.

Herdr agents and Agent Factory workspace terminals are different concepts.
Herdr owns agent panes; `crates/terminal-runtime` owns user-created workspace
PTYs. Do not route agent lifecycle through the workspace-terminal subsystem.

## Run and Orchestration Invariants

- Starting a Run asks Herdr to create a fresh Orchestrator session in the
  binding's Workspace after Rust resolves and applies the selected Environment.
- The Orchestrator decides when another managed agent is required and calls the
  narrow authenticated Agent Factory control boundary. Rust validates the Run
  command, applies the Environment, and asks Herdr to create the session. The
  Orchestrator then prompts, waits for, reads, and judges that agent through
  Herdr.
- A managed sub-agent must not receive the Run control token. Only the
  Orchestrator may issue semantic Run commands.
- Every side-effecting control request carries stable intent identity and fresh
  Run, Herdr, and Git preconditions appropriate to the action. A retry must not
  create another agent, handoff, worktree, publication, or cleanup operation.
  Reject and reconcile stale commands instead of guessing that they succeeded.
- Run state changes only through explicit authenticated commands. `idle`,
  `working`, `blocked`, `done`, pane exit, terminal text, and transcript content
  never advance or finish a Run.
- A Run records the exact managed sessions it authorized. It must not claim
  every agent found in the Workspace.
- Herdr Agents without a Factory association remain visible under other runtime
  activity. Do not hide them, silently adopt them into a Run, or persist
  artificial history after they disappear.
- One mutable Draft may have at most one live Run. Parallel mutable Runs require
  distinct Draft worktrees, Workspace Bindings, and Herdr Workspaces.
- Finishing or cancelling a Run records the outcome and revokes its control
  authority. It does not delete the Herdr Workspace or Git worktree.

## Live State and UI Actions

Use Herdr lifecycle values directly. Do not reinterpret them as Factory Run
states:

- `idle` is ready for input.
- `working` is actively producing output.
- `blocked` means Herdr recognized an approval or question surface in the
  agent's own UI.
- `done` is unseen background work that has returned to the same underlying
  ready state as `idle`; it is not process exit or Run completion.
- `unknown` means an agent is present but Herdr cannot classify it confidently;
  it never proves readiness or completion.

Runtime actions are enabled only from a fresh Herdr observation. Allow prompt
or resume actions for settled agents, observation and interruption while
working, and agent-native input while blocked. When state is stale or Herdr is
unavailable, retain the durable product view, label the runtime view as last
observed, and disable commands that require live preconditions.

The Draft and Run overview derives its complete live agent tree from Herdr.
Group managed agents by Run, role, and parent session; group every remaining
agent in the binding's Workspace as other runtime activity. A persisted managed
session that no longer has a live Herdr object is historical, not `done`,
`idle`, or live.

## Workspace and Worktree Invariants

- `.agent-factory/target-agent.json` is the portable target identity. Machine-
  local roots and runtime references stay outside the portable manifest.
- The source repository may define `.agent-factory/config.json` schema v1 with
  a relative `worktreesDirectory`. Rust validates it as product policy and asks
  Herdr's worktree infrastructure to apply it for new Drafts. Projects do not
  own or override it, and existing worktrees are never silently moved.
- One Target Agent may have multiple Workspace Bindings for projects or
  worktrees. A binding is the durable context boundary for Drafts, sessions,
  and Factory Runs.
- A binding has zero or one current Factory-managed Herdr Workspace. Locate it
  by stable Factory metadata, retaining a Herdr ID only as a revalidated
  reference. Herdr remains authoritative about whether the Workspace exists.
- Sequential Runs reuse the binding's Herdr Workspace but create fresh managed
  sessions. A Run references its exact sessions and never owns the Workspace.
- Herdr performs worktree create, open, and remove operations. Git remains the
  authority on actual path, branch, HEAD, cleanliness, diff, commit, and tag.
  Do not retain a second Rust worktree-management lifecycle.
- Persist only the binding/worktree association, provenance, expected branch
  policy, Git anchors required as historical evidence, and whether Agent
  Factory may request cleanup. Current Git state is always read from Git.
- A Harness agent may modify files and make task-level commits within its
  assigned checkout. It must not own worktree creation, adoption, relocation,
  removal, or Draft cleanup policy.
- Publishing or discarding a Draft may authorize worktree cleanup. Run
  completion, session exit, UI pane closure, and Herdr Workspace closure do not.
- A Version is an immutable Git snapshot and has no persistent Herdr Workspace
  destination. Inspect Versions ephemerally from their commit.
- A Work Context is durable Rust state and explicitly references a Draft,
  managed Agent Session, Factory Run, or neither for Target Activity. Do not
  replace this with a loose polymorphic identifier pair.
- The workspace shows at most three panes, and a Work Context appears at most
  once. Selecting visible work focuses its pane; selecting hidden work opens or
  restores it according to the requested placement.
- Closing a pane is presentation-only. It must never stop an agent session,
  cancel a Factory Run, or close a workspace terminal.
- Closing a Herdr Workspace ends its live panes and processes but does not
  manufacture a Run verdict or remove its worktree. Surface the runtime
  interruption without changing the Run's semantic phase or outcome until an
  authorized command reconciles or concludes it.
- Focus derives the active target, binding, and work item. Do not add parallel
  mutable project/session/run selections.
- Target Activity remains an intentional empty state until a concrete
  activity-domain capability exists. Do not synthesize an activity feed from
  unrelated records.

## Events, Recovery, and Persistence

Herdr snapshots and direct reads are authoritative. Events are invalidations,
not a durable transition log:

- Subscribe to the relevant Herdr Workspace, worktree, tab, pane, process, and
  Agent events, then obtain a complete runtime snapshot.
- Re-read the affected entity after an event. Do not depend on event arrival
  order or apply an old event payload over a newer snapshot.
- On startup, WebView reload, Rust restart, Herdr reconnect, subscription loss,
  revision gap, or invalid payload, obtain a full Herdr snapshot and fresh Git
  observations before enabling live commands.
- A failed list or snapshot says the authority is unavailable; it never means
  that every Workspace or Agent has disappeared.
- Reconcile managed sessions through their stable Herdr identity and Factory
  association. Treat cached Workspace, pane, and process IDs only as locators
  that must be revalidated.
- Read terminal text and recent unwrapped transcripts from Herdr on demand. Do
  not persist a duplicate transcript, structured turn model, or full lifecycle
  event history.
- Generate notifications only from freshly reconciled transitions and user
  preferences. Notification delivery does not change agent or Run state.

Persist only facts that must outlive Herdr and mutable Git state:

- Target Agent, Draft, Version, Workspace Binding, Environment, provider,
  trust, and permission records;
- Run definition, semantic state, accepted handoffs, managed session lineage,
  escalation, and final outcome;
- command-delivery receipts needed to prevent duplicate prompts or launches;
- starting and final Git anchors, evaluation evidence, and immutable artifacts;
- worktree provenance and cleanup authorization; and
- Factory-specific Work Context, pane, dock, terminal descriptor, and settings
  state that the product intentionally restores.

Do not persist current Herdr lifecycle, runtime topology, process IDs, focus,
current Git status or diff, transcripts, unassociated agent history, or copied
runtime event streams as authoritative state. A last-observed cache must include
freshness and must never authorize a command.

## Environments, Providers, Secrets, and Plugins

Environments are independent execution boundaries. Their strict versioned
descriptors link provider selection, model narrowing, environment variables,
working directory, plugin Skills and MCP configuration, Harness selection,
permissions, and registries. A launch selects one Environment explicitly; Rust
resolves it once into the input used when the Herdr pane is created. There is no
global active Environment.

Providers are reusable app-level records in SQLite. Environment descriptors
reference provider IDs and opaque credential references; they never inline
provider configuration or raw secrets. Raw credentials live in the platform
credential store and are write-only. They must never appear in SQLite,
descriptors, logs, Debug output, IPC payloads, UI projections, Native-SDK state,
or Herdr environment variables.

Execution-affecting provider changes mark linked Environments as needing setup;
cosmetic changes such as a rename do not. An unready Environment cannot start a
new session. Treat readiness as current state, not as a version-history system.

Provider traffic uses the Rust-owned per-session loopback gateway. It injects
the endpoint, sentinel credential, and selected model needed by the agent and
neutralizes inherited upstream provider variables. Native code stays
provider-neutral.

Plugins distribute tools and Skills, not hosted applications or WebViews. Do
not create a separate global MCP/Skills management model. Plugin installation,
resolution, and transport planning are Rust-owned and fail closed: require
explicit trust for local stdio execution, validate paths and manifests, enforce
HTTPS and size/digest/signature rules for remote artifacts, and reject unsafe
redirects or credentials in URLs. Agents use standard MCP or agent-native
transports; do not invent a product-private agent protocol.

## Static UI, Native Host, and IPC

The production frontend is a statically exported Next.js application. Do not
add a Next.js server, API routes, Route Handlers, Server Actions, middleware,
ISR, or any runtime feature that requires a web server. Development uses the
exact origin `http://127.0.0.1:3000/`; packaged content uses `zero://app`. Keep
origin policy, CSP, connect-src, and bridge allowlists synchronized.

The native bridge exposes `window.zero.invoke("runtime.invoke", request)` for
the Rust runtime plus narrowly scoped platform intents. Runtime IPC is one
length-prefixed stream with a 1 MiB maximum frame. Rust stdout contains framed
protocol bytes only; logs go to stderr. Treat all bridge responses as untrusted
until validated.

Filesystem operations require an explicitly trusted project and remain confined
to canonical project roots. Reject path traversal, escaping symlinks, special
files, and filesystem targets outside the authorized roots; do not weaken this
boundary in UI or native code.

`crates/runtime-contract` is the sole authority for runtime messages and
projections. Generate JSON Schema and TypeScript bindings into
`packages/shared/runtime-client`; never hand-edit generated bindings or create
parallel handwritten transport types. Connect layers only through generated,
versioned contracts.

Window dragging is presentation plus platform behavior: React performs DOM
hit-testing and sends the typed drag intent, while Native-SDK starts the move.
Rust must not own dragging. Preserve exclusions for buttons, inputs, and other
interactive controls.

## Project Structure

- `apps/web-ui`: static Next.js UI. Group components by product domain such as
  `shell`, `sessions`, `factory-runs`, `terminal`, and `settings`; colocate
  tests.
- `apps/native-host`: Zig Native-SDK host, platform policy, window behavior,
  sidecar lifecycle, packaging, and bridge transport.
- `services/runtime`: Rust composition root and IPC service.
  `services/runtime/src/herdr_sessions.rs` is the only runtime module that uses
  the Herdr client.
- `crates/herdr-client`: the only crate that speaks the Herdr socket protocol.
- `crates/app-core`, `crates/project-store`, and `crates/git-runtime`:
  product transitions, Run evidence, projections, and the minimal durable
  SQLite ledger (`services/runtime` is the composition root). They do not mirror
  Herdr's runtime state machine.
- `crates/environment-runtime`, `crates/llm-provider-runtime`,
  `crates/llm-gateway`, and `crates/platform-secrets`: Environment, provider,
  gateway, and credential boundaries.
- `crates/ipc-contract` and `crates/runtime-contract`: framing plus the
  authoritative runtime request, response, intent, and projection schemas.
- `crates/agent-control` and `services/agent-cli`: the contract an Orchestrator
  drives its own Factory Run through, and the `agent-factory` command it types.
  Commands are authorized by a per-Run token injected into the Orchestrator's
  pane; no other agent receives one.
- `crates/filesystem-runtime`: validated workspace filesystem behavior.
- `crates/plugin-runtime`, `crates/terminal-runtime`, and
  `crates/update-runtime`: plugin security, workspace PTYs, and updates.
- `packages/shared/ui`: shared shadcn primitives; do not duplicate them in apps.
- `packages/shared/theme`: semantic design tokens.
- `packages/shared/runtime-client`: generated frontend bindings.
- `plugins/`: fixtures and plugin packages. `scripts/` and `tooling/`:
  repository orchestration. `docs/spec/`: ownership notes and accepted
  ADRs.

Start architecture work with `docs/spec/ownership.md`,
`docs/spec/herdr.md`, and the accepted ADRs. When prose and executable
contracts disagree, stop and reconcile the owning contract or ADR instead of
silently creating a third interpretation. The Herdr-native authority model in
this file is the current product direction. Revise older ADRs that assign direct
worktree lifecycle or persisted live runtime authority to Rust; do not preserve
both models through adapters, aliases, or dual writes.

## UI and React Principles

Use only the project's shadcn primitives, importing shared primitives from
`packages/shared/ui`. Reuse existing AI Elements where the product already uses
them. Do not add a competing component library or duplicate a shared primitive.
Use semantic Tailwind tokens only: no arbitrary colors and no arbitrary spacing
values. All actions require accessible names, and every flow must support
keyboard navigation and visible focus.

Use `text-muted-foreground` for non-semantic icons by default, transitioning
them to `text-foreground` on hover, keyboard focus, or active states such as
selected, pressed, or expanded. Preserve explicit semantic colors for status,
warning, and destructive icons.

Use **Add** for actions that create or bring in configuration objects —
Providers, Environments, Secrets, and Environment Variables. Use **Create** for
authoring target-agent artifacts such as agents, drafts, and versions.

Keep components declarative and split by change boundary. Derive values during
render, handle user-caused work in event handlers, and subscribe to external
stores with `useSyncExternalStore`. Do not mirror props or Rust projections into
local state. Use lazy or dynamic loading for genuinely heavy optional UI, and
avoid broad barrel imports when direct imports preserve a smaller client graph.
Do not add memoization without a measured render or computation problem.

Avoid `useEffect`. Prefer:

- data supplied by the Rust projection or an established query abstraction;
- derived values computed during render, using `useMemo` only when expensive;
- event-driven work performed directly in event handlers;
- `useSyncExternalStore` for external subscriptions;
- a `key` boundary when state must reset with identity; and
- callback refs for imperative DOM or third-party-library setup.

If an external imperative library truly requires lifecycle synchronization,
`useEffect` is allowed only with a nearby comment explaining the external
system, setup, and cleanup. It must not become a hidden domain transition.

TypeScript is strict and uses two spaces, 80 columns, no semicolons, double
quotes, and ES5 trailing commas. Use `PascalCase` components and `camelCase`
symbols. Keep public props minimal and use discriminated unions for product
states rather than boolean combinations.

The Run's Herdr terminal renders through Native-SDK's retained
`<terminal pty={key}>` surface over Ghostty-VT and hosts the Herdr client TUI.
Factory-owned workspace PTYs remain a separate `crates/terminal-runtime`
capability. Do not add a web terminal renderer or introduce `xterm.js`.

## Build, Test, and Development

Use Node 24+, pnpm 11+, and the pinned Native SDK through `pnpm exec native`.

- `pnpm dev:web` serves only the browser UI at
  `http://127.0.0.1:3000/`. It does not validate Native-SDK, Rust, Herdr, or
  packaged behavior.
- `pnpm native:dev` builds the Rust runtime and launches the development host at
  the fixed loopback origin. Herdr must already be available; Agent Factory does
  not start it.
- `pnpm build` builds contracts, Rust, and static assets.
- `pnpm native:build` also builds the ReleaseFast native host.
- `pnpm validate` runs formatting/lint, type checks, contract checks, and
  manifest/schema validation.
- `pnpm test` runs web/Node, Rust, and Zig suites.
- `pnpm smoke:web` validates browser-visible keyboard, accessibility, and UI
  behavior.
- `pnpm smoke` adds Native-SDK bridge and packaged-runtime validation.

Make architecture testable at its ownership seams. Add focused regression tests
beside changed behavior, then run the narrow checks plus the relevant smoke
suite. Rust tests must not connect to the developer's Herdr workspace; use a
detached runtime and a stand-in socket via `AGENT_FACTORY_HERDR_SOCKET`,
following `crates/herdr-client/tests`. Native automation owns window, bridge,
sidecar, and packaging evidence; browser automation owns static UI interaction
evidence.

Format Rust with `cargo fmt`. Do not hand-edit generated schema or bindings.
Run `pnpm changeset` for public behavior changes. Use scoped Conventional Commit
subjects such as `feat(herdr): ...` or `fix(shell): ...`.

## Codex automations

- A Codex task can have only one active thread heartbeat. Extend the existing
  heartbeat when workflows need the same session context; use a dedicated task
  for a separate heartbeat rather than creating a fallback cron job.
- Thread heartbeats require a thread destination and inherit the task's model
  and reasoning settings; use cron automations only for standalone project work.

## User experience guidelines

- Avoid visual affordances that imply interactivity where none exists.

<!-- gitbutler-agent-setup:start -->
## Version control

- Use GitButler (`but`) for version-control inspection and write operations, including status, diffs, branching, committing, pushing, and history edits.
- Assume multiple agents may be working in this repository. Do not move, amend, squash, discard, commit, push, or otherwise modify another agent's work unless the user asks.
- For commit just/only/specific changes on a new branch (selected-change requests), use the two-command fast path from the GitButler skill: `but diff`, then `but commit -b <branch> -m "message" <id> <id>`.
- For that fast path, after the commit succeeds, stop and summarize; do not run separate branch, staging, status, or diff commands unless the commit output is missing information you need.
- Use the installed GitButler skill for command recipes and syntax before guessing flags, using `--help`, or translating Git habits directly.
- Mutation commands report their result without appending workspace status. Add `--status-after` only when the next step needs resulting workspace IDs or details; otherwise do not rerun status or diff to verify success.
- Use a dedicated GitButler branch for each agent session, unless the user asks for a different branch structure. Commit only changes that belong to that session.
- Do not push or open pull requests unless the user asks.
- Keep commit messages and pull request descriptions succinct: explain what changed, why it changed, and any important decision.

### Amend local fixes into the right commits

- For small cleanup or follow-up fixes, amend an unpublished local commit when the change clearly belongs with that commit's intent.
- Do not create tiny fixup commits unless the user asks.
- Use GitButler to move the relevant changes into the commit where they belong.
- Ask before rewriting pushed, reviewed, shared, or ambiguous history.

### Split unrelated changes into separate commits

- If one file contains unrelated changes, split them by hunk instead of committing the whole file.
- Keep tests with the behavior they verify.
- Split generated output, docs-only edits, or mechanical cleanup into separate commits when each commit remains coherent on its own.
- If the split is ambiguous, summarize the options before committing.

### Create stacked pull requests

- If this session depends on another in-flight branch, stack its branch on top of that dependency instead of mixing the changes.
- If this session is working in a stack, put commits on the branch where they belong.
- Ask before moving commits onto lower, pushed, reviewed, or shared branches.
- Use `but move` for branch stacking and restacking. Do not recreate branches to simulate stacking.
- For stacked branches, create pull requests with `but pr`, not `gh`, so GitButler keeps the right PR base branches and stack metadata.

### Update from the target branch automatically

- When GitButler status shows new changes on the target branch and the workspace holds only this session's branches, update with `but pull` directly — its output reports the result and `but undo` reverts it.
- If an update you started on your own initiative reports conflicted commits, stop and ask before resolving them (`but undo` reverts the pull if the user prefers).
- When other agents' branches are applied, run `but pull --check` first and ask before updating if it reports conflicts or their branches would move.
- If the user asks you to handle update conflicts, use GitButler's conflict tools. Ask before resolving semantic conflicts, dependency updates, generated files, or conflicts involving another person's work.

### Open draft pull requests by default

- When asked to open a pull request, create it as a draft with GitButler unless the user says it is ready for review.
- Remember that creating a draft pull request still publishes the branch.

### Skip pull requests and land onto the target

- This setup uses the skip-the-PR workflow: when work is approved to publish, land the session branch directly onto the target with `but land <branch>` instead of pushing a branch or opening a pull request.
- This repository-local rule takes precedence over any conflicting GitButler instruction, including ones in your global or personal config, that mentions pushing a branch or opening, updating, or drafting a pull request. Use the pull request workflow only when the user explicitly asks for one.
- `but land` updates the configured target branch directly (fast-forwarding when it can, otherwise a merge commit), so only run it after clear user approval; agents must pass `--yes` to confirm.

### Publish on a shortcut phrase

- When the user says `ship it`, commit this session's changes on its dedicated GitButler branch, creating one if needed.
- Then land that branch onto the target with `but land <branch> --yes` instead of opening a pull request, following the skip-the-PR rules above.
- Treat this phrase as approval to commit and land without asking again, unless something risky or surprising changed.

### Branch naming

- When creating a GitButler branch for an agent session, use `<type>/<short-description>`.

### Commit message convention

- Follow the `type(scope): summary` commit-message convention when writing commit messages.

### Commit checkpoints after each turn

- Commit after a working checkpoint, when the requested change is complete and relevant checks have passed or been reported.
- Treat checkpoint commits as local savepoints, not final review history.
- When the user asks you to tidy the history, use GitButler to squash commits, reword commits, and move changes between commits where appropriate.
- Only tidy unpublished local history unless the user explicitly authorizes changing pushed or shared history.
<!-- gitbutler-agent-setup:end -->
