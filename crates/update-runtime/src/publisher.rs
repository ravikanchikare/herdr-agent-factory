use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use base64::Engine;
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use thiserror::Error;
use zeroize::Zeroize;

use crate::{
    Architecture, Channel, Release, ReleaseArtifact, SignedManifest, UpdateManifest,
    VerifiedManifest,
};

pub const MANIFEST_FILENAME: &str = "agent-factory-update-manifest-v1.json";
pub const SIGNATURE_FILENAME: &str = "agent-factory-update-manifest-v1.json.sig";
pub const PUBLIC_KEY_FILENAME: &str = "agent-factory-update-ed25519.pub";

#[derive(Clone, Debug)]
pub struct ArtifactInput {
    pub architecture: Architecture,
    pub path: PathBuf,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedManifestFiles {
    pub manifest_bytes: Vec<u8>,
    pub signature_base64: String,
    pub public_key_base64: String,
}

pub fn build_and_sign_manifest(
    version: &str,
    channel: Channel,
    minimum_macos_version: &str,
    bundle_id: &str,
    key_id: &str,
    artifacts: &[ArtifactInput],
    signing_seed: &[u8; 32],
) -> Result<SignedManifestFiles, PublishError> {
    if artifacts.len() != 2 {
        return Err(PublishError::ArchitectureSet);
    }
    let expected = BTreeSet::from([
        Architecture::Aarch64AppleDarwin,
        Architecture::X86_64AppleDarwin,
    ]);
    let actual: BTreeSet<_> = artifacts
        .iter()
        .map(|artifact| artifact.architecture)
        .collect();
    if actual != expected {
        return Err(PublishError::ArchitectureSet);
    }

    let mut release_artifacts = Vec::with_capacity(2);
    let mut sorted = artifacts.to_vec();
    sorted.sort_by_key(|artifact| artifact.architecture);
    for input in sorted {
        let metadata = fs::metadata(&input.path).map_err(|_| PublishError::ArtifactRead)?;
        if !metadata.is_file() || metadata.len() == 0 || metadata.len() > crate::MAX_ARTIFACT_BYTES
        {
            return Err(PublishError::ArtifactRead);
        }
        let mut file = File::open(&input.path).map_err(|_| PublishError::ArtifactRead)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = file
                .read(&mut buffer)
                .map_err(|_| PublishError::ArtifactRead)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        release_artifacts.push(ReleaseArtifact {
            architecture: input.architecture,
            minimum_macos_version: minimum_macos_version.to_owned(),
            bundle_id: bundle_id.to_owned(),
            url: input.url,
            size: metadata.len(),
            sha256: hex::encode(hasher.finalize()),
        });
    }

    let manifest = UpdateManifest {
        schema_version: crate::manifest::MANIFEST_SCHEMA_VERSION,
        key_id: key_id.to_owned(),
        releases: vec![Release {
            version: version.to_owned(),
            channel,
            artifacts: release_artifacts,
        }],
    };
    let manifest_bytes = serde_json::to_vec(&manifest).map_err(|_| PublishError::Manifest)?;
    let signing_key = SigningKey::from_bytes(signing_seed);
    let signature = signing_key.sign(&manifest_bytes);
    let signature_base64 = base64::engine::general_purpose::STANDARD.encode(signature.to_bytes());
    let public_key = signing_key.verifying_key().to_bytes();
    let public_key_base64 = base64::engine::general_purpose::STANDARD.encode(public_key);

    VerifiedManifest::verify(
        SignedManifest {
            bytes: &manifest_bytes,
            signature_base64: &signature_base64,
        },
        key_id,
        &public_key,
    )
    .map_err(|_| PublishError::Manifest)?;

    Ok(SignedManifestFiles {
        manifest_bytes,
        signature_base64,
        public_key_base64,
    })
}

pub fn write_signed_manifest(
    output_directory: &Path,
    signed: &SignedManifestFiles,
) -> Result<(), PublishError> {
    fs::create_dir_all(output_directory).map_err(|_| PublishError::Output)?;
    let outputs = [
        (MANIFEST_FILENAME, signed.manifest_bytes.as_slice()),
        (SIGNATURE_FILENAME, signed.signature_base64.as_bytes()),
        (PUBLIC_KEY_FILENAME, signed.public_key_base64.as_bytes()),
    ];
    if outputs
        .iter()
        .any(|(name, _)| output_directory.join(name).exists())
    {
        return Err(PublishError::OutputExists);
    }

    let mut temporary_files = Vec::with_capacity(outputs.len());
    for (_, bytes) in &outputs {
        let mut temporary =
            NamedTempFile::new_in(output_directory).map_err(|_| PublishError::Output)?;
        temporary
            .write_all(bytes)
            .and_then(|()| temporary.as_file().sync_all())
            .map_err(|_| PublishError::Output)?;
        temporary_files.push(temporary);
    }
    for ((name, _), temporary) in outputs.iter().zip(temporary_files) {
        temporary
            .persist_noclobber(output_directory.join(name))
            .map_err(|_| PublishError::Output)?;
    }
    File::open(output_directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| PublishError::Output)
}

pub fn decode_signing_seed_base64(encoded: &mut String) -> Result<[u8; 32], PublishError> {
    let decoded = base64::engine::general_purpose::STANDARD.decode(encoded.trim());
    encoded.zeroize();
    let mut decoded = decoded.map_err(|_| PublishError::SigningKey)?;
    if decoded.len() != 32 {
        decoded.zeroize();
        return Err(PublishError::SigningKey);
    }
    let mut seed = [0_u8; 32];
    seed.copy_from_slice(&decoded);
    decoded.zeroize();
    Ok(seed)
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PublishError {
    #[error("exactly one arm64 and one x86_64 artifact are required")]
    ArchitectureSet,
    #[error("release artifact could not be read or is outside size limits")]
    ArtifactRead,
    #[error("manifest inputs are invalid")]
    Manifest,
    #[error("signing key must be a base64-encoded 32-byte Ed25519 seed")]
    SigningKey,
    #[error("signed manifest output could not be written durably")]
    Output,
    #[error("signed manifest output already exists")]
    OutputExists,
    #[error("publisher arguments are invalid")]
    Arguments,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(directory: &Path) -> Vec<ArtifactInput> {
        let arm = directory.join("agent-factory-arm64.zip");
        let intel = directory.join("agent-factory-x86_64.zip");
        fs::write(&arm, b"arm64 artifact").unwrap();
        fs::write(&intel, b"intel artifact").unwrap();
        vec![
            ArtifactInput {
                architecture: Architecture::X86_64AppleDarwin,
                path: intel,
                url: "https://releases.example/agent-factory-x86_64.zip".to_owned(),
            },
            ArtifactInput {
                architecture: Architecture::Aarch64AppleDarwin,
                path: arm,
                url: "https://releases.example/agent-factory-arm64.zip".to_owned(),
            },
        ]
    }

    #[test]
    fn build_is_deterministic_and_verifiable() {
        let temp = tempfile::tempdir().unwrap();
        let inputs = inputs(temp.path());
        let first = build_and_sign_manifest(
            "1.2.3",
            Channel::Stable,
            "13.0",
            "app.agentfactory.desktop",
            "release-2026",
            &inputs,
            &[9; 32],
        )
        .unwrap();
        let mut reversed = inputs.clone();
        reversed.reverse();
        let second = build_and_sign_manifest(
            "1.2.3",
            Channel::Stable,
            "13.0",
            "app.agentfactory.desktop",
            "release-2026",
            &reversed,
            &[9; 32],
        )
        .unwrap();
        assert_eq!(first, second);
        assert!(
            std::str::from_utf8(&first.manifest_bytes)
                .unwrap()
                .contains("aarch64-apple-darwin"),
        );
    }

    #[test]
    fn requires_both_release_architectures() {
        let temp = tempfile::tempdir().unwrap();
        let inputs = inputs(temp.path());
        assert_eq!(
            build_and_sign_manifest(
                "1.2.3",
                Channel::Stable,
                "13.0",
                "app.agentfactory.desktop",
                "release-2026",
                &inputs[..1],
                &[9; 32],
            ),
            Err(PublishError::ArchitectureSet),
        );
    }

    #[test]
    fn writes_without_overwriting_existing_release_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let signed = build_and_sign_manifest(
            "1.2.3",
            Channel::Stable,
            "13.0",
            "app.agentfactory.desktop",
            "release-2026",
            &inputs(temp.path()),
            &[9; 32],
        )
        .unwrap();
        let output = temp.path().join("output");
        write_signed_manifest(&output, &signed).unwrap();
        assert_eq!(
            write_signed_manifest(&output, &signed),
            Err(PublishError::OutputExists),
        );
    }
}
