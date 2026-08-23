use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use base64::Engine;
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use thiserror::Error;
use url::{Host, Url};

use crate::{Channel, validate_https_url};

pub const UPDATE_CLIENT_CONFIG_FILENAME: &str = "update-config-v1.json";
pub const EXPECTED_APP_BUNDLE_ID: &str = "app.agentfactory.desktop";
const CONFIG_SCHEMA_VERSION: u32 = 1;
const MAX_CONFIG_BYTES: u64 = 16 * 1024;
const HOST_EXECUTABLE_NAME: &str = "agent-factory";
const RUNTIME_EXECUTABLE_NAME: &str = "agent-factory-runtime";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateClientConfig {
    pub enabled: bool,
    pub channel: Channel,
    pub manifest_url: Option<Url>,
    pub detached_signature_url: Option<Url>,
    pub key_id: Option<String>,
    pub public_key: Option<[u8; 32]>,
    pub require_user_confirmation: bool,
    pub expected_bundle_id: String,
}

impl UpdateClientConfig {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            channel: Channel::Stable,
            manifest_url: None,
            detached_signature_url: None,
            key_id: None,
            public_key: None,
            require_user_confirmation: true,
            expected_bundle_id: EXPECTED_APP_BUNDLE_ID.to_owned(),
        }
    }

    pub fn enabled(
        channel: Channel,
        manifest_url: &str,
        detached_signature_url: &str,
        key_id: &str,
        public_key_base64: &str,
        expected_bundle_id: &str,
    ) -> Result<Self, UpdateConfigError> {
        validate_key_id(key_id)?;
        crate::manifest::validate_bundle_id(expected_bundle_id)
            .map_err(|_| UpdateConfigError::BundleId)?;
        let manifest_url = validate_config_url(manifest_url)?;
        let detached_signature_url = validate_config_url(detached_signature_url)?;
        let public_key = decode_public_key(public_key_base64)?;
        Ok(Self {
            enabled: true,
            channel,
            manifest_url: Some(manifest_url),
            detached_signature_url: Some(detached_signature_url),
            key_id: Some(key_id.to_owned()),
            public_key: Some(public_key),
            require_user_confirmation: true,
            expected_bundle_id: expected_bundle_id.to_owned(),
        })
    }

    pub fn public_key_base64(&self) -> Option<String> {
        self.public_key
            .map(|key| base64::engine::general_purpose::STANDARD.encode(key))
    }

    fn document(&self) -> UpdateClientConfigDocument {
        UpdateClientConfigDocument {
            schema_version: CONFIG_SCHEMA_VERSION,
            enabled: self.enabled,
            channel: self.channel,
            manifest_url: self.manifest_url.as_ref().map(Url::to_string),
            detached_signature_url: self.detached_signature_url.as_ref().map(Url::to_string),
            key_id: self.key_id.clone(),
            public_key_base64: self.public_key_base64(),
            require_user_confirmation: self.require_user_confirmation,
            expected_bundle_id: self.expected_bundle_id.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateConfigLoadStatus {
    Loaded,
    Missing,
    Invalid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadedUpdateClientConfig {
    pub config: UpdateClientConfig,
    pub status: UpdateConfigLoadStatus,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UpdateClientConfigDocument {
    schema_version: u32,
    enabled: bool,
    channel: Channel,
    manifest_url: Option<String>,
    detached_signature_url: Option<String>,
    key_id: Option<String>,
    public_key_base64: Option<String>,
    require_user_confirmation: bool,
    expected_bundle_id: String,
}

pub fn load_update_client_config(path: &Path) -> LoadedUpdateClientConfig {
    match read_update_client_config(path) {
        Ok(config) => LoadedUpdateClientConfig {
            config,
            status: UpdateConfigLoadStatus::Loaded,
        },
        Err(UpdateConfigError::Missing) => LoadedUpdateClientConfig {
            config: UpdateClientConfig::disabled(),
            status: UpdateConfigLoadStatus::Missing,
        },
        Err(_) => LoadedUpdateClientConfig {
            config: UpdateClientConfig::disabled(),
            status: UpdateConfigLoadStatus::Invalid,
        },
    }
}

pub fn load_packaged_update_client_config(executable_path: &Path) -> LoadedUpdateClientConfig {
    match packaged_update_config_path(executable_path) {
        Ok(path) => load_update_client_config(&path),
        Err(_) => LoadedUpdateClientConfig {
            config: UpdateClientConfig::disabled(),
            status: UpdateConfigLoadStatus::Invalid,
        },
    }
}

pub fn packaged_update_config_path(executable_path: &Path) -> Result<PathBuf, UpdateConfigError> {
    if !executable_path.is_absolute()
        || executable_path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(UpdateConfigError::PackagedPath);
    }
    let executable_name = executable_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(UpdateConfigError::PackagedPath)?;
    let executable_directory = executable_path
        .parent()
        .ok_or(UpdateConfigError::PackagedPath)?;
    let expected_directory = match executable_name {
        HOST_EXECUTABLE_NAME => "MacOS",
        RUNTIME_EXECUTABLE_NAME => "Resources",
        _ => return Err(UpdateConfigError::PackagedPath),
    };
    if executable_directory
        .file_name()
        .and_then(|name| name.to_str())
        != Some(expected_directory)
    {
        return Err(UpdateConfigError::PackagedPath);
    }
    let contents_directory = executable_directory
        .parent()
        .filter(|path| path.file_name().and_then(|name| name.to_str()) == Some("Contents"))
        .ok_or(UpdateConfigError::PackagedPath)?;
    let bundle = contents_directory
        .parent()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".app"))
        })
        .ok_or(UpdateConfigError::PackagedPath)?;
    Ok(bundle
        .join("Contents")
        .join("Resources")
        .join(UPDATE_CLIENT_CONFIG_FILENAME))
}

