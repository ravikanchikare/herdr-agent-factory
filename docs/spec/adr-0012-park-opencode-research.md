# ADR 0012: Park the OpenCode runtime research

Status: accepted

## Decision

The current architecture is Herdr-native with a Rust durable ledger.
`docs/spec/research/opencode-api-port.md` is a comparison draft only.
It is not a migration plan and must not be treated as architecture.

Do not implement an OpenCode or pure-TypeScript runtime, an Electron
shell, or a second product ledger in order to "port" Agent Factory.
A later intent would be required to reopen that question.

## Consequences

- `AGENTS.md`, `docs/spec/ownership.md`, and
  `intent/2026-08-24-herdr-native-control-plane/` are the product
  record.
- The research file stays under `docs/spec/research/` with a warning
  banner.
- Reviewers reject diffs that introduce a parallel agent runtime or
  replace Herdr/Rust because of that draft.
