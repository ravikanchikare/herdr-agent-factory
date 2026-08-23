use std::collections::BTreeSet;
use std::fmt;

use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use semver::Version;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

use crate::MAX_MANIFEST_BYTES;

pub const MANIFEST_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    Stable,
    Beta,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Architecture {
    Aarch64AppleDarwin,
    X86_64AppleDarwin,
}

impl Architecture {
    pub fn current() -> Result<Self, UpdateError> {
        match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => Ok(Self::Aarch64AppleDarwin),
            ("macos", "x86_64") => Ok(Self::X86_64AppleDarwin),
            _ => Err(UpdateError::UnsupportedPlatform),
        }
    }
}

impl fmt::Display for Architecture {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Aarch64AppleDarwin => "aarch64-apple-darwin",
            Self::X86_64AppleDarwin => "x86_64-apple-darwin",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateManifest {
    pub schema_version: u32,
    pub key_id: String,
    pub releases: Vec<Release>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Release {
    pub version: String,
    pub channel: Channel,
    pub artifacts: Vec<ReleaseArtifact>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleaseArtifact {
    pub architecture: Architecture,
    pub minimum_macos_version: String,
    pub bundle_id: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

pub struct SignedManifest<'a> {
    pub bytes: &'a [u8],
    pub signature_base64: &'a str,
}

#[derive(Clone, Debug)]
pub struct VerifiedManifest {
    manifest: UpdateManifest,
}

impl VerifiedManifest {
    pub fn verify(
        signed: SignedManifest<'_>,
        expected_key_id: &str,
        public_key: &[u8; 32],
    ) -> Result<Self, UpdateError> {
        if signed.bytes.is_empty() || signed.bytes.len() as u64 > MAX_MANIFEST_BYTES {
            return Err(UpdateError::ManifestSize);
        }
        let signature_bytes = base64::engine::general_purpose::STANDARD
            .decode(signed.signature_base64.trim())
            .map_err(|_| UpdateError::BadSignature)?;
        let signature =
            Signature::from_slice(&signature_bytes).map_err(|_| UpdateError::BadSignature)?;
        let verifying_key =
            VerifyingKey::from_bytes(public_key).map_err(|_| UpdateError::BadPublicKey)?;
        verifying_key
            .verify(signed.bytes, &signature)
            .map_err(|_| UpdateError::BadSignature)?;

        let manifest: UpdateManifest =
            serde_json::from_slice(signed.bytes).map_err(|_| UpdateError::InvalidManifest)?;
        validate_manifest(&manifest, expected_key_id)?;
        Ok(Self { manifest })
    }

    pub fn manifest(&self) -> &UpdateManifest {
        &self.manifest
    }
}

#[derive(Clone, Debug)]
pub struct SelectionRequest<'a> {
    pub current_version: &'a str,
    pub channel: Channel,
    pub architecture: Architecture,
    pub macos_version: &'a str,
    pub requested_version: Option<&'a str>,
    pub allow_rollback: bool,
}

pub fn select_release(
    verified: &VerifiedManifest,
    request: &SelectionRequest<'_>,
) -> Result<Option<(Release, ReleaseArtifact)>, UpdateError> {
    let current = parse_version(request.current_version)?;
    let os_version = parse_version(request.macos_version)?;
    let requested = request.requested_version.map(parse_version).transpose()?;
    if let Some(target) = &requested {
        if target < &current && !request.allow_rollback {
            return Err(UpdateError::DowngradeDenied);
        }
        if target == &current {
            return Ok(None);
        }
    }

    let mut matches = Vec::new();
    for release in &verified.manifest.releases {
        if release.channel != request.channel {
            continue;
        }
        let version = parse_version(&release.version)?;
        if requested.as_ref().is_some_and(|target| target != &version) {
            continue;
        }
        if requested.is_none() && version <= current {
            continue;
        }
        if version < current && !request.allow_rollback {
            continue;
        }
        for artifact in &release.artifacts {
            if artifact.architecture != request.architecture {
                continue;
            }
            if parse_version(&artifact.minimum_macos_version)? > os_version {
                continue;
            }
            matches.push((version.clone(), release.clone(), artifact.clone()));
        }
    }
    matches.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(matches
        .pop()
        .map(|(_, release, artifact)| (release, artifact)))
}

fn validate_manifest(manifest: &UpdateManifest, expected_key_id: &str) -> Result<(), UpdateError> {
    if manifest.schema_version != MANIFEST_SCHEMA_VERSION {
        return Err(UpdateError::UnsupportedManifestVersion(
            manifest.schema_version,
        ));
    }
    if manifest.key_id != expected_key_id || expected_key_id.is_empty() {
        return Err(UpdateError::WrongKeyId);
    }
    if manifest.key_id.len() > 128
        || !manifest
            .key_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(UpdateError::WrongKeyId);
    }
    if manifest.releases.is_empty() || manifest.releases.len() > 100 {
        return Err(UpdateError::InvalidManifest);
    }
    let mut release_keys = BTreeSet::new();
    for release in &manifest.releases {
        let version = parse_version(&release.version)?;
        if release.channel == Channel::Stable && !version.pre.is_empty() {
            return Err(UpdateError::InvalidVersion(release.version.clone()));
        }
        if !release_keys.insert((version, release.channel)) {
            return Err(UpdateError::InvalidManifest);
        }
        if release.artifacts.is_empty() || release.artifacts.len() > 8 {
            return Err(UpdateError::InvalidManifest);
        }
        let mut architectures = BTreeSet::new();
        for artifact in &release.artifacts {
            if !architectures.insert(artifact.architecture) {
                return Err(UpdateError::InvalidManifest);
            }
            parse_version(&artifact.minimum_macos_version)?;
            validate_bundle_id(&artifact.bundle_id)?;
            validate_https_url(&artifact.url)?;
            if artifact.size == 0 || artifact.size > crate::MAX_ARTIFACT_BYTES {
                return Err(UpdateError::ArtifactSize);
            }
            if artifact.sha256.len() != 64
                || !artifact.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(UpdateError::InvalidHash);
            }
        }
    }
    Ok(())
}

