use std::collections::BTreeSet;

use base64::Engine;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::download::validate_registry_url;
use crate::error::{PluginError, Result};
use crate::loader::valid_plugin_name;

const MAX_CATALOG_BYTES: usize = 8 * 1024 * 1024;
const MAX_CATALOG_PLUGINS: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegistryCatalog {
    pub schema_version: u32,
    pub generated_at: String,
    pub plugins: Vec<RegistryPlugin>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegistryPlugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub archive_url: String,
    pub sha256: String,
}

#[derive(Debug, Clone)]
pub struct VerifiedCatalog {
    catalog: RegistryCatalog,
}

#[derive(Debug, Clone, Copy)]
pub struct VerifiedRegistryPlugin<'a> {
    entry: &'a RegistryPlugin,
}

impl VerifiedCatalog {
    pub fn catalog(&self) -> &RegistryCatalog {
        &self.catalog
    }

    pub fn plugin_by_id(&self, id: &str) -> Option<VerifiedRegistryPlugin<'_>> {
        self.catalog
            .plugins
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| VerifiedRegistryPlugin { entry })
    }
}

impl VerifiedRegistryPlugin<'_> {
    pub fn entry(&self) -> &RegistryPlugin {
        self.entry
    }
}

pub fn verify_catalog(
    catalog_bytes: &[u8],
    signature_base64: &str,
    public_key: &[u8; 32],
) -> Result<VerifiedCatalog> {
    if catalog_bytes.len() > MAX_CATALOG_BYTES {
        return Err(PluginError::InvalidCatalog(format!(
            "catalog exceeds {MAX_CATALOG_BYTES} bytes"
        )));
    }
    let key = VerifyingKey::from_bytes(public_key)
        .map_err(|error| PluginError::InvalidSignature(error.to_string()))?;
    let signature_bytes = base64::engine::general_purpose::STANDARD
        .decode(signature_base64.trim())
        .map_err(|error| PluginError::InvalidSignature(error.to_string()))?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|error| PluginError::InvalidSignature(error.to_string()))?;
    key.verify_strict(catalog_bytes, &signature)
        .map_err(|_| PluginError::InvalidSignature("Ed25519 verification failed".into()))?;

    let catalog: RegistryCatalog = serde_json::from_slice(catalog_bytes)
        .map_err(|error| PluginError::InvalidCatalog(error.to_string()))?;
    validate_catalog(&catalog)?;
    Ok(VerifiedCatalog { catalog })
}

fn validate_catalog(catalog: &RegistryCatalog) -> Result<()> {
    if catalog.schema_version != 1 {
        return Err(PluginError::InvalidCatalog(format!(
            "unsupported schemaVersion {}",
            catalog.schema_version
        )));
    }
    if OffsetDateTime::parse(&catalog.generated_at, &Rfc3339).is_err() {
        return Err(PluginError::InvalidCatalog(
            "generatedAt must be an RFC 3339 date-time".into(),
        ));
    }
    if catalog.plugins.len() > MAX_CATALOG_PLUGINS {
        return Err(PluginError::InvalidCatalog(format!(
            "catalog contains more than {MAX_CATALOG_PLUGINS} plugins"
        )));
    }
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut versions = BTreeSet::new();
    for plugin in &catalog.plugins {
        if plugin.id.is_empty() || plugin.id.len() > 64 {
            return Err(PluginError::InvalidCatalog(
                "plugin id must contain 1-64 characters".into(),
            ));
        }
        if plugin.id != plugin.name || !valid_plugin_name(&plugin.name) {
            return Err(PluginError::InvalidCatalog(format!(
                "plugin id and valid Agent Plugins name must match: {:?} vs {:?}",
                plugin.id, plugin.name
            )));
        }
        if semver::Version::parse(&plugin.version).is_err() {
            return Err(PluginError::InvalidCatalog(format!(
                "{} has invalid semantic version {:?}",
                plugin.id, plugin.version
            )));
        }
        if !ids.insert(plugin.id.clone()) {
            return Err(PluginError::InvalidCatalog(format!(
                "duplicate plugin id {:?}",
                plugin.id
            )));
        }
        if !names.insert(plugin.name.clone()) {
            return Err(PluginError::InvalidCatalog(format!(
                "duplicate plugin name {:?}",
                plugin.name
            )));
        }
        if !versions.insert((plugin.name.clone(), plugin.version.clone())) {
            return Err(PluginError::InvalidCatalog(format!(
                "duplicate plugin version {}@{}",
                plugin.name, plugin.version
            )));
        }
        validate_registry_url(&plugin.archive_url).map_err(|_| {
            PluginError::InvalidCatalog(format!("{} has invalid archiveUrl", plugin.id))
        })?;
        if plugin.sha256.len() != 64
            || !plugin
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(PluginError::InvalidCatalog(format!(
                "{} has an invalid lowercase SHA-256 digest",
                plugin.id
            )));
        }
    }
    Ok(())
}
