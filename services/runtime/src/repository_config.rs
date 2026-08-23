use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

const AGENT_FACTORY_DIRECTORY: &str = ".agent-factory";
const CONFIG_FILENAME: &str = "config.json";
const CONFIG_SCHEMA_VERSION: u8 = 1;
const DEFAULT_WORKTREES_DIRECTORY: &str = "worktrees";
const MAX_CONFIG_BYTES: u64 = 16 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RepositoryConfig {
    worktrees_directory: PathBuf,
}

impl Default for RepositoryConfig {
    fn default() -> Self {
        Self {
            worktrees_directory: PathBuf::from(DEFAULT_WORKTREES_DIRECTORY),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RepositoryConfigDocument {
    schema_version: u8,
    worktrees_directory: String,
}

impl RepositoryConfig {
    pub(crate) fn load(repository_root: &Path) -> Result<Self, RepositoryConfigError> {
        let boundary = repository_boundary(repository_root)?;
        let config_path = boundary.join(CONFIG_FILENAME);
        let metadata = match fs::symlink_metadata(&config_path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(RepositoryConfigError::io(&config_path, error)),
        };
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.len() == 0
            || metadata.len() > MAX_CONFIG_BYTES
        {
            return Err(RepositoryConfigError::InvalidConfigFile(config_path));
        }

        let file = File::open(&config_path)
            .map_err(|error| RepositoryConfigError::io(&config_path, error))?;
        let opened_metadata = file
            .metadata()
            .map_err(|error| RepositoryConfigError::io(&config_path, error))?;
        if !opened_metadata.is_file()
            || opened_metadata.len() == 0
            || opened_metadata.len() > MAX_CONFIG_BYTES
            || !same_file_identity(&metadata, &opened_metadata)
        {
            return Err(RepositoryConfigError::InvalidConfigFile(config_path));
        }

        let mut bytes = Vec::with_capacity(opened_metadata.len() as usize);
        file.take(MAX_CONFIG_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| RepositoryConfigError::io(&config_path, error))?;
        if bytes.is_empty() || bytes.len() as u64 > MAX_CONFIG_BYTES {
            return Err(RepositoryConfigError::InvalidConfigFile(config_path));
        }
        let document: RepositoryConfigDocument =
            serde_json::from_slice(&bytes).map_err(|error| RepositoryConfigError::InvalidJson {
                path: config_path.clone(),
                message: error.to_string(),
            })?;
        if document.schema_version != CONFIG_SCHEMA_VERSION {
            return Err(RepositoryConfigError::UnsupportedSchema {
                path: config_path,
                found: document.schema_version,
            });
        }
        let worktrees_directory = validate_relative_directory(&document.worktrees_directory)?;
        Ok(Self {
            worktrees_directory,
        })
    }

    pub(crate) fn prepare_worktrees_directory(
        &self,
        repository_root: &Path,
    ) -> Result<PathBuf, RepositoryConfigError> {
        let boundary = repository_boundary(repository_root)?;
        let mut directory = boundary.clone();
        for component in self.worktrees_directory.components() {
            let Component::Normal(component) = component else {
                return Err(RepositoryConfigError::InvalidWorktreesDirectory(
                    self.worktrees_directory.display().to_string(),
                ));
            };
            directory.push(component);
            ensure_directory(&directory)?;
        }
        let canonical = fs::canonicalize(&directory)
            .map_err(|error| RepositoryConfigError::io(&directory, error))?;
        if !canonical.starts_with(&boundary) || canonical == boundary {
            return Err(RepositoryConfigError::OutsideBoundary(canonical));
        }
        ensure_local_ignore(&canonical)?;
        Ok(canonical)
    }
}

pub(crate) fn reject_path_collision(path: &Path) -> Result<(), RepositoryConfigError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(RepositoryConfigError::WorktreeCollision(path.to_path_buf())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(RepositoryConfigError::io(path, error)),
    }
}

fn repository_boundary(repository_root: &Path) -> Result<PathBuf, RepositoryConfigError> {
    let repository_root = fs::canonicalize(repository_root)
        .map_err(|error| RepositoryConfigError::io(repository_root, error))?;
    let boundary = repository_root.join(AGENT_FACTORY_DIRECTORY);
    ensure_directory(&boundary)?;
    let canonical =
        fs::canonicalize(&boundary).map_err(|error| RepositoryConfigError::io(&boundary, error))?;
    if canonical.parent() != Some(repository_root.as_path()) {
        return Err(RepositoryConfigError::OutsideBoundary(canonical));
    }
    Ok(canonical)
}

fn ensure_directory(path: &Path) -> Result<(), RepositoryConfigError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            Err(RepositoryConfigError::UnsafeDirectory(path.to_path_buf()))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(|error| RepositoryConfigError::io(path, error))?;
            let metadata = fs::symlink_metadata(path)
                .map_err(|error| RepositoryConfigError::io(path, error))?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(RepositoryConfigError::UnsafeDirectory(path.to_path_buf()));
            }
            Ok(())
        }
        Err(error) => Err(RepositoryConfigError::io(path, error)),
    }
}

