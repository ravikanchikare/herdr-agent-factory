use std::path::{Component, Path, PathBuf};

use crate::error::{PluginError, Result};

pub(crate) fn canonical_directory(path: &Path) -> Result<PathBuf> {
    let canonical = std::fs::canonicalize(path).map_err(|error| PluginError::io(path, error))?;
    let metadata =
        std::fs::metadata(&canonical).map_err(|error| PluginError::io(&canonical, error))?;
    if !metadata.is_dir() {
        return Err(PluginError::UnsafePath(format!(
            "{} is not a directory",
            path.display()
        )));
    }
    Ok(canonical)
}

pub(crate) fn normalize_relative(path: &Path) -> Result<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => normalized.push(value),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(PluginError::UnsafePath(format!(
                    "path escapes its root: {}",
                    path.display()
                )));
            }
        }
    }
    if normalized.as_os_str().is_empty() {
        return Err(PluginError::UnsafePath("empty package path".into()));
    }
    Ok(normalized)
}

pub(crate) fn contained_path(root: &Path, relative: &Path) -> Result<PathBuf> {
    let root = canonical_directory(root)?;
    let relative = normalize_relative(relative)?;
    let candidate = root.join(relative);

    let mut existing = candidate.as_path();
    while !existing.exists() {
        existing = existing.parent().ok_or_else(|| {
            PluginError::UnsafePath(format!("cannot resolve {}", candidate.display()))
        })?;
    }
    let resolved =
        std::fs::canonicalize(existing).map_err(|error| PluginError::io(existing, error))?;
    if !resolved.starts_with(&root) {
        return Err(PluginError::UnsafePath(format!(
            "{} resolves outside {}",
            candidate.display(),
            root.display()
        )));
    }
    Ok(candidate)
}

pub(crate) fn require_contained_existing(root: &Path, path: &Path) -> Result<PathBuf> {
    let root = canonical_directory(root)?;
    let resolved = std::fs::canonicalize(path).map_err(|error| PluginError::io(path, error))?;
    if !resolved.starts_with(&root) {
        return Err(PluginError::UnsafePath(format!(
            "{} resolves outside {}",
            path.display(),
            root.display()
        )));
    }
    Ok(resolved)
}

pub(crate) fn validate_identifier(value: &str, label: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        || value.starts_with('.')
        || value.contains("..")
    {
        return Err(PluginError::InvalidEnvironmentSelection(format!(
            "invalid {label}: {value:?}"
        )));
    }
    Ok(())
}
