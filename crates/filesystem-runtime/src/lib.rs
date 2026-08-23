//! Read-only, trusted-root-confined filesystem operations.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};
use thiserror::Error;

pub const DEFAULT_PAGE_SIZE: usize = 100;
pub const MAX_PAGE_SIZE: usize = 500;
pub const DEFAULT_READ_BYTES: usize = 256 * 1024;
pub const MAX_READ_BYTES: usize = 1024 * 1024;

pub struct FileSystem {
    roots: Vec<PathBuf>,
}

impl FileSystem {
    pub fn new(roots: impl IntoIterator<Item = PathBuf>) -> Result<Self, FileError> {
        let mut canonical = Vec::new();
        for root in roots {
            if !root.is_absolute() || !root.is_dir() {
                return Err(FileError::InvalidRoot(root));
            }
            canonical.push(fs::canonicalize(root)?);
        }
        canonical.sort();
        canonical.dedup();
        Ok(Self { roots: canonical })
    }

    pub fn authorize(&self, path: &Path) -> Result<PathBuf, FileError> {
        if !path.is_absolute() {
            return Err(FileError::PathMustBeAbsolute);
        }
        let canonical = fs::canonicalize(path)?;
        if !self.roots.iter().any(|root| canonical.starts_with(root)) {
            return Err(FileError::OutsideTrustedRoot(canonical));
        }
        Ok(canonical)
    }

    pub fn list(
        &self,
        path: &Path,
        cursor: Option<&str>,
        page_size: usize,
    ) -> Result<DirectoryPage, FileError> {
        let path = self.authorize(path)?;
        if !path.is_dir() {
            return Err(FileError::NotDirectory(path));
        }
        if page_size == 0 || page_size > MAX_PAGE_SIZE {
            return Err(FileError::InvalidPageSize(page_size));
        }
        let offset = match cursor {
            Some(value) => value
                .parse::<usize>()
                .map_err(|_| FileError::InvalidCursor(value.into()))?,
            None => 0,
        };

        let mut paths = fs::read_dir(&path)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()?;
        paths.sort_by(|left, right| {
            left.file_name()
                .cmp(&right.file_name())
                .then_with(|| left.cmp(right))
        });
        if offset > paths.len() {
            return Err(FileError::InvalidCursor(offset.to_string()));
        }

        let end = (offset + page_size).min(paths.len());
        let mut entries = Vec::with_capacity(end.saturating_sub(offset));
        for entry_path in &paths[offset..end] {
            let metadata = fs::symlink_metadata(entry_path)?;
            let canonical = self.authorize(entry_path)?;
            let kind = if metadata.file_type().is_symlink() {
                EntryKind::Symlink
            } else if metadata.is_dir() {
                EntryKind::Directory
            } else if metadata.is_file() {
                EntryKind::File
            } else {
                EntryKind::Other
            };
            entries.push(DirectoryEntry {
                name: entry_path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                path: canonical,
                kind,
                size: metadata.len(),
            });
        }

        Ok(DirectoryPage {
            path,
            entries,
            next_cursor: (end < paths.len()).then(|| end.to_string()),
        })
    }

    pub fn read_text(&self, path: &Path, max_bytes: usize) -> Result<FileRead, FileError> {
        let path = self.authorize(path)?;
        if !path.is_file() {
            return Err(FileError::NotFile(path));
        }
        if max_bytes == 0 || max_bytes > MAX_READ_BYTES {
            return Err(FileError::InvalidReadLimit(max_bytes));
        }
        let size = fs::metadata(&path)?.len();
        let mut bytes = Vec::with_capacity(max_bytes.saturating_add(4));
        File::open(&path)?
            .take(max_bytes.saturating_add(4) as u64)
            .read_to_end(&mut bytes)?;
        let truncated = size > max_bytes as u64;

        if bytes.iter().take(8 * 1024).any(|byte| *byte == 0) {
            return Ok(FileRead {
                path,
                size,
                kind: FileKind::Binary,
                content: None,
                truncated,
            });
        }

        let content = match std::str::from_utf8(&bytes) {
            Ok(text) => {
                let mut end = max_bytes.min(text.len());
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                text[..end].to_owned()
            }
            Err(error) if truncated && error.error_len().is_none() => {
                bytes.truncate(error.valid_up_to().min(max_bytes));
                String::from_utf8(bytes).expect("validated UTF-8 prefix")
            }
            Err(_) => {
                return Ok(FileRead {
                    path,
                    size,
                    kind: FileKind::Binary,
                    content: None,
                    truncated,
                });
            }
        };

        Ok(FileRead {
            path,
            size,
            kind: FileKind::Text,
            content: Some(content),
            truncated,
        })
    }