fn ensure_local_ignore(directory: &Path) -> Result<(), RepositoryConfigError> {
    let path = directory.join(".gitignore");
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut file) => {
            use std::io::Write;
            if let Err(error) = file.write_all(b"*\n").and_then(|()| file.sync_all()) {
                drop(file);
                let _ = fs::remove_file(&path);
                return Err(RepositoryConfigError::io(&path, error));
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| RepositoryConfigError::io(&path, error))?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(RepositoryConfigError::UnsafeIgnoreFile(path));
            }
            let contents =
                fs::read(&path).map_err(|error| RepositoryConfigError::io(&path, error))?;
            if contents != b"*\n" {
                return Err(RepositoryConfigError::UnsafeIgnoreFile(path));
            }
            Ok(())
        }
        Err(error) => Err(RepositoryConfigError::io(&path, error)),
    }
}

fn validate_relative_directory(value: &str) -> Result<PathBuf, RepositoryConfigError> {
    if value.is_empty()
        || value.len() > 1024
        || value.trim() != value
        || value.contains('\\')
        || value
            .split('/')
            .any(|component| component.is_empty() || matches!(component, "." | ".."))
    {
        return Err(RepositoryConfigError::InvalidWorktreesDirectory(
            value.to_owned(),
        ));
    }
    let path = PathBuf::from(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(RepositoryConfigError::InvalidWorktreesDirectory(
            value.to_owned(),
        ));
    }
    Ok(path)
}

#[cfg(unix)]
fn same_file_identity(before: &fs::Metadata, after: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    before.dev() == after.dev() && before.ino() == after.ino()
}

#[cfg(not(unix))]
fn same_file_identity(before: &fs::Metadata, after: &fs::Metadata) -> bool {
    before.len() == after.len()
        && before.modified().ok() == after.modified().ok()
        && before.created().ok() == after.created().ok()
}

