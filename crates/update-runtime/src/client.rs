use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use url::{Host, Url};

use crate::{
    MAX_ARTIFACT_BYTES, MAX_MANIFEST_BYTES, Release, ReleaseArtifact, SignedManifest, UpdateError,
    VerifiedManifest, validate_https_url,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadPolicy {
    Production,
    #[cfg(test)]
    TestOnlyAllowHttp,
}

pub trait ArtifactDownloader: Send + Sync {
    fn get(&self, url: &str, limit: u64, policy: DownloadPolicy) -> Result<Vec<u8>, UpdateError>;
}

pub struct HttpsDownloader {
    agent: ureq::Agent,
}

impl Default for HttpsDownloader {
    fn default() -> Self {
        let config = ureq::config::Config::builder()
            .max_redirects(0)
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .timeout_recv_body(Some(Duration::from_secs(60)))
            .timeout_global(Some(Duration::from_secs(30 * 60)))
            .build();
        Self {
            agent: config.into(),
        }
    }
}

impl ArtifactDownloader for HttpsDownloader {
    fn get(&self, url: &str, limit: u64, policy: DownloadPolicy) -> Result<Vec<u8>, UpdateError> {
        if policy != DownloadPolicy::Production {
            return Err(UpdateError::HttpsRequired);
        }
        download_with_redirect_policy(&self.agent, url, limit)
    }
}

enum HttpResponse {
    Data(Vec<u8>),
    Redirect(String),
}

trait HttpTransport {
    fn get_once(&self, url: &str, limit: u64) -> Result<HttpResponse, UpdateError>;
}

impl HttpTransport for ureq::Agent {
    fn get_once(&self, url: &str, limit: u64) -> Result<HttpResponse, UpdateError> {
        // Requests intentionally carry no authorization or cookie headers, so
        // redirect hops cannot forward ambient credentials.
        let mut response = self.get(url).call().map_err(|_| UpdateError::Network)?;
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get("location")
                .and_then(|value| value.to_str().ok())
                .filter(|value| !value.is_empty())
                .ok_or(UpdateError::RedirectDenied)?;
            return Ok(HttpResponse::Redirect(location.to_owned()));
        }
        let bytes = response
            .body_mut()
            .with_config()
            .limit(limit.saturating_add(1))
            .read_to_vec()
            .map_err(|_| UpdateError::DownloadTooLarge)?;
        if bytes.len() as u64 > limit {
            return Err(UpdateError::DownloadTooLarge);
        }
        Ok(HttpResponse::Data(bytes))
    }
}

fn download_with_redirect_policy(
    transport: &impl HttpTransport,
    initial_url: &str,
    limit: u64,
) -> Result<Vec<u8>, UpdateError> {
    let initial = validate_https_url(initial_url)?;
    let github_redirects_allowed = is_github_update_url(&initial);
    let mut current = initial;
    let mut visited = BTreeSet::new();

    for redirect_count in 0..=3 {
        if !visited.insert(current.as_str().to_owned()) {
            return Err(UpdateError::RedirectDenied);
        }
        match transport.get_once(current.as_str(), limit)? {
            HttpResponse::Data(bytes) => return Ok(bytes),
            HttpResponse::Redirect(location) => {
                if !github_redirects_allowed || redirect_count == 3 {
                    return Err(UpdateError::RedirectDenied);
                }
                let target = current
                    .join(&location)
                    .map_err(|_| UpdateError::RedirectDenied)?;
                validate_github_redirect_target(&target)?;
                current = target;
            }
        }
    }
    Err(UpdateError::RedirectDenied)
}

fn is_github_update_url(url: &Url) -> bool {
    if url.host_str() != Some("github.com") || url.port().is_some() {
        return false;
    }
    let segments: Vec<_> = url
        .path_segments()
        .map(Iterator::collect)
        .unwrap_or_default();
    if segments.len() != 6 || segments[0].is_empty() || segments[1].is_empty() {
        return false;
    }
    if segments[2] != "releases" {
        return false;
    }
    let release_path = (segments[3] == "latest" && segments[4] == "download")
        || (segments[3] == "download" && !segments[4].is_empty());
    if !release_path {
        return false;
    }
    let filename = segments[5];
    matches!(
        filename,
        "agent-factory-update-manifest-v1.json" | "agent-factory-update-manifest-v1.json.sig"
    ) || (filename.starts_with("agent-factory-") && filename.ends_with(".zip"))
}

