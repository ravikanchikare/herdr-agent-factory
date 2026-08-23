use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use tempfile::Builder;
use zip::ZipArchive;

use crate::{Architecture, Release, ReleaseArtifact, StagedArtifact, UpdateError};

const MAX_ARCHIVE_ENTRIES: usize = 20_000;
const MAX_EXPANDED_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtractedBundle {
    extraction_root: PathBuf,
    bundle_path: PathBuf,
    release: Release,
    artifact: ReleaseArtifact,
}

impl ExtractedBundle {
    pub fn extraction_root(&self) -> &Path {
        &self.extraction_root
    }

    pub fn bundle_path(&self) -> &Path {
        &self.bundle_path
    }

    pub fn release(&self) -> &Release {
        &self.release
    }

    pub fn artifact(&self) -> &ReleaseArtifact {
        &self.artifact
    }
}

/// Extracts a previously size/hash-verified release zip into a new private
/// directory, then verifies the signed release metadata against the bundle.
pub fn extract_macos_bundle(
    staged: &StagedArtifact,
    extraction_parent: &Path,
) -> Result<ExtractedBundle, UpdateError> {
    extract_with_limits(
        staged,
        extraction_parent,
        MAX_ARCHIVE_ENTRIES,
        MAX_EXPANDED_BYTES,
    )
}

fn extract_with_limits(
    staged: &StagedArtifact,
    extraction_parent: &Path,
    max_entries: usize,
    max_expanded_bytes: u64,
) -> Result<ExtractedBundle, UpdateError> {
    fs::create_dir_all(extraction_parent).map_err(|_| UpdateError::Staging)?;
    let temporary = Builder::new()
        .prefix(".agent-factory-extract-")
        .tempdir_in(extraction_parent)
        .map_err(|_| UpdateError::Staging)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(temporary.path(), fs::Permissions::from_mode(0o700))
            .map_err(|_| UpdateError::Staging)?;
    }

    let archive_file = File::open(staged.path()).map_err(|_| UpdateError::InvalidArchive)?;
    let mut archive = ZipArchive::new(archive_file).map_err(|_| UpdateError::InvalidArchive)?;
    if archive.is_empty() {
        return Err(UpdateError::InvalidArchive);
    }
    if archive.len() > max_entries {
        return Err(UpdateError::TooManyArchiveEntries);
    }

    let mut top_level_app: Option<PathBuf> = None;
    let mut names = BTreeSet::new();
    let mut expanded_bytes = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|_| UpdateError::InvalidArchive)?;
        if entry.encrypted() || entry.is_symlink() || entry.name().contains('\\') {
            return Err(UpdateError::UnsafeArchiveEntry);
        }
        let enclosed = entry
            .enclosed_name()
            .ok_or(UpdateError::UnsafeArchiveEntry)?;
        if is_ignorable_metadata(&enclosed) {
            continue;
        }
        validate_relative_path(&enclosed)?;
        let case_key = enclosed.to_string_lossy().to_lowercase();
        if !names.insert(case_key) {
            return Err(UpdateError::UnsafeArchiveEntry);
        }

        let first = enclosed
            .components()
            .next()
            .and_then(|component| match component {
                Component::Normal(value) => Some(PathBuf::from(value)),
                _ => None,
            })
            .ok_or(UpdateError::UnsafeArchiveEntry)?;
        if first.extension().and_then(|value| value.to_str()) != Some("app") {
            return Err(UpdateError::UnsafeArchiveEntry);
        }
        match &top_level_app {
            None => top_level_app = Some(first),
            Some(expected) if expected == &first => {}
            Some(_) => return Err(UpdateError::UnsafeArchiveEntry),
        }

        let output = temporary.path().join(&enclosed);
        let mode = entry.unix_mode().unwrap_or(0);
        let file_type = mode & 0o170000;
        if entry.is_dir() {
            if file_type != 0 && file_type != 0o040000 {
                return Err(UpdateError::UnsafeArchiveEntry);
            }
            fs::create_dir_all(&output).map_err(|_| UpdateError::Staging)?;
            continue;
        }
        if file_type != 0 && file_type != 0o100000 {
            return Err(UpdateError::UnsafeArchiveEntry);
        }
        if entry.size() > max_expanded_bytes.saturating_sub(expanded_bytes) {
            return Err(UpdateError::ExpandedSize);
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(|_| UpdateError::Staging)?;
        }
        let mut output_file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&output)
            .map_err(|_| UpdateError::UnsafeArchiveEntry)?;
        let remaining = max_expanded_bytes.saturating_sub(expanded_bytes);
        let written = std::io::copy(&mut entry.by_ref().take(remaining + 1), &mut output_file)
            .map_err(|_| UpdateError::InvalidArchive)?;
        if written > remaining {
            return Err(UpdateError::ExpandedSize);
        }
        expanded_bytes += written;
        #[cfg(unix)]
        if mode != 0 {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&output, fs::Permissions::from_mode(mode & 0o777))
                .map_err(|_| UpdateError::Staging)?;
        }
        output_file.sync_all().map_err(|_| UpdateError::Staging)?;
    }

    let bundle_name = top_level_app.ok_or(UpdateError::InvalidArchive)?;
    let bundle_path = temporary.path().join(bundle_name);
    validate_bundle_metadata(&bundle_path, staged.release(), staged.artifact())?;
    sync_directories(temporary.path())?;
    let root = temporary.keep();
    let bundle_path = root.join(bundle_path.file_name().ok_or(UpdateError::InvalidArchive)?);
    Ok(ExtractedBundle {
        extraction_root: root,
        bundle_path,
        release: staged.release().clone(),
        artifact: staged.artifact().clone(),
    })
}