#[derive(Debug, Error)]
pub(crate) enum RepositoryConfigError {
    #[error("repository configuration file is not a regular file: {0}")]
    InvalidConfigFile(PathBuf),
    #[error("repository configuration is not valid JSON at {path}: {message}")]
    InvalidJson { path: PathBuf, message: String },
    #[error(
        "repository configuration at {path} uses unsupported schemaVersion {found}; expected 1"
    )]
    UnsupportedSchema { path: PathBuf, found: u8 },
    #[error("worktreesDirectory must be a confined relative directory: {0:?}")]
    InvalidWorktreesDirectory(String),
    #[error("repository configuration directory is unsafe: {0}")]
    UnsafeDirectory(PathBuf),
    #[error("repository worktree directory escapes .agent-factory: {0}")]
    OutsideBoundary(PathBuf),
    #[error("repository worktree ignore file is unsafe: {0}")]
    UnsafeIgnoreFile(PathBuf),
    #[error("repository worktree path already exists: {0}")]
    WorktreeCollision(PathBuf),
    #[error("repository configuration I/O failed at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl RepositoryConfigError {
    fn io(path: &Path, source: std::io::Error) -> Self {
        Self::Io {
            path: path.to_path_buf(),
            source,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;
    use tempfile::TempDir;

    use super::*;

    fn repository() -> TempDir {
        TempDir::new().unwrap()
    }

    fn write_config(repository: &Path, value: serde_json::Value) {
        let directory = repository.join(AGENT_FACTORY_DIRECTORY);
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join(CONFIG_FILENAME),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn defaults_to_repository_local_worktrees() {
        let repository = repository();
        let config = RepositoryConfig::load(repository.path()).unwrap();
        let directory = config
            .prepare_worktrees_directory(repository.path())
            .unwrap();

        assert_eq!(
            directory,
            fs::canonicalize(repository.path())
                .unwrap()
                .join(".agent-factory/worktrees")
        );
        assert_eq!(
            fs::read_to_string(directory.join(".gitignore")).unwrap(),
            "*\n"
        );
    }

    #[test]
    fn accepts_a_nested_relative_worktrees_directory() {
        let repository = repository();
        write_config(
            repository.path(),
            json!({"schemaVersion": 1, "worktreesDirectory": "generated/drafts"}),
        );

        let config = RepositoryConfig::load(repository.path()).unwrap();
        assert_eq!(
            config
                .prepare_worktrees_directory(repository.path())
                .unwrap(),
            fs::canonicalize(repository.path())
                .unwrap()
                .join(".agent-factory/generated/drafts")
        );
    }

    #[test]
    fn rejects_invalid_schema_paths_and_unknown_fields() {
        for value in [
            json!({"schemaVersion": 2, "worktreesDirectory": "worktrees"}),
            json!({"schemaVersion": 1, "worktreesDirectory": "/tmp/worktrees"}),
            json!({"schemaVersion": 1, "worktreesDirectory": "../worktrees"}),
            json!({"schemaVersion": 1, "worktreesDirectory": "nested/../../outside"}),
            json!({"schemaVersion": 1, "worktreesDirectory": "worktrees", "extra": true}),
        ] {
            let repository = repository();
            write_config(repository.path(), value);
            assert!(RepositoryConfig::load(repository.path()).is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_config_and_directory_components() {
        use std::os::unix::fs::symlink;

        let config_repository = repository();
        let config_directory = config_repository.path().join(AGENT_FACTORY_DIRECTORY);
        fs::create_dir(&config_directory).unwrap();
        let outside_config = config_repository.path().join("outside.json");
        fs::write(
            &outside_config,
            br#"{"schemaVersion":1,"worktreesDirectory":"worktrees"}"#,
        )
        .unwrap();
        symlink(&outside_config, config_directory.join(CONFIG_FILENAME)).unwrap();
        assert!(RepositoryConfig::load(config_repository.path()).is_err());

        let directory_repository = repository();
        write_config(
            directory_repository.path(),
            json!({"schemaVersion": 1, "worktreesDirectory": "generated/drafts"}),
        );
        let outside_directory = directory_repository.path().join("outside");
        fs::create_dir(&outside_directory).unwrap();
        symlink(
            &outside_directory,
            directory_repository.path().join(".agent-factory/generated"),
        )
        .unwrap();
        let config = RepositoryConfig::load(directory_repository.path()).unwrap();
        assert!(
            config
                .prepare_worktrees_directory(directory_repository.path())
                .is_err()
        );
    }

    #[test]
    fn rejects_existing_worktree_paths_including_broken_symlinks() {
        let repository = repository();
        let path = repository.path().join("collision");
        fs::write(&path, "occupied").unwrap();
        assert!(matches!(
            reject_path_collision(&path),
            Err(RepositoryConfigError::WorktreeCollision(_))
        ));

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;

            let broken = repository.path().join("broken");
            symlink(repository.path().join("missing"), &broken).unwrap();
            assert!(matches!(
                reject_path_collision(&broken),
                Err(RepositoryConfigError::WorktreeCollision(_))
            ));
        }
    }

    #[test]
    fn rejects_an_existing_incompatible_ignore_file() {
        let repository = repository();
        let directory = repository.path().join(".agent-factory/worktrees");
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join(".gitignore"), "keep-me\n").unwrap();

        let config = RepositoryConfig::load(repository.path()).unwrap();
        assert!(matches!(
            config.prepare_worktrees_directory(repository.path()),
            Err(RepositoryConfigError::UnsafeIgnoreFile(_))
        ));
    }
}
