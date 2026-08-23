# ADR 0007: Rust owns durable workspace presentation

Status: accepted (revised for the Herdr-native authority model)

## Decision

Persist Agent Factory Work Contexts, visible Workspace Panes, focus, docks, and
workspace-terminal descriptors in Rust. A pane is a presentation reference to
one durable Work Context. At most three panes may be visible, and one Work
Context may appear in at most one pane.

A Work Context explicitly references a Draft, managed Agent Session, Factory
Run, or neither for Target Activity. It is not a loose polymorphic identifier
pair. Workspace terminals reference their Work Context and are owned by
`crates/terminal-runtime`; they are distinct from Herdr-owned agent panes.

The durable layout does not persist Herdr Workspace, tab, pane, process, focus,
or lifecycle state. Live agent topology is joined from a fresh Herdr snapshot.
Version file inspection is an ephemeral read-only surface resolved from the
immutable Git commit and is not a persistent Work Context destination.

## Consequences

- Closing an Agent Factory pane is presentation-only. It never stops a Herdr
  agent, cancels a Run, closes a Herdr Workspace, or closes a workspace PTY.
- Closing a Herdr Workspace interrupts its live processes but does not invent a
  Run verdict, close Agent Factory panes, or remove a worktree.
- Selecting visible work focuses its pane; selecting hidden work opens or
  restores it according to requested placement.
- Focus derives the active Target Agent, binding, and work item. There are no
  independent mutable project, session, or Run selections.
- WebView reloads and application restarts restore only intentional Factory
  presentation state. Live runtime state is freshly reconciled from Herdr.
