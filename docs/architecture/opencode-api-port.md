# OpenCode port mapping: Agent Factory on a pure TypeScript runtime

Status: research draft

Scope: replace the Herdr runtime (agent execution, workspaces, terminals,
lifecycle) and the Rust product ledger/service sidecar (domain policy,
SQLite, gateway, IPC, update) with a single pure-TypeScript application
driven by OpenCode V2 as the agent runtime. OpenCode is the fixed authority;
nothing in this mapping assumes Herdr or Rust survives.

Source of truth for OpenCode API facts in this document: the V2 API reference
at https://opencode.ai/v2/docs/api, the raw OpenAPI document served from
https://opencode.ai/v2/openapi.json, and the client guide at
https://opencode.ai/v2/docs/build/client. The OpenCode V2 API and client are
beta; method names and shapes may change before stable.

## 1. Authority model after the port

| Authority (today) | Owner after port |
|---|---|
| Herdr: live Workspaces, tabs, panes, agents, sessions, processes, terminals, lifecycle | OpenCode server (background service): sessions, PTY sessions, shell commands, permissions, forms, worktrees, VCS, config. OpenCode has no tab/pane topology and no TUI embed; the app renders its own presentation. |
| Rust: durable product ledger, control policy, Environments, providers, secrets, gateways, IPC, updates, recovery | TypeScript application ledger (SQLite via better-sqlite3) plus OpenCode policy surfaces (permissions, forms, instructions). Policy enforcement moves into the TS core; enforcement points move to OpenCode's request/reply protocol. |
| Git: worktree/branch/HEAD/status/diff/commits/tags | OpenCode worktree + VCS endpoints (create/remove worktrees, branch info, file status, diff) — with a TS gap-fill for commit/tag reads (see §8.6). |
| The Orchestrator: workflow decisions inside a Herdr pane | An OpenCode agent (the Factory Orchestrator agent) driving sessions through the OpenCode HTTP API; the TS core validates every prompt against Run state. |
| web-ui: projections + typed intents | The same static web UI, now talking to the TS core (in-process or via the same HTTP API it exposes), or directly to OpenCode through `@opencode-ai/client`. |
| native-host: window, transport, packaging | A TypeScript desktop shell (Electron/Tauri is excluded by the no-Rust constraint, so Electron) owning window, terminal renderer, and packaging; no domain logic. |

The single-writer rule survives: the TS SQLite ledger is the only writer of
durable Factory facts; OpenCode is the only writer of sessions, processes,
PTYs, worktrees, permissions, and Git mutations; the UI never mutates either.

## 2. Concept map