fn is_ignorable_metadata(path: &Path) -> bool {
    let first = path.components().next();
    if matches!(first, Some(Component::Normal(value)) if value == "__MACOSX") {
        return true;
    }
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| name == ".DS_Store" || name.starts_with("._"))
}

fn validate_relative_path(path: &Path) -> Result<(), UpdateError> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(UpdateError::UnsafeArchiveEntry);
    }
    Ok(())
}

fn validate_bundle_metadata(
    bundle: &Path,
    release: &Release,
    artifact: &ReleaseArtifact,
) -> Result<(), UpdateError> {
    let info = plist::Value::from_file(bundle.join("Contents/Info.plist"))
        .map_err(|_| UpdateError::BundleMetadataMismatch)?;
    let dictionary = info
        .as_dictionary()
        .ok_or(UpdateError::BundleMetadataMismatch)?;
    require_plist_string(dictionary, "CFBundleIdentifier", &artifact.bundle_id)?;
    require_plist_string(dictionary, "CFBundleShortVersionString", &release.version)?;
    require_plist_string(dictionary, "CFBundleVersion", &release.version)?;
    require_plist_string(
        dictionary,
        "LSMinimumSystemVersion",
        &artifact.minimum_macos_version,
    )?;
    let executable = dictionary
        .get("CFBundleExecutable")
        .and_then(plist::Value::as_string)
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 255
                && !value.contains(['/', '\\'])
                && value != &"."
                && value != &".."
        })
        .ok_or(UpdateError::BundleMetadataMismatch)?;
    let executable = bundle.join("Contents/MacOS").join(executable);
    validate_macho_architecture(&executable, artifact.architecture)
}

fn require_plist_string(
    dictionary: &plist::Dictionary,
    key: &str,
    expected: &str,
) -> Result<(), UpdateError> {
    let actual = dictionary
        .get(key)
        .and_then(plist::Value::as_string)
        .ok_or(UpdateError::BundleMetadataMismatch)?;
    if actual == expected {
        Ok(())
    } else {
        Err(UpdateError::BundleMetadataMismatch)
    }
}

fn validate_macho_architecture(
    executable: &Path,
    expected: Architecture,
) -> Result<(), UpdateError> {
    let mut file = File::open(executable).map_err(|_| UpdateError::BundleArchitectureMismatch)?;
    let mut header = [0_u8; 4096];
    let read = file
        .read(&mut header)
        .map_err(|_| UpdateError::BundleArchitectureMismatch)?;
    let architectures = macho_architectures(&header[..read])?;
    let expected_cpu = match expected {
        Architecture::Aarch64AppleDarwin => 0x0100_000c,
        Architecture::X86_64AppleDarwin => 0x0100_0007,
    };
    if architectures.len() == 1 && architectures.contains(&expected_cpu) {
        Ok(())
    } else {
        Err(UpdateError::BundleArchitectureMismatch)
    }
}