pub fn read_update_client_config(path: &Path) -> Result<UpdateClientConfig, UpdateConfigError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            UpdateConfigError::Missing
        } else {
            UpdateConfigError::Read
        }
    })?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_CONFIG_BYTES
    {
        return Err(UpdateConfigError::FileType);
    }
    let file = open_no_follow(path)?;
    let opened_metadata = file.metadata().map_err(|_| UpdateConfigError::Read)?;
    if !opened_metadata.is_file()
        || opened_metadata.len() == 0
        || opened_metadata.len() > MAX_CONFIG_BYTES
        || !same_file_identity(&metadata, &opened_metadata)
    {
        return Err(UpdateConfigError::FileType);
    }
    let mut bytes = Vec::with_capacity(opened_metadata.len() as usize);
    file.take(MAX_CONFIG_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| UpdateConfigError::Read)?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_CONFIG_BYTES {
        return Err(UpdateConfigError::FileType);
    }
    parse_update_client_config(&bytes)
}

pub fn write_update_client_config(
    path: &Path,
    config: &UpdateClientConfig,
) -> Result<(), UpdateConfigError> {
    let parent = path.parent().ok_or(UpdateConfigError::Write)?;
    fs::create_dir_all(parent).map_err(|_| UpdateConfigError::Write)?;
    let bytes =
        serde_json::to_vec_pretty(&config.document()).map_err(|_| UpdateConfigError::Json)?;
    let mut temporary = NamedTempFile::new_in(parent).map_err(|_| UpdateConfigError::Write)?;
    temporary
        .write_all(&bytes)
        .and_then(|()| temporary.write_all(b"\n"))
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|_| UpdateConfigError::Write)?;
    temporary
        .persist(path)
        .map_err(|_| UpdateConfigError::Write)?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| UpdateConfigError::Write)
}

fn parse_update_client_config(bytes: &[u8]) -> Result<UpdateClientConfig, UpdateConfigError> {
    let document: UpdateClientConfigDocument =
        serde_json::from_slice(bytes).map_err(|_| UpdateConfigError::Json)?;
    if document.schema_version != CONFIG_SCHEMA_VERSION || !document.require_user_confirmation {
        return Err(UpdateConfigError::Policy);
    }
    crate::manifest::validate_bundle_id(&document.expected_bundle_id)
        .map_err(|_| UpdateConfigError::BundleId)?;

    if !document.enabled {
        if document.manifest_url.is_some()
            || document.detached_signature_url.is_some()
            || document.key_id.is_some()
            || document.public_key_base64.is_some()
        {
            return Err(UpdateConfigError::Policy);
        }
        return Ok(UpdateClientConfig {
            enabled: false,
            channel: document.channel,
            manifest_url: None,
            detached_signature_url: None,
            key_id: None,
            public_key: None,
            require_user_confirmation: true,
            expected_bundle_id: document.expected_bundle_id,
        });
    }

    UpdateClientConfig::enabled(
        document.channel,
        document
            .manifest_url
            .as_deref()
            .ok_or(UpdateConfigError::Policy)?,
        document
            .detached_signature_url
            .as_deref()
            .ok_or(UpdateConfigError::Policy)?,
        document
            .key_id
            .as_deref()
            .ok_or(UpdateConfigError::Policy)?,
        document
            .public_key_base64
            .as_deref()
            .ok_or(UpdateConfigError::Policy)?,
        &document.expected_bundle_id,
    )
}