    pub fn write_text(&self, path: &Path, content: &str) -> Result<(), FileError> {
        if content.len() > MAX_READ_BYTES {
            return Err(FileError::InvalidWriteLimit(content.len()));
        }
        let path = self.authorize_write_path(path)?;
        let parent = path
            .parent()
            .ok_or_else(|| FileError::InvalidWritePath(path.clone()))?;
        let temporary = parent.join(format!(".agent-factory-write-{}", uuid::Uuid::new_v4()));
        let result = (|| {
            let mut file = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)?;
            file.write_all(content.as_bytes())?;
            file.sync_all()?;
            fs::rename(&temporary, &path)?;
            Ok::<(), std::io::Error>(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result.map_err(FileError::Io)
    }

    fn authorize_write_path(&self, path: &Path) -> Result<PathBuf, FileError> {
        if !path.is_absolute() {
            return Err(FileError::PathMustBeAbsolute);
        }
        if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err(FileError::SymlinkWriteRejected(path.to_path_buf()));
        }
        if path.exists() {
            let canonical = self.authorize(path)?;
            if !canonical.is_file() {
                return Err(FileError::NotFile(canonical));
            }
            return Ok(canonical);
        }
        let requested_parent = path
            .parent()
            .ok_or_else(|| FileError::InvalidWritePath(path.to_path_buf()))?;
        let parent = self.authorize(requested_parent)?;
        if !parent.is_dir() {
            return Err(FileError::NotDirectory(parent));
        }
        let name = path
            .file_name()
            .filter(|name| !name.is_empty())
            .ok_or_else(|| FileError::InvalidWritePath(path.to_path_buf()))?;
        Ok(parent.join(name))
    }

    pub fn diff(
        &self,
        before_path: &Path,
        after_path: &Path,
        context_lines: usize,
    ) -> Result<StructuredDiff, FileError> {
        if context_lines > 20 {
            return Err(FileError::InvalidContextLines(context_lines));
        }
        let before = self.read_text(before_path, MAX_READ_BYTES)?;
        let after = self.read_text(after_path, MAX_READ_BYTES)?;
        if before.kind == FileKind::Binary || after.kind == FileKind::Binary {
            return Err(FileError::BinaryDiffUnsupported);
        }
        if before.truncated || after.truncated {
            return Err(FileError::DiffTooLarge);
        }
        let before_content = before.content.as_deref().unwrap_or_default();
        let after_content = after.content.as_deref().unwrap_or_default();
        let diff = TextDiff::from_lines(before_content, after_content);
        let mut hunks = Vec::new();

        for operations in diff.grouped_ops(context_lines) {
            let first = operations.first().expect("grouped diff is non-empty");
            let last = operations.last().expect("grouped diff is non-empty");
            let old_start = first.old_range().start + 1;
            let new_start = first.new_range().start + 1;
            let old_lines = last.old_range().end.saturating_sub(first.old_range().start);
            let new_lines = last.new_range().end.saturating_sub(first.new_range().start);
            let mut lines = Vec::new();

            for operation in operations {
                for change in diff.iter_changes(&operation) {
                    lines.push(DiffLine {
                        kind: match change.tag() {
                            ChangeTag::Equal => DiffLineKind::Context,
                            ChangeTag::Delete => DiffLineKind::Delete,
                            ChangeTag::Insert => DiffLineKind::Insert,
                        },
                        old_line: change.old_index().map(|index| index + 1),
                        new_line: change.new_index().map(|index| index + 1),
                        text: change
                            .value()
                            .strip_suffix('\n')
                            .unwrap_or(change.value())
                            .into(),
                    });
                }
            }

            hunks.push(DiffHunk {
                old_start,
                old_lines,
                new_start,
                new_lines,
                lines,
            });
        }

        Ok(StructuredDiff {
            before_path: before.path,
            after_path: after.path,
            hunks,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryPage {
    pub path: PathBuf,
    pub entries: Vec<DirectoryEntry>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryEntry {
    pub name: String,
    pub path: PathBuf,
    pub kind: EntryKind,
    pub size: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    File,
    Directory,
    Symlink,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileRead {
    pub path: PathBuf,
    pub size: u64,
    pub kind: FileKind,
    pub content: Option<String>,
    pub truncated: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Text,
    Binary,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StructuredDiff {
    pub before_path: PathBuf,
    pub after_path: PathBuf,
    pub hunks: Vec<DiffHunk>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiffHunk {
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    pub lines: Vec<DiffLine>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub old_line: Option<usize>,
    pub new_line: Option<usize>,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiffLineKind {
    Context,
    Delete,
    Insert,
}

#[derive(Debug, Error)]
pub enum FileError {
    #[error("trusted root is not an existing absolute directory: {0}")]
    InvalidRoot(PathBuf),
    #[error("path must be absolute")]
    PathMustBeAbsolute,
    #[error("path is outside every trusted project root: {0}")]
    OutsideTrustedRoot(PathBuf),
    #[error("path is not a directory: {0}")]
    NotDirectory(PathBuf),
    #[error("path is not a regular file: {0}")]
    NotFile(PathBuf),
    #[error("invalid directory cursor `{0}`")]
    InvalidCursor(String),
    #[error("page size {0} is outside 1..={MAX_PAGE_SIZE}")]
    InvalidPageSize(usize),
    #[error("read limit {0} is outside 1..={MAX_READ_BYTES}")]
    InvalidReadLimit(usize),
    #[error("write size {0} exceeds the allowed limit")]
    InvalidWriteLimit(usize),
    #[error("write target is invalid: {0}")]
    InvalidWritePath(PathBuf),
    #[error("writing through a symbolic link is not allowed: {0}")]
    SymlinkWriteRejected(PathBuf),
    #[error("diff context {0} exceeds 20 lines")]
    InvalidContextLines(usize),
    #[error("binary files cannot be rendered as a text diff")]
    BinaryDiffUnsupported,
    #[error("file exceeds the maximum structured diff size")]
    DiffTooLarge,
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn rejects_symlinks_that_escape_a_trusted_root() {
        let trusted = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let secret = outside.path().join("secret.txt");
        fs::write(&secret, "secret").unwrap();
        let link = trusted.path().join("escape.txt");
        symlink(&secret, &link).unwrap();
        let files = FileSystem::new([trusted.path().to_path_buf()]).unwrap();

        assert!(matches!(
            files.read_text(&link, 100),
            Err(FileError::OutsideTrustedRoot(_))
        ));
    }

    #[test]
    fn writes_atomically_inside_the_root_and_rejects_symlink_targets() {
        let trusted = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let files = FileSystem::new([trusted.path().to_path_buf()]).unwrap();
        let target = trusted.path().join("new.txt");
        files.write_text(&target, "hello").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "hello");

        let outside_target = outside.path().join("outside.txt");
        fs::write(&outside_target, "secret").unwrap();
        let link = trusted.path().join("link.txt");
        symlink(&outside_target, &link).unwrap();
        assert!(matches!(
            files.write_text(&link, "overwrite"),
            Err(FileError::SymlinkWriteRejected(_))
        ));
        assert_eq!(fs::read_to_string(outside_target).unwrap(), "secret");
    }

    #[test]
    fn directory_listing_is_sorted_and_paginated() {
        let trusted = TempDir::new().unwrap();
        for name in ["c.txt", "a.txt", "b.txt"] {
            fs::write(trusted.path().join(name), name).unwrap();
        }
        let files = FileSystem::new([trusted.path().to_path_buf()]).unwrap();
        let first = files.list(trusted.path(), None, 2).unwrap();
        let second = files
            .list(trusted.path(), first.next_cursor.as_deref(), 2)
            .unwrap();

        assert_eq!(
            first
                .entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            ["a.txt", "b.txt"]
        );
        assert_eq!(second.entries[0].name, "c.txt");
        assert_eq!(second.next_cursor, None);
    }

    #[test]
    fn text_reads_are_bounded_without_splitting_unicode() {
        let trusted = TempDir::new().unwrap();
        let path = trusted.path().join("unicode.txt");
        fs::write(&path, "hello 🌍 trailing").unwrap();
        let files = FileSystem::new([trusted.path().to_path_buf()]).unwrap();
        let read = files.read_text(&path, 8).unwrap();

        assert_eq!(read.kind, FileKind::Text);
        assert_eq!(read.content.as_deref(), Some("hello "));
        assert!(read.truncated);
    }

    #[test]
    fn null_bytes_classify_a_file_as_binary() {
        let trusted = TempDir::new().unwrap();
        let path = trusted.path().join("binary.dat");
        fs::write(&path, [1, 0, 2, 3]).unwrap();
        let files = FileSystem::new([trusted.path().to_path_buf()]).unwrap();
        let read = files.read_text(&path, 100).unwrap();

        assert_eq!(read.kind, FileKind::Binary);
        assert_eq!(read.content, None);
    }

    #[test]
    fn builds_a_structured_line_diff() {
        let trusted = TempDir::new().unwrap();
        let before = trusted.path().join("before.txt");
        let after = trusted.path().join("after.txt");
        fs::write(&before, "one\ntwo\nthree\n").unwrap();
        fs::write(&after, "one\nchanged\nthree\n").unwrap();
        let files = FileSystem::new([trusted.path().to_path_buf()]).unwrap();
        let diff = files.diff(&before, &after, 1).unwrap();

        assert_eq!(diff.hunks.len(), 1);
        assert!(
            diff.hunks[0]
                .lines
                .iter()
                .any(|line| line.kind == DiffLineKind::Delete && line.text == "two")
        );
        assert!(
            diff.hunks[0]
                .lines
                .iter()
                .any(|line| line.kind == DiffLineKind::Insert && line.text == "changed")
        );
    }
}