pub fn validate_https_url(raw: &str) -> Result<Url, UpdateError> {
    if raw.len() > 2048 || raw.contains('\0') {
        return Err(UpdateError::InvalidUrl);
    }
    let url = Url::parse(raw).map_err(|_| UpdateError::InvalidUrl)?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(UpdateError::HttpsRequired);
    }

    let path_source = raw
        .split_once("//")
        .map(|(_, rest)| rest)
        .unwrap_or(raw)
        .split(['?', '#'])
        .next()
        .unwrap_or_default();
    let path = path_source
        .split_once('/')
        .map(|(_, path)| path)
        .unwrap_or_default();
    let decoded = percent_decode(path)?;
    if decoded
        .split(['/', '\\'])
        .any(|segment| matches!(segment, "." | ".."))
    {
        return Err(UpdateError::PathTraversal);
    }
    Ok(url)
}

fn percent_decode(value: &str) -> Result<String, UpdateError> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let encoded = bytes
                .get(index + 1..index + 3)
                .ok_or(UpdateError::InvalidUrl)?;
            let encoded = std::str::from_utf8(encoded).map_err(|_| UpdateError::InvalidUrl)?;
            decoded.push(u8::from_str_radix(encoded, 16).map_err(|_| UpdateError::InvalidUrl)?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).map_err(|_| UpdateError::InvalidUrl)
}

pub(crate) fn validate_bundle_id(value: &str) -> Result<(), UpdateError> {
    if value.is_empty()
        || value.len() > 200
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
    {
        return Err(UpdateError::InvalidBundleId);
    }
    Ok(())
}

