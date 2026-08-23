#![forbid(unsafe_code)]
//! Validated application-level LLM Provider configuration.

use std::collections::BTreeSet;

use platform_secrets::SecretRef;
use serde::{Deserialize, Serialize};

pub const MAX_PROVIDER_NAME_CHARS: usize = 128;
pub const MAX_MODEL_ID_CHARS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LlmProviderKind {
    Ollama,
    Litellm,
    Meta,
    OpenAi,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LlmProviderConfiguration {
    #[serde(rename = "type")]
    pub provider_type: LlmProviderKind,
    pub endpoint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_ref: Option<SecretRef>,
    pub allowed_models: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum LlmProviderError {
    #[error("Intelligence Provider name is empty, malformed, or too long")]
    InvalidName,
    #[error("invalid Intelligence Provider endpoint: {0}")]
    InvalidEndpoint(String),
    #[error("Intelligence Provider exposes no allowed models")]
    MissingModels,
    #[error("invalid model id: {0}")]
    InvalidModel(String),
    #[error("duplicate model {0:?}")]
    DuplicateModel(String),
}

pub fn validate_provider_name(name: &str) -> Result<(), LlmProviderError> {
    if name.is_empty()
        || name.trim() != name
        || name.chars().count() > MAX_PROVIDER_NAME_CHARS
        || name.chars().any(char::is_control)
    {
        return Err(LlmProviderError::InvalidName);
    }
    Ok(())
}

pub fn validate_model_id(model: &str) -> Result<(), LlmProviderError> {
    if model.is_empty()
        || model.trim() != model
        || model.chars().count() > MAX_MODEL_ID_CHARS
        || model.chars().any(char::is_control)
    {
        return Err(LlmProviderError::InvalidModel(
            "must be non-empty, trimmed, control-free, and at most 256 characters".into(),
        ));
    }
    Ok(())
}

impl LlmProviderConfiguration {
    pub fn validate(&self) -> Result<(), LlmProviderError> {
        self.validate_for_discovery()?;
        if self.allowed_models.is_empty() {
            return Err(LlmProviderError::MissingModels);
        }
        let mut models = BTreeSet::new();
        for model in &self.allowed_models {
            validate_model_id(model)?;
            if !models.insert(model.as_str()) {
                return Err(LlmProviderError::DuplicateModel(model.clone()));
            }
        }
        Ok(())
    }

    pub fn validate_for_discovery(&self) -> Result<(), LlmProviderError> {
        validate_provider_url(&self.endpoint).map_err(LlmProviderError::InvalidEndpoint)
    }
}

fn validate_provider_url(value: &str) -> Result<(), String> {
    let url = url::Url::parse(value).map_err(|_| "must be an absolute URL".to_owned())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("scheme must be HTTP or HTTPS".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("userinfo is not allowed".into());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("query strings and fragments are not allowed".into());
    }
    let host = url.host().ok_or_else(|| "host is required".to_owned())?;
    if url.scheme() == "http" {
        let loopback = match host {
            url::Host::Ipv4(address) => address.is_loopback(),
            url::Host::Ipv6(address) => address.is_loopback(),
            url::Host::Domain(name) => name.eq_ignore_ascii_case("localhost"),
        };
        if !loopback {
            return Err("HTTP is allowed only for loopback hosts".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_provider_policy() {
        let provider = LlmProviderConfiguration {
            provider_type: LlmProviderKind::Ollama,
            endpoint: "http://127.0.0.1:11434".into(),
            credential_ref: None,
            allowed_models: vec!["qwen3-coder".into()],
        };
        provider.validate().unwrap();
    }

    #[test]
    fn rejects_non_loopback_http() {
        let provider = LlmProviderConfiguration {
            provider_type: LlmProviderKind::OpenAi,
            endpoint: "http://example.com/v1".into(),
            credential_ref: None,
            allowed_models: vec!["model".into()],
        };
        assert!(matches!(
            provider.validate(),
            Err(LlmProviderError::InvalidEndpoint(_))
        ));
    }
}
