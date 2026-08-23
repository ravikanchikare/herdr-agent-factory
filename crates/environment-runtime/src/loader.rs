use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::error::{EnvironmentError, Result};
use crate::model::{
    DEFAULT_HARNESS_ID, ENVIRONMENT_SCHEMA_VERSION, EnvironmentDescriptor, validate_environment_id,
};

/// The complete editable configuration of an Environment, as authored in one form.
/// Creating and saving take the same shape so a create is a single atomic write
/// rather than a create-then-configure sequence that can strand a half-built
/// Environment on disk.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EnvironmentDraft {
    pub name: String,
    pub environment_variables: BTreeMap<String, crate::model::EnvironmentValue>,
    pub llm: Option<crate::model::EnvironmentLlmPolicy>,
    pub plugins: Vec<crate::model::EnvironmentPlugin>,
    pub registries: Vec<String>,
    /// How much an agent may do unattended. A Factory Run advances without a
    /// person watching, so this is what decides whether it can.
    pub permissions: crate::model::EnvironmentPermissions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogLimits {
    pub max_environments: usize,
    pub max_descriptor_bytes: u64,
    pub max_total_descriptor_bytes: u64,
    pub max_plugins_per_environment: usize,
    pub max_component_refs_per_plugin: usize,
    pub max_environment_variables: usize,
    pub max_registries_per_environment: usize,
}

