use std::io::Cursor;
use std::time::Duration;

use url::Url;

use crate::InspectedPlugin;
use crate::error::{PluginError, Result};
use crate::registry::{VerifiedCatalog, VerifiedRegistryPlugin, verify_catalog};
use crate::store::{InstalledPlugin, PluginStore};

const MAX_CATALOG_BYTES: u64 = 8 * 1024 * 1024;
const MAX_SIGNATURE_BYTES: u64 = 1024;

pub trait RegistryDownloader: Send + Sync {
    /// Returns at most `limit` bytes and must not follow redirects.
    fn get(&self, url: &str, limit: u64) -> Result<Vec<u8>>;
}

pub struct HttpsRegistryDownloader {
    agent: ureq::Agent,
}

impl Default for HttpsRegistryDownloader {
    fn default() -> Self {
        let config = ureq::config::Config::builder()
            .max_redirects(0)
            .timeout_global(Some(Duration::from_secs(30)))
            .build();
        Self {
            agent: config.into(),
        }
    }
}

impl RegistryDownloader for HttpsRegistryDownloader {
    fn get(&self, url: &str, limit: u64) -> Result<Vec<u8>> {
        validate_registry_url(url)?;
        let mut response = self.agent.get(url).call().map_err(|error| {
            let message = error.to_string();
            if message.contains("redirect") || message.contains("status code 3") {
                PluginError::RedirectDenied
            } else {
                PluginError::Network
            }
        })?;
        if response.status().is_redirection() {
            return Err(PluginError::RedirectDenied);
        }
        let bytes = response
            .body_mut()
            .with_config()
            .limit(limit.saturating_add(1))
            .read_to_vec()
            .map_err(|_| PluginError::DownloadTooLarge { limit })?;
        enforce_download_limit(bytes, limit)
    }
}

pub struct RegistryClient<D> {
    downloader: D,
    store: PluginStore,
    public_key: [u8; 32],
}

impl<D: RegistryDownloader> RegistryClient<D> {
    pub fn new(downloader: D, store: PluginStore, public_key: [u8; 32]) -> Self {
        Self {
            downloader,
            store,
            public_key,
        }
    }

    pub fn fetch_catalog(&self, catalog_url: &str, signature_url: &str) -> Result<VerifiedCatalog> {
        validate_registry_url(catalog_url)?;
        validate_registry_url(signature_url)?;
        let catalog = enforce_download_limit(
            self.downloader.get(catalog_url, MAX_CATALOG_BYTES)?,
            MAX_CATALOG_BYTES,
        )?;
        let signature = enforce_download_limit(
            self.downloader.get(signature_url, MAX_SIGNATURE_BYTES)?,
            MAX_SIGNATURE_BYTES,
        )?;
        let signature = std::str::from_utf8(&signature)
            .map_err(|_| PluginError::InvalidSignature("signature is not UTF-8".into()))?;
        verify_catalog(&catalog, signature, &self.public_key)
    }

    pub fn download_and_install(
        &self,
        plugin: VerifiedRegistryPlugin<'_>,
    ) -> Result<InstalledPlugin> {
        let entry = plugin.entry();
        validate_registry_url(&entry.archive_url)?;
        let limit = self.store.archive_limits().max_compressed_bytes;
        let bytes = enforce_download_limit(self.downloader.get(&entry.archive_url, limit)?, limit)?;
        let staged = self.store.stage(Cursor::new(bytes))?;
        self.store.install_and_activate(plugin, &staged)
    }

    pub fn download_and_inspect(
        &self,
        plugin: VerifiedRegistryPlugin<'_>,
    ) -> Result<InspectedPlugin> {
        let entry = plugin.entry();
        validate_registry_url(&entry.archive_url)?;
        let limit = self.store.archive_limits().max_compressed_bytes;
        let bytes = enforce_download_limit(self.downloader.get(&entry.archive_url, limit)?, limit)?;
        let staged = self.store.stage(Cursor::new(bytes))?;
        self.store.inspect_registry_plugin(plugin, &staged)
    }
}

pub fn validate_registry_url(value: &str) -> Result<Url> {
    let url = Url::parse(value).map_err(|_| PluginError::HttpsRequired)?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(PluginError::HttpsRequired);
    }
    Ok(url)
}

fn enforce_download_limit(bytes: Vec<u8>, limit: u64) -> Result<Vec<u8>> {
    if bytes.len() as u64 > limit {
        Err(PluginError::DownloadTooLarge { limit })
    } else {
        Ok(bytes)
    }
}
