use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use plugin_runtime::{
    EnvironmentPluginEntry, EnvironmentPluginSelection, PluginStore, ResolvedEnvironmentPlugins,
};
use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

use crate::error::{EnvironmentError, Result};

pub const MAX_ENVIRONMENT_ID_CHARS: usize = 64;
const MAX_ENVIRONMENT_NAME_CHARS: usize = 128;
const MAX_PLUGIN_NAME_CHARS: usize = 64;
const MAX_LITERAL_BYTES: usize = 64 * 1024;
const MAX_REGISTRY_CHARS: usize = 64;
pub const ENVIRONMENT_SCHEMA_VERSION: u8 = 1;
/// The Herdr agent kind a new Environment starts with. Herdr decides whether it
/// is actually installed; this is only the default selection.
pub const DEFAULT_HARNESS_ID: &str = "claude";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentDescriptor {
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub schema_version: u8,
    pub id: String,
    pub name: String,
    pub harnesses: EnvironmentHarnesses,
    pub plugins: Vec<EnvironmentPlugin>,
    pub permissions: EnvironmentPermissions,
    #[serde(deserialize_with = "deserialize_environment")]
    pub environment_variables: BTreeMap<String, EnvironmentValue>,
    pub registries: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm: Option<EnvironmentLlmPolicy>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentLlmPolicy {
    pub provider_id: Uuid,
    pub allowed_models: Vec<String>,
    pub default_model: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentHarnesses {
    pub coding: String,
    pub evaluation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentPlugin {
    pub name: String,
    pub enabled_mcp_servers: Vec<String>,
    pub default_skills: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionPolicy {
    Allow,
    Ask,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentPermissions {
    pub trusted_read: PermissionPolicy,
    pub trusted_write: PermissionPolicy,
    pub terminal: PermissionPolicy,
}

impl Default for EnvironmentPermissions {
    /// Cautious by default: an agent may read, and must ask before writing or
    /// running commands.
    fn default() -> Self {
        Self {
            trusted_read: PermissionPolicy::Allow,
            trusted_write: PermissionPolicy::Ask,
            terminal: PermissionPolicy::Ask,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EnvironmentValue {
    Literal(LiteralEnvironmentValue),
    Secret(SecretEnvironmentValue),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiteralEnvironmentValue {
    pub literal: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SecretEnvironmentValue {
    #[serde(deserialize_with = "deserialize_secret_ref")]
    pub secret_ref: platform_secrets::SecretRef,
}

impl EnvironmentDescriptor {
    pub(crate) fn validate(&self, path: &Path) -> Result<()> {
        if self.schema_version != ENVIRONMENT_SCHEMA_VERSION {
            return Err(EnvironmentError::invalid(
                path,
                format!("schemaVersion must be {ENVIRONMENT_SCHEMA_VERSION}"),
            ));
        }
        validate_environment_id(&self.id)
            .map_err(|message| EnvironmentError::invalid(path, format!("invalid id: {message}")))?;
        if self.name.chars().count() > MAX_ENVIRONMENT_NAME_CHARS
            || self.name.is_empty()
            || self.name.trim() != self.name
            || self.name.chars().any(char::is_control)
        {
            return Err(EnvironmentError::invalid(
                path,
                "name is empty, malformed, or too long",
            ));
        }
        validate_harness_id(&self.harnesses.coding).map_err(|message| {
            EnvironmentError::invalid(path, format!("invalid coding agent: {message}"))
        })?;
        validate_harness_id(&self.harnesses.evaluation).map_err(|message| {
            EnvironmentError::invalid(path, format!("invalid evaluation agent: {message}"))
        })?;

        let mut plugins = BTreeSet::new();
        for plugin in &self.plugins {
            validate_plugin_name(&plugin.name).map_err(|message| {
                EnvironmentError::invalid(path, format!("invalid plugin name: {message}"))
            })?;
            if !plugins.insert(plugin.name.as_str()) {
                return Err(EnvironmentError::invalid(
                    path,
                    format!("duplicate plugin {:?}", plugin.name),
                ));
            }
            reject_component_duplicates(
                path,
                &plugin.name,
                "MCP server",
                &plugin.enabled_mcp_servers,
            )?;
            reject_component_duplicates(
                path,
                &plugin.name,
                "default skill",
                &plugin.default_skills,
            )?;
        }

        for (name, value) in &self.environment_variables {
            validate_environment_name(name)
                .map_err(|message| EnvironmentError::invalid(path, message))?;
            if let EnvironmentValue::Literal(value) = value
                && value.literal.chars().count() > MAX_LITERAL_BYTES
            {
                return Err(EnvironmentError::invalid(
                    path,
                    format!("environment literal {name:?} exceeds 64 KiB"),
                ));
            }
        }

        if let Some(policy) = &self.llm {
            policy.validate(path)?;
        }

        let mut registries = BTreeSet::new();
        for registry in &self.registries {
            validate_registry_ref(registry)
                .map_err(|message| EnvironmentError::invalid(path, message))?;
            if !registries.insert(registry.as_str()) {
                return Err(EnvironmentError::invalid(
                    path,
                    format!("duplicate registry reference {registry:?}"),
                ));
            }
        }
        Ok(())
    }

    /// Converts the descriptor into plugin-runtime's non-executing selection.
    pub fn plugin_selection(&self) -> EnvironmentPluginSelection {
        EnvironmentPluginSelection {
            environment_id: self.id.clone(),
            plugins: self
                .plugins
                .iter()
                .map(|plugin| EnvironmentPluginEntry {
                    name: plugin.name.clone(),
                    enabled_mcp_servers: Some(plugin.enabled_mcp_servers.clone()),
                    default_skills: plugin.default_skills.clone(),
                })
                .collect(),
        }
    }

    /// Resolves installed plugin components to typed plans without executing them.
    pub fn resolve_plugin_plan(&self, store: &PluginStore) -> Result<ResolvedEnvironmentPlugins> {
        store
            .resolve_environment_plugins(&self.plugin_selection())
            .map_err(|error| EnvironmentError::PluginPlan {
                environment_id: self.id.clone(),
                message: error.to_string(),
            })
    }
}

impl EnvironmentLlmPolicy {
    pub fn validate(&self, path: &Path) -> Result<()> {
        let mut models = BTreeSet::new();
        if self.allowed_models.is_empty() {
            return Err(EnvironmentError::invalid(
                path,
                "Environment available models must not be empty",
            ));
        }
        for model in &self.allowed_models {
            llm_provider_runtime::validate_model_id(model)
                .map_err(|error| EnvironmentError::invalid(path, error.to_string()))?;
            if !models.insert(model.as_str()) {
                return Err(EnvironmentError::invalid(
                    path,
                    format!("duplicate model {model:?}"),
                ));
            }
        }
        llm_provider_runtime::validate_model_id(&self.default_model)
            .map_err(|error| EnvironmentError::invalid(path, error.to_string()))?;
        if !models.contains(self.default_model.as_str()) {
            return Err(EnvironmentError::invalid(
                path,
                format!(
                    "default model {:?} is not among the Environment available models",
                    self.default_model
                ),
            ));
        }
        Ok(())
    }
}

fn reject_component_duplicates(
    path: &Path,
    plugin: &str,
    label: &str,
    values: &[String],
) -> Result<()> {
    let mut unique = BTreeSet::new();
    for value in values {
        validate_component_name(value, 128).map_err(|message| {
            EnvironmentError::invalid(
                path,
                format!("invalid {label} for plugin {plugin:?}: {message}"),
            )
        })?;
        if !unique.insert(value) {
            return Err(EnvironmentError::invalid(
                path,
                format!("duplicate {label} {value:?} for plugin {plugin:?}"),
            ));
        }
    }
    Ok(())
}

/// Derives an Environment id from a display name.
///
/// Users name an Environment; they never author its id. The result always satisfies
/// [`validate_environment_id`], so the caller's only remaining job is making it unique.
///
/// Names that begin with something other than a letter are *prefixed* rather
/// than trimmed: dropping leading digits would collapse "2024 Review" and
/// "2025 Review" onto the same base. Non-ASCII characters are dropped rather
/// than transliterated — the id is a directory name and a foreign key, never
/// shown to the user, so it is not worth a transliteration dependency.
pub fn derive_environment_id_base(name: &str) -> String {
    let mut slug = String::new();
    for character in name.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_lowercase() || character.is_ascii_digit() {
            slug.push(character);
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        return "environment".into();
    }
    let slug = if slug.starts_with(|character: char| character.is_ascii_lowercase()) {
        slug.to_owned()
    } else {
        format!("environment-{slug}")
    };
    let mut slug = truncate_environment_id(&slug);
    if slug.is_empty() {
        slug = "environment".into();
    }
    slug
}

/// Truncates to the id length limit on a character boundary, then re-trims any
/// separator the cut exposed.
pub(crate) fn truncate_environment_id(value: &str) -> String {
    value
        .chars()
        .take(MAX_ENVIRONMENT_ID_CHARS)
        .collect::<String>()
        .trim_end_matches('-')
        .to_owned()
}

pub fn validate_environment_id(value: &str) -> std::result::Result<(), &'static str> {
    let bytes = value.as_bytes();
    if bytes.is_empty() || value.chars().count() > MAX_ENVIRONMENT_ID_CHARS {
        return Err("must contain 1 to 64 characters");
    }
    if !bytes[0].is_ascii_lowercase()
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
    {
        return Err("must match ^[a-z][a-z0-9-]*$");
    }
    Ok(())
}

fn validate_harness_id(value: &str) -> std::result::Result<(), &'static str> {
    if value.is_empty()
        || value.len() > 128
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err("must be a non-empty, bounded identifier");
    }
    Ok(())
}

fn validate_component_name(value: &str, max_chars: usize) -> std::result::Result<(), &'static str> {
    if value.is_empty()
        || value.chars().count() > max_chars
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err("must be non-empty, trimmed, contain no controls, and fit the size limit");
    }
    Ok(())
}

fn validate_plugin_name(value: &str) -> std::result::Result<(), &'static str> {
    if value.is_empty()
        || value.chars().count() > MAX_PLUGIN_NAME_CHARS
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        || value.starts_with('.')
        || value.contains("..")
    {
        return Err("must identify a safe Agent Plugin package");
    }
    Ok(())
}

fn validate_environment_name(value: &str) -> std::result::Result<(), String> {
    const RESERVED: [&str; 5] = [
        "AGENT_FACTORY_SESSION_ROLE",
        "HOME",
        "PATH",
        "SHELL",
        "TMPDIR",
    ];
    let bytes = value.as_bytes();
    let syntactically_valid = bytes
        .first()
        .is_some_and(|byte| byte.is_ascii_uppercase() || *byte == b'_')
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || *byte == b'_');
    if !syntactically_valid {
        return Err(format!("invalid environment variable name {value:?}"));
    }
    if RESERVED.contains(&value)
        || value.starts_with("DYLD_")
        || value.starts_with("LD_")
        || value.starts_with("ANTHROPIC_")
        || matches!(value, "CLAUDE_CODE_USE_BEDROCK" | "CLAUDE_CODE_USE_VERTEX")
    {
        return Err(format!("reserved environment variable {value:?}"));
    }
    Ok(())
}

fn validate_registry_ref(value: &str) -> std::result::Result<(), String> {
    if value.is_empty()
        || value.chars().count() > MAX_REGISTRY_CHARS
        || value.contains("--")
        || value.contains("..")
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
        })
        || !value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        || !value
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
    {
        return Err(format!("invalid registry reference {value:?}"));
    }
    Ok(())
}

fn deserialize_environment<'de, D>(
    deserializer: D,
) -> std::result::Result<BTreeMap<String, EnvironmentValue>, D::Error>
where
    D: Deserializer<'de>,
{
    struct EnvironmentVisitor;

    impl<'de> serde::de::Visitor<'de> for EnvironmentVisitor {
        type Value = BTreeMap<String, EnvironmentValue>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an environment object with unique variable names")
        }

        fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
        where
            A: serde::de::MapAccess<'de>,
        {
            let mut values = BTreeMap::new();
            while let Some((name, value)) = map.next_entry::<String, EnvironmentValue>()? {
                if values.insert(name.clone(), value).is_some() {
                    return Err(serde::de::Error::custom(format!(
                        "duplicate environment variable {name:?}"
                    )));
                }
            }
            Ok(values)
        }
    }

    deserializer.deserialize_map(EnvironmentVisitor)
}

fn deserialize_secret_ref<'de, D>(
    deserializer: D,
) -> std::result::Result<platform_secrets::SecretRef, D::Error>
where
    D: Deserializer<'de>,
{
    String::deserialize(deserializer)?
        .parse()
        .map_err(|_| serde::de::Error::custom("invalid opaque secret reference"))
}
