# ADR 0006: Normalize the target-development ledger

Status: accepted (revised for the Herdr-native authority model)

## Decision

Use distinct Rust-owned durable records for Projects, Target Agents, mutable
Drafts, immutable Versions, Workspace Bindings, managed-session history,
Factory Runs, Work Contexts, Environments, providers, and presentation state.
“Agent” and “Run” remain the user-facing names for Target Agent and Factory Run.

A portable `.agent-factory/target-agent.json` manifest defines Target Agent
identity and its Draft or Version lifecycle. Machine-local roots, runtime
references, cleanup authorization, and presentation state remain in the local
ledger. Projects are metadata and local-container boundaries, not a second
primary navigation model.

A Workspace Binding joins one Target Agent to one Project or worktree and is
the durable execution-context identity. Drafts, managed sessions, and Runs
reference the binding and derive Target Agent and Project identity from it. A
binding has zero or one current Factory-managed Herdr Workspace, located by
stable Factory metadata and revalidated against Herdr.

Rust validates the repository's optional `.agent-factory/config.json` worktree
policy and asks Herdr to create, open, or remove Draft worktrees. Git remains
authoritative for the resulting checkout, branch, HEAD, cleanliness, diff,
commits, and tags. Publishing creates an immutable Version from a Git commit and
tag; Version inspection reads that commit ephemerally and has no persistent
Herdr Workspace destination.

A Factory Run is a durable semantic record. It snapshots the selected Draft's
definition, Environment, and starting Git anchor, then records its
Orchestrator and explicitly authorized managed-session lineage, accepted
handoffs, evidence, final Git anchor, escalation, and final outcome. It does not
persist fixed Coding or Evaluation slots, a copied workspace revision, an
iteration state machine, or live Herdr topology. `escalated` is a resumable
semantic state; terminal `needs_review` is a finished verdict.

Managed-session records preserve binding, Run, role, parentage, Environment,
stable Herdr identity, prompt-delivery receipt, and durable outcome. Current
lifecycle, placement, process identity, attention, transcripts, and
unassociated-agent history are never Project Store authority.

The schema is greenfield. Opening an older schema resets it instead of adding
aliases, dual reads, migrations, or inferred relationships.

## Consequences

- One Target Agent may have multiple concurrent Draft worktrees and Workspace
  Bindings.
- Repository policy affects only new Draft worktrees; existing worktrees are
  never silently relocated.
- Only one mutable Run may be live per Draft. Parallel mutable Runs require
  distinct Draft worktrees, bindings, and Herdr Workspaces.
- Sequential Runs reuse the binding's Workspace but create fresh managed
  sessions.
- Run completion and managed-session exit do not remove a Workspace or
  worktree. Only Draft publication or discard may authorize cleanup.
- Versions are immutable Git and product snapshots, never Run mutation targets.
- Foreign keys and write-time binding validation reject invalid cross-context
  state before it can be persisted.
- Obsolete session-event, copied-runtime, iteration, baseline, and
  Factory-owned worktree lifecycle tables are absent from the schema.
