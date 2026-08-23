# ADR 0009: Use Target Agent and Draft-first navigation

Status: accepted (revised for the Herdr-native authority model)

## Decision

Target Agents are the primary sidebar identity. Each Agent is one disclosure-
only folder control with no navigation or selected state. Its Draft and Version
rows remain selectable, flat, and aligned to the Agent name axis. The existing
compact Sidebar and multi-pane Workspace remain the product's design direction.

The Draft workspace owns editable definition, trust, Git metadata, Run history,
publication, and safe discard. Its live runtime section derives the complete
agent tree from the binding's Herdr Workspace. Factory-managed agents are
grouped by Run, role, and parent session. Every remaining Herdr agent appears as
other runtime activity rather than being hidden or adopted.

Runs and managed sessions may open as Work Contexts, but they do not become new
top-level sidebar identities. Selecting already visible work focuses its pane;
selecting hidden work opens or restores it. Closing a pane changes presentation
only.

Immutable Version rows open a transient read-only files inspector resolved from
the Version's Git commit. The inspector is neither stored in the durable
workspace-pane layout nor represented by a persistent Version Work Context.

Target Activity remains an intentional empty state until a concrete activity
domain capability exists. The UI does not synthesize a feed from unrelated Run,
session, Herdr, or Git records.

## Consequences

- Project metadata and Workspace Binding context stay beneath the Target Agent
  rather than adding another navigation hierarchy.
- Draft and Run views show `Live`, `Reconnecting`, `Last observed`, or
  `Historical` authority state and disable commands whose live preconditions
  are not fresh.
- Historical managed sessions retain role, lineage, Environment, and outcome;
  they do not masquerade as live `done` agents.
- Editing changes only the Draft. Creating a Version is explicit and blocked
  during a live Run or when the Draft has no substantive change from its base.
- Settings remains a separate stable destination and is not reshaped by this
  navigation decision.
