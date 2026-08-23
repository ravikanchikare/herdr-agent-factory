# Herdr integration

Herdr is Agent Factory's permanent agent runtime. Agent Factory connects as a
client over Herdr's control socket (`~/.config/herdr/herdr.sock`, or a named
session under `sessions/<name>/`). `AGENT_FACTORY_HERDR_SESSION` selects a named
Herdr session. `AGENT_FACTORY_HERDR_SOCKET` points tests at an isolated stand-in
server. Socket discovery honors `herdr-client.toml` (see `crates/herdr-client`).

Agent Factory does not start, stop, restart, update, or kill Herdr. When it is
unavailable, the durable product view remains usable, the last runtime
observation is labelled, and commands that need live preconditions are disabled.

## Workspace Bindings and placement

One Workspace Binding has zero or one current Factory-managed Herdr Workspace.
Rust locates it by stable Factory metadata represented by the human-readable
`<Target Agent> / <Workspace Binding> (<short binding suffix>)` label. The
enclosing application already supplies the Agent Factory product context, so
the label does not repeat that prefix. A cached Herdr ID is only a locator and
must be revalidated. Sequential Runs reuse the binding's Workspace and launch
fresh managed sessions; a Run never owns the Workspace.

Starting a Run asks Herdr to create a fresh Orchestrator session in that
Workspace after Rust resolves the explicitly selected Environment. The
Orchestrator may then issue authenticated Run commands. Rust validates each
command, applies the Run's Environment, records the authorized managed-session
lineage, and asks Herdr to create the requested Coding or Evaluation session.
Only the Orchestrator receives the per-Run control token.

The Environment boundary is fixed when a pane is created: resolved variables,
working directory, loopback gateway, and selected model are passed to Herdr.
If a newly created agent is not ready for its first prompt, Agent Factory keeps
its durable delivery receipt pending and retries against fresh Herdr state. It
does not invent a lifecycle state to represent delivery.

## Live projection and lifecycle

Rust obtains a complete Herdr session snapshot and joins it with the durable
Factory ledger. The Draft and Run workspace shows every live agent in the
binding's Workspace:

- managed agents are grouped by Run, role, and parent session;
- every unmatched agent is shown as other runtime activity; and
- a managed record without a live Herdr object is historical, not a live agent
  with a synthetic lifecycle.

Herdr lifecycle values are projected without reinterpretation:

- `idle` is settled and ready for input;
- `working` is actively producing output;
- `blocked` means Herdr recognized an approval or question surface in the
  agent's own interface;
- `done` is unseen background work returned to the same ready state as `idle`,
  not process exit or Run completion; and
- `unknown` means present but unclassified and proves neither readiness nor
  completion.

Prompt and resume actions require a fresh snapshot and a settled agent.
Observation and interruption remain available while working, and blocked input
stays inside the agent's own interface. Herdr lifecycle, pane exit, terminal
text, and transcript content never advance or finish a Factory Run.

## Events and reconciliation

Herdr events are invalidations, not a transition log. Agent Factory subscribes
to relevant Workspace, worktree, tab, pane, process, and agent events, then
re-reads a complete snapshot. It does not depend on event order or apply an old
payload over a newer observation.

On startup, reconnect, subscription loss, or snapshot failure, durable records
remain intact. A failed list never means every Workspace or agent disappeared.
Previously observed topology may be shown as `Last observed`, but cannot satisfy
a command precondition. A managed agent absent from a fresh successful snapshot
becomes historical and may receive a durable interruption outcome; current
Herdr lifecycle and placement are never written to SQLite.

## Worktrees and Git

Herdr performs Draft worktree creation, opening, and removal through its
worktree API. Before requesting an operation, Rust validates Agent Factory's
repository policy and stable intent identity — specifically
`.agent-factory/config.json` `worktreesDirectory` via
`services/runtime/src/repository_config.rs` before calling
`Herdr.create_worktree`. Afterward, Rust observes Git to verify the actual
checkout, branch, HEAD, cleanliness, diff, commits, and tags.

Publishing or discarding a Draft may authorize worktree cleanup. Finishing a
Run, closing a pane, ending a managed session, or closing the Herdr Workspace
does not. Closing the Workspace ends its live panes and processes but leaves the
Run's semantic state and Git worktree untouched until an explicit authorized
command reconciles them.

## Reading output

Recent unwrapped transcript text is read from Herdr on demand. It is not
streamed into the main projection or persisted, and Agent Factory does not
reconstruct tool calls, plans, usage, approvals, or semantic turns from pane
text. Evaluation evidence and final Run outcomes arrive through explicit
Orchestrator commands and immutable artifacts, not transcript parsing.
