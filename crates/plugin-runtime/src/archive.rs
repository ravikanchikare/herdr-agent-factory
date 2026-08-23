use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;

use crate::error::{PluginError, Result};
use crate::path::normalize_relative;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArchiveLimits {
    pub max_compressed_bytes: u64,
    pub max_expanded_bytes: u64,
    pub max_entries: u64,
}

impl Default for ArchiveLimits {
    fn default() -> Self {
        Self {
            max_compressed_bytes: 64 * 1024 * 1024,
            max_expanded_bytes: 256 * 1024 * 1024,
            max_entries: 10_000,
        }
    }
}

pub struct StagedArchive {
    file: NamedTempFile,
    sha256: String,
    compressed_bytes: u64,
}

impl StagedArchive {
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    pub fn compressed_bytes(&self) -> u64 {
        self.compressed_bytes
    }

    pub(crate) fn path(&self) -> &Path {
        self.file.path()
    }
}

pub fn stage_archive(
    staging_directory: &Path,
    mut input: impl Read,
    limits: ArchiveLimits,
) -> Result<StagedArchive> {
    std::fs::create_dir_all(staging_directory)
        .map_err(|error| PluginError::io(staging_directory, error))?;
    let mut file = tempfile::Builder::new()
        .prefix("artifact-")
        .suffix(".tar.gz.part")
        .tempfile_in(staging_directory)
        .map_err(|error| PluginError::io(staging_directory, error))?;
    let mut digest = Sha256::new();
    let mut count = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = input
            .read(&mut buffer)
            .map_err(|error| PluginError::io(file.path(), error))?;
        if read == 0 {
            break;
        }
        count = count.saturating_add(read as u64);
        if count > limits.max_compressed_bytes {
            return Err(PluginError::UnsafeArchive(format!(
                "compressed artifact exceeds {} bytes",
                limits.max_compressed_bytes
            )));
        }
        digest.update(&buffer[..read]);
        file.write_all(&buffer[..read])
            .map_err(|error| PluginError::io(file.path(), error))?;
    }
    file.flush()
        .map_err(|error| PluginError::io(file.path(), error))?;
    file.as_file()
        .sync_all()
        .map_err(|error| PluginError::io(file.path(), error))?;
    file.seek(SeekFrom::Start(0))
        .map_err(|error| PluginError::io(file.path(), error))?;
    Ok(StagedArchive {
        file,
        sha256: hex::encode(digest.finalize()),
        compressed_bytes: count,
    })
}

pub(crate) fn extract_archive(
    archive: &StagedArchive,
    destination: &Path,
    limits: ArchiveLimits,
) -> Result<()> {
    std::fs::create_dir_all(destination).map_err(|error| PluginError::io(destination, error))?;
    let file =
        File::open(archive.path()).map_err(|error| PluginError::io(archive.path(), error))?;
    let decoder = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    let entries = archive
        .entries()
        .map_err(|error| PluginError::UnsafeArchive(format!("invalid tar stream: {error}")))?;
    let mut seen = BTreeSet::new();
    let mut entry_count = 0_u64;
    let mut expanded_bytes = 0_u64;

    for entry in entries {
        let mut entry = entry
            .map_err(|error| PluginError::UnsafeArchive(format!("invalid tar entry: {error}")))?;
        entry_count = entry_count.saturating_add(1);
        if entry_count > limits.max_entries {
            return Err(PluginError::UnsafeArchive(format!(
                "archive contains more than {} entries",
                limits.max_entries
            )));
        }
        let raw_path = entry
            .path()
            .map_err(|error| PluginError::UnsafeArchive(format!("invalid entry path: {error}")))?;
        if raw_path.is_absolute() {
            return Err(PluginError::UnsafeArchive(format!(
                "absolute entry path {}",
                raw_path.display()
            )));
        }
        let relative = normalize_relative(&raw_path)
            .map_err(|error| PluginError::UnsafeArchive(error.to_string()))?;
        if !seen.insert(relative.clone()) {
            return Err(PluginError::UnsafeArchive(format!(
                "duplicate archive entry {}",
                relative.display()
            )));
        }
        let entry_type = entry.header().entry_type();
        let output = destination.join(&relative);
        if entry_type.is_dir() {
            ensure_directory(&output)?;
            continue;
        }
        if !entry_type.is_file() {
            return Err(PluginError::UnsafeArchive(format!(
                "{} is a link or special file",
                relative.display()
            )));
        }
        let size = entry
            .header()
            .size()
            .map_err(|error| PluginError::UnsafeArchive(format!("invalid file size: {error}")))?;
        expanded_bytes = expanded_bytes.saturating_add(size);
        if expanded_bytes > limits.max_expanded_bytes {
            return Err(PluginError::UnsafeArchive(format!(
                "expanded artifact exceeds {} bytes",
                limits.max_expanded_bytes
            )));
        }
        if let Some(parent) = output.parent() {
            ensure_directory(parent)?;
        }
        let mut output_file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&output)
            .map_err(|error| PluginError::io(&output, error))?;
        let copied = std::io::copy(&mut entry.by_ref().take(size + 1), &mut output_file)
            .map_err(|error| PluginError::io(&output, error))?;
        if copied != size {
            return Err(PluginError::UnsafeArchive(format!(
                "{} declared {size} bytes but yielded {copied}",
                relative.display()
            )));
        }
        output_file
            .flush()
            .map_err(|error| PluginError::io(&output, error))?;
        set_sanitized_permissions(&output, entry.header().mode().unwrap_or(0))?;
    }
    Ok(())
}

fn ensure_directory(path: &Path) -> Result<()> {
    if path.exists() {
        if path.is_dir() {
            return Ok(());
        }
        return Err(PluginError::UnsafeArchive(format!(
            "{} collides with a non-directory",
            path.display()
        )));
    }
    std::fs::create_dir_all(path).map_err(|error| PluginError::io(path, error))
}

#[cfg(unix)]
fn set_sanitized_permissions(path: &Path, archive_mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mode = if archive_mode & 0o111 == 0 {
        0o644
    } else {
        0o755
    };
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .map_err(|error| PluginError::io(path, error))
}

#[cfg(not(unix))]
fn set_sanitized_permissions(_path: &Path, _archive_mode: u32) -> Result<()> {
    Ok(())
}

pub(crate) fn version_storage_key(version: &str) -> String {
    hex::encode(Sha256::digest(version.as_bytes()))
}
