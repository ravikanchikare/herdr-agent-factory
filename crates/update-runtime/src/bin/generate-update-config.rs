use std::env;
use std::path::PathBuf;

use update_runtime::{
    Channel, EXPECTED_APP_BUNDLE_ID, UpdateClientConfig, UpdateConfigError,
    write_update_client_config,
};

const MANIFEST_FILENAME: &str = "agent-factory-update-manifest-v1.json";

fn main() {
    if let Err(error) = run() {
        eprintln!("update config generation failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), GenerateError> {
    let output = output_path(env::args().skip(1))?;
    let config = if env::var("RELEASE_UPDATE_CONFIG").as_deref() == Ok("1") {
        let repository = required_env("GITHUB_REPOSITORY")?;
        let release_tag = required_env("RELEASE_TAG")?;
        let key_id = required_env("UPDATE_MANIFEST_KEY_ID")?;
        let public_key = required_env("UPDATE_MANIFEST_PUBLIC_KEY_BASE64")?;
        release_config(&repository, &release_tag, &key_id, &public_key)?
    } else {
        UpdateClientConfig::disabled()
    };
    write_update_client_config(&output, &config)?;
    Ok(())
}

fn release_config(
    repository: &str,
    release_tag: &str,
    key_id: &str,
    public_key: &str,
) -> Result<UpdateClientConfig, GenerateError> {
    validate_repository(repository)?;
    validate_release_tag(release_tag)?;
    let base_url = format!("https://github.com/{repository}/releases/latest/download");
    let manifest_url = format!("{base_url}/{MANIFEST_FILENAME}");
    Ok(UpdateClientConfig::enabled(
        Channel::Stable,
        &manifest_url,
        &format!("{manifest_url}.sig"),
        key_id,
        public_key,
        EXPECTED_APP_BUNDLE_ID,
    )?)
}

fn output_path(arguments: impl Iterator<Item = String>) -> Result<PathBuf, GenerateError> {
    let arguments: Vec<_> = arguments.collect();
    if arguments.len() != 2 || arguments[0] != "--output" || arguments[1].is_empty() {
        return Err(GenerateError::Arguments);
    }
    Ok(PathBuf::from(&arguments[1]))
}

fn required_env(name: &'static str) -> Result<String, GenerateError> {
    env::var(name)
        .ok()
        .filter(|value| !value.is_empty())
        .ok_or(GenerateError::Environment(name))
}

fn validate_repository(value: &str) -> Result<(), GenerateError> {
    let mut parts = value.split('/');
    let valid_part = |part: &str| {
        !part.is_empty()
            && !matches!(part, "." | "..")
            && part.len() <= 100
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    };
    if !parts.next().is_some_and(valid_part)
        || !parts.next().is_some_and(valid_part)
        || parts.next().is_some()
    {
        return Err(GenerateError::Repository);
    }
    Ok(())
}

fn validate_release_tag(value: &str) -> Result<(), GenerateError> {
    if !value.starts_with('v')
        || value.len() < 2
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+' | b'_'))
    {
        return Err(GenerateError::ReleaseTag);
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
enum GenerateError {
    #[error("expected exactly --output <path>")]
    Arguments,
    #[error("required environment variable is unset: {0}")]
    Environment(&'static str),
    #[error("GitHub repository identifier is invalid")]
    Repository,
    #[error("release tag is invalid")]
    ReleaseTag,
    #[error(transparent)]
    Config(#[from] UpdateConfigError),
}

#[cfg(test)]
mod tests {
    use base64::Engine;
    use ed25519_dalek::SigningKey;

    use super::*;

    #[test]
    fn repository_and_tag_reject_url_injection() {
        for repository in [
            "owner",
            "owner/repo/extra",
            "owner/repo#fragment",
            "../repo",
        ] {
            assert!(validate_repository(repository).is_err());
        }
        for tag in ["", "1.0.0", "v1/asset", "v1#fragment"] {
            assert!(validate_release_tag(tag).is_err());
        }
    }

    #[test]
    fn release_config_tracks_latest_stable_manifest() {
        let public_key = base64::engine::general_purpose::STANDARD.encode(
            SigningKey::from_bytes(&[7_u8; 32])
                .verifying_key()
                .to_bytes(),
        );
        let config = release_config(
            "example/agent-factory",
            "v1.2.3",
            "stable-2026",
            &public_key,
        )
        .unwrap();
        assert_eq!(config.channel, Channel::Stable);
        assert_eq!(
            config.manifest_url.unwrap().as_str(),
            "https://github.com/example/agent-factory/releases/latest/download/agent-factory-update-manifest-v1.json",
        );
        assert_eq!(
            config.detached_signature_url.unwrap().as_str(),
            "https://github.com/example/agent-factory/releases/latest/download/agent-factory-update-manifest-v1.json.sig",
        );
    }
}
