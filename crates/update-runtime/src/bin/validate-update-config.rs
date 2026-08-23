use std::env;
use std::path::PathBuf;

use update_runtime::{UpdateConfigError, read_update_client_config};

fn main() {
    if let Err(error) = run() {
        eprintln!("update config validation failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), ValidateError> {
    let path = config_path(env::args().skip(1))?;
    let config = read_update_client_config(&path)?;
    let release = env::var("RELEASE_UPDATE_CONFIG").as_deref() == Ok("1");
    if config.enabled != release {
        return Err(ValidateError::State);
    }
    if release {
        let expected_key_id = env::var("UPDATE_MANIFEST_KEY_ID")
            .ok()
            .filter(|value| !value.is_empty())
            .ok_or(ValidateError::Environment)?;
        let expected_public_key = env::var("UPDATE_MANIFEST_PUBLIC_KEY_BASE64")
            .ok()
            .filter(|value| !value.is_empty())
            .ok_or(ValidateError::Environment)?;
        if config.key_id.as_deref() != Some(expected_key_id.as_str())
            || config.public_key_base64().as_deref() != Some(expected_public_key.as_str())
        {
            return Err(ValidateError::TrustRoot);
        }
    } else if config.key_id.is_some() || config.public_key.is_some() {
        return Err(ValidateError::TrustRoot);
    }
    Ok(())
}

fn config_path(arguments: impl Iterator<Item = String>) -> Result<PathBuf, ValidateError> {
    let arguments: Vec<_> = arguments.collect();
    if arguments.len() != 2 || arguments[0] != "--path" || arguments[1].is_empty() {
        return Err(ValidateError::Arguments);
    }
    Ok(PathBuf::from(&arguments[1]))
}

#[derive(Debug, thiserror::Error)]
enum ValidateError {
    #[error("expected exactly --path <path>")]
    Arguments,
    #[error("expected release trust environment is missing")]
    Environment,
    #[error("update config enabled state does not match package mode")]
    State,
    #[error("packaged update trust root does not match release configuration")]
    TrustRoot,
    #[error(transparent)]
    Config(#[from] UpdateConfigError),
}
