---
"@agent-factory/runtime-client": major
"@agent-factory/web-ui": major
"@agent-factory/ui": major
"@agent-factory/theme": major
---

Baseline for the Herdr architecture. Nothing has been released, so this replaces
the accumulated pre-release changesets rather than adding to them.

Agent Factory runs Coding Sessions and Evaluation Sessions as agents inside
Herdr panes. Rust owns the Herdr connection and every state transition; the web
UI renders typed projections and emits intents; the Native-SDK host owns window
lifecycle and byte transport.

- **Harnesses** are Herdr agent kinds, resolved from Herdr's agent manifests.
  Agent Factory never probes `PATH`, and never bundles, installs, or starts an
  agent — or Herdr itself. An unreachable Herdr is an explained state with the
  command to fix it.
- **Environments** apply their resolved variables and working directory when the
  session's Herdr tab is created, so the agent process inherits exactly what the
  Environment declares, including the loopback LLM gateway.
- **Approvals belong to the agent.** Herdr reports a `blocked` lifecycle state
  and what the agent is waiting on; the UI surfaces that and forwards keys.
- **Transcripts are terminal text** read on demand. There is no structured
  tool-call, plan, or usage stream, because Herdr publishes none.
- **Sessions outlive the app.** Herdr owns the processes, so a restart reattaches
  to live agents by name instead of tearing sessions down.
- **Run terminals** render through Native-SDK's retained `<terminal pty={key}>`
  surface over Ghostty-VT and host the Herdr client TUI. Factory-owned workspace
  PTYs remain a separate Rust capability and are not used to render Herdr panes.