fn validate_github_redirect_target(url: &Url) -> Result<(), UpdateError> {
    validate_https_url(url.as_str()).map_err(|_| UpdateError::RedirectDenied)?;
    if url.port().is_some() || !matches!(url.host(), Some(Host::Domain(_))) {
        return Err(UpdateError::RedirectDenied);
    }
    match url.host_str() {
        Some("github.com") if is_github_update_url(url) => Ok(()),
        Some("release-assets.githubusercontent.com" | "objects.githubusercontent.com") => Ok(()),
        _ => Err(UpdateError::RedirectDenied),
    }
}

#[derive(Clone)]
enum MemoryResponse {
    Data(Vec<u8>),
    Redirect,
}

#[derive(Clone, Default)]
pub struct MemoryDownloader {
    responses: Arc<RwLock<BTreeMap<String, MemoryResponse>>>,
}

impl MemoryDownloader {
    pub fn insert(&self, url: impl Into<String>, bytes: impl Into<Vec<u8>>) {
        self.responses
            .write()
            .unwrap()
            .insert(url.into(), MemoryResponse::Data(bytes.into()));
    }

    pub fn insert_redirect(&self, url: impl Into<String>) {
        self.responses
            .write()
            .unwrap()
            .insert(url.into(), MemoryResponse::Redirect);
    }
}

impl ArtifactDownloader for MemoryDownloader {
    fn get(&self, url: &str, limit: u64, _policy: DownloadPolicy) -> Result<Vec<u8>, UpdateError> {
        let response = self
            .responses
            .read()
            .map_err(|_| UpdateError::Network)?
            .get(url)
            .cloned()
            .ok_or(UpdateError::Network)?;
        let bytes = match response {
            MemoryResponse::Data(bytes) => bytes,
            MemoryResponse::Redirect => return Err(UpdateError::RedirectDenied),
        };
        if bytes.len() as u64 > limit {
            return Err(UpdateError::DownloadTooLarge);
        }
        Ok(bytes)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StagedArtifact {
    pub(crate) path: PathBuf,
    pub(crate) release: Release,
    pub(crate) artifact: ReleaseArtifact,
}

impl StagedArtifact {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn release(&self) -> &Release {
        &self.release
    }

    pub fn artifact(&self) -> &ReleaseArtifact {
        &self.artifact
    }
}

pub struct UpdateClient<D> {
    downloader: D,
    policy: DownloadPolicy,
    staging_dir: PathBuf,
}

impl<D: ArtifactDownloader> UpdateClient<D> {
    pub fn new(downloader: D, staging_dir: PathBuf) -> Self {
        Self {
            downloader,
            policy: DownloadPolicy::Production,
            staging_dir,
        }
    }

    #[cfg(test)]
    pub fn for_tests(downloader: D, staging_dir: PathBuf) -> Self {
        Self {
            downloader,
            policy: DownloadPolicy::TestOnlyAllowHttp,
            staging_dir,
        }
    }

    pub fn fetch_and_verify_manifest(
        &self,
        manifest_url: &str,
        signature_url: &str,
        expected_key_id: &str,
        public_key: &[u8; 32],
    ) -> Result<VerifiedManifest, UpdateError> {
        if self.policy == DownloadPolicy::Production {
            validate_https_url(manifest_url)?;
            validate_https_url(signature_url)?;
        }
        let manifest = self
            .downloader
            .get(manifest_url, MAX_MANIFEST_BYTES, self.policy)?;
        let signature = self.downloader.get(signature_url, 1024, self.policy)?;
        let signature = std::str::from_utf8(&signature).map_err(|_| UpdateError::BadSignature)?;
        VerifiedManifest::verify(
            SignedManifest {
                bytes: &manifest,
                signature_base64: signature,
            },
            expected_key_id,
            public_key,
        )
    }

    pub fn download_and_stage(
        &self,
        release: &Release,
        artifact: &ReleaseArtifact,
    ) -> Result<StagedArtifact, UpdateError> {
        if self.policy == DownloadPolicy::Production {
            validate_https_url(&artifact.url)?;
        }
        if artifact.size == 0 || artifact.size > MAX_ARTIFACT_BYTES {
            return Err(UpdateError::ArtifactSize);
        }
        let bytes = self.downloader.get(
            &artifact.url,
            artifact.size.min(MAX_ARTIFACT_BYTES),
            self.policy,
        )?;
        if bytes.len() as u64 != artifact.size {
            return Err(UpdateError::SizeMismatch);
        }
        let actual_hash = hex::encode(Sha256::digest(&bytes));
        if !constant_time_hex_eq(&actual_hash, &artifact.sha256) {
            return Err(UpdateError::HashMismatch);
        }

        fs::create_dir_all(&self.staging_dir).map_err(|_| UpdateError::Staging)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.staging_dir, fs::Permissions::from_mode(0o700))
                .map_err(|_| UpdateError::Staging)?;
        }
        let mut staged =
            NamedTempFile::new_in(&self.staging_dir).map_err(|_| UpdateError::Staging)?;
        staged.write_all(&bytes).map_err(|_| UpdateError::Staging)?;
        staged
            .as_file()
            .sync_all()
            .map_err(|_| UpdateError::Staging)?;
        let filename = format!(
            "update-{}-{}.artifact",
            release.version, artifact.architecture
        );
        let final_path = self.staging_dir.join(filename);
        if final_path.exists() {
            return Err(UpdateError::Staging);
        }
        staged
            .persist(&final_path)
            .map_err(|_| UpdateError::Staging)?;
        sync_dir(&self.staging_dir)?;
        Ok(StagedArtifact {
            path: final_path,
            release: release.clone(),
            artifact: artifact.clone(),
        })
    }
}

