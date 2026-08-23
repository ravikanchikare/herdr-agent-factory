# ADR 0011: Herdr workspace identity and the terminal surface

Status: accepted

## Decision

Agent Factory owns the human-readable correlation between a Target Agent,
Workspace Binding, Draft, Run, and managed session. Herdr owns the live
Workspace, tabs, panes, agent processes, terminal state, and pane topology.
Agent Factory does not persist Herdr workspace IDs or mirror individual agent
terminals in its web UI.

Factory-created Herdr workspaces use this label shape:

```text
<Target Agent> / <Workspace Binding> (<short binding suffix>)
```

The product name is omitted because the enclosing application already supplies
that context. The short suffix only disambiguates repeated names. Herdr agent
names use the role, a slug of the target/binding context, and a short session
suffix while remaining within Herdr's naming rules. Human titles are used for
tabs; full UUIDs remain internal identifiers.

The Run view has one Open action. For an active Run, Rust resolves the Run's
Workspace Binding and target repository/worktree, then asks Herdr to
`worktree.open` that worktree with focus enabled. The response is live Herdr
state; no Factory topology or workspace ID is persisted. A stale or unavailable
Herdr authority disables the action.

Herdr is the complete terminal workspace experience for a Run. Agent Factory
renders durable Run/session metadata and live status, but does not offer an
Open action for each managed agent and does not embed a second terminal surface
for those panes. Herdr may dynamically create, split, remove, and focus panes
without a duplicate Factory layout model.

## Native SDK terminal compatibility

The Native SDK `<terminal pty={key}>` surface is a native-rendered terminal
whose PTY key, output feed, resize lifecycle, and Ghostty-VT emulator are
owned by the Native runtime. Agent Factory uses that surface for one full Herdr
client TUI, not for a Rust-owned workspace PTY and not for an individual Herdr
pane. The PTY child is the resolved `herdr` executable with only the configured
Herdr session selector. Herdr remains authoritative for every Workspace, tab,
pane, agent process, terminal, and topology rendered inside the client.

The native shell starts with the terminal absent and the WebView filling the
window. Run Open first asks Rust to resolve the Run through its Workspace
Binding, open and focus the Factory-managed Herdr Workspace, and return a
bounded Herdr client launch descriptor. The WebView then invokes the narrow
native terminal command. Every terminal action in the title bar and Run
workspace toggles that same surface. The native shell reveals a full-height
split with the WebView at 30 percent and an edge-to-edge `<terminal>` at 70
percent, and the React shell hides its sidebar as ephemeral presentation state.
Reopening another active Run focuses that Run's Herdr Workspace and reuses the
existing Herdr client. The web UI does not embed a second terminal renderer.
The embedded client uses an app-owned Herdr config with a 32-column default
sidebar (six columns wider than Herdr's default) so generated workspace and
agent labels remain readable without mutating the user's global Herdr config.

The native shell does not receive pane IDs, reconstruct layouts, mirror output,
or launch harness processes. Factory-owned workspace PTYs remain a separate
`crates/terminal-runtime` capability and are not used to render Herdr agent
panes in the web UI.

## Consequences

- A Run can be reopened after restart by recomputing its label and worktree.
- Herdr IDs and placements remain locators in live projections, not product
  identity.
- Historical sessions remain readable after Herdr objects disappear.
- Factory layout state continues to describe Factory presentation panes only;
  it is not a copy of Herdr's agent topology.
- Native terminal visibility is shell presentation state, not a persisted Run
  or Herdr lifecycle fact.