fn macho_architectures(bytes: &[u8]) -> Result<BTreeSet<u32>, UpdateError> {
    let magic: [u8; 4] = bytes
        .get(..4)
        .ok_or(UpdateError::BundleArchitectureMismatch)?
        .try_into()
        .map_err(|_| UpdateError::BundleArchitectureMismatch)?;
    let mut architectures = BTreeSet::new();
    match magic {
        [0xcf, 0xfa, 0xed, 0xfe] => {
            architectures.insert(read_u32(bytes, 4, true)?);
        }
        [0xfe, 0xed, 0xfa, 0xcf] => {
            architectures.insert(read_u32(bytes, 4, false)?);
        }
        [0xca, 0xfe, 0xba, 0xbe] | [0xca, 0xfe, 0xba, 0xbf] => {
            let count = read_u32(bytes, 4, false)? as usize;
            let stride = if magic[3] == 0xbe { 20 } else { 32 };
            if count == 0 || count > 16 || bytes.len() < 8 + count * stride {
                return Err(UpdateError::BundleArchitectureMismatch);
            }
            for index in 0..count {
                architectures.insert(read_u32(bytes, 8 + index * stride, false)?);
            }
        }
        [0xbe, 0xba, 0xfe, 0xca] | [0xbf, 0xba, 0xfe, 0xca] => {
            let count = read_u32(bytes, 4, true)? as usize;
            let stride = if magic[0] == 0xbe { 20 } else { 32 };
            if count == 0 || count > 16 || bytes.len() < 8 + count * stride {
                return Err(UpdateError::BundleArchitectureMismatch);
            }
            for index in 0..count {
                architectures.insert(read_u32(bytes, 8 + index * stride, true)?);
            }
        }
        _ => return Err(UpdateError::BundleArchitectureMismatch),
    }
    Ok(architectures)
}

fn read_u32(bytes: &[u8], offset: usize, little_endian: bool) -> Result<u32, UpdateError> {
    let value: [u8; 4] = bytes
        .get(offset..offset + 4)
        .ok_or(UpdateError::BundleArchitectureMismatch)?
        .try_into()
        .map_err(|_| UpdateError::BundleArchitectureMismatch)?;
    Ok(if little_endian {
        u32::from_le_bytes(value)
    } else {
        u32::from_be_bytes(value)
    })
}

