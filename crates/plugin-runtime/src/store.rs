use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::archive::{
    ArchiveLimits, StagedArchive, extract_archive, stage_archive, version_storage_key,
};
use crate::error::{PluginError, Result};
use crate::loader::load_plugin;
use crate::model::{InspectedPlugin, LoadedPlugin};
use crate::path::{canonical_directory, validate_identifier};
use crate::registry::VerifiedRegistryPlugin;

#[derive(Debug, Clone)]
pub struct PluginStore {
    root: PathBuf,
    limits: ArchiveLimits,
}

#[derive(Debug, Clone)]
pub struct InstalledPlugin {
    pub name: String,
    pub version: String,
    pub root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPluginState {
    pub name: String,
    pub active_version: String,
    pub previous_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ActivationState {
    active_version: String,
    previous_version: Option<String>,
}

impl PluginStore {
    pub fn open(root: impl AsRef<Path>, limits: ArchiveLimits) -> Result<Self> {
        let root = root.as_ref();
        std::fs::create_dir_all(root).map_err(|error| PluginError::io(root, error))?;
        let root = canonical_directory(root)?;
        for directory in ["plugins", "data", ".staging"] {
            let path = root.join(directory);
            std::fs::create_dir_all(&path).map_err(|error| PluginError::io(path, error))?;
        }
        Ok(Self { root, limits })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn archive_limits(&self) -> ArchiveLimits {
        self.limits
    }

    pub fn stage(&self, input: impl Read) -> Result<StagedArchive> {
        stage_archive(&self.root.join(".staging"), input, self.limits)
    }

    pub fn install_and_activate(
        &self,
        entry: VerifiedRegistryPlugin<'_>,
        archive: &StagedArchive,
    ) -> Result<InstalledPlugin> {
        let entry = entry.entry();
        if archive.sha256() != entry.sha256 {
            return Err(PluginError::Integrity(format!(
                "expected {}, received {}",
                entry.sha256,
                archive.sha256()
            )));
        }
        let extraction = tempfile::Builder::new()
            .prefix("extract-")
            .tempdir_in(self.root.join(".staging"))
            .map_err(|error| PluginError::io(self.root.join(".staging"), error))?;
        extract_archive(archive, extraction.path(), self.limits)?;
        let loaded = load_plugin(extraction.path())?;
        let artifact_version = loaded.manifest.version.as_deref().unwrap_or("0.0.0");
        if loaded.manifest.name != entry.name || artifact_version != entry.version {
            return Err(PluginError::RegistryMismatch(format!(
                "catalog has {}@{}, artifact has {}@{}",
                entry.name, entry.version, loaded.manifest.name, artifact_version
            )));
        }

        let plugin_directory = self.plugin_directory(&entry.name)?;
        let versions = plugin_directory.join("versions");
        std::fs::create_dir_all(&versions).map_err(|error| PluginError::io(&versions, error))?;
        let final_root = versions.join(version_storage_key(&entry.version));
        if final_root.exists() {
            return Err(PluginError::AlreadyInstalled {
                name: entry.name.clone(),
                version: entry.version.clone(),
            });
        }
        let extracted_path = extraction.keep();
        std::fs::rename(&extracted_path, &final_root)
            .map_err(|error| PluginError::io(&final_root, error))?;

        let old_state = self.read_state(&entry.name).ok();
        self.write_state(
            &entry.name,
            &ActivationState {
                active_version: entry.version.clone(),
                previous_version: old_state.map(|state| state.active_version),
            },
        )?;
        Ok(InstalledPlugin {
            name: entry.name.clone(),
            version: entry.version.clone(),
            root: final_root,
        })
    }

    /// Validate and inspect a signed registry artifact without adding it to the
    /// installed-plugin state. This uses the same integrity, extraction, and
    /// manifest checks as installation and leaves no durable plugin files.
    pub fn inspect_registry_plugin(
        &self,
        entry: VerifiedRegistryPlugin<'_>,
        archive: &StagedArchive,
    ) -> Result<InspectedPlugin> {
        let entry = entry.entry();
        if archive.sha256() != entry.sha256 {
            return Err(PluginError::Integrity(format!(
                "expected {}, received {}",
                entry.sha256,
                archive.sha256()
            )));
        }
        let extraction = tempfile::Builder::new()
            .prefix("inspect-")
            .tempdir_in(self.root.join(".staging"))
            .map_err(|error| PluginError::io(self.root.join(".staging"), error))?;
        extract_archive(archive, extraction.path(), self.limits)?;
        let loaded = load_plugin(extraction.path())?;
        let artifact_version = loaded.manifest.version.as_deref().unwrap_or("0.0.0");
        if loaded.manifest.name != entry.name || artifact_version != entry.version {
            return Err(PluginError::RegistryMismatch(format!(
                "catalog has {}@{}, artifact has {}@{}",
                entry.name, entry.version, loaded.manifest.name, artifact_version
            )));
        }
        Ok(InspectedPlugin {
            manifest: loaded.manifest,
            skills: loaded.skills,
            mcp: loaded.mcp,
            diagnostics: loaded.diagnostics,
        })
    }

    pub fn active_plugin(&self, name: &str) -> Result<LoadedPlugin> {
        let state = self.read_state(name)?;
        let root = self.version_root(name, &state.active_version)?;
        if !root.is_dir() {
            return Err(PluginError::NotInstalled(format!(
                "{}@{}",
                name, state.active_version
            )));
        }
        let loaded = load_plugin(&root)?;
        if loaded.manifest.name != name
            || loaded.manifest.version.as_deref().unwrap_or("0.0.0") != state.active_version
        {
            return Err(PluginError::RegistryMismatch(format!(
                "active state for {name} does not match installed manifest"
            )));
        }
        Ok(loaded)
    }

    pub fn list_installed(&self) -> Result<Vec<InstalledPluginState>> {
        let plugins_root = self.root.join("plugins");
        let mut names = Vec::new();
        for entry in
            fs::read_dir(&plugins_root).map_err(|error| PluginError::io(&plugins_root, error))?
        {
            let entry = entry.map_err(|error| PluginError::io(&plugins_root, error))?;
            let file_type = entry
                .file_type()
                .map_err(|error| PluginError::io(entry.path(), error))?;
            if !file_type.is_dir() || file_type.is_symlink() {
                return Err(PluginError::UnsafePath(format!(
                    "unexpected entry in plugin store: {}",
                    entry.path().display()
                )));
            }
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| PluginError::UnsafePath("plugin name is not UTF-8".into()))?;
            validate_identifier(&name, "plugin name")?;
            names.push(name);
        }
        names.sort();
        names
            .into_iter()
            .map(|name| {
                let state = self.read_state(&name)?;
                self.active_plugin(&name)?;
                Ok(InstalledPluginState {
                    name,
                    active_version: state.active_version,
                    previous_version: state.previous_version,
                })
            })
            .collect()
    }

    pub fn rollback(&self, name: &str) -> Result<InstalledPlugin> {
        let state = self.read_state(name)?;
        let previous = state
            .previous_version
            .clone()
            .ok_or_else(|| PluginError::NoRollback(name.into()))?;
        let root = self.version_root(name, &previous)?;
        let loaded = load_plugin(&root)?;
        if loaded.manifest.name != name
            || loaded.manifest.version.as_deref().unwrap_or("0.0.0") != previous
        {
            return Err(PluginError::RegistryMismatch(format!(
                "rollback target for {name} does not match installed manifest"
            )));
        }
        self.write_state(
            name,
            &ActivationState {
                active_version: previous.clone(),
                previous_version: Some(state.active_version),
            },
        )?;
        Ok(InstalledPlugin {
            name: name.into(),
            version: previous,
            root,
        })
    }

    pub fn uninstall(&self, name: &str) -> Result<()> {
        self.read_state(name)?;
        let plugin_directory = self.plugin_directory(name)?;
        let metadata = fs::symlink_metadata(&plugin_directory)
            .map_err(|error| PluginError::io(&plugin_directory, error))?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(PluginError::UnsafePath(format!(
                "installed plugin is not a directory: {}",
                plugin_directory.display(),
            )));
        }

        let staging = tempfile::Builder::new()
            .prefix("uninstall-")
            .tempdir_in(self.root.join(".staging"))
            .map_err(|error| PluginError::io(self.root.join(".staging"), error))?;
        let removed = staging.path().join(name);
        fs::rename(&plugin_directory, &removed)
            .map_err(|error| PluginError::io(&plugin_directory, error))?;
        if let Err(error) = fs::remove_dir_all(&removed) {
            let _ = fs::rename(&removed, &plugin_directory);
            return Err(PluginError::io(&removed, error));
        }
        sync_directory(&self.root.join("plugins"))?;
        Ok(())
    }

    pub fn plugin_data_directory(
        &self,
        environment_id: &str,
        plugin_name: &str,
    ) -> Result<PathBuf> {
        validate_identifier(environment_id, "environment id")?;
        validate_identifier(plugin_name, "plugin name")?;
        let path = self
            .root
            .join("data")
            .join(environment_id)
            .join(plugin_name);
        std::fs::create_dir_all(&path).map_err(|error| PluginError::io(&path, error))?;
        canonical_directory(&path)
    }

    fn plugin_directory(&self, name: &str) -> Result<PathBuf> {
        validate_identifier(name, "plugin name")?;
        Ok(self.root.join("plugins").join(name))
    }

    fn version_root(&self, name: &str, version: &str) -> Result<PathBuf> {
        Ok(self
            .plugin_directory(name)?
            .join("versions")
            .join(version_storage_key(version)))
    }

    fn state_path(&self, name: &str) -> Result<PathBuf> {
        Ok(self.plugin_directory(name)?.join("state.json"))
    }

    fn read_state(&self, name: &str) -> Result<ActivationState> {
        let path = self.state_path(name)?;
        let file = File::open(&path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                PluginError::NotInstalled(name.into())
            } else {
                PluginError::io(&path, error)
            }
        })?;
        serde_json::from_reader(file).map_err(|error| {
            PluginError::RegistryMismatch(format!("invalid activation state for {name}: {error}"))
        })
    }

    fn write_state(&self, name: &str, state: &ActivationState) -> Result<()> {
        let plugin_directory = self.plugin_directory(name)?;
        std::fs::create_dir_all(&plugin_directory)
            .map_err(|error| PluginError::io(&plugin_directory, error))?;
        let path = plugin_directory.join("state.json");
        let mut temp = tempfile::Builder::new()
            .prefix("state-")
            .tempfile_in(&plugin_directory)
            .map_err(|error| PluginError::io(&plugin_directory, error))?;
        serde_json::to_writer(&mut temp, state).map_err(|error| {
            PluginError::RegistryMismatch(format!("cannot serialize activation state: {error}"))
        })?;
        temp.write_all(b"\n")
            .map_err(|error| PluginError::io(temp.path(), error))?;
        temp.flush()
            .map_err(|error| PluginError::io(temp.path(), error))?;
        temp.as_file()
            .sync_all()
            .map_err(|error| PluginError::io(temp.path(), error))?;
        temp.persist(&path)
            .map_err(|error| PluginError::io(&path, error.error))?;
        sync_directory(&plugin_directory)?;
        Ok(())
    }
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| PluginError::io(path, error))
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<()> {
    Ok(())
}
