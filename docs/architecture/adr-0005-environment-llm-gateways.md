# ADR 0005: Environment-local LLM gateways

Status: superseded by ADR 0010

The original decision placed complete LLM Provider configuration inside each
Environment. ADR 0010 replaces that ownership model with reusable
application-level LLM Providers and Environment-owned narrowing policy.

The session-scoped loopback gateway security properties remain: Rust resolves
credentials, exposes only the narrow Anthropic Messages surface, enforces the
effective model, denies redirects, and never sends raw credentials through
SQLite, IPC, React, the native host, or a Herdr pane.
