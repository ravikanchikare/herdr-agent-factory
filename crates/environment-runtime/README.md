# Environment runtime

`environment-runtime` is the fail-closed authored-policy boundary for Agent
Factory Environments. It loads mutable user-owned `environment.json`
descriptors from `environments/<id>/`, validates every value and resource
limit, and exposes deterministic typed data to the Rust application runtime.

The crate owns no persistence and performs no process execution. It provides:

- strict schema-v1 descriptor types;
- bounded, symlink-free catalog discovery with duplicate rejection;
- supported-agent validation for coding and evaluation roles;
- typed read/write/terminal policies;
- opaque Keychain reference resolution through an injected trait;
- conversion to `plugin-runtime` selection and resolved, non-executing plans.

The application persists current Environment readiness in SQLite. It reloads
and validates descriptors at startup, then Rust composes the Environment chosen
for each launch into a non-serializable execution boundary. Resolved secret
values are never serializable or included in debug output and remain in
zeroizing `platform-secrets::SecretValue` buffers.