fn sync_directories(path: &Path) -> Result<(), UpdateError> {
    for entry in fs::read_dir(path).map_err(|_| UpdateError::Staging)? {
        let path = entry.map_err(|_| UpdateError::Staging)?.path();
        if path.is_dir() {
            sync_directories(&path)?;
        }
    }
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| UpdateError::Staging)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Channel;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    fn staged_zip(entries: &[(&str, &[u8], u32)]) -> (tempfile::TempDir, StagedArtifact) {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("release.zip");
        let file = File::create(&archive_path).unwrap();
        let mut archive = ZipWriter::new(file);
        for (name, bytes, mode) in entries {
            let options = SimpleFileOptions::default()
                .compression_method(CompressionMethod::Deflated)
                .unix_permissions(*mode);
            archive.start_file(*name, options).unwrap();
            archive.write_all(bytes).unwrap();
        }
        archive.finish().unwrap().sync_all().unwrap();
        let artifact = ReleaseArtifact {
            architecture: Architecture::Aarch64AppleDarwin,
            minimum_macos_version: "13.0".to_owned(),
            bundle_id: "app.agentfactory.desktop".to_owned(),
            url: "https://updates.example/release.zip".to_owned(),
            size: fs::metadata(&archive_path).unwrap().len(),
            sha256: "0".repeat(64),
        };
        (
            temp,
            StagedArtifact {
                path: archive_path,
                release: Release {
                    version: "1.2.3".to_owned(),
                    channel: Channel::Stable,
                    artifacts: vec![artifact.clone()],
                },
                artifact,
            },
        )
    }

    fn info_plist() -> Vec<u8> {
        br#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>agent-factory</string>
<key>CFBundleIdentifier</key><string>app.agentfactory.desktop</string>
<key>CFBundleShortVersionString</key><string>1.2.3</string>
<key>CFBundleVersion</key><string>1.2.3</string>
<key>LSMinimumSystemVersion</key><string>13.0</string>
</dict></plist>"#
            .to_vec()
    }

    fn arm64_macho() -> Vec<u8> {
        let mut bytes = vec![0_u8; 32];
        bytes[..4].copy_from_slice(&[0xcf, 0xfa, 0xed, 0xfe]);
        bytes[4..8].copy_from_slice(&0x0100_000c_u32.to_le_bytes());
        bytes
    }

    #[test]
    fn extracts_one_matching_signed_bundle() {
        let plist = info_plist();
        let executable = arm64_macho();
        let (_source, staged) = staged_zip(&[
            ("Agent Factory.app/Contents/Info.plist", &plist, 0o100644),
            (
                "Agent Factory.app/Contents/MacOS/agent-factory",
                &executable,
                0o100755,
            ),
        ]);
        let output = tempfile::tempdir().unwrap();
        let extracted = extract_macos_bundle(&staged, output.path()).unwrap();
        assert!(
            extracted
                .bundle_path()
                .join("Contents/Info.plist")
                .is_file()
        );
    }

    #[test]
    fn rejects_traversal_case_collisions_symlinks_and_multiple_apps() {
        for entries in [
            vec![("Agent Factory.app/../escape", b"x".as_slice(), 0o100644)],
            vec![
                ("Agent Factory.app/File", b"a".as_slice(), 0o100644),
                ("Agent Factory.app/file", b"b".as_slice(), 0o100644),
            ],
            vec![
                ("Agent Factory.app/file", b"a".as_slice(), 0o100644),
                ("Other.app/file", b"b".as_slice(), 0o100644),
            ],
        ] {
            let (_source, staged) = staged_zip(&entries);
            let output = tempfile::tempdir().unwrap();
            assert!(matches!(
                extract_macos_bundle(&staged, output.path()),
                Err(UpdateError::UnsafeArchiveEntry),
            ));
        }

        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("symlink.zip");
        let file = File::create(&archive_path).unwrap();
        let mut archive = ZipWriter::new(file);
        archive
            .add_symlink(
                "Agent Factory.app/link",
                "target",
                SimpleFileOptions::default(),
            )
            .unwrap();
        archive.finish().unwrap();
        let artifact = ReleaseArtifact {
            architecture: Architecture::Aarch64AppleDarwin,
            minimum_macos_version: "13.0".to_owned(),
            bundle_id: "app.agentfactory.desktop".to_owned(),
            url: "https://updates.example/release.zip".to_owned(),
            size: fs::metadata(&archive_path).unwrap().len(),
            sha256: "0".repeat(64),
        };
        let staged = StagedArtifact {
            path: archive_path,
            release: Release {
                version: "1.2.3".to_owned(),
                channel: Channel::Stable,
                artifacts: vec![artifact.clone()],
            },
            artifact,
        };
        let output = tempfile::tempdir().unwrap();
        assert_eq!(
            extract_macos_bundle(&staged, output.path()),
            Err(UpdateError::UnsafeArchiveEntry),
        );
    }

    #[test]
    fn rejects_entry_count_expansion_and_metadata_mismatch() {
        let (_source, staged) =
            staged_zip(&[("Agent Factory.app/large", b"12345".as_slice(), 0o100644)]);
        let output = tempfile::tempdir().unwrap();
        assert_eq!(
            extract_with_limits(&staged, output.path(), 0, 100),
            Err(UpdateError::TooManyArchiveEntries),
        );
        assert_eq!(
            extract_with_limits(&staged, output.path(), 10, 4),
            Err(UpdateError::ExpandedSize),
        );

        let plist = info_plist();
        let x86 = {
            let mut bytes = vec![0_u8; 32];
            bytes[..4].copy_from_slice(&[0xcf, 0xfa, 0xed, 0xfe]);
            bytes[4..8].copy_from_slice(&0x0100_0007_u32.to_le_bytes());
            bytes
        };
        let (_source, staged) = staged_zip(&[
            ("Agent Factory.app/Contents/Info.plist", &plist, 0o100644),
            (
                "Agent Factory.app/Contents/MacOS/agent-factory",
                &x86,
                0o100755,
            ),
        ]);
        let output = tempfile::tempdir().unwrap();
        assert_eq!(
            extract_macos_bundle(&staged, output.path()),
            Err(UpdateError::BundleArchitectureMismatch),
        );
    }
}