fn parse_version(value: &str) -> Result<Version, UpdateError> {
    let trimmed = value.trim_start_matches('v');
    if let Ok(version) = Version::parse(trimmed) {
        return Ok(version);
    }
    let dot_count = trimmed.bytes().filter(|byte| *byte == b'.').count();
    let normalized = match dot_count {
        0 => format!("{trimmed}.0.0"),
        1 => format!("{trimmed}.0"),
        _ => return Err(UpdateError::InvalidVersion(value.to_owned())),
    };
    Version::parse(&normalized).map_err(|_| UpdateError::InvalidVersion(value.to_owned()))
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum UpdateError {
    #[error("manifest size is invalid")]
    ManifestSize,
    #[error("manifest signature is invalid")]
    BadSignature,
    #[error("manifest public key is invalid")]
    BadPublicKey,
    #[error("manifest key id does not match the configured key")]
    WrongKeyId,
    #[error("manifest schema version {0} is unsupported")]
    UnsupportedManifestVersion(u32),
    #[error("manifest JSON or structure is invalid")]
    InvalidManifest,
    #[error("version is invalid: {0}")]
    InvalidVersion(String),
    #[error("update would downgrade without explicit rollback approval")]
    DowngradeDenied,
    #[error("artifact URL must be HTTPS and contain no credentials or fragment")]
    HttpsRequired,
    #[error("artifact URL is invalid")]
    InvalidUrl,
    #[error("artifact URL contains path traversal")]
    PathTraversal,
    #[error("artifact size is invalid")]
    ArtifactSize,
    #[error("artifact SHA-256 is invalid")]
    InvalidHash,
    #[error("application bundle id is invalid")]
    InvalidBundleId,
    #[error("platform is unsupported")]
    UnsupportedPlatform,
    #[error("network request failed")]
    Network,
    #[error("HTTP redirects are not allowed")]
    RedirectDenied,
    #[error("download exceeded its declared or configured size")]
    DownloadTooLarge,
    #[error("downloaded artifact size does not match the manifest")]
    SizeMismatch,
    #[error("downloaded artifact hash does not match the manifest")]
    HashMismatch,
    #[error("staging failed")]
    Staging,
    #[error("update archive is invalid")]
    InvalidArchive,
    #[error("update archive contains an unsafe path or entry type")]
    UnsafeArchiveEntry,
    #[error("update archive has too many entries")]
    TooManyArchiveEntries,
    #[error("update archive expands beyond its configured limit")]
    ExpandedSize,
    #[error("staged bundle metadata does not match the signed manifest")]
    BundleMetadataMismatch,
    #[error("staged bundle executable architecture does not match the signed manifest")]
    BundleArchitectureMismatch,
    #[error("update state transition is invalid")]
    InvalidTransition,
    #[error("update confirmation does not match the available release")]
    ConfirmationMismatch,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn signed_manifest(json: &str) -> (VerifiedManifest, SigningKey) {
        let signing_key = SigningKey::from_bytes(&[7; 32]);
        let signature = signing_key.sign(json.as_bytes());
        let signature = base64::engine::general_purpose::STANDARD.encode(signature.to_bytes());
        let verified = VerifiedManifest::verify(
            SignedManifest {
                bytes: json.as_bytes(),
                signature_base64: &signature,
            },
            "release-2026",
            &signing_key.verifying_key().to_bytes(),
        )
        .unwrap();
        (verified, signing_key)
    }

    fn manifest_json(version: &str, minimum_os: &str, url: &str) -> String {
        format!(
            r#"{{"schemaVersion":1,"keyId":"release-2026","releases":[{{"version":"{version}","channel":"stable","artifacts":[{{"architecture":"aarch64-apple-darwin","minimumMacosVersion":"{minimum_os}","bundleId":"app.agentfactory.desktop","url":"{url}","size":4,"sha256":"{}"}}]}}]}}"#,
            "a".repeat(64),
        )
    }

    #[test]
    fn verifies_signature_over_exact_manifest_bytes() {
        let json = manifest_json("1.1.0", "13.0", "https://updates.example/app.zip");
        let (_, key) = signed_manifest(&json);
        let signature = key.sign(json.as_bytes());
        let encoded = base64::engine::general_purpose::STANDARD.encode(signature.to_bytes());
        let altered = format!("{json}\n");
        assert_eq!(
            VerifiedManifest::verify(
                SignedManifest {
                    bytes: altered.as_bytes(),
                    signature_base64: &encoded,
                },
                "release-2026",
                &key.verifying_key().to_bytes(),
            )
            .unwrap_err(),
            UpdateError::BadSignature,
        );
    }

    #[test]
    fn rejects_wrong_keys_and_bad_signatures() {
        let json = manifest_json("1.1.0", "13.0", "https://updates.example/app.zip");
        let (_, key) = signed_manifest(&json);
        assert_eq!(
            VerifiedManifest::verify(
                SignedManifest {
                    bytes: json.as_bytes(),
                    signature_base64: "not-base64",
                },
                "release-2026",
                &key.verifying_key().to_bytes(),
            )
            .unwrap_err(),
            UpdateError::BadSignature,
        );
    }

    #[test]
    fn selection_enforces_arch_channel_os_and_downgrade() {
        let json = manifest_json("1.2.0", "14.0", "https://updates.example/app.zip");
        let (manifest, _) = signed_manifest(&json);
        let mut request = SelectionRequest {
            current_version: "1.0.0",
            channel: Channel::Stable,
            architecture: Architecture::Aarch64AppleDarwin,
            macos_version: "13.5",
            requested_version: None,
            allow_rollback: false,
        };
        assert!(select_release(&manifest, &request).unwrap().is_none());
        request.macos_version = "14.0";
        assert_eq!(
            select_release(&manifest, &request)
                .unwrap()
                .unwrap()
                .0
                .version,
            "1.2.0"
        );

        let rollback_json = manifest_json("0.9.0", "13.0", "https://updates.example/old.zip");
        let (rollback, _) = signed_manifest(&rollback_json);
        request.requested_version = Some("0.9.0");
        assert_eq!(
            select_release(&rollback, &request),
            Err(UpdateError::DowngradeDenied)
        );
        request.allow_rollback = true;
        assert!(select_release(&rollback, &request).unwrap().is_some());
    }

    #[test]
    fn rejects_insecure_and_traversing_urls() {
        assert_eq!(
            validate_https_url("http://updates.example/app.zip"),
            Err(UpdateError::HttpsRequired),
        );
        assert_eq!(
            validate_https_url("https://updates.example/a/%2e%2e/secret"),
            Err(UpdateError::PathTraversal),
        );
        assert_eq!(
            validate_https_url("https://user:pass@updates.example/app.zip"),
            Err(UpdateError::HttpsRequired),
        );
    }
}
