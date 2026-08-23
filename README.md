# Agent Factory

Agent Factory is a macOS desktop client for building and evaluating agents with
Herdr, which owns live agent Workspaces, panes, processes, and the terminal TUI.
Factory's Rust runtime owns the durable ledger and user-created workspace PTYs.
A Harness is a Herdr agent kind; Agent Factory installs nothing and starts
nothing itself. Coding and Test/Eval sessions always occupy independent Herdr
panes.

```text
Next.js static UI -> Native-SDK host -> Rust runtime -> Herdr -> agents
```

Ownership is deliberately strict:

- `apps/web-ui` renders typed projections and sends intents.
- `apps/native-host` owns the WebView, window lifecycle, platform commands,
  sidecar byte transport, and macOS packaging.
- Rust under `services/` and `crates/` owns every application state transition,
  Herdr connection, workspace PTY, file, database, secret, plugin, and update,
  and coordinates requests to Herdr and Git.
- Herdr owns agent processes and reports their lifecycle; Agent Factory never
  bundles, installs, or starts an agent, or Herdr itself.

The current scope, subsystem authority map, architecture, and accepted
decisions are in [`docs/spec/`](docs/spec/), with repository
ownership rules in [`AGENTS.md`](AGENTS.md).

## Requirements

- macOS 13 or newer
- Node.js 24 or newer
- pnpm 10.11.0
- Rust 1.93.1 (pinned by `rust-toolchain.toml`)
- Native-SDK through the pinned `@native-sdk/cli` package
- Herdr running, with the agent kinds you intend to use installed for it

## Development

```bash
pnpm install --frozen-lockfile
pnpm dev:web
pnpm native:dev
```

The web development origin is `http://127.0.0.1:3000/`, loaded inside the
Native-SDK WebView by `pnpm native:dev`. Production uses a Next.js static
export served from the sealed application bundle; there is no Next.js server.

## Verification

```bash
pnpm validate
pnpm test
pnpm smoke:web
pnpm build
pnpm native:package
```

`pnpm smoke` adds Native-SDK bridge coverage. macOS release packaging produces
separate arm64 and x86_64 signed/notarized archives with SBOMs and checksums.

## Release trust

Agent Factory fails closed when release trust inputs are absent. Local and
development packages seal a disabled update configuration. Release packages
enable updates only when CI injects a pinned public key and key ID plus signed
manifest endpoints; the Rust runtime then requires explicit version
confirmation before secure extraction and updater-helper installation. Plugin
catalogs likewise require an exact-byte Ed25519 signature and verified artifact
hash before installation.

## Scope boundary

Release 0.1 intentionally excludes Automations, Tasks, standalone MCP and Skills
management, hosted gateway infrastructure, and bundled agents. Application-level
LLM Providers are reusable configuration entities; an Environment may be
authored without a provider and is launch-ready only after its provider and
model policy are configured. Rust exposes that provider to Harness agents
through session-scoped loopback gateways. Plugin MCP servers and Skills remain
Environment-selected plugin payloads, not standalone product areas.
