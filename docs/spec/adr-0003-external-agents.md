# ADR 0003: Herdr owns the agent runtime

Status: accepted (revised; supersedes the external-adapter decision)

## Decision

Agent Factory is a Herdr-native control plane. It does not launch agent
processes or implement an interchangeable agent-runtime abstraction. Herdr
owns Workspaces, tabs, panes, native sessions, agents, processes, terminals,
topology, and reported lifecycle.

Agent Factory connects to Herdr's control socket as a client. Starting a Run
asks Herdr for a fresh Orchestrator session in the Workspace associated with the
selected Workspace Binding. When the Orchestrator explicitly requests a Coding
or Evaluation session, Rust validates the authenticated command, resolves the
Run's Environment, records durable lineage, and asks Herdr to create it. A Run
records only the sessions it authorized; it does not own the Workspace or every
agent found there.

A Harness is a supported Herdr agent kind discovered from Herdr manifests.
Manifest warnings supply readiness. Agent Factory never probes `PATH`, bundles,
downloads, installs, or updates agents, and never starts or controls the Herdr
server.

## Consequences

- Readiness and lifecycle are Herdr's answers. Agent Factory's supported-kind
  seed only limits which manifest kinds the product presents.
- Agent authentication and approvals stay inside the agent's own interface.
  Agent Factory surfaces Herdr's `blocked` state and sends agent-native input;
  it has no parallel approval protocol.
- The live projection contains the complete agent tree for a binding's
  Workspace, including unassociated agents under other runtime activity.
- Herdr events invalidate cached observations. Rust refreshes a full snapshot
  instead of persisting or replaying an event-derived lifecycle state machine.
- Herdr outlives Agent Factory. Managed sessions reconcile through stable Herdr
  identity and Factory association; unmatched durable records are historical,
  and unmatched live agents are not adopted.
- The socket protocol is tested against a stand-in Herdr server on a real Unix
  socket, so the client is exercised without a developer Herdr Workspace.
