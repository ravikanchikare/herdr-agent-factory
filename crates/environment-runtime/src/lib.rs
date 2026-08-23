#![forbid(unsafe_code)]
//! Validated, deterministic environment definitions.
//!
//! Environments select agents, permission defaults, environment inputs, registries,
//! and Agent Plugin components. Loading and resolution are read-only: this
//! crate does not launch agents, execute plugins, or persist runtime selections.

mod error;
mod loader;
mod model;
mod secrets;

pub use error::{EnvironmentError, Result};
pub use loader::{
    CatalogLimits, EnvironmentCatalog, EnvironmentDraft, LoadedEnvironment, RejectedEnvironment,
};
pub use model::{
    DEFAULT_HARNESS_ID, ENVIRONMENT_SCHEMA_VERSION, EnvironmentDescriptor, EnvironmentHarnesses,
    EnvironmentLlmPolicy, EnvironmentPermissions, EnvironmentPlugin, EnvironmentValue,
    LiteralEnvironmentValue, MAX_ENVIRONMENT_ID_CHARS, PermissionPolicy, SecretEnvironmentValue,
    derive_environment_id_base, validate_environment_id,
};
pub use secrets::{
    ResolvedEnvironment, ResolvedEnvironmentValueRef, SecretResolver, StoredSecretResolver,
};

/// Canonical JSON Schema for environment descriptors.
pub const ENVIRONMENT_SCHEMA_JSON: &str = include_str!("../schema/environment.schema.json");