fn validate_config_url(raw: &str) -> Result<Url, UpdateConfigError> {
    let url = validate_https_url(raw).map_err(|_| UpdateConfigError::Url)?;
    match url.host() {
        Some(Host::Domain(host))
            if !host.eq_ignore_ascii_case("localhost")
                && !host.to_ascii_lowercase().ends_with(".localhost") =>
        {
            Ok(url)
        }
        _ => Err(UpdateConfigError::Url),
    }
}

fn validate_key_id(value: &str) -> Result<(), UpdateConfigError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(UpdateConfigError::KeyId);
    }
    Ok(())
}

fn decode_public_key(value: &str) -> Result<[u8; 32], UpdateConfigError> {
    if value.trim() != value {
        return Err(UpdateConfigError::PublicKey);
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|_| UpdateConfigError::PublicKey)?;
    let bytes: [u8; 32] = decoded
        .try_into()
        .map_err(|_| UpdateConfigError::PublicKey)?;
    let verifying_key =
        VerifyingKey::from_bytes(&bytes).map_err(|_| UpdateConfigError::PublicKey)?;
    if base64::engine::general_purpose::STANDARD.encode(bytes) != value || verifying_key.is_weak() {
        return Err(UpdateConfigError::PublicKey);
    }
    Ok(bytes)
}

#[cfg(unix)]
fn open_no_follow(path: &Path) -> Result<File, UpdateConfigError> {
    use std::os::unix::fs::OpenOptionsExt;

    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|_| UpdateConfigError::Read)
}

#[cfg(not(unix))]
fn open_no_follow(path: &Path) -> Result<File, UpdateConfigError> {
    OpenOptions::new()
        .read(true)
        .open(path)
        .map_err(|_| UpdateConfigError::Read)
}

#[cfg(unix)]
fn same_file_identity(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;

    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(not(unix))]
