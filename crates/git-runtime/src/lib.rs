//! Constrained Git operations for Agent Draft worktrees.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use thiserror::Error;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_OUTPUT_BYTES: usize = 256 * 1024;
const MAX_VERSION_TREE_BYTES: usize = 768 * 1024;
pub const MAX_VERSION_FILE_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepositoryState {
    pub root: PathBuf,
    pub head: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorktreeStatus {
    pub entries: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkingTreeChangeKind {
    Added,
    Modified,
    Deleted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkingTreeChange {
    pub path: String,
    pub kind: WorkingTreeChangeKind,
}

impl WorktreeStatus {
    pub fn is_clean(&self) -> bool {
        self.entries.is_empty()
    }

    fn without_runtime_local_data(mut self) -> Self {
        self.entries
            .retain(|entry| !is_untracked_runtime_local_entry(entry));
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublishedCommit {
    pub commit: String,
    pub tag: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitTreeEntryKind {
    File,
    Symlink,
    Submodule,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitTreeEntry {
    pub path: String,
    pub kind: CommitTreeEntryKind,
    pub size: Option<u64>,
    object_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitFileKind {
    Text,
    Binary,
    TooLarge,
    Unsupported,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitFile {
    pub path: String,
    pub size: Option<u64>,
    pub kind: CommitFileKind,
    pub content: Option<String>,
}

#[derive(Debug, Error)]
pub enum GitError {
    #[error("Git is required to create and publish Agent Drafts")]
    MissingGit,
    #[error("Git command timed out while {0}")]
    Timeout(&'static str),
    #[error("Git returned too much output while {0}")]
    OutputTooLarge(&'static str),
    #[error("unsupported repository state: {0}")]
    UnsupportedRepository(String),
    #[error("Git reference already exists: {0}")]
    ReferenceCollision(String),
    #[error("Git identity is not configured; set user.name and user.email before publishing")]
    MissingIdentity,
    #[error("Draft contains local data and cannot be removed: {0}")]
    DirtyWorktree(String),
    #[error("Git operation failed while {operation}: {message}")]
    Command {
        operation: &'static str,
        message: String,
    },
    #[error("invalid Git path: {0}")]
    InvalidPath(String),
    #[error("invalid Version file path: {0}")]
    InvalidVersionPath(String),
    #[error("invalid immutable Git object ID: {0}")]
    InvalidObjectId(String),
    #[error("invalid Git tree data: {0}")]
    InvalidTree(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Clone, Debug)]
pub struct GitRuntime {
    executable: PathBuf,
    timeout: Duration,
}

impl Default for GitRuntime {
    fn default() -> Self {
        Self {
            executable: PathBuf::from("git"),
            timeout: COMMAND_TIMEOUT,
        }
    }
}

impl GitRuntime {
    /// Accept an existing clean Git root, or initialize an empty directory as
    /// a new repository with an initial empty commit so Draft worktrees can
    /// branch from HEAD.
    pub fn ensure_repository(&self, repository: &Path) -> Result<RepositoryState, GitError> {
        let root = std::fs::canonicalize(repository)
            .map_err(|_| GitError::InvalidPath(repository.display().to_string()))?;
        if !root.is_dir() {
            return Err(GitError::InvalidPath(root.display().to_string()));
        }
        if !self.is_repository_root(&root)? {
            self.init_empty_repository(&root)?;
        }
        self.preflight(&root)
    }

    pub fn preflight(&self, repository: &Path) -> Result<RepositoryState, GitError> {
        let root = std::fs::canonicalize(repository)
            .map_err(|_| GitError::InvalidPath(repository.display().to_string()))?;
        if !root.is_dir() {
            return Err(GitError::InvalidPath(root.display().to_string()));
        }
        let discovered = self.run(
            &root,
            "discovering the repository",
            [
                OsString::from("rev-parse"),
                OsString::from("--show-toplevel"),
            ],
        )?;
        let discovered = std::fs::canonicalize(discovered.trim())?;
        if discovered != root {
            return Err(GitError::UnsupportedRepository(
                "select the repository root, not a directory inside it".into(),
            ));
        }
        let bare = self.run(
            &root,
            "checking the repository",
            [
                OsString::from("rev-parse"),
                OsString::from("--is-bare-repository"),
            ],
        )?;
        if bare.trim() != "false" {
            return Err(GitError::UnsupportedRepository(
                "bare repositories cannot host Agent Drafts".into(),
            ));
        }
        let head = self.run(
            &root,
            "resolving HEAD",
            [
                OsString::from("rev-parse"),
                OsString::from("--verify"),
                OsString::from("HEAD^{commit}"),
            ],
        )?;
        self.require_no_operation(&root)?;
        let status = self.status(&root, true)?.without_runtime_local_data();
        if !status.is_clean() {
            return Err(GitError::DirtyWorktree(status.entries.join(", ")));
        }
        Ok(RepositoryState {
            root,
            head: head.trim().to_owned(),
        })
    }

    fn is_repository_root(&self, root: &Path) -> Result<bool, GitError> {
        let output = self.run_status(
            root,
            "discovering the repository",
            [
                OsString::from("rev-parse"),
                OsString::from("--show-toplevel"),
            ],
        )?;
        if !output.status.success() {
            return Ok(false);
        }
        let discovered = String::from_utf8_lossy(&output.stdout);
        let discovered = std::fs::canonicalize(discovered.trim())?;
        Ok(discovered == root)
    }

    fn init_empty_repository(&self, root: &Path) -> Result<(), GitError> {
        let mut entries = std::fs::read_dir(root)?;
        if entries.next().transpose()?.is_some() {
            return Err(GitError::UnsupportedRepository(
                "choose an empty folder or an existing Git repository".into(),
            ));
        }
        self.run(
            root,
            "initializing the repository",
            [OsString::from("init")],
        )?;
        // Local identity is only for the bootstrap empty commit so HEAD exists.
        if self.require_identity(root).is_err() {
            self.run(
                root,
                "configuring local Git identity",
                [
                    OsString::from("config"),
                    OsString::from("user.name"),
                    OsString::from("Agent Factory"),
                ],
            )?;
            self.run(
                root,
                "configuring local Git identity",
                [
                    OsString::from("config"),
                    OsString::from("user.email"),
                    OsString::from("agent-factory@localhost"),
                ],
            )?;
        }
        self.run(
            root,
            "creating the initial commit",
            [
                OsString::from("commit"),
                OsString::from("--allow-empty"),
                OsString::from("-m"),
                OsString::from("Initial commit"),
            ],
        )?;
        Ok(())
    }

    pub fn status(
        &self,
        worktree: &Path,
        include_ignored: bool,
    ) -> Result<WorktreeStatus, GitError> {
        let mut args = vec![
            OsString::from("status"),
            OsString::from("--porcelain=v1"),
            OsString::from("--untracked-files=all"),
        ];
        if include_ignored {
            args.push(OsString::from("--ignored=matching"));
        }
        let output = self.run(worktree, "checking Draft changes", args)?;
        Ok(WorktreeStatus {
            entries: output.lines().map(str::to_owned).collect(),
        })
    }

    pub fn has_substantive_changes(&self, worktree: &Path) -> Result<bool, GitError> {
        Ok(!self.status(worktree, false)?.is_clean())
    }

    pub fn has_changes_except_manifest(&self, worktree: &Path) -> Result<bool, GitError> {
        Ok(self
            .status(worktree, false)?
            .entries
            .into_iter()
            .any(|entry| {
                let path = status_path(&entry);
                path != ".agent-factory/target-agent.json"
                    && !is_untracked_runtime_local_entry(&entry)
            }))
    }

    /// Observe the checkout relative to the immutable commit that started a
    /// Factory Run. This includes committed, staged, unstaged, and untracked
    /// paths without copying file contents into Agent Factory's ledger.
    pub fn changes_since(
        &self,
        worktree: &Path,
        starting_commit: &str,
    ) -> Result<Vec<WorkingTreeChange>, GitError> {
        validate_object_id(starting_commit)?;
        let mut changes = BTreeMap::new();
        let tracked = self.run_bytes(
            worktree,
            "observing Run changes",
            [
                OsString::from("diff"),
                OsString::from("--name-status"),
                OsString::from("--no-renames"),
                OsString::from("-z"),
                OsString::from(starting_commit),
                OsString::from("--"),
            ],
            MAX_OUTPUT_BYTES,
        )?;
        let mut fields = tracked
            .split(|byte| *byte == 0)
            .filter(|field| !field.is_empty());
        while let Some(status) = fields.next() {
            let path = fields.next().ok_or_else(|| {
                GitError::InvalidTree("Git returned an incomplete name-status record".into())
            })?;
            let path = String::from_utf8(path.to_vec())
                .map_err(|_| GitError::InvalidPath("a changed path is not UTF-8".into()))?;
            let kind = match status.first().copied() {
                Some(b'A') => WorkingTreeChangeKind::Added,
                Some(b'D') => WorkingTreeChangeKind::Deleted,
                Some(b'M' | b'T' | b'U') => WorkingTreeChangeKind::Modified,
                Some(other) => {
                    return Err(GitError::InvalidTree(format!(
                        "unsupported name-status code `{}`",
                        char::from(other),
                    )));
                }
                None => {
                    return Err(GitError::InvalidTree(
                        "Git returned an empty name-status code".into(),
                    ));
                }
            };
            changes.insert(path, kind);
        }

        let untracked = self.run_bytes(
            worktree,
            "observing untracked Run files",
            [
                OsString::from("ls-files"),
                OsString::from("--others"),
                OsString::from("--exclude-standard"),
                OsString::from("-z"),
            ],
            MAX_OUTPUT_BYTES,
        )?;
        for path in untracked
            .split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
        {
            let path = String::from_utf8(path.to_vec())
                .map_err(|_| GitError::InvalidPath("an untracked path is not UTF-8".into()))?;
            changes.insert(path, WorkingTreeChangeKind::Added);
        }

        Ok(changes
            .into_iter()
            .map(|(path, kind)| WorkingTreeChange { path, kind })
            .collect())
    }

    pub fn publish(
        &self,
        worktree: &Path,
        tag: &str,
        message: &str,
    ) -> Result<PublishedCommit, GitError> {
        validate_ref(tag)?;
        self.require_identity(worktree)?;
        if self.ref_exists(worktree, &format!("refs/tags/{tag}"))? {
            return Err(GitError::ReferenceCollision(tag.into()));
        }
        self.run(
            worktree,
            "staging Draft files",
            [
                OsString::from("add"),
                OsString::from("--all"),
                OsString::from("--"),
                OsString::from("."),
            ],
        )?;
        let staged = self.run_status(
            worktree,
            "checking staged Draft changes",
            [
                OsString::from("diff"),
                OsString::from("--cached"),
                OsString::from("--quiet"),
                OsString::from("--exit-code"),
            ],
        )?;
        if staged.status.success() {
            return Err(GitError::UnsupportedRepository(
                "the Draft has no substantive changes to publish".into(),
            ));
        }
        if staged.status.code() != Some(1) {
            return Err(command_error("checking staged Draft changes", &staged));
        }
        self.run(
            worktree,
            "committing the Draft",
            [
                OsString::from("commit"),
                OsString::from("--no-gpg-sign"),
                OsString::from("-m"),
                OsString::from(message),
            ],
        )?;
        let commit = self.head(worktree)?;
        self.run(
            worktree,
            "tagging the Version",
            [
                OsString::from("tag"),
                OsString::from("--annotate"),
                OsString::from("--no-sign"),
                OsString::from(tag),
                OsString::from("-m"),
                OsString::from(message),
                OsString::from(&commit),
            ],
        )?;
        Ok(PublishedCommit {
            commit,
            tag: tag.into(),
        })
    }

    /// Validate that Herdr may remove this checkout. Git remains the authority
    /// on cleanliness; the actual worktree operation belongs to Herdr.
    pub fn prepare_clean_worktree_removal(&self, worktree: &Path) -> Result<(), GitError> {
        self.remove_runtime_local_data(worktree)?;
        let status = self.status(worktree, true)?;
        if !status.is_clean() {
            return Err(GitError::DirtyWorktree(status.entries.join(", ")));
        }
        Ok(())
    }

    /// Remove only Agent Factory's disposable local definition and validate
    /// that no user-authored data would be lost. Herdr performs the removal.
    pub fn prepare_draft_discard(&self, worktree: &Path) -> Result<(), GitError> {
        self.remove_runtime_local_data(worktree)?;
        let status = self.status(worktree, true)?;
        let remaining = status
            .entries
            .into_iter()
            .filter(|entry| {
                let path = entry.get(3..).unwrap_or(entry.as_str());
                path != ".agent-factory/target-agent.json" && path != ".agent-factory/"
            })
            .collect::<Vec<_>>();
        if !remaining.is_empty() {
            return Err(GitError::DirtyWorktree(remaining.join(", ")));
        }
        let ignored = self.run(
            worktree,
            "checking ignored Draft data",
            [
                OsString::from("ls-files"),
                OsString::from("--others"),
                OsString::from("--ignored"),
                OsString::from("--exclude-standard"),
                OsString::from("--"),
                OsString::from(".agent-factory"),
            ],
        )?;
        let ignored = ignored
            .lines()
            .filter(|path| *path != ".agent-factory/target-agent.json")
            .collect::<Vec<_>>();
        if !ignored.is_empty() {
            return Err(GitError::DirtyWorktree(ignored.join(", ")));
        }
        let manifest = worktree.join(".agent-factory/target-agent.json");
        let tracked = self.run_status(
            worktree,
            "checking the Draft manifest",
            [
                OsString::from("ls-files"),
                OsString::from("--error-unmatch"),
                OsString::from("--"),
                OsString::from(".agent-factory/target-agent.json"),
            ],
        )?;
        if tracked.status.success() {
            self.run(
                worktree,
                "restoring the Draft manifest",
                [
                    OsString::from("restore"),
                    OsString::from("--staged"),
                    OsString::from("--worktree"),
                    OsString::from("--source=HEAD"),
                    OsString::from("--"),
                    OsString::from(".agent-factory/target-agent.json"),
                ],
            )?;
        } else if manifest.exists() {
            std::fs::remove_file(&manifest)?;
            if let Some(parent) = manifest.parent() {
                let _ = std::fs::remove_dir(parent);
            }
        }
        self.prepare_clean_worktree_removal(worktree)
    }

    /// Remove only harness memory and run-scoped coordination files that are
    /// not tracked by Git. The portable target manifest and every unknown file
    /// under `.agent-factory` remain protected by the normal dirty-worktree
    /// checks.
    fn remove_runtime_local_data(&self, worktree: &Path) -> Result<(), GitError> {
        self.remove_untracked_desktop_and_herdr_data(worktree)?;

        let tracked_remember = self.run(
            worktree,
            "checking tracked harness memory",
            [
                OsString::from("ls-files"),
                OsString::from("--"),
                OsString::from(".remember"),
            ],
        )?;
        let remember = worktree.join(".remember");
        if tracked_remember.trim().is_empty() && remember.exists() {
            let metadata = std::fs::symlink_metadata(&remember)?;
            if metadata.file_type().is_symlink() || metadata.is_file() {
                std::fs::remove_file(&remember)?;
            } else if metadata.is_dir() {
                std::fs::remove_dir_all(&remember)?;
            }
        }

        let coordination = worktree.join(".agent-factory");
        let Ok(entries) = std::fs::read_dir(&coordination) else {
            return Ok(());
        };
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !is_run_coordination_file(&name) {
                continue;
            }
            let relative = format!(".agent-factory/{name}");
            let tracked = self.run_status(
                worktree,
                "checking tracked run coordination data",
                [
                    OsString::from("ls-files"),
                    OsString::from("--error-unmatch"),
                    OsString::from("--"),
                    OsString::from(&relative),
                ],
            )?;
            if tracked.status.success() {
                continue;
            }
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() || metadata.is_file() {
                std::fs::remove_file(path)?;
            }
        }
        Ok(())
    }

    /// Remove only untracked files that macOS or Herdr installs as local
    /// workspace integration. These paths are not target-agent source, but a
    /// tracked file at the same location remains protected like any other
    /// authored repository content.
    fn remove_untracked_desktop_and_herdr_data(&self, worktree: &Path) -> Result<(), GitError> {
        let entries = self.status(worktree, true)?.entries;
        for entry in entries {
            if !(entry.starts_with("?? ") || entry.starts_with("!! ")) {
                continue;
            }
            let relative = status_path(&entry);
            if !is_disposable_desktop_or_herdr_path(relative) {
                continue;
            }
            let relative_path = Path::new(relative);
            if relative_path.is_absolute()
                || relative_path
                    .components()
                    .any(|component| !matches!(component, Component::Normal(_)))
            {
                return Err(GitError::InvalidPath(relative.into()));
            }
            let tracked = self.run(
                worktree,
                "checking tracked workspace integration data",
                [
                    OsString::from("ls-files"),
                    OsString::from("--"),
                    OsString::from(relative),
                ],
            )?;
            if !tracked.trim().is_empty() {
                continue;
            }
            let path = worktree.join(relative_path);
            let metadata = match std::fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            if metadata.file_type().is_symlink() || metadata.is_file() {
                std::fs::remove_file(path)?;
            } else if metadata.is_dir() && is_herdr_integration_path(relative) {
                std::fs::remove_dir_all(path)?;
            }
        }
        Ok(())
    }

    /// Lists the immutable tree stored at an exact commit object ID.
    ///
    /// The caller supplies a repository root selected from durable application
    /// state. This API intentionally accepts only a full object ID, not an
    /// arbitrary ref, revision expression, or working-tree path.
    pub fn list_commit_files(
        &self,
        repository: &Path,
        commit: &str,
    ) -> Result<Vec<CommitTreeEntry>, GitError> {
        validate_object_id(commit)?;
        let output = self.run_bytes(
            repository,
            "reading an immutable Version tree",
            [
                OsString::from("ls-tree"),
                OsString::from("-r"),
                OsString::from("-z"),
                OsString::from("-l"),
                OsString::from(commit),
                OsString::from("--"),
            ],
            MAX_VERSION_TREE_BYTES,
        )?;
        parse_commit_tree(&output)
    }

    /// Reads one blob selected from the immutable tree at `commit`.
    ///
    /// The path is matched against the listed tree first, then `cat-file`
    /// receives only the tree-provided object ID. User input therefore never
    /// becomes a Git revision expression or filesystem path.
    pub fn read_commit_file(
        &self,
        repository: &Path,
        commit: &str,
        path: &str,
    ) -> Result<CommitFile, GitError> {
        validate_version_path(path)?;
        let entry = self
            .list_commit_files(repository, commit)?
            .into_iter()
            .find(|entry| entry.path == path)
            .ok_or_else(|| GitError::InvalidVersionPath(path.to_owned()))?;
        if entry.kind == CommitTreeEntryKind::Submodule {
            return Ok(CommitFile {
                path: entry.path,
                size: None,
                kind: CommitFileKind::Unsupported,
                content: None,
            });
        }
        let size = entry
            .size
            .ok_or_else(|| GitError::InvalidTree(format!("blob `{}` has no size", entry.path)))?;
        if size > MAX_VERSION_FILE_BYTES as u64 {
            return Ok(CommitFile {
                path: entry.path,
                size: Some(size),
                kind: CommitFileKind::TooLarge,
                content: None,
            });
        }
        let bytes = self.run_bytes(
            repository,
            "reading an immutable Version file",
            [
                OsString::from("cat-file"),
                OsString::from("blob"),
                OsString::from(&entry.object_id),
            ],
            MAX_VERSION_FILE_BYTES,
        )?;
        if bytes.len() as u64 != size {
            return Err(GitError::InvalidTree(format!(
                "blob `{}` size changed while reading",
                entry.path,
            )));
        }
        if bytes.iter().take(8 * 1024).any(|byte| *byte == 0) {
            return Ok(CommitFile {
                path: entry.path,
                size: Some(size),
                kind: CommitFileKind::Binary,
                content: None,
            });
        }
        let content = match String::from_utf8(bytes) {
            Ok(content) => content,
            Err(_) => {
                return Ok(CommitFile {
                    path: entry.path,
                    size: Some(size),
                    kind: CommitFileKind::Binary,
                    content: None,
                });
            }
        };
        Ok(CommitFile {
            path: entry.path,
            size: Some(size),
            kind: CommitFileKind::Text,
            content: Some(content),
        })
    }

    pub fn head(&self, worktree: &Path) -> Result<String, GitError> {
        Ok(self
            .run(
                worktree,
                "resolving Draft HEAD",
                [
                    OsString::from("rev-parse"),
                    OsString::from("--verify"),
                    OsString::from("HEAD^{commit}"),
                ],
            )?
            .trim()
            .to_owned())
    }

    pub fn resolve_ref(&self, repository: &Path, reference: &str) -> Result<String, GitError> {
        validate_ref(reference)?;
        Ok(self
            .run(
                repository,
                "resolving a Git reference",
                [
                    OsString::from("rev-parse"),
                    OsString::from("--verify"),
                    OsString::from(format!("{reference}^{{commit}}")),
                ],
            )?
            .trim()
            .to_owned())
    }

    pub fn tag_commit(
        &self,
        repository: &Path,
        tag: &str,
        commit: &str,
        message: &str,
    ) -> Result<(), GitError> {
        validate_ref(tag)?;
        validate_ref(commit)?;
        self.require_identity(repository)?;
        if self.ref_exists(repository, &format!("refs/tags/{tag}"))? {
            return Err(GitError::ReferenceCollision(tag.into()));
        }
        self.run(
            repository,
            "tagging the recovered Version",
            [
                OsString::from("tag"),
                OsString::from("--annotate"),
                OsString::from("--no-sign"),
                OsString::from(tag),
                OsString::from("-m"),
                OsString::from(message),
                OsString::from(commit),
            ],
        )?;
        Ok(())
    }

    pub fn ref_exists(&self, repository: &Path, reference: &str) -> Result<bool, GitError> {
        validate_ref(reference)?;
        let output = self.run_status(
            repository,
            "checking a Git reference",
            [
                OsString::from("show-ref"),
                OsString::from("--verify"),
                OsString::from("--quiet"),
                OsString::from(reference),
            ],
        )?;
        match output.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(command_error("checking a Git reference", &output)),
        }
    }

    fn require_identity(&self, repository: &Path) -> Result<(), GitError> {
        for key in ["user.name", "user.email"] {
            let result = self.run_status(
                repository,
                "checking Git identity",
                [
                    OsString::from("config"),
                    OsString::from("--get"),
                    OsString::from(key),
                ],
            )?;
            if !result.status.success() || result.stdout.is_empty() {
                return Err(GitError::MissingIdentity);
            }
        }
        Ok(())
    }

    fn require_no_operation(&self, root: &Path) -> Result<(), GitError> {
        let git_dir = self.run(
            root,
            "locating Git state",
            [
                OsString::from("rev-parse"),
                OsString::from("--absolute-git-dir"),
            ],
        )?;
        let git_dir = PathBuf::from(git_dir.trim());
        for marker in [
            "MERGE_HEAD",
            "CHERRY_PICK_HEAD",
            "REVERT_HEAD",
            "BISECT_LOG",
            "rebase-apply",
            "rebase-merge",
        ] {
            if git_dir.join(marker).exists() {
                return Err(GitError::UnsupportedRepository(format!(
                    "finish the active Git operation ({marker}) first",
                )));
            }
        }
        Ok(())
    }

    fn run<I>(&self, cwd: &Path, operation: &'static str, args: I) -> Result<String, GitError>
    where
        I: IntoIterator<Item = OsString>,
    {
        let output = self.run_status(cwd, operation, args)?;
        if !output.status.success() {
            return Err(command_error(operation, &output));
        }
        if output.stdout.len() > MAX_OUTPUT_BYTES || output.stderr.len() > MAX_OUTPUT_BYTES {
            return Err(GitError::OutputTooLarge(operation));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }

    fn run_bytes<I>(
        &self,
        cwd: &Path,
        operation: &'static str,
        args: I,
        max_output_bytes: usize,
    ) -> Result<Vec<u8>, GitError>
    where
        I: IntoIterator<Item = OsString>,
    {
        let output = self.run_status_with_limit(cwd, operation, args, max_output_bytes)?;
        if !output.status.success() {
            return Err(command_error(operation, &output));
        }
        Ok(output.stdout)
    }

    fn run_status<I>(
        &self,
        cwd: &Path,
        operation: &'static str,
        args: I,
    ) -> Result<std::process::Output, GitError>
    where
        I: IntoIterator<Item = OsString>,
    {
        self.run_status_with_limit(cwd, operation, args, MAX_OUTPUT_BYTES)
    }

    fn run_status_with_limit<I>(
        &self,
        cwd: &Path,
        operation: &'static str,
        args: I,
        max_output_bytes: usize,
    ) -> Result<std::process::Output, GitError>
    where
        I: IntoIterator<Item = OsString>,
    {
        let mut child = Command::new(&self.executable)
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    GitError::MissingGit
                } else {
                    GitError::Io(error)
                }
            })?;
        let started = Instant::now();
        loop {
            if child.try_wait()?.is_some() {
                let output = child.wait_with_output()?;
                if output.stdout.len() > max_output_bytes || output.stderr.len() > MAX_OUTPUT_BYTES
                {
                    return Err(GitError::OutputTooLarge(operation));
                }
                return Ok(output);
            }
            if started.elapsed() >= self.timeout {
                let _ = child.kill();
                let _ = child.wait();
                return Err(GitError::Timeout(operation));
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

fn status_path(entry: &str) -> &str {
    entry.get(3..).unwrap_or(entry)
}

fn is_untracked_runtime_local_entry(entry: &str) -> bool {
    let path = status_path(entry);
    ((entry.starts_with("?? ") || entry.starts_with("!! ")) && is_runtime_local_path(path))
        || (entry.starts_with("!! ") && path.starts_with(".agent-factory/"))
}

fn is_runtime_local_path(path: &str) -> bool {
    path == ".remember"
        || path.starts_with(".remember/")
        || is_disposable_desktop_or_herdr_path(path)
        || path
            .strip_prefix(".agent-factory/")
            .is_some_and(is_run_coordination_file)
}

fn is_disposable_desktop_or_herdr_path(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .is_some_and(|name| name == ".DS_Store")
        || is_herdr_integration_path(path)
}

fn is_herdr_integration_path(path: &str) -> bool {
    [".agents/skills/herdr", ".claude/skills/herdr"]
        .iter()
        .any(|root| path == *root || path.starts_with(&format!("{root}/")))
}

fn is_run_coordination_file(name: &str) -> bool {
    (name.starts_with("orchestrator-decision-") || name.starts_with("verdict-"))
        && name.ends_with(".json")
}

fn validate_ref(reference: &str) -> Result<(), GitError> {
    if reference.is_empty()
        || reference.starts_with('-')
        || reference.contains("..")
        || reference.contains("@{")
        || reference
            .bytes()
            .any(|byte| byte <= b' ' || b"~^:?*[\\".contains(&byte))
    {
        return Err(GitError::UnsupportedRepository(format!(
            "invalid Git reference `{reference}`",
        )));
    }
    Ok(())
}

fn validate_object_id(object_id: &str) -> Result<(), GitError> {
    if !matches!(object_id.len(), 40 | 64)
        || !object_id.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(GitError::InvalidObjectId(object_id.to_owned()));
    }
    Ok(())
}

fn validate_version_path(path: &str) -> Result<(), GitError> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\0')
        || path
            .split('/')
            .any(|component| component.is_empty() || matches!(component, "." | ".."))
    {
        return Err(GitError::InvalidVersionPath(path.to_owned()));
    }
    Ok(())
}

fn parse_commit_tree(output: &[u8]) -> Result<Vec<CommitTreeEntry>, GitError> {
    let mut entries = Vec::new();
    for record in output
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let tab = record
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or_else(|| {
                GitError::InvalidTree("tree entry is missing its path separator".into())
            })?;
        let header = std::str::from_utf8(&record[..tab])
            .map_err(|_| GitError::InvalidTree("tree header is not UTF-8".into()))?;
        let mut fields = header.split_ascii_whitespace();
        let mode = fields
            .next()
            .ok_or_else(|| GitError::InvalidTree("tree entry is missing its mode".into()))?;
        let object_type = fields
            .next()
            .ok_or_else(|| GitError::InvalidTree("tree entry is missing its type".into()))?;
        let object_id = fields
            .next()
            .ok_or_else(|| GitError::InvalidTree("tree entry is missing its object ID".into()))?;
        validate_object_id(object_id)?;
        let size_field = fields
            .next()
            .ok_or_else(|| GitError::InvalidTree("tree entry is missing its size".into()))?;
        if fields.next().is_some() {
            return Err(GitError::InvalidTree(
                "tree entry contains unexpected metadata".into(),
            ));
        }
        let kind = match (mode, object_type) {
            ("120000", "blob") => CommitTreeEntryKind::Symlink,
            ("160000", "commit") => CommitTreeEntryKind::Submodule,
            (_, "blob") => CommitTreeEntryKind::File,
            _ => {
                return Err(GitError::InvalidTree(format!(
                    "unsupported tree entry mode `{mode}` and type `{object_type}`",
                )));
            }
        };
        let size = if size_field == "-" {
            None
        } else {
            Some(size_field.parse::<u64>().map_err(|_| {
                GitError::InvalidTree(format!("invalid tree entry size `{size_field}`"))
            })?)
        };
        if kind != CommitTreeEntryKind::Submodule && size.is_none() {
            return Err(GitError::InvalidTree(format!(
                "blob `{object_id}` is missing its size",
            )));
        }
        let path = String::from_utf8(record[tab + 1..].to_vec())
            .map_err(|_| GitError::InvalidTree("tree path is not UTF-8".into()))?;
        validate_version_path(&path)?;
        entries.push(CommitTreeEntry {
            path,
            kind,
            size,
            object_id: object_id.to_owned(),
        });
    }
    Ok(entries)
}

fn command_error(operation: &'static str, output: &std::process::Output) -> GitError {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    GitError::Command {
        operation,
        message: if stderr.is_empty() { stdout } else { stderr },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn git(cwd: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    }

    fn repository() -> (TempDir, PathBuf) {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("source");
        std::fs::create_dir(&root).unwrap();
        git(&root, &["init"]);
        git(&root, &["config", "user.name", "Agent Factory Tests"]);
        git(&root, &["config", "user.email", "tests@example.invalid"]);
        std::fs::write(root.join("README.md"), "base\n").unwrap();
        std::fs::write(root.join("obsolete.md"), "remove me\n").unwrap();
        git(&root, &["add", "README.md", "obsolete.md"]);
        git(&root, &["commit", "-m", "base"]);
        (temp, root)
    }

    fn create_worktree(repository: &Path, worktree: &Path, branch: &str) {
        let status = Command::new("git")
            .args(["worktree", "add", "-b", branch])
            .arg(worktree)
            .arg("HEAD")
            .current_dir(repository)
            .status()
            .unwrap();
        assert!(status.success(), "Git fixture could not create {branch}");
    }

    #[test]
    fn preflight_requires_a_clean_committed_repository() {
        let (_temp, root) = repository();
        let runtime = GitRuntime::default();
        assert_eq!(
            runtime.preflight(&root).unwrap().root,
            root.canonicalize().unwrap()
        );
        std::fs::create_dir(root.join(".remember")).unwrap();
        std::fs::write(root.join(".remember/now.md"), "harness memory").unwrap();
        assert_eq!(
            runtime.preflight(&root).unwrap().root,
            root.canonicalize().unwrap(),
            "untracked harness memory must not block test setup"
        );
        std::fs::create_dir_all(root.join(".agent-factory/worktrees/draft")).unwrap();
        std::fs::write(root.join(".agent-factory/worktrees/.gitignore"), "*\n").unwrap();
        std::fs::write(
            root.join(".agent-factory/worktrees/draft/README.md"),
            "nested Draft",
        )
        .unwrap();
        assert_eq!(
            runtime.preflight(&root).unwrap().root,
            root.canonicalize().unwrap(),
            "ignored Agent Factory runtime data must not block new Drafts"
        );
        std::fs::write(root.join(".DS_Store"), "Finder metadata").unwrap();
        std::fs::create_dir_all(root.join(".agents/skills/herdr")).unwrap();
        std::fs::write(
            root.join(".agents/skills/herdr/SKILL.md"),
            "Herdr integration",
        )
        .unwrap();
        std::fs::create_dir_all(root.join(".claude/skills/herdr")).unwrap();
        std::fs::write(root.join(".claude/.DS_Store"), "Finder metadata").unwrap();
        std::fs::write(
            root.join(".claude/skills/herdr/SKILL.md"),
            "Herdr integration",
        )
        .unwrap();
        assert_eq!(
            runtime.preflight(&root).unwrap().root,
            root.canonicalize().unwrap(),
            "untracked desktop and Herdr integration data must not block a Draft"
        );
        std::fs::write(root.join("dirty.txt"), "dirty").unwrap();
        assert!(matches!(
            runtime.preflight(&root),
            Err(GitError::DirtyWorktree(_))
        ));
    }

    #[test]
    fn tracked_herdr_skill_changes_remain_protected() {
        let (_temp, root) = repository();
        let runtime = GitRuntime::default();
        std::fs::create_dir_all(root.join(".agents/skills/herdr")).unwrap();
        std::fs::write(
            root.join(".agents/skills/herdr/SKILL.md"),
            "tracked integration\n",
        )
        .unwrap();
        git(&root, &["add", ".agents/skills/herdr/SKILL.md"]);
        git(&root, &["commit", "-m", "track Herdr integration"]);
        std::fs::write(
            root.join(".agents/skills/herdr/SKILL.md"),
            "user modification\n",
        )
        .unwrap();

        assert!(matches!(
            runtime.preflight(&root),
            Err(GitError::DirtyWorktree(_))
        ));
    }

    #[test]
    fn ensure_repository_initializes_an_empty_folder() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("empty-workspace");
        std::fs::create_dir(&root).unwrap();
        let runtime = GitRuntime::default();
        let state = runtime.ensure_repository(&root).unwrap();
        assert_eq!(state.root, root.canonicalize().unwrap());
        assert!(!state.head.is_empty());
        // Second call reuses the initialized repository.
        assert_eq!(runtime.ensure_repository(&root).unwrap().head, state.head);
    }

    #[test]
    fn ensure_repository_rejects_non_empty_non_git_folders() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("notes");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("README.md"), "notes").unwrap();
        let runtime = GitRuntime::default();
        assert!(matches!(
            runtime.ensure_repository(&root),
            Err(GitError::UnsupportedRepository(_))
        ));
    }

    #[test]
    fn observes_committed_staged_unstaged_and_untracked_changes() {
        let (temp, root) = repository();
        let runtime = GitRuntime::default();
        let worktree = temp.path().join("draft");
        create_worktree(&root, &worktree, "agent-factory/a/drafts/one");
        let starting_commit = runtime.head(&worktree).unwrap();

        std::fs::write(worktree.join("README.md"), "changed\n").unwrap();
        std::fs::remove_file(worktree.join("obsolete.md")).unwrap();
        std::fs::write(worktree.join("staged.md"), "staged\n").unwrap();
        git(&worktree, &["add", "staged.md"]);
        std::fs::write(worktree.join("untracked.md"), "untracked\n").unwrap();

        assert_eq!(
            runtime.changes_since(&worktree, &starting_commit).unwrap(),
            vec![
                WorkingTreeChange {
                    path: "README.md".into(),
                    kind: WorkingTreeChangeKind::Modified,
                },
                WorkingTreeChange {
                    path: "obsolete.md".into(),
                    kind: WorkingTreeChangeKind::Deleted,
                },
                WorkingTreeChange {
                    path: "staged.md".into(),
                    kind: WorkingTreeChangeKind::Added,
                },
                WorkingTreeChange {
                    path: "untracked.md".into(),
                    kind: WorkingTreeChangeKind::Added,
                },
            ]
        );
    }

    #[test]
    fn tag_collisions_are_rejected() {
        let (_temp, root) = repository();
        let runtime = GitRuntime::default();
        std::fs::write(root.join("agent.md"), "draft one").unwrap();
        let published = runtime
            .publish(&root, "agent-factory/a/v0.1.0", "Create Agent v0.1.0")
            .unwrap();
        assert!(matches!(
            runtime.tag_commit(
                &root,
                "agent-factory/a/v0.1.0",
                &published.commit,
                "duplicate",
            ),
            Err(GitError::ReferenceCollision(_))
        ));
    }

    #[test]
    fn discard_preparation_removes_only_factory_owned_local_data() {
        let (temp, root) = repository();
        let runtime = GitRuntime::default();
        let clean = temp.path().join("clean-draft");
        create_worktree(&root, &clean, "agent-factory/a/drafts/clean");
        std::fs::create_dir(clean.join(".agent-factory")).unwrap();
        std::fs::write(clean.join(".agent-factory/target-agent.json"), "{}").unwrap();
        std::fs::write(
            clean.join(".agent-factory/orchestrator-decision-run.json"),
            "{}",
        )
        .unwrap();
        std::fs::write(clean.join(".agent-factory/verdict-session.json"), "{}").unwrap();
        std::fs::create_dir(clean.join(".remember")).unwrap();
        std::fs::write(clean.join(".remember/now.md"), "harness memory").unwrap();
        std::fs::write(clean.join(".DS_Store"), "Finder metadata").unwrap();
        std::fs::create_dir_all(clean.join(".agents/skills/herdr")).unwrap();
        std::fs::write(
            clean.join(".agents/skills/herdr/SKILL.md"),
            "Herdr integration",
        )
        .unwrap();
        std::fs::create_dir_all(clean.join(".claude/skills/herdr")).unwrap();
        std::fs::write(clean.join(".claude/.DS_Store"), "Finder metadata").unwrap();
        std::fs::write(
            clean.join(".claude/skills/herdr/SKILL.md"),
            "Herdr integration",
        )
        .unwrap();
        assert!(
            !runtime.has_changes_except_manifest(&clean).unwrap(),
            "runtime-local files are not authored target changes"
        );
        runtime.prepare_draft_discard(&clean).unwrap();
        assert!(
            clean.exists(),
            "Herdr, not GitRuntime, removes the worktree"
        );
        assert!(!clean.join(".agent-factory/target-agent.json").exists());
        assert!(!clean.join(".remember").exists());
        assert!(!clean.join(".DS_Store").exists());
        assert!(!clean.join(".agents/skills/herdr/SKILL.md").exists());
        assert!(!clean.join(".claude/.DS_Store").exists());
        assert!(!clean.join(".claude/skills/herdr/SKILL.md").exists());
        assert!(runtime.status(&clean, true).unwrap().is_clean());

        std::fs::write(root.join(".gitignore"), ".agent-factory/cache\n").unwrap();
        git(&root, &["add", ".gitignore"]);
        git(&root, &["commit", "-m", "ignore cache"]);
        let dirty = temp.path().join("dirty-draft");
        create_worktree(&root, &dirty, "agent-factory/a/drafts/dirty");
        std::fs::create_dir_all(dirty.join(".agent-factory/cache")).unwrap();
        std::fs::write(dirty.join(".agent-factory/cache/state"), "keep").unwrap();
        assert!(matches!(
            runtime.prepare_draft_discard(&dirty),
            Err(GitError::DirtyWorktree(_))
        ));
    }
}
