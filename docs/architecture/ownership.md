# Architecture ownership

Agent Factory joins six authorities without copying one authority's live state
into another writable state machine.

1. `apps/web-ui` renders validated Rust projections and emits typed intents. It
   owns ephemeral interaction state only.
2. `apps/native-host` owns Native-SDK window lifecycle, platform integration,
   packaging, security policy, the Rust sidecar, and bounded opaque transport.
3. Rust owns Agent Factory's durable product ledger and control policy:
   Projects, Target Agents, Drafts, Versions, Workspace Bindings, Factory Runs,
   managed-session lineage and outcomes, Work Contexts, workspace terminals,
   Environments, providers, secrets, plugins, updates, IPC validation, and
   recovery coordination.
4. Herdr owns live Workspaces, tabs, panes, agents, native sessions, processes,
   terminals, topology, and the lifecycle values it reports.
5. Git owns checkout and worktree existence, branches, HEAD, status, diffs,
   commits, and tags.
6. The Orchestrator owns workflow decisions: starting managed sub-agents,
   iterating, evaluating, escalating, and finishing a Run.

Rust may authorize and request Herdr or Git operations, but it does not replace
their observations with cached claims. The production frontend is a Next.js
static export. Server Actions, API routes, middleware, ISR, and a Next.js server
runtime are prohibited.

## State and projection boundary

Rust commits durable Factory facts before publishing their monotonically
revisioned projection. Fresh Herdr and Git observations are joined into that
projection without first becoming SQLite state. Every observed value carries
source identity and freshness; a cache may support a labelled last-observed
view but cannot authorize a live command.

React may retain only ephemeral view state. A WebView reload, runtime restart,
Herdr reconnect, subscription loss, revision gap, or invalid payload triggers a
full snapshot. Herdr events are invalidations: Rust re-reads the affected
authority instead of applying event payloads as ordered domain transitions.

Native-SDK never parses runtime payloads or applies application policy. It
validates bridge origin and command policy, forwards a bounded request to Rust,
and returns the bounded response.

## Harnesses and managed sessions

A Harness is a Herdr agent kind selected from Herdr manifests and limited to
the kinds Agent Factory supports. Herdr manifest warnings determine readiness.
Agent Factory never probes `PATH`, bundles or installs agents, or starts, stops,
or restarts the Herdr server.

A managed-session row is durable Factory lineage, not a copy of a Herdr agent.
It records the Workspace Binding, Run and parent association, role,
Environment, stable Herdr identity, accepted prompt delivery, and durable
outcome. Lifecycle, pane placement, process identity, attention, and transcript
remain live Herdr facts. Unassociated Herdr agents stay visible as other runtime
activity and are never silently adopted or persisted as managed history.

## Repository and worktree authority

The source repository identified by `TargetAgent.repository_root` may define
`.agent-factory/config.json` schema v1 with a repository-relative
`worktreesDirectory`. Rust validates this product policy, then asks Herdr's
worktree infrastructure to create, open, or remove Draft worktrees. Git remains
the authority for the resulting path, branch, HEAD, cleanliness, diff, commits,
and tags.

The configured directory has no global, environment-variable, Project, React,
or Native-SDK override. Existing worktrees are never silently relocated. Each
Draft carries the portable `.agent-factory/target-agent.json` identity manifest;
machine-local paths, cleanup authorization, and runtime references remain in
the local ledger.

## LLM Provider ownership

Rust owns an application-level catalog of reusable LLM Providers. SQLite stores
each provider's UUID, name, type, endpoint, opaque credential reference,
allowlist, default model, and current readiness. Raw credential values remain in
Keychain and are resolved immediately before discovery or gateway startup.

Provider type, endpoint, credential reference, allowlist, and default model are
execution-affecting fields. Saving any of them marks every linked Environment as
needing setup. A provider rename does not. Dependency integrity is represented
only by current persisted setup/readiness state, never by provider versions or
configuration hashes.

## Environment ownership

An Environment is the complete boundary within which Harness agents run. Its
strict schema-v1 descriptor may be authored without a provider; once `llm` is
present, it references one provider and stores its allowed models and default
model. It also owns Environment Variables, plugin-backed Skills and MCP Tools,
Harness selection, permissions, and registry references. It never contains raw
secrets or inline provider configuration.

Before launch, Rust resolves those authored references once into the input used
to create the Herdr pane. Readiness requires a linked ready provider, a valid
effective model policy, available Environment Variable Secrets, and a
resolvable Skills and Tools plan. Unready Environments cannot start new
sessions. Every Run or direct managed session selects an Environment explicitly;
there is no application-wide active Environment.

Each new managed session receives its own loopback gateway. The gateway injects
only the loopback Anthropic endpoint, a non-secret sentinel, and the selected
model into the child process. It replaces upstream credentials and enforces the
Environment-bounded model policy. Inherited Anthropic, Bedrock, and Vertex
configuration is neutralized.