fn constant_time_hex_eq(left: &str, right: &str) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.bytes()
        .zip(right.bytes().map(|byte| byte.to_ascii_lowercase()))
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

fn sync_dir(path: &Path) -> Result<(), UpdateError> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|_| UpdateError::Staging)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::{Architecture, Channel};

    fn release_and_artifact(payload: &[u8]) -> (Release, ReleaseArtifact) {
        let artifact = ReleaseArtifact {
            architecture: Architecture::Aarch64AppleDarwin,
            minimum_macos_version: "13.0".to_owned(),
            bundle_id: "app.agentfactory.desktop".to_owned(),
            url: "http://127.0.0.1/artifact".to_owned(),
            size: payload.len() as u64,
            sha256: hex::encode(Sha256::digest(payload)),
        };
        (
            Release {
                version: "1.1.0".to_owned(),
                channel: Channel::Stable,
                artifacts: vec![artifact.clone()],
            },
            artifact,
        )
    }

    #[test]
    fn stages_only_exact_size_and_hash() {
        let payload = b"signed release archive";
        let (release, artifact) = release_and_artifact(payload);
        let downloader = MemoryDownloader::default();
        downloader.insert(&artifact.url, payload.to_vec());
        let temp = tempfile::tempdir().unwrap();
        let client = UpdateClient::for_tests(downloader.clone(), temp.path().to_path_buf());
        let staged = client.download_and_stage(&release, &artifact).unwrap();
        assert_eq!(fs::read(staged.path()).unwrap(), payload);

        let mut bad_hash = artifact.clone();
        bad_hash.sha256 = "0".repeat(64);
        assert_eq!(
            client.download_and_stage(&release, &bad_hash),
            Err(UpdateError::HashMismatch),
        );

        downloader.insert(&artifact.url, b"short".to_vec());
        assert_eq!(
            client.download_and_stage(&release, &artifact),
            Err(UpdateError::SizeMismatch),
        );
    }

    #[test]
    fn rejects_downloads_larger_than_the_bound() {
        let downloader = MemoryDownloader::default();
        downloader.insert("memory://large", vec![0; 5]);
        assert_eq!(
            downloader.get("memory://large", 4, DownloadPolicy::TestOnlyAllowHttp,),
            Err(UpdateError::DownloadTooLarge),
        );
    }

    #[test]
    fn redirects_are_rejected() {
        let downloader = MemoryDownloader::default();
        downloader.insert_redirect("https://updates.example/redirect");
        assert_eq!(
            downloader.get(
                "https://updates.example/redirect",
                1024,
                DownloadPolicy::Production,
            ),
            Err(UpdateError::RedirectDenied),
        );
    }

    #[derive(Clone)]
    enum FakeHttpResponse {
        Data(Vec<u8>),
        Redirect(String),
    }

    #[derive(Default)]
    struct FakeHttpTransport {
        responses: BTreeMap<String, FakeHttpResponse>,
        fetched: Mutex<Vec<String>>,
    }

    impl FakeHttpTransport {
        fn redirect(mut self, from: &str, to: &str) -> Self {
            self.responses
                .insert(from.to_owned(), FakeHttpResponse::Redirect(to.to_owned()));
            self
        }

        fn data(mut self, url: &str, bytes: &[u8]) -> Self {
            self.responses
                .insert(url.to_owned(), FakeHttpResponse::Data(bytes.to_vec()));
            self
        }

        fn fetched(&self) -> Vec<String> {
            self.fetched.lock().unwrap().clone()
        }
    }

    impl HttpTransport for FakeHttpTransport {
        fn get_once(&self, url: &str, _limit: u64) -> Result<HttpResponse, UpdateError> {
            self.fetched.lock().unwrap().push(url.to_owned());
            match self.responses.get(url).cloned() {
                Some(FakeHttpResponse::Data(bytes)) => Ok(HttpResponse::Data(bytes)),
                Some(FakeHttpResponse::Redirect(location)) => Ok(HttpResponse::Redirect(location)),
                None => Err(UpdateError::Network),
            }
        }
    }

    #[test]
    fn github_latest_redirects_to_tag_then_asset() {
        let latest = "https://github.com/example/agent-factory/releases/latest/download/agent-factory-update-manifest-v1.json";
        let tagged = "https://github.com/example/agent-factory/releases/download/v1.2.3/agent-factory-update-manifest-v1.json";
        let asset = "https://release-assets.githubusercontent.com/github-production-release-asset/123/manifest";
        let transport = FakeHttpTransport::default()
            .redirect(latest, tagged)
            .redirect(tagged, asset)
            .data(asset, b"signed manifest");

        assert_eq!(
            download_with_redirect_policy(&transport, latest, 1024).unwrap(),
            b"signed manifest",
        );
        assert_eq!(transport.fetched(), vec![latest, tagged, asset]);
    }

    #[test]
    fn github_redirects_reject_evil_private_and_local_targets_before_fetch() {
        let latest = "https://github.com/example/agent-factory/releases/latest/download/agent-factory-update-manifest-v1.json";
        for target in [
            "https://evil.example/manifest",
            "https://127.0.0.1/manifest",
            "https://localhost/manifest",
            "http://release-assets.githubusercontent.com/manifest",
            "https://user:pass@release-assets.githubusercontent.com/manifest",
            "https://release-assets.githubusercontent.com/manifest#replacement",
        ] {
            let transport = FakeHttpTransport::default().redirect(latest, target);
            assert_eq!(
                download_with_redirect_policy(&transport, latest, 1024),
                Err(UpdateError::RedirectDenied),
                "target should be rejected: {target}",
            );
            assert_eq!(transport.fetched(), vec![latest]);
        }
    }

    #[test]
    fn github_redirects_reject_loops_and_more_than_three_hops() {
        let latest = "https://github.com/example/agent-factory/releases/latest/download/agent-factory-update-manifest-v1.json";
        let tagged = "https://github.com/example/agent-factory/releases/download/v1.2.3/agent-factory-update-manifest-v1.json";
        let looping = FakeHttpTransport::default()
            .redirect(latest, tagged)
            .redirect(tagged, latest);
        assert_eq!(
            download_with_redirect_policy(&looping, latest, 1024),
            Err(UpdateError::RedirectDenied),
        );
        assert_eq!(looping.fetched(), vec![latest, tagged]);

        let asset_one = "https://release-assets.githubusercontent.com/one";
        let asset_two = "https://objects.githubusercontent.com/two";
        let denied_before_fetch = "https://objects.githubusercontent.com/four";
        let too_many = FakeHttpTransport::default()
            .redirect(latest, tagged)
            .redirect(tagged, asset_one)
            .redirect(asset_one, asset_two)
            .redirect(asset_two, denied_before_fetch)
            .data(denied_before_fetch, b"must not fetch");
        assert_eq!(
            download_with_redirect_policy(&too_many, latest, 1024),
            Err(UpdateError::RedirectDenied),
        );
        assert_eq!(
            too_many.fetched(),
            vec![latest, tagged, asset_one, asset_two]
        );
        assert!(!too_many.fetched().contains(&denied_before_fetch.to_owned()));
    }

    #[test]
    fn non_github_downloads_keep_redirect_deny() {
        let generic = "https://updates.example/manifest.json";
        let target = "https://release-assets.githubusercontent.com/manifest";
        let transport = FakeHttpTransport::default().redirect(generic, target);
        assert_eq!(
            download_with_redirect_policy(&transport, generic, 1024),
            Err(UpdateError::RedirectDenied),
        );
        assert_eq!(transport.fetched(), vec![generic]);
    }

    #[test]
    fn never_overwrites_an_existing_staged_artifact() {
        let payload = b"signed release archive";
        let (release, artifact) = release_and_artifact(payload);
        let downloader = MemoryDownloader::default();
        downloader.insert(&artifact.url, payload.to_vec());
        let temp = tempfile::tempdir().unwrap();
        let client = UpdateClient::for_tests(downloader, temp.path().to_path_buf());
        let staged = client.download_and_stage(&release, &artifact).unwrap();
        assert_eq!(
            client.download_and_stage(&release, &artifact),
            Err(UpdateError::Staging),
        );
        assert_eq!(fs::read(staged.path()).unwrap(), payload);
    }
}
