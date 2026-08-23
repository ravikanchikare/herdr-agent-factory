//! Signed update manifest verification, target selection, bounded download,
//! artifact staging, and an explicit user-confirmed lifecycle.

mod archive;
mod client;
mod config;
mod manifest;
mod publisher;
mod state;

pub use client::{
    ArtifactDownloader, DownloadPolicy, HttpsDownloader, MemoryDownloader, StagedArtifact,
    UpdateClient,
};
pub use config::{
    EXPECTED_APP_BUNDLE_ID, LoadedUpdateClientConfig, UPDATE_CLIENT_CONFIG_FILENAME,
    UpdateClientConfig, UpdateConfigError, UpdateConfigLoadStatus,
    load_packaged_update_client_config, load_update_client_config, packaged_update_config_path,
    read_update_client_config, write_update_client_config,
};
pub use manifest::{
    Architecture, Channel, Release, ReleaseArtifact, SelectionRequest, SignedManifest, UpdateError,
    UpdateManifest, VerifiedManifest, select_release, validate_https_url,
};
pub use publisher::{
    ArtifactInput, PublishError, SignedManifestFiles, build_and_sign_manifest,
    decode_signing_seed_base64, write_signed_manifest,
};
pub use state::{UpdateState, UpdateStateMachine};

pub const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
pub const MAX_ARTIFACT_BYTES: u64 = 1024 * 1024 * 1024;
pub use archive::{ExtractedBundle, extract_macos_bundle};
