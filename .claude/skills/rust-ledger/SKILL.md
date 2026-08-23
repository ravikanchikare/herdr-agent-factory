---
name: rust-ledger
description: Enforce Rust-owned durable ledger and IPC/contract boundaries. Use whenever modifying Rust crates, SQLite persistence, runtime projections, IPC handling, provider/environment/secret logic, or generated contracts/bindings.
---

# Rust Ledger & Contracts

Rust owns Agent Factory's product ledger and control policy. The UI and native host are presentation and transport.

## Durable vs observed

Persist only facts that must outlive Herdr and mutable Git:

- Target Agent, Draft, Version, Workspace Binding, Environment, provider, trust, permission records
- Run definition, semantic state, handoffs, managed-session lineage, escalation, final outcome
- Command-delivery receipts (prevent duplicate prompts/launches)
- Starting/final Git anchors, evaluation evidence, immutable artifacts
- Worktree provenance + cleanup authorization
- Factory Work Context / pane / dock / terminal descriptor / settings

Do not persist current Herdr lifecycle, runtime topology, process IDs, focus, current Git status/diff, transcripts, unassociated agent history, or copied event streams as authoritative state. A last-observed cache must include freshness and never authorize a command.

## Contracts

- `crates/runtime-contract` is the sole authority for runtime messages and projections. Generate JSON Schema + TypeScript bindings into `packages/shared/runtime-client` via `pnpm contracts:generate`; never hand-edit generated bindings or create parallel transport types.
- `crates/ipc-contract` owns framing. Runtime IPC is one length-prefixed stream, 1 MiB max frame; Rust stdout is framed protocol bytes only, logs to stderr. Treat all bridge responses as untrusted until validated.
- Filesystem operations require an explicitly trusted project and remain confined to canonical project roots; reject traversal, escaping symlinks, special files, filesystem targets outside authorized roots.

## Providers / secrets / environments

- Providers are reusable app-level SQLite records; environment descriptors reference provider IDs + opaque credential references, never inline config or raw secrets. Raw credentials live in Keychain (write-only), never in SQLite/descriptors/logs/Debug/IPC/UI/Herdr env.
- Execution-affecting provider changes mark linked Environments `needs_setup`; cosmetic rename does not. Unready Environments cannot start new sessions.
- Per-session loopback gateway injects endpoint + sentinel credential + selected model and neutralizes inherited upstream provider vars. Native code stays provider-neutral.

## Checks

- `pnpm contracts:check && pnpm validate` must pass when touching contracts.
- `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings` for Rust changes.
