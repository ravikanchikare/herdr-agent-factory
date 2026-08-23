# ADR 0010: Independent LLM Providers and the Environment boundary

Status: accepted

## Decision

LLM Providers are application-level entities persisted in SQLite. Each has a
runtime-generated UUID, mutable name, provider type, endpoint, optional opaque
credential reference, model allowlist, and default model. Raw credential values
remain in the platform Keychain. Multiple Environments may link the same
provider.

An Environment is the authored execution boundary for Coding and Evaluation
Harness agents. Strict Environment schema v1 permits an incomplete authored
descriptor while it is being configured. When `llm` is present, it stores the
required `providerId`, `allowedModels`, and `defaultModel`, alongside Environment
Variables, plugin-backed Skills and MCP Tools, agent selection, permissions, and
registry references in `environments/<id>/environment.json`. A provider swap
re-seeds the model policy from the selected provider; the saved configuration
must still pass readiness before launch.

Rust resolves a non-serializable `ResolvedEnvironment` before launching a
Harness agent. It contains the ready provider and effective model policy,
resolved Environment Variables, the resolved Skills and Tools plan, agents, and
permissions. React renders typed projections and sends intents; the native host
remains provider-neutral byte transport.

## Dependency integrity

`needs setup` is persisted current state, not a provider version, configuration
hash, or dependency revision. Provider type, endpoint, credential reference,
allowlist, and default model affect execution. Saving any of them marks every
linked Environment as needing setup, including an additive allowlist edit.
An additive allowlist edit and existing authored model policy are preserved for
explicit review. Renaming a provider alone does not invalidate linked
Environments.

Saving a valid reviewed Environment clears its setup state even when no authored
field changed. Readiness also requires an available provider Secret, a valid
effective model policy, available Environment Variable Secrets, and resolvable
Skills and Tools. An unready Environment cannot be used to create a new session.
Sessions and Factory Runs select an Environment explicitly at launch; there is
no global activation state.

Provider deletion first persists the unlink in every referencing Environment,
then deletes the provider. Linked Environments remain and need setup; the
underlying Secret is retained.

## Consequences

- The current SQLite schema and Environment schema version 1 are greenfield
  resets; no legacy loaders, aliases, paths, or migrations remain.
- Application revisions order projections and events only. They never express
  dependency validity.
- New Agent Sessions snapshot the resolved provider and model route. The
  dedicated session-integrity follow-up owns current setup state for already
  created sessions, relaunch, and reattachment behavior.
- The internal `llm-gateway` remains a session-scoped loopback mechanism that
  resolves credentials inside Rust and enforces the selected model.
