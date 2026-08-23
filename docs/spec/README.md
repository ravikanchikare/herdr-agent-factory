# Spec index

> Canonical authority is `AGENTS.md`. This directory elaborates the Herdr-native architecture; `AGENTS.md` takes precedence on any conflict.

## Core specs

- `ownership.md` — Six-authority model (Web UI, Native host, Rust, Herdr, Git, Orchestrator), projection boundary, provider/environment ownership.
- `herdr.md` — Herdr as permanent runtime: Workspace Bindings, live projection, lifecycle, events, worktrees/Git, transcript reads.
- `ipc.md` — Runtime IPC v1 framing (`MAX_FRAME_BYTES=1 MiB`, `PROTOCOL_VERSION:u16=1`), envelopes (`Uuid` ids), `Ready`/`Hello`/`Shutdown` variants, WebView bridge.

## ADRs

- `adr-0001-rust-owned-state.md` — Rust owns Agent Factory product state (accepted, Herdr-native revision).
- `adr-0002-static-next-webview.md` — Next.js is a static WebView frontend (accepted).
- `adr-0003-external-agents.md` — Herdr owns the agent runtime (accepted, supersedes external-adapter).
- `adr-0004-agent-plugins.md` — Agent Plugins are Environment-scoped interoperability packages (accepted).
- `adr-0005-environment-llm-gateways.md` — Environment-local LLM gateways (superseded by ADR 0010).
- `adr-0006-normalized-target-domain.md` — Normalize the target-development ledger (accepted).
- `adr-0007-rust-owned-workspace.md` — Rust owns durable workspace presentation (accepted).
- `adr-0008-reserved.md` — Reserved — never used; number skipped (0007 → 0009) to avoid tooling confusion.
- `adr-0009-target-centric-work-feed.md` — Target Agent and Draft-first navigation (accepted).
- `adr-0010-independent-llm-providers-and-environment-boundary.md` — Independent LLM Providers and Environment boundary (accepted).
- `adr-0011-herdr-workspace-identity-and-terminal-surface.md` — Herdr workspace identity and terminal surface (accepted).

## Research (quarantined)

- `research/opencode-api-port.md` — ⚠️ Research draft — speculative. NOT the current Herdr-native architecture. Current direction is `AGENTS.md` and `docs/spec/ownership.md`; see ADR 0012 for parking decision. Maps Agent Factory onto a pure TypeScript + OpenCode V2 runtime for comparison only.

> The research draft is quarantined under `research/` and must not be treated as architecture.
