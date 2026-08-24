@AGENTS.md

# Agent Factory — CLAUDE.md

> Institutional knowledge the agent reads every session. Keep under one page; when Claude repeats a mistake twice, add the correction here. Changes are reviewed like code; code owners approve.

## Commands

- Requires: Node 24+, pnpm 11+, Rust 1.93, Herdr running (see `AGENTS.md`)
- Build: `pnpm build` (contracts + Rust release + static Next.js export)
- Runtime only: `cargo build --package agent-factory-runtime`
- Dev web: `pnpm dev:web` → http://127.0.0.1:3000 (static UI only; no Native-SDK/Rust/Herdr)
- Dev host: `pnpm native:dev` (builds Rust + launches Native-SDK host; Herdr must already run)
- Validate: `pnpm validate` (schemas + lint + typecheck + cargo fmt/clippy + native validate)
- Test: `pnpm test` (web + Rust + native-tooling + native); web: `pnpm test:web` (turbo), Rust: `cargo test --workspace`
- Contracts: `pnpm contracts:check` (must pass before commit if `crates/runtime-contract` changed)
- Smoke: `pnpm smoke:web` (browser a11y/keyboard), `pnpm smoke` (adds Native-SDK bridge)
- Clean dev seed: `pnpm native:dev:clean` (resets dev DB before launch)

Healthy outputs: `cargo test` → `test result: ok`, `pnpm validate` → zero warnings, `cargo clippy` → no `-D warnings` failures.

## Conventions

- TypeScript: strict, 2 spaces, 80 cols, no semicolons, double quotes, ES5 trailing commas. `PascalCase` components, `camelCase` symbols. Discriminated unions over boolean combos.
- React: shadcn primitives from `packages/shared/ui` only; semantic Tailwind tokens only (no arbitrary colors/spacing); `useSyncExternalStore` for external stores; avoid `useEffect` except for imperative third-party setup with comment.
- Rust: `cargo fmt` required. No `Debug` of secrets. Raw credentials never in SQLite, logs, IPC, or UI projections — Keychain only.
- Commits: Conventional Commit `type(scope): summary`.
- Contracts: `crates/runtime-contract` is sole authority for runtime messages/projections. Generate via `pnpm contracts:generate`; never hand-edit `packages/shared/runtime-client`.
- Static UI only: no Next.js server, API routes, Server Actions, middleware, ISR. Dev origin `http://127.0.0.1:3000`, prod `zero://app`.

## Architecture

- Next.js static UI (`apps/web-ui`) → Native-SDK host (`apps/native-host`, Zig) → Rust runtime (`services/runtime`, `crates/*`) → Herdr (live Workspaces/agents/panes) + Git (worktrees/commits)
- Six authorities — one writer each (see `docs/spec/ownership.md`): Web UI (ephemeral view state), Native host (window/sidecar/transport), Rust (durable ledger + policy), Herdr (live topology/lifecycle), Git (repo facts), Orchestrator (workflow decisions via `agent-control` token).
- Herdr never started/stopped by us; harnesses discovered from Herdr manifests only (`PATH` probing forbidden). Worktrees created via Herdr; Git is authority on path/branch/HEAD/dirty/diff.
- `services/runtime/src/herdr_sessions.rs` is the only runtime module that touches the Herdr client. `crates/herdr-client` is the only crate speaking the Herdr socket.
- Intent/spec/plan artifacts live in `intent/<slug>/` (see `intent/README.md`); chain of commits is the audit trail.

## Verifying your work

Run all three before reporting any task complete and paste the output. Fix the code, not the tests.

- Build: `pnpm validate` (must finish with zero warnings)
- Test: `pnpm test:web && cargo test --workspace` (all green; never skip/delete a failing test)
- Lint: included in `pnpm validate` (cargo fmt + clippy + eslint + tsc)

For bug fixes: write the failing test first, confirm it fails for the expected reason, commit it, then make it pass without editing the test (hook blocks test edits during fix tasks).

For UI: give Claude a browser/screenshot tool, implement → screenshot → compare to mock → adjust (2–3 rounds).

## Things Claude gets wrong

- Do not bundle/install/start Herdr or probe `PATH` for agents — Herdr is external, harnesses are manifest-discovered.
- Do not copy Herdr/Git live state into a writable cache that authorizes commands — join fresh snapshots; stale views are labelled last-observed only.
- Do not infer Orchestrator decisions from pane lifecycle or terminal text — only explicit authenticated `agent-control` commands advance a Run.
- Do not add execution profiles / target executions / standalone evaluation runs — use Projects, Target Agents, Drafts, Versions, Workspace Bindings, Factory Runs as defined.
- Do not hand-edit `packages/shared/runtime-client` or add a competing state machine in React/native/Rust persistence.
- Do not weaken filesystem trust boundary — canonical project roots only, reject traversal/escaping symlinks/special files.