| Agent Factory requirement | Today (Herdr/Rust) | OpenCode V2 API | TS port owns |
|---|---|---|---|
| Agent runtime authority | Herdr control socket | OpenCode background service via `@opencode-ai/client`; `Service.ensure()`/`discover()`/`stop()` + `Service.headers()` (Node entrypoint) | service lifecycle policy, availability labels |
| Health/liveness | Herdr availability | `GET /api/health` (`healthy, version, pid`), `POST /api/service/stop`, `GET /api/server` (connect URLs) | "last observed" labelling, disabling live commands |
| Workspace Binding + Factory Workspace | Herdr Workspace per binding, label `<Target> / <Binding> (<suffix>)` | OpenCode `Location.Ref {directory, workspaceID}` + `GET /api/location` resolve; `GET /api/project`, `GET /api/project/current` | binding records, label convention, location↔binding mapping |
| Orchestrator session (Run start) | Herdr creates Orchestrator pane in Workspace | `POST /api/session` with `{agent, model, location}` (agent = Factory Orchestrator, model = resolved from Environment) | Environment resolution, Run record, delivery receipts |
| Managed sub-agent sessions (start coding/evaluation) | Herdr pane per agent, lineage in SQLite | `POST /api/session` (new session at the binding location); `Session.Info.parentID` + `GET /api/session?parentID=` gives the lineage tree; `POST /api/session/{id}/fork` for copy-on-boundary children | managed-session lineage records, Run association, role |
| Lifecycle values | Herdr `idle/working/blocked/done/unknown` | `SessionStatus = idle \| busy \| retry` via `session.status` events; step/execution events (`session.execution.started/succeeded/failed/interrupted`, `session.step.*`, `session.retry.scheduled`); `session.idle` event | projecting OpenCode status without reinterpretation |
| Prompt / resume | Herdr prompt/resume on settled agent | `POST /api/session/{id}/prompt` with client `id` (`^msg_`), `delivery: "steer"\|"queue"`, `resume: false` to admit without executing | settledness gate, prompt receipts |
| Queue/steer work | Herdr input delivery | `GET /api/session/{id}/inbox`, `POST .../inbox/{inboxID}/steer`, `.../queue`, `DELETE .../inbox/{inboxID}`; `session.inbox.delivered/enqueued/cancelled` events | durable delivery receipts; retry-until-ready semantics |
| Interrupt working session | Herdr interruption | `POST /api/session/{id}/interrupt` | only from fresh snapshot |
| "Blocked" (approval/UI surface) | opaque inside Herdr agent UI | `POST /api/session/{id}/permission` (evaluates → `effect: allow\|deny\|ask`), `GET /api/permission/request`, `POST /api/session/{id}/permission/{req}/reply`, `permission.asked/replied` events | rendering approvals, Environment permission policy (auto-allow/deny per rules) |
| Question surfaces / forms | blocked UI in agent | `POST /api/session/{id}/form` (schema-driven), `GET .../form`, `GET .../form/{f}/state`, `POST .../form/{f}/reply`, `.../cancel`, `GET /api/form/request`, `form.created/replied/cancelled` events | form rendering, keyboard/accessibility |
| Read recent output | Herdr on-demand transcript | `GET /api/session/{id}/message` (messages), `GET /api/session/{id}/context` (post-compaction context), `GET /api/session/{id}/log` (experimental SSE), `GET /api/session/{id}/export` | on-demand reads only; no transcript persistence |
| Terminal surface for Run | Native `<terminal>` running Herdr TUI | `POST /api/pty` `{command,args,cwd,title,env}`, `GET /api/pty/{id}/connect` (WebSocket), `POST /api/pty/{id}/connect-token` `{ticket, expires_in}`; `pty.created/updated/exited/deleted` events | terminal renderer (ghostty-web WASM, the same emulator OpenCode's own app uses — §9), PTY lifecycle UI, reconnect/replay |
| Factory-owned workspace terminals | `crates/terminal-runtime` PTYs | same PTY API (one surface for both concepts) | terminal descriptor persistence, bounded rendering |
| Non-interactive commands | git-runtime subprocesses, updater | `POST /api/shell` `{command,cwd,...}`, `GET /api/shell/{id}/output`, `PATCH /api/shell/{id}/timeout`, `DELETE /api/shell/{id}`, `shell.created/exited/deleted` events | command authorization, output pages |
| Git observations | `crates/git-runtime` subprocess | `GET /api/vcs` (`branch.current/default`), `GET /api/vcs/status` (`Vcs.FileStatus[]`), `GET /api/vcs/diff?mode=working\|branch&context=`; `vcs.branch.updated` event | freshness labels; commit/tag/HEAD-anchor reads fall back to a TS git lib (§8.6) |
| Draft worktrees | Herdr worktree API | `GET/POST/DELETE /api/worktree/{projectID}` (create: `{strategy, from, directory, name}`), `POST .../refresh`; `worktree.updated/resolved` events; `WorktreeError.forceRequired` surfaced | `.agent-factory/config.json` policy validation (relative `worktreesDirectory`), cleanup authorization, provenance |
| Environments | Rust descriptors + resolution | no OpenCode equivalent; maps onto session create inputs (`agent`, `model`) + app-side permission rules + PTY `env` | strict descriptor schema, readiness computation, launch resolution |
| LLM providers | `crates/llm-provider-runtime` | `GET /api/provider`, `GET /api/provider/{id}` (`disabled`, `settings`, `headers`, `body`; 503 when unavailable), `GET /api/model`, `GET /api/model/default` | app-level provider records (name/type/endpoint refs), readiness derivation, model allowlist validation |
| Provider credentials | Keychain + `SecretRef` | `POST /api/integration/{id}/connect/key`, OAuth connect/status/cancel/complete, command connect/status/cancel; `PATCH/DELETE /api/credential/{credentialID}`; `integration.updated` events | opaque references only; never persist raw credentials |
| LLM gateway + sentinel | `crates/llm-gateway` loopback proxy | no equivalent; OpenCode owns provider traffic and credentials end-to-end | provider config applied via `connect/key`; environment-scoped model enforcement via per-session `model` + switch |
| Environments "needing setup" | Rust readiness state | `GET /api/provider` + model catalog freshness (`models-dev.refreshed`, `catalog.updated`, `integration.connection.updated` events) | recompute readiness; block new sessions when unready |
| Skills / Plugins | `crates/plugin-runtime` | `GET /api/skill`, `POST /api/session/{id}/skill`, prompt `skills[]` attachments; `GET /api/plugin`; `GET /api/command` | plugin registry/trust metadata, skill plan validation |
| MCP servers | plugin MCP config | `PUT/DELETE /api/mcp/{server}`, `POST .../connect`, `POST .../disconnect`, `GET /api/mcp/resource`; `mcp.status.changed`, `mcp.resources.changed` events | local trust decisions, transport plan (stdio/remote), validation |
| Filesystem access | `crates/filesystem-runtime` (read-only, confined) | `GET /api/fs/read/*`, `GET /api/fs/list`, `GET /api/fs/find` (location-scoped) | canonical-root confinement checks stay in TS; writes remain agent-side only |
| Permissions | Rust control policy | full Permission API (request create/list/get/reply, saved list/remove); `Permission.Rule {action, resource, effect}` | Environment permission descriptors → auto-reply policy; saved-permission management |
| Run control token / agent-cli | `crates/agent-control` + `services/agent-cli` | OpenCode has no CLI-in-a-pane; the Orchestrator is a normal agent. "Commands" become: prompts (`prompt`), slash commands (`POST /api/session/{id}/command`), instructions entries (`PUT /api/session/{id}/instructions/entries/{key}`), or permission requests the TS core evaluates | TS core validates each control call against Run state; per-Run control semantics enforced in the core, not a socket |
| Durable ledger | `crates/project-store` SQLite (17 tables) | no OpenCode equivalent (sessions are its store) | TS SQLite ledger: Projects, Target Agents, Drafts, Versions, Bindings, Runs, session lineage, tokens, Work Contexts, panes, terminals, settings |
| Projection + revision | Rust revisioned snapshot over IPC | no revisioned snapshot in OpenCode; app composes its own projection from ledger + fresh API reads | snapshot store in TS with revision counter; full re-read on reconnect/gap |
| Events as invalidations | Herdr event stream | `GET /api/event` SSE; **volatile by contract** (slow consumer overflows; missed events on disconnect) — matches the invalidation model | resubscribe + full snapshot on reconnect; never treat events as a transition log |
| Notifications | Rust `notification.requested` | `tui.toast.show` exists for TUI consumers; app can derive transitions from fresh reads | generate notifications only from reconciled transitions + user prefs |
| Updates | `crates/update-runtime` + updater-helper | `installation.updated`, `installation.update-available` events; `GET /api/server` version | Electron autoUpdater; signed manifests stay in TS |
| IPC contract (1 MiB frames) | `crates/ipc-contract` | no longer needed: HTTP + SSE between app and OpenCode; typed client generated from the OpenAPI spec | generate TS client from OpenCode's OpenAPI (or use `@opencode-ai/client`) |
| Web UI state | `useSyncExternalStore` on IPC projection | same pattern, fed by TS core snapshot store | unchanged React principles |
| Static Next.js UI | unchanged | unchanged | unchanged |
| Native window/transport | Zig Native-SDK | — | Electron main process; keep `window.zero`-style bridge only if a sandboxed renderer remains |

## 3. Run and orchestration flow on OpenCode

1. **Start Run**: TS core resolves the Environment (readiness, model policy,
   permissions, skills plan), validates the binding, then
   `POST /api/session {agent: "orchestrator", model: Model.Ref, location:
   {directory: worktree}}`. It records the Run and the session as the
   Orchestrator session.
2. **Orchestrator drives**: the Orchestrator agent receives the Run charter
   (via instructions entries or the initial prompt). When it needs a managed
   sub-agent it asks through the OpenCode permission/instruction surface; the
   TS core authenticates the request against the Run (session ID matches the
   Orchestrator, Run is live), resolves the Environment again, and calls
   `POST /api/session` for the Coding/Evaluation agent with `agent` selected
   and `model` narrowed.
3. **State changes only via commands**: TS core accepts only explicit control
   calls (prompt/admit, interrupt, permission reply, form reply, finish).
   `session.status`, `session.idle`, text deltas, tool events, and PTY exit
   never advance the Run ledger — identical rule to today, now enforced in TS.
4. **Finish/cancel**: records outcome, revokes control (no more prompt/command
   accepted for that Run), leaves sessions and worktree intact.

## 4. Live tree and "other runtime activity"

`GET /api/session?workspace={bindingWorkspace}` (or `directory=`) returns the
binding's sessions; `parentID=null` filters roots; `parentID=ses...` builds
the tree. `GET /api/session/active` returns only running sessions. Sessions
not referenced by a managed-session row render as other runtime activity and
are never adopted. A managed row whose session no longer appears in a fresh
list is historical, not live.

## 5. Event mapping

| Factory requirement | OpenCode event(s) |
|---|---|
| Agent/session invalidation | `session.created`, `session.deleted`, `session.forked`, `session.renamed`, `session.moved`, `session.agent.selected`, `session.model.selected`, `session.usage.updated`, `agent.updated` |
| Lifecycle invalidation | `session.status` (`idle\|busy\|retry`), `session.idle`, `session.execution.started/succeeded/failed/interrupted`, `session.step.started/ended/failed`, `session.retry.scheduled`, `session.compaction.*`, `session.revert.*` |
| Output invalidation | `session.text.started/delta/ended`, `session.reasoning.*`, `session.tool.input.*`, `session.tool.called/progress/success/failed` (re-read messages on demand) |
| Delivery receipts | `session.inbox.delivered/enqueued/cancelled/delivery.changed` |
| Approval/blocked surfaces | `permission.asked`, `permission.replied`, `form.created`, `form.replied`, `form.cancelled` |
| Git/worktree invalidation | `vcs.branch.updated`, `worktree.updated`, `worktree.resolved`, `filesystem.changed`, `reference.updated` |
| Config/catalog invalidation | `config.updated`, `command.updated`, `skill.updated`, `plugin.added/updated`, `models-dev.refreshed`, `catalog.updated`, `integration.updated`, `integration.connection.updated`, `websearch.updated` |
| Terminal/shell invalidation | `pty.created/updated/exited/deleted`, `shell.created/exited/deleted`, `session.shell.started/ended` |
| MCP invalidation | `mcp.status.changed`, `mcp.resources.changed` |
| Update invalidation | `installation.updated`, `installation.update-available` |
| Connectivity | `V2Event.server.connected` |

## 6. Full endpoint → requirement index

| OpenCode endpoint | Ported requirement |
|---|---|
| `GET /api/health`, `POST /api/service/stop`, `GET /api/server` | availability, reconnect policy, server URLs, graceful stop |
| `GET /api/location`, `GET /api/project`, `GET /api/project/current` | location resolution, project identity for bindings/worktrees |
| `GET /api/agent`, `GET /api/agent/{id}` | Harness selection → agent catalog (mode `subagent\|primary\|all`), agent readiness |
| `POST /api/session`, `GET /api/session`, `GET /api/session/{id}`, `DELETE`, `GET /api/session/active` | Run start, managed sessions, live tree, historical reconciliation |
| `POST /api/session/{id}/prompt` | prompt/resume with receipts (`delivery: steer\|queue`, `resume`) |
| `POST /api/session/{id}/command` | orchestrator slash commands (Factory command definitions) |
| `POST /api/session/{id}/interrupt`, `POST .../wait`, `POST .../compact` | interruption, settledness gating, long-run compaction |
| `GET .../message`, `GET .../context`, `GET .../export`, `GET .../log` | on-demand transcript/output reads, evidence export |
| `GET .../inbox`, `.../inbox/{id}` (cancel/steer/queue) | durable delivery receipts, queued prompts |
| `GET/PUT/DELETE .../instructions/entries/{key}` | per-session charter/context injection for Orchestrator and sub-agents |
| `POST .../fork` | draft-forks of session history (if product needs them) |
| `POST .../synthetic`, `POST .../generate` | evaluation steering / one-shot generation |
| `POST .../switchagent`, `POST .../switchmodel`, `POST .../rename`, `POST .../move` | session maintenance within a binding |
| `GET .../permission` + create/get/reply; `GET /api/permission/request`, `GET/DELETE /api/permission/saved` | blocked/approval surfaces, Environment permission policy, saved rules |
| Form endpoints (`/api/session/{id}/form...`, `GET /api/form/request`) | structured question surfaces |
| `GET /api/provider`, `GET /api/provider/{id}`, `GET /api/model`, `GET /api/model/default` | provider catalog, model narrowing, readiness |
| Integration endpoints | credential acquisition (key/OAuth/command flows) |
| `PATCH/DELETE /api/credential/{id}` | credential lifecycle (references only in ledger) |
| `PUT/DELETE /api/mcp/{server}`, connect/disconnect, `GET /api/mcp/resource` | Environment MCP tools plan |
| `GET /api/skill`, `POST /api/session/{id}/skill` | Skills plan activation |
| `GET /api/plugin`, `GET /api/command`, `GET /api/skill` | plugin/command/skill catalogs for settings UI |
| `GET /api/fs/read/*`, `/list`, `/find` | read-only file inspection within trusted roots |
| `GET /api/vcs`, `/vcs/status`, `/vcs/diff` | branch, dirty state, diffs for Draft/Version overviews |
| `GET/POST/DELETE /api/worktree/{projectID}`, `POST .../refresh` | Draft worktree create/open/remove, cleanup authorization |
| `POST /api/pty`, `GET /api/pty/{id}/connect`, `POST .../connect-token`, `PUT/DELETE /api/pty/{id}` | Run terminal surface + Factory workspace terminals (ticket-authenticated WebSocket, §9) |
| `POST /api/shell`, `GET /api/shell/{id}/output`, `PATCH .../timeout`, `DELETE /api/shell/{id}` | non-interactive commands (evidence, tooling) |
| `GET /api/reference` | project references |
| `GET /api/websearch/provider`, `POST /api/websearch` | optional web-search capability |
| `GET /api/config` | effective OpenCode config documents for the location |
| `GET /api/event` | invalidation stream (volatile by contract → always re-read) |
| `GET /api/debug/location`, `DELETE /api/debug/location` | loaded-location inspection/eviction (diagnostics) |
| `GET /api/experimental/migration/v1` | V1 migration status (not needed for a fresh port) |

## 7. What the TS core still must own (no OpenCode equivalent)

- The product ledger (Runs, Drafts, Versions, Bindings, sessions lineage,
  Work Contexts, panes, app settings) — port of `project-store` schema.
- Environment descriptor schema, resolution, and readiness computation.
- Model allowlist and environment-bounded model policy enforcement
  (pre-session-create validation + `switchmodel` guard).
- Binding/location correlation and label conventions.
- Delivery receipts and retry-until-settled semantics.
- Run control policy (which sessions may trigger which actions).
- Worktree policy (`config.json` `worktreesDirectory`), cleanup authorization.
- Secret reference handling (raw values only ever passed to
  `connect/key`-style OpenCode surfaces).
- Snapshot projection + revision counter; freshness labels.
- Plugin trust and transport-plan validation (OpenCode executes; the app
  approves and records).
- Update orchestration (signed manifests, staging, install) via Electron.

## 8. Gaps and risks

1. **No per-session environment variables**: session create accepts only
   agent/model/location. Env-var boundaries must be encoded in the agent
   config, PTY `env`, or dropped for API sessions — document per Environment.
2. **No revisioned snapshot**: OpenCode exposes no monotonic projection;
   the TS core's snapshot revision is its own bookkeeping over fresh reads.
3. **No pane/tab topology**: the Run view becomes a session tree + terminal +
   approvals, not a mirrored terminal workspace. The Herdr TUI embed (ADR
   0011) disappears; the terminal is an app-attached PTY.
4. **No session-creation parent field in the current OpenAPI body**: create
   takes `id/title/agent/model/location`; parentage is expressed through
   `fork` or through the list filter (`parentID`) — verify against the live
   SDK before relying on `parentID` at create time.
5. **Git reads beyond branch/status/diff**: commit/tag/HEAD-anchor reads for
   Versions have no endpoint; use a TS git implementation (or `git` subprocess)
   for those observations only.
6. **Volatile event stream**: slow consumers and disconnects lose events —
   this is compatible with (and reinforces) the invalidation+full-read model,
   but the TS core must never derive state from events alone.
7. **Permissions are per-session and runtime**: Environment permission
   descriptors must be translated into `permission.create`/auto-reply policy
   at launch; there is no per-environment persistent ruleset in the API.
8. **The `agent-factory` CLI disappears**: the Orchestrator's control
   vocabulary becomes prompts, slash commands, instruction entries, and
   permission requests — the TS core is the authorizer.
9. **Update runtime and OS-level bundle swap**: replaced by Electron
   auto-updater; privilege-separated helpers are no longer needed.
10. **Beta surface**: V2 API is beta; pin the client version and regenerate
    bindings from the server's `/openapi.json` rather than hand-rolling.
11. **"No web terminal renderer" rule expires**: AGENTS.md's xterm.js
    prohibition exists because the Herdr TUI was rendered natively. After the
    port the app *must* render a web terminal; OpenCode's own web app uses
    Ghostty compiled to WebAssembly (`ghostty-web`), not xterm.js — use the
    same emulator for consistency (§9).

## 9. OpenCode's web terminal stack (from `/Users/ravi/code/opencode` source)

OpenCode does not embed a terminal TUI in its web app. Instead the server
owns PTY processes and exposes a WebSocket attach surface; the web client
renders them with **Ghostty compiled to WebAssembly**, not xterm.js.

### Server side (`packages/core/src/pty/*`, `packages/opencode/src/server/.../httpapi/handlers/pty.ts`)

- PTY processes are spawned with **`@lydell/node-pty`** (1.2.0-beta.12, the
  maintained node-pty fork; `packages/core/src/pty/pty.node.ts`), with
  `useConptyDll: true` on Windows. The Bun runtime variant uses `bun-pty`
  (`pty.bun.ts`).
- Spawn environment forces `TERM=xterm-256color` and `OPENCODE_TERMINAL=1`;
  cwd defaults to the location directory; shell selection honors the
  configured `shell` preference.
- The PTY service (`packages/core/src/pty.ts`) keeps a **retained output
  buffer capped at 2 MiB** with an absolute character `cursor`. Attach
  replays from a requested cursor (`-1` tails from the live end), then
  streams live chunks to subscribers. Exited sessions stay observable
  (status, exit code, retained output) until removed; at most 25 exited
  sessions are retained.
- WebSocket attach: `GET /api/pty/{ptyID}/connect?location[directory]=...&cursor=N&ticket=...`
  (v2) or the legacy `/pty/{id}/connect` (v1). Authorization is a
  **one-time ticket** from `POST /api/pty/{id}/connect-token`, which requires
  the `x-opencode-ticket: 1` header and a valid CORS origin; without a
  ticket the legacy `auth_token` query (Basic credentials) applies.
- Wire protocol (`packages/core/src/pty/protocol.ts`): **outbound frames are
  raw UTF-8 terminal chunks** (replay is sent in 64 KiB bounded frames) plus
  exactly one control frame — a `0x00` byte followed by UTF-8 JSON
  `{"cursor": N}` carrying the absolute output cursor after replay, so
  clients can resume later. **Inbound frames are raw text** (keystrokes);
  invalid UTF-8 input is dropped. Exit closes the socket with code 1000.
- Resize is `PUT /api/pty/{ptyID}` `{size: {cols, rows}}`; the client debounces
  (100 ms) and coalesces size updates.

### Client side (`packages/app/src/components/terminal.tsx`)

- The renderer is **`ghostty-web`** — Ghostty's terminal emulator compiled to
  WebAssembly (pinned `github:anomalyco/ghostty-web`). Loading is shared and
  lazy: `import("ghostty-web")` → `Ghostty.load()` → `new Terminal({...})`
  with `cursorBlink`, `cursorStyle`, `cols/rows`, `fontSize`, `fontFamily`,
  `allowTransparency: false`, `convertEol: false`, `theme`, and
  `scrollback: 10_000`.
- Two addons: `FitAddon` (fit to container, observe resize) and a custom
  `SerializeAddon` that serializes the terminal buffer so **scrollback,
  cursor, size, and viewport scroll survive remounts and tab switches**
  (`onCleanup({id, buffer, cursor, rows, cols, scrollY})`, restored on
  mount). This replaces native terminal retention in the browser.
- Terminal colors are derived from the app theme (background, foreground,
  cursor, selection background) and pushed with `setOptionIfSupported(term,
  "theme", colors)`.
- The connection is a browser `WebSocket` (`binaryType = "arraybuffer"`)
  with: `connect-token` handshake, exponential-backoff reconnect
  (250 ms · 2^tries, capped at 4 s), `pty.get` existence/status check before
  retry, resume from the last acknowledged cursor, and a `0x00` control-frame
  parser that updates the cursor. Keystrokes stream up via `term.onData` →
  `ws.send`; output is written through a throttling `terminalWriter` to
  avoid overwhelming the emulator.
- Platform extras: `ctrl+shift+c` copies the selection; modifier-click on
  hovered links opens them (`file:` → `openLocalFile`, others →
  `openExternal`).

### Consequence for the port

The ported Agent Factory uses exactly this stack: `ghostty-web` in the
desktop shell (or web UI) + the ticket WebSocket protocol against OpenCode's
PTY API. Terminal history persistence moves client-side via serialization;
the server retains only the 2 MiB rolling buffer and exited-session metadata.
The Run terminal is simply a PTY session whose command is whatever the
product wants (a shell in the Draft worktree, or any command), opened at the
binding's location.

## 10. Suggested ported layout

```
apps/desktop/          Electron shell (window, terminal renderer, packaging)
apps/web-ui/           static Next.js UI (unchanged principles)
packages/core/         TS domain core: ledger (better-sqlite3), Environments,
                       providers refs, Runs, snapshots, policy, worktree policy
packages/opencode/     generated client from OpenCode OpenAPI + service wrapper
                       (Service.ensure/discover/stop), event stream reconnection
packages/runtime-client/ generated bindings for the app's own projection contract
packages/ui/           shared shadcn primitives (unchanged)
packages/theme/        tokens (unchanged)
```

The IPC contract, runtime-contract, sidecar, native host, agent-control
socket, and gateway all disappear; their boundaries become module boundaries
inside `packages/core` with the OpenCode HTTP API as the single external
authority boundary.
