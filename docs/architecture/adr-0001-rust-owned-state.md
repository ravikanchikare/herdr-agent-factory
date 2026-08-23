# ADR 0001: Rust owns Agent Factory product state

Status: accepted (revised for the Herdr-native authority model)

## Decision

Rust owns Agent Factory's durable product ledger, authorization, policy, and
side-effect coordination. Next.js is a declarative projection client.
Native-SDK is a platform shell and bounded transport boundary.

This ownership does not make Rust authoritative for external facts. Herdr owns
live runtime topology, processes, terminals, and agent lifecycle. Git owns
repositories, worktrees, branches, commits, tags, status, and diffs. Rust joins
fresh observations from those authorities with the durable ledger and never
persists them as competing writable state.

## Consequences

- The Herdr connection and invalidation subscription survive WebView reloads.
- Durable Runs, managed-session lineage, Projects, terminals, plugins, and
  settings behavior remain testable without a browser.
- A full projection snapshot recovers WebView reloads, revision gaps, invalid
  payloads, Rust restarts, and Herdr reconnects.
- Bridge messages are versioned and generated from Rust definitions.
- Native and UI code cannot introduce provider-specific or agent-runtime
  lifecycle behavior.
- The earlier prototype's TypeScript protocol engine and split business
  ownership are not migrated.
