---
name: herdr-authority
description: Enforce Herdr-native authority model. Use whenever creating or modifying Herdr integration, workspace/tab/pane/process handling, agent lifecycle usage, worktree operations, or any code that reads Herdr state or events.
---

# Herdr Authority

Herdr owns live runtime topology. Agent Factory connects as a client; it never implements a second agent runtime.

## Rules

- Read Herdr snapshots and direct reads as authoritative. Events are invalidations — re-read the affected entity; do not apply event payloads as ordered transitions.
- On startup, WebView reload, Rust restart, Herdr reconnect, subscription loss, revision gap, or invalid payload, obtain a full Herdr snapshot + fresh Git observations before enabling live commands.
- A failed list/snapshot means the authority is unavailable; it never means every Workspace/Agent disappeared.
- Reconcile managed sessions through stable Herdr identity + Factory association. Treat cached Workspace/pane/process IDs as revalidated locators only.
- Use Herdr lifecycle values directly (`idle`, `working`, `blocked`, `done`, `unknown`) — do not reinterpret as Factory Run states. Enable prompt/resume only for settled agents; observation/interruption while `working`; agent-native input while `blocked`.
- Read terminal text and recent unwrapped transcripts from Herdr on demand; do not persist duplicate transcripts or structured turn models.
- Worktrees: Herdr performs create/open/remove; Git is authority on actual path/branch/HEAD/cleanliness/diff/commit/tag. Persist only association/provenance/branch policy/anchors/cleanup authorization.
- Do not add `xterm.js` or a web terminal renderer — the Run terminal uses Native-SDK's retained `<terminal pty={key}>` over Ghostty-VT; workspace PTYs are `crates/terminal-runtime`.

## Reference

- `docs/spec/herdr.md`, `docs/spec/ownership.md`
- `crates/herdr-client/tests` — how to test without connecting to the developer's Herdr workspace (`AGENT_FACTORY_HERDR_SOCKET` + detached runtime)
