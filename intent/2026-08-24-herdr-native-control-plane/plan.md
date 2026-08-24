# Plan: herdr-native-control-plane (from intent.md 2026-08-24)

Author: Agent Factory engineering · Status: accepted · Spec: `spec.md`

This plan is reconstructed from the tree that shipped. Later diffs
that match this product should still name files, order, risks, and
proof the same way.

## Files that change

Composition and contracts:

- `Cargo.toml`, `pnpm-workspace.yaml`, `package.json`, `turbo.json`
- `crates/runtime-contract/src/lib.rs` — request methods, DTOs,
  generated JSON Schema
- `crates/ipc-contract/` — framed stdio
- `packages/shared/runtime-client/` — generated bindings only
- `services/runtime/src/` — composition root, IPC service,
  `herdr_sessions.rs`, `control_socket.rs`, `repository_config.rs`

Durable ledger and Run control:

- `crates/app-core/src/lib.rs` — projections, Factory Run types
- `crates/project-store/src/lib.rs` — SQLite schema
- `crates/git-runtime/` — observe and publish; no worktree lifecycle
- `crates/agent-control/src/lib.rs` — Orchestrator verbs
- `services/agent-cli/src/main.rs` — `agent-factory` CLI

Herdr, Environments, platform:

- `crates/herdr-client/` — socket protocol
- `crates/environment-runtime/`, `crates/llm-provider-runtime/`,
  `crates/llm-gateway/`, `crates/platform-secrets/`
- `crates/filesystem-runtime/`, `crates/plugin-runtime/`,
  `crates/terminal-runtime/`, `crates/update-runtime/`
- `services/updater-helper/`

UI and host:

- `apps/web-ui/` — static Next.js shell, agents, settings, workspace
- `packages/shared/ui/`, `packages/shared/theme/`
- `apps/native-host/` — Zig host, bridge, sidecar, Herdr TUI, packaging

Policy and proof:

- `AGENTS.md`, `CLAUDE.md`, `docs/spec/`
- `.claude/skills/{agent-factory-architecture,herdr-authority,rust-ledger}/`
- `.claude/hooks/`, `evals/`
- Tests colocated with the behavior they cover

## Order of work

Matches the history that produced this tree:

1. Workspace, tooling, and ownership docs (`1c4f4b3`).
2. Ledger, contracts, Git observations, agent-control
   (`1731251`).
3. Herdr client and session orchestration (`e75dd79`).
   `herdr_sessions.rs` is the only runtime Herdr module.
4. Environments, providers, secrets, loopback gateway (`e0cc382`).
5. Filesystem trust, workspace PTYs, plugins, updates (`0c3ca73`).
6. Static Next.js shell and shared design system (`66c1d4d`).
7. Zig Native-SDK host, bridge, updater (`b903c55`).
8. Draft/Run workspace, version inspector, Herdr TUI chrome, and
   UI corrections on that shell (`4f14b59`, `23cd41e`).
9. Record the product as `intent.md` / `spec.md` / `plan.md` so later
   work has an artifact chain to check against.

Do not invert this order: generated contracts before UI wiring, Herdr
client before session orchestration, Environment resolution before
Run create, native terminal after the WebView can request a launch
descriptor.

## Risks

- **Herdr as a second product.** Blast radius: a parallel agent
  runtime in Rust or React. Guard: `herdr-authority` skill, hooks,
  and the single `herdr_sessions.rs` boundary.
- **Stale snapshots authorizing launches.** Duplicate Orchestrators
  or worktrees. Guard: intent identity + fresh preconditions; tests
  against a stand-in socket via `AGENT_FACTORY_HERDR_SOCKET`.
- **Secrets in the wrong store.** Guard: Keychain-only values;
  `block-secrets.sh`; no `Debug` of credentials.
- **Hand-edited bindings.** Guard: `block-generated.sh` and
  `pnpm contracts:check`.
- **Next.js server features.** Guard: `output: "export"`, origin
  policy, and review compliance pass.
- **Pane close stopping agents.** Guard: presentation-only pane
  intents; Herdr owns process lifetime.

Protected paths: `packages/shared/runtime-client/**`,
`crates/runtime-contract/generated`, raw secret material, Herdr
server control.

## Proof

Before calling the work done:

- `pnpm contracts:check` if `runtime-contract` changed
- `pnpm validate` — zero warnings
- `pnpm test:web` and `cargo test --workspace`
- `pnpm smoke:web` for keyboard, a11y, and static UI
- `pnpm smoke` when native bridge or packaging changed
- Rust tests never attach to a developer Herdr Workspace
- UI: screenshot of Draft overview + Herdr TUI against
  `assets/application.png`

The merged tree should still satisfy every acceptance criterion in
`spec.md`. When implementation departs from this plan, update this
file in the same commit.

## Alternatives considered

- **Interchangeable agent-runtime adapters.** Rejected. Herdr is
  permanent; an adapter layer would invent a second lifecycle.
- **SQLite as source of truth for live Herdr/Git.** Rejected. Join
  fresh snapshots; last-observed cannot authorize commands.
- **Next.js server, API routes, or Server Actions.** Rejected.
  Static export only; every operation crosses the typed bridge.
- **Web terminal (`xterm.js`) for Herdr panes.** Rejected. One
  Native-SDK Ghostty-VT surface hosts the Herdr client TUI.
  Factory PTYs stay in `crates/terminal-runtime`.
- **PATH probing or bundled harnesses.** Rejected. Manifest
  discovery only; explain missing prerequisites.
- **Inferring Run progress from pane text or `done`.** Rejected.
  Only authenticated Orchestrator commands advance a Run.
- **OpenCode + TypeScript ledger.** Parked as research
  (`docs/spec/research/opencode-api-port.md`, ADR 0012). Not the
  current architecture.
- **Target Activity feed from Runs/sessions.** Rejected until a
  real activity domain exists.
- **A second version-control or changelog tool besides Git.**
  Rejected. Commits use ordinary Git and Conventional Commit
  subjects.