fn same_file_identity(_left: &fs::Metadata, _right: &fs::Metadata) -> bool {
    true
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum UpdateConfigError {
    #[error("update client config is missing")]
    Missing,
    #[error("update client config could not be read")]
    Read,
    #[error("update client config must be a bounded regular file")]
    FileType,
    #[error("update client config JSON is invalid")]
    Json,
    #[error("update client config policy is invalid")]
    Policy,
    #[error("update client config URL is invalid")]
    Url,
    #[error("update client config key id is invalid")]
    KeyId,
    #[error("update client config public key is invalid")]
    PublicKey,
    #[error("update client config bundle id is invalid")]
    BundleId,
    #[error("application executable path is not a sealed package path")]
    PackagedPath,
    #[error("update client config could not be written")]
    Write,
}

#[cfg(test)]
mod tests {
    use std::fs;

    use ed25519_dalek::SigningKey;
    use tempfile::tempdir;

    use super::*;

    fn public_key_base64() -> String {
        base64::engine::general_purpose::STANDARD.encode(
            SigningKey::from_bytes(&[7_u8; 32])
                .verifying_key()
                .to_bytes(),
        )
    }

    fn enabled_document() -> Vec<u8> {
        let config = UpdateClientConfig::enabled(
            Channel::Stable,
            "https://github.com/example/agent-factory/releases/download/v1.2.3/agent-factory-update-manifest-v1.json",
            "https://github.com/example/agent-factory/releases/download/v1.2.3/agent-factory-update-manifest-v1.json.sig",
            "stable-2026",
            &public_key_base64(),
            EXPECTED_APP_BUNDLE_ID,
        )
        .unwrap();
        serde_json::to_vec(&config.document()).unwrap()
    }

    #[test]
    fn canonical_disabled_release_config_has_no_trust_material() {
        let config = parse_update_client_config(include_bytes!(
            "../tests/fixtures/update-config-v1.disabled.json"
        ))
        .unwrap();
        assert_eq!(config, UpdateClientConfig::disabled());
    }

    #[test]
    fn loads_enabled_config_with_raw_public_key() {
        let directory = tempdir().unwrap();
        let path = directory.path().join(UPDATE_CLIENT_CONFIG_FILENAME);
        fs::write(&path, enabled_document()).unwrap();
        let loaded = load_update_client_config(&path);
        assert_eq!(loaded.status, UpdateConfigLoadStatus::Loaded);
        assert!(loaded.config.enabled);
        assert_eq!(loaded.config.public_key_base64(), Some(public_key_base64()));
    }

    #[test]
    fn absent_and_invalid_configs_fail_closed() {
        let directory = tempdir().unwrap();
        let missing = load_update_client_config(&directory.path().join("missing.json"));
        assert_eq!(missing.status, UpdateConfigLoadStatus::Missing);
        assert!(!missing.config.enabled);

        let invalid_path = directory.path().join("invalid.json");
        fs::write(&invalid_path, br#"{"schemaVersion":1,"enabled":true}"#).unwrap();
        let invalid = load_update_client_config(&invalid_path);
        assert_eq!(invalid.status, UpdateConfigLoadStatus::Invalid);
        assert_eq!(invalid.config, UpdateClientConfig::disabled());
    }

    #[test]
    fn rejects_malicious_urls_and_keys() {
        let key = public_key_base64();
        for url in [
            "http://updates.example/manifest.json",
            "https://user:pass@updates.example/manifest.json",
            "https://updates.example/manifest.json#replacement",
            "https://updates.example/a/%2e%2e/manifest.json",
            "https://127.0.0.1/manifest.json",
            "https://localhost/manifest.json",
        ] {
            assert_eq!(
                UpdateClientConfig::enabled(
                    Channel::Stable,
                    url,
                    "https://updates.example/manifest.json.sig",
                    "stable-2026",
                    &key,
                    EXPECTED_APP_BUNDLE_ID,
                ),
                Err(UpdateConfigError::Url),
            );
        }
        for key in [
            "",
            "not-base64",
            &base64::engine::general_purpose::STANDARD.encode([0; 31]),
            &base64::engine::general_purpose::STANDARD.encode({
                let mut weak = [0_u8; 32];
                weak[0] = 1;
                weak
            }),
        ] {
            assert_eq!(
                UpdateClientConfig::enabled(
                    Channel::Stable,
                    "https://updates.example/manifest.json",
                    "https://updates.example/manifest.json.sig",
                    "stable-2026",
                    key,
                    EXPECTED_APP_BUNDLE_ID,
                ),
                Err(UpdateConfigError::PublicKey),
            );
        }
    }

    #[test]
    fn rejects_unknown_fields_trust_in_disabled_config_and_no_confirmation() {
        let mut value: serde_json::Value = serde_json::from_slice(&enabled_document()).unwrap();
        value["unexpected"] = serde_json::json!(true);
        assert_eq!(
            parse_update_client_config(&serde_json::to_vec(&value).unwrap()),
            Err(UpdateConfigError::Json),
        );

        value.as_object_mut().unwrap().remove("unexpected");
        value["enabled"] = serde_json::json!(false);
        assert_eq!(
            parse_update_client_config(&serde_json::to_vec(&value).unwrap()),
            Err(UpdateConfigError::Policy),
        );

        value["enabled"] = serde_json::json!(true);
        value["requireUserConfirmation"] = serde_json::json!(false);
        assert_eq!(
            parse_update_client_config(&serde_json::to_vec(&value).unwrap()),
            Err(UpdateConfigError::Policy),
        );
    }

    #[test]
    fn rejects_symlink_and_oversized_config() {
        let directory = tempdir().unwrap();
        let target = directory.path().join("target.json");
        fs::write(&target, enabled_document()).unwrap();

        #[cfg(unix)]
        {
            let link = directory.path().join("link.json");
            std::os::unix::fs::symlink(&target, &link).unwrap();
            assert_eq!(
                read_update_client_config(&link),
                Err(UpdateConfigError::FileType),
            );
        }

        let oversized = directory.path().join("oversized.json");
        fs::write(&oversized, vec![b' '; MAX_CONFIG_BYTES as usize + 1]).unwrap();
        assert_eq!(
            read_update_client_config(&oversized),
            Err(UpdateConfigError::FileType),
        );
    }

    #[test]
    fn derives_only_exact_packaged_executable_relative_path() {
        assert_eq!(
            packaged_update_config_path(Path::new(
                "/Applications/Agent Factory.app/Contents/MacOS/agent-factory",
            ))
            .unwrap(),
            PathBuf::from(
                "/Applications/Agent Factory.app/Contents/Resources/update-config-v1.json",
            ),
        );
        assert_eq!(
            packaged_update_config_path(Path::new(
                "/Applications/Agent Factory.app/Contents/Resources/agent-factory-runtime",
            ))
            .unwrap(),
            PathBuf::from(
                "/Applications/Agent Factory.app/Contents/Resources/update-config-v1.json",
            ),
        );
        for path in [
            "relative/Agent Factory.app/Contents/MacOS/agent-factory",
            "/Applications/Agent Factory.app/Contents/MacOS/other",
            "/Applications/not-an-app/Contents/MacOS/agent-factory",
            "/Applications/Agent Factory.app/Contents/MacOS/../agent-factory",
            "/Applications/Agent Factory.app/Contents/Resources/updater-helper",
            "/Applications/Agent Factory.app/Contents/MacOS/agent-factory-runtime",
            "/Applications/Agent Factory.app/Contents/Resources/agent-factory",
        ] {
            assert_eq!(
                packaged_update_config_path(Path::new(path)),
                Err(UpdateConfigError::PackagedPath),
            );
        }
    }
}