impl Default for CatalogLimits {
    fn default() -> Self {
        Self {
            max_environments: 128,
            max_descriptor_bytes: 256 * 1024,
            max_total_descriptor_bytes: 4 * 1024 * 1024,
            max_plugins_per_environment: 64,
            max_component_refs_per_plugin: 128,
            max_environment_variables: 128,
            max_registries_per_environment: 64,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedEnvironment {
    pub descriptor: EnvironmentDescriptor,
    pub descriptor_path: PathBuf,
}

/// An Environment directory the catalog could not load. It is reported rather
/// than thrown, so one unreadable descriptor cannot keep the app from starting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RejectedEnvironment {
    pub id: String,
    pub path: PathBuf,
    pub reason: String,
}

#[derive(Debug, Clone, Default)]

pub struct EnvironmentCatalog {
    environments: BTreeMap<String, LoadedEnvironment>,
    rejected: Vec<RejectedEnvironment>,
    user_root: PathBuf,
}

impl EnvironmentCatalog {
    /// Loads every Environment the user owns. A missing root is an empty catalog, not
    /// an error: that is the first-boot state, and there are no application
    /// environments to fall back on.
    pub fn load(
        user_root: &Path,
        supported_harnesses: &BTreeSet<String>,
        limits: CatalogLimits,
    ) -> Result<Self> {
        validate_limits(limits)?;
        let mut catalog = Self {
            environments: BTreeMap::new(),
            rejected: Vec::new(),
            user_root: user_root.to_path_buf(),
        };
        catalog.load_root(user_root, supported_harnesses, limits)?;
        Ok(catalog)
    }

    /// Environments on disk that could not be loaded, and why. Callers surface
    /// these so a rejected Environment is explained rather than silently absent.
    pub fn rejected(&self) -> &[RejectedEnvironment] {
        &self.rejected
    }

    pub fn get(&self, id: &str) -> Option<&LoadedEnvironment> {
        self.environments.get(id)
    }

    /// The directory this catalog owns. Callers allocating an Environment id need it to
    /// check whether a candidate directory is already on disk.
    pub fn user_root(&self) -> &Path {
        &self.user_root
    }

    /// Atomically persist an Environment's complete configuration. This is the single
    /// save path: name, environment, provider, plugins, and registries are
    /// authored together in one form and replaced together, so a save can never
    /// leave the descriptor holding a mix of two edits.
    ///
    /// The plugin and registry selections are validated for shape and limits
    /// only; whether a referenced plugin or component is actually installed is
    /// resolved later, when a session is launched.
    ///
    /// The Environment's id is never derived from the name again, so renaming leaves
    /// the id and its directory untouched.
    pub fn save_configuration(
        &mut self,
        id: &str,
        draft: EnvironmentDraft,
    ) -> Result<EnvironmentDescriptor> {
        let loaded = self
            .environments
            .get(id)
            .ok_or_else(|| EnvironmentError::NotFound(id.to_owned()))?;
        let mut descriptor = loaded.descriptor.clone();
        descriptor.name = draft.name;
        descriptor.environment_variables = draft.environment_variables;
        descriptor.llm = draft.llm;
        descriptor.plugins = draft.plugins;
        descriptor.registries = draft.registries;
        validate_collection_limits(
            &descriptor,
            CatalogLimits::default(),
            &loaded.descriptor_path,
        )?;
        self.persist_user_descriptor(id, descriptor)
    }

    /// Remove an Environment from disk and from the catalog.
    ///
    /// The directory is renamed to a hidden tombstone before it is emptied, so
    /// the Environment namespace is never observed half-removed. Deleting `environment.json`
    /// first would be wrong in the opposite direction: the intermediate state is
    /// an empty directory named after a valid Environment id, which is exactly what a
    /// subsequent load has to reject.
    pub fn delete_user_environment(&mut self, id: &str) -> Result<()> {
        let loaded = self
            .environments
            .get(id)
            .ok_or_else(|| EnvironmentError::NotFound(id.to_owned()))?;
        let directory = loaded
            .descriptor_path
            .parent()
            .ok_or_else(|| {
                EnvironmentError::UnsafePath("environment descriptor has no parent".into())
            })?
            .to_path_buf();

        let metadata = std::fs::symlink_metadata(&directory)
            .map_err(|error| EnvironmentError::io(&directory, error))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(EnvironmentError::UnsafePath(format!(
                "environment directory must be a real directory: {}",
                directory.display()
            )));
        }
        let canonical_root = std::fs::canonicalize(&self.user_root)
            .map_err(|error| EnvironmentError::io(&self.user_root, error))?;
        if directory.parent() != Some(canonical_root.as_path()) {
            return Err(EnvironmentError::UnsafePath(format!(
                "environment directory escapes its root: {}",
                directory.display()
            )));
        }

        let tombstone = canonical_root.join(format!(".deleted-{id}-{}", std::process::id()));
        std::fs::rename(&directory, &tombstone)
            .map_err(|error| EnvironmentError::io(&directory, error))?;
        // The tombstone name fails `validate_environment_id`, so a later load skips it
        // even if this best-effort cleanup does not finish.
        let _ = std::fs::remove_dir_all(&tombstone);
        self.environments.remove(id);
        Ok(())
    }

    fn persist_user_descriptor(
        &mut self,
        id: &str,
        descriptor: EnvironmentDescriptor,
    ) -> Result<EnvironmentDescriptor> {
        let loaded = self
            .environments
            .get(id)
            .ok_or_else(|| EnvironmentError::NotFound(id.to_owned()))?;
        let path = loaded.descriptor_path.clone();
        descriptor.validate(&path)?;
        let bytes = serde_json::to_vec_pretty(&descriptor).map_err(|error| {
            EnvironmentError::invalid(&path, format!("failed to encode JSON: {error}"))
        })?;
        let metadata =
            std::fs::symlink_metadata(&path).map_err(|error| EnvironmentError::io(&path, error))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(EnvironmentError::UnsafePath(format!(
                "environment descriptor must be a regular file: {}",
                path.display()
            )));
        }
        // The temp file lives in the root, not the Environment directory: a crash
        // between create and rename must not leave debris inside an Environment that a
        // later load would have to interpret. The dot prefix fails
        // `validate_environment_id`, so the root skips it.
        let temporary = self
            .user_root
            .join(format!(".environment-{id}.json.tmp-{}", std::process::id()));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| EnvironmentError::io(&temporary, error))?;
        if let Err(error) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
            let _ = std::fs::remove_file(&temporary);
            return Err(EnvironmentError::io(&temporary, error));
        }
        if let Err(error) = std::fs::rename(&temporary, &path) {
            let _ = std::fs::remove_file(&temporary);
            return Err(EnvironmentError::io(&path, error));
        }

        if let Some(loaded) = self.environments.get_mut(id) {
            loaded.descriptor = descriptor.clone();
        }
        Ok(descriptor)
    }

    /// Create an Environment from a complete draft. The descriptor is validated before
    /// it becomes visible in the catalog and is written atomically, so a create
    /// either lands whole or not at all.
    ///
    /// The id is allocated by the caller, which is the only layer that can see
    /// both this catalog and the ids reserved by deleted Environments.
    pub fn create_user_environment(
        &mut self,
        id: String,
        draft: EnvironmentDraft,
    ) -> Result<EnvironmentDescriptor> {
        validate_environment_id(&id).map_err(|message| {
            EnvironmentError::UnsafePath(format!("invalid environment id: {message}"))
        })?;
        if self.environments.contains_key(&id) {
            return Err(EnvironmentError::DuplicateEnvironment(id));
        }
        let descriptor = EnvironmentDescriptor {
            schema: None,
            schema_version: ENVIRONMENT_SCHEMA_VERSION,
            id: id.clone(),
            name: draft.name,
            harnesses: crate::model::EnvironmentHarnesses {
                coding: DEFAULT_HARNESS_ID.into(),
                evaluation: DEFAULT_HARNESS_ID.into(),
            },
            plugins: draft.plugins,
            permissions: draft.permissions,
            environment_variables: draft.environment_variables,
            registries: draft.registries,
            llm: draft.llm,
        };
        let descriptor_path = self.user_root.join(&id).join("environment.json");
        descriptor.validate(&descriptor_path)?;
        validate_collection_limits(&descriptor, CatalogLimits::default(), &descriptor_path)?;
        if let Ok(metadata) = std::fs::symlink_metadata(&self.user_root)
            && (metadata.file_type().is_symlink() || !metadata.is_dir())
        {
            return Err(EnvironmentError::UnsafePath(format!(
                "user environment root must be a real directory: {}",
                self.user_root.display()
            )));
        }
        if let Some(parent) = descriptor_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| EnvironmentError::io(parent, error))?;
        }
        let bytes = serde_json::to_vec_pretty(&descriptor).map_err(|error| {
            EnvironmentError::invalid(&descriptor_path, format!("failed to encode JSON: {error}"))
        })?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&descriptor_path)
            .map_err(|error| EnvironmentError::io(&descriptor_path, error))?;
        if let Err(error) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
            let _ = std::fs::remove_file(&descriptor_path);
            return Err(EnvironmentError::io(&descriptor_path, error));
        }
        let canonical = std::fs::canonicalize(&descriptor_path)
            .map_err(|error| EnvironmentError::io(&descriptor_path, error))?;
        self.environments.insert(
            id,
            LoadedEnvironment {
                descriptor: descriptor.clone(),
                descriptor_path: canonical,
            },
        );
        Ok(descriptor)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &LoadedEnvironment)> {
        self.environments
            .iter()
            .map(|(id, environment)| (id.as_str(), environment))
    }

    pub fn len(&self) -> usize {
        self.environments.len()
    }

    pub fn is_empty(&self) -> bool {
        self.environments.is_empty()
    }

    /// Reads every Environment directory under `root`.
    ///
    /// Security violations stay fatal: symlinks, descriptors escaping the root,
    /// an id that disagrees with its directory, malformed or oversized
    /// descriptors. Untidiness does not: stray files, unnamable directories, and
    /// directories without a descriptor are skipped. The user root is writable
    /// and there is no application Environment to fall back on, so a `.DS_Store` or a
    /// leftover temp file must not make the catalog — and with it the app —
    /// refuse to start.
    fn load_root(
        &mut self,
        root: &Path,
        supported_harnesses: &BTreeSet<String>,
        limits: CatalogLimits,
    ) -> Result<()> {
        let mut total_bytes = 0_u64;
        let total_bytes = &mut total_bytes;
        let root_metadata = match std::fs::symlink_metadata(root) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(());
            }
            Err(error) => return Err(EnvironmentError::io(root, error)),
        };
        if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
            return Err(EnvironmentError::UnsafePath(format!(
                "environment root {} must be a real directory",
                root.display()
            )));
        }
        let canonical_root =
            std::fs::canonicalize(root).map_err(|error| EnvironmentError::io(root, error))?;
        let entries = sorted_entries(&canonical_root)?;
        for entry in entries {
            let file_name = entry.file_name();
            let Some(file_name) = file_name.to_str() else {
                return Err(EnvironmentError::UnsafePath(format!(
                    "non-UTF-8 entry in {}",
                    canonical_root.display()
                )));
            };
            let file_type = entry
                .file_type()
                .map_err(|error| EnvironmentError::io(entry.path(), error))?;
            if file_type.is_symlink() {
                return Err(EnvironmentError::UnsafePath(format!(
                    "symlink is not allowed: {}",
                    entry.path().display()
                )));
            }
            // Stray files and directories that could never name an Environment are the
            // user's business, not ours.
            if !file_type.is_dir() || validate_environment_id(file_name).is_err() {
                continue;
            }
            if self.environments.len() >= limits.max_environments {
                return Err(EnvironmentError::LimitExceeded(format!(
                    "more than {} environments",
                    limits.max_environments
                )));
            }
            let directory = entry.path();
            if !require_descriptor(&directory)? {
                continue;
            }
            // A descriptor that cannot be read or understood disqualifies its own
            // Environment and nothing else. Catalog-wide limits stay fatal: they
            // mean the whole root is untrustworthy, not one entry in it.
            match Self::load_descriptor(
                &canonical_root,
                &directory,
                file_name,
                supported_harnesses,
                limits,
                total_bytes,
            ) {
                Ok(Some((environment_id, loaded))) => {
                    if self
                        .environments
                        .insert(environment_id.clone(), loaded)
                        .is_some()
                    {
                        return Err(EnvironmentError::DuplicateEnvironment(environment_id));
                    }
                }
                Ok(None) => {}
                Err(EnvironmentError::LimitExceeded(message)) => {
                    return Err(EnvironmentError::LimitExceeded(message));
                }
                Err(error) => {
                    self.rejected.push(RejectedEnvironment {
                        id: file_name.to_owned(),
                        path: directory.join("environment.json"),
                        reason: error.to_string(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Load one Environment directory. Every failure here is scoped to that
    /// Environment.
    #[allow(clippy::too_many_arguments)]
    fn load_descriptor(
        canonical_root: &Path,
        directory: &Path,
        file_name: &str,
        supported_harnesses: &BTreeSet<String>,
        limits: CatalogLimits,
        total_bytes: &mut u64,
    ) -> Result<Option<(String, LoadedEnvironment)>> {
        let descriptor_path = directory.join("environment.json");
        let canonical_descriptor = read_safe_descriptor_path(canonical_root, &descriptor_path)?;
        let bytes = read_bounded(&canonical_descriptor, limits.max_descriptor_bytes)?;
        *total_bytes = total_bytes.checked_add(bytes.len() as u64).ok_or_else(|| {
            EnvironmentError::LimitExceeded("descriptor byte count overflow".into())
        })?;
        if *total_bytes > limits.max_total_descriptor_bytes {
            return Err(EnvironmentError::LimitExceeded(format!(
                "descriptor catalog exceeds {} bytes",
                limits.max_total_descriptor_bytes
            )));
        }
        let descriptor: EnvironmentDescriptor =
            serde_json::from_slice(&bytes).map_err(|error| {
                EnvironmentError::invalid(&canonical_descriptor, format!("invalid JSON: {error}"))
            })?;
        descriptor.validate(&canonical_descriptor)?;
        validate_collection_limits(&descriptor, limits, &canonical_descriptor)?;
        if descriptor.id != file_name {
            return Err(EnvironmentError::invalid(
                &canonical_descriptor,
                format!(
                    "descriptor ID {:?} does not match directory {file_name:?}",
                    descriptor.id
                ),
            ));
        }
        validate_supported_harnesses(&descriptor, supported_harnesses)?;
        let environment_id = descriptor.id.clone();
        Ok(Some((
            environment_id,
            LoadedEnvironment {
                descriptor,
                descriptor_path: canonical_descriptor,
            },
        )))
    }
}

fn validate_limits(limits: CatalogLimits) -> Result<()> {
    if limits.max_environments == 0
        || limits.max_descriptor_bytes == 0
        || limits.max_total_descriptor_bytes == 0
        || limits.max_plugins_per_environment == 0
        || limits.max_component_refs_per_plugin == 0
        || limits.max_environment_variables == 0
        || limits.max_registries_per_environment == 0
    {
        return Err(EnvironmentError::LimitExceeded(
            "catalog limits must all be positive".into(),
        ));
    }
    Ok(())
}

fn sorted_entries(root: &Path) -> Result<Vec<std::fs::DirEntry>> {
    let mut entries = std::fs::read_dir(root)
        .map_err(|error| EnvironmentError::io(root, error))?
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|error| EnvironmentError::io(root, error))?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    Ok(entries)
}

/// Reports whether `directory` holds a descriptor worth loading, sweeping up any
/// abandoned temp file it finds along the way. A directory with no `environment.json`
/// is not an Environment; siblings of the descriptor are ignored.
fn require_descriptor(directory: &Path) -> Result<bool> {
    let metadata = std::fs::symlink_metadata(directory)
        .map_err(|error| EnvironmentError::io(directory, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(EnvironmentError::UnsafePath(format!(
            "environment directory must not be a symlink: {}",
            directory.display()
        )));
    }
    let mut has_descriptor = false;
    for entry in sorted_entries(directory)? {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name == "environment.json" {
            has_descriptor = true;
        } else if name.starts_with("environment.json.tmp-") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    Ok(has_descriptor)
}

fn read_safe_descriptor_path(root: &Path, path: &Path) -> Result<PathBuf> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|error| EnvironmentError::io(path, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(EnvironmentError::UnsafePath(format!(
            "environment descriptor must be a regular file: {}",
            path.display()
        )));
    }
    let canonical =
        std::fs::canonicalize(path).map_err(|error| EnvironmentError::io(path, error))?;
    if !canonical.starts_with(root) {
        return Err(EnvironmentError::UnsafePath(format!(
            "environment descriptor escapes its root: {}",
            path.display()
        )));
    }
    Ok(canonical)
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let metadata = std::fs::metadata(path).map_err(|error| EnvironmentError::io(path, error))?;
    if metadata.len() > limit {
        return Err(EnvironmentError::LimitExceeded(format!(
            "{} exceeds {limit} bytes",
            path.display()
        )));
    }
    let file = File::open(path).map_err(|error| EnvironmentError::io(path, error))?;
    let mut bytes = Vec::with_capacity(metadata.len().try_into().unwrap_or(0));
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| EnvironmentError::io(path, error))?;
    if bytes.len() as u64 > limit {
        return Err(EnvironmentError::LimitExceeded(format!(
            "{} exceeds {limit} bytes",
            path.display()
        )));
    }
    Ok(bytes)
}

fn validate_supported_harnesses(
    descriptor: &EnvironmentDescriptor,
    supported_harnesses: &BTreeSet<String>,
) -> Result<()> {
    for (role, harness_id) in [
        ("coding", &descriptor.harnesses.coding),
        ("evaluation", &descriptor.harnesses.evaluation),
    ] {
        if !supported_harnesses.contains(harness_id) {
            return Err(EnvironmentError::UnsupportedHarness {
                environment_id: descriptor.id.clone(),
                role,
                harness_id: harness_id.clone(),
            });
        }
    }
    Ok(())
}

fn validate_collection_limits(
    descriptor: &EnvironmentDescriptor,
    limits: CatalogLimits,
    path: &Path,
) -> Result<()> {
    if descriptor.plugins.len() > limits.max_plugins_per_environment {
        return Err(EnvironmentError::invalid(path, "too many plugins"));
    }
    if descriptor.environment_variables.len() > limits.max_environment_variables {
        return Err(EnvironmentError::invalid(
            path,
            "too many environment variables",
        ));
    }
    if descriptor.registries.len() > limits.max_registries_per_environment {
        return Err(EnvironmentError::invalid(
            path,
            "too many registry references",
        ));
    }
    for plugin in &descriptor.plugins {
        if plugin.enabled_mcp_servers.len() > limits.max_component_refs_per_plugin
            || plugin.default_skills.len() > limits.max_component_refs_per_plugin
        {
            return Err(EnvironmentError::invalid(
                path,
                format!("too many component references for plugin {:?}", plugin.name),
            ));
        }
    }
    Ok(())
}
