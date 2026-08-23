use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid plugin manifest: {0}")]
    InvalidManifest(String),
    #[error("invalid registry catalog: {0}")]
    InvalidCatalog(String),
    #[error("registry catalog signature is invalid: {0}")]
    InvalidSignature(String),
    #[error("artifact integrity check failed: {0}")]
    Integrity(String),
    #[error("archive rejected: {0}")]
    UnsafeArchive(String),
    #[error("plugin path rejected: {0}")]
    UnsafePath(String),
    #[error("plugin is not installed: {0}")]
    NotInstalled(String),
    #[error("plugin version is already installed: {name}@{version}")]
    AlreadyInstalled { name: String, version: String },
    #[error("plugin has no previous version to roll back to: {0}")]
    NoRollback(String),
    #[error("registry entry does not match artifact: {0}")]
    RegistryMismatch(String),
    #[error("environment plugin selection is invalid: {0}")]
    InvalidEnvironmentSelection(String),
    #[error("registry source URL is not a recognized GitHub repository: {0}")]
    InvalidRegistrySource(String),
    #[error("registry URL must be HTTPS without credentials or a fragment")]
    HttpsRequired,
    #[error("registry redirect was denied")]
    RedirectDenied,
    #[error("registry download failed")]
    Network,
    #[error("registry download exceeds {limit} bytes")]
    DownloadTooLarge { limit: u64 },
}

impl PluginError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

pub type Result<T> = std::result::Result<T, PluginError>;
