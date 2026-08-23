#![forbid(unsafe_code)]
//! Agent Plugins 1.0 package loading and client-owned registry installation.
//!
//! This crate never executes a plugin. It validates portable package content,
//! stages and verifies registry artifacts, activates immutable versions, and
//! resolves one environment's enabled skills and MCP definitions into typed plans.

mod archive;
mod download;
mod error;
mod loader;
mod model;
mod path;
mod registry;
mod resolve;
mod store;

pub use archive::{ArchiveLimits, StagedArchive, stage_archive};
pub use download::{
    HttpsRegistryDownloader, RegistryClient, RegistryDownloader, validate_registry_url,
};
pub use error::{PluginError, Result};
pub use loader::load_plugin;
pub use model::{
    Diagnostic, DiagnosticBoundary, EnvironmentPluginEntry, EnvironmentPluginSelection,
    ExecutableTrustClass, InspectedPlugin, LoadedPlugin, MCP_SCHEMA_V1, McpComponent,
    McpServerDefinition, PLUGIN_SCHEMA_V1, PluginAuthor, PluginManifest,
    ResolvedEnvironmentPlugins, ResolvedMcpServer, ResolvedSkill, SkillDefinition,
};
pub use registry::{
    RegistryCatalog, RegistryPlugin, VerifiedCatalog, VerifiedRegistryPlugin, verify_catalog,
};
pub use store::{InstalledPlugin, InstalledPluginState, PluginStore};

/// Vendored canonical Agent Plugins 1.0 manifest schema.
pub const PLUGIN_SCHEMA_JSON: &str =
    include_str!("../../../plugins/schemas/1.0.0/plugin.schema.json");

/// Vendored canonical Agent Plugins 1.0 MCP schema.
pub const MCP_SCHEMA_JSON: &str = include_str!("../../../plugins/schemas/1.0.0/mcp.schema.json");
