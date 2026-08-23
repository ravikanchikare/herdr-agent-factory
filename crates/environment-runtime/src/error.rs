use std::path::PathBuf;

use platform_secrets::{SecretError, SecretRef};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EnvironmentError {
    #[error("environment I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("environment path is unsafe: {0}")]
    UnsafePath(String),
    #[error("environment catalog exceeds the configured limit: {0}")]
    LimitExceeded(String),
    #[error("invalid environment descriptor at {path}: {message}")]
    InvalidDescriptor { path: PathBuf, message: String },
    #[error("duplicate environment ID {0:?}")]
    DuplicateEnvironment(String),
    #[error("environment {0:?} was not found")]
    NotFound(String),
    #[error(
        "environment {environment_id:?} selects {role} harness {harness_id:?}, which Herdr does not provide"
    )]
    UnsupportedHarness {
        environment_id: String,
        role: &'static str,
        harness_id: String,
    },
    #[error("secret {reference} could not be resolved: {source}")]
    SecretResolution {
        reference: SecretRef,
        #[source]
        source: SecretError,
    },
    #[error("secret {0} is not valid UTF-8")]
    SecretNotUtf8(SecretRef),
    #[error("plugin plan for environment {environment_id:?} is invalid: {message}")]
    PluginPlan {
        environment_id: String,
        message: String,
    },
}

impl EnvironmentError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    pub(crate) fn invalid(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self::InvalidDescriptor {
            path: path.into(),
            message: message.into(),
        }
    }
}

pub type Result<T> = std::result::Result<T, EnvironmentError>;
