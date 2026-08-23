use std::collections::BTreeMap;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use update_runtime::{
    Architecture, ArtifactInput, Channel, PublishError, build_and_sign_manifest,
    decode_signing_seed_base64, write_signed_manifest,
};
use zeroize::Zeroize;

fn main() {
    if let Err(error) = run() {
        eprintln!("update manifest publisher failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), PublishError> {
    let arguments = parse_arguments(std::env::args().skip(1))?;
    let channel = match required(&arguments, "channel")? {
        "stable" => Channel::Stable,
        "beta" => Channel::Beta,
        _ => return Err(PublishError::Arguments),
    };
    let base_url = required(&arguments, "base-url")?.trim_end_matches('/');
    if !base_url.starts_with("https://") {
        return Err(PublishError::Arguments);
    }
    let arm64 = PathBuf::from(required(&arguments, "arm64")?);
    let x86_64 = PathBuf::from(required(&arguments, "x86-64")?);
    let artifacts = vec![
        artifact_input(Architecture::Aarch64AppleDarwin, arm64, base_url)?,
        artifact_input(Architecture::X86_64AppleDarwin, x86_64, base_url)?,
    ];

    let mut encoded_seed = String::new();
    io::stdin()
        .take(1025)
        .read_to_string(&mut encoded_seed)
        .map_err(|_| PublishError::SigningKey)?;
    if encoded_seed.len() > 1024 {
        encoded_seed.zeroize();
        return Err(PublishError::SigningKey);
    }
    let mut seed = decode_signing_seed_base64(&mut encoded_seed)?;
    let signed = build_and_sign_manifest(
        required(&arguments, "version")?,
        channel,
        required(&arguments, "minimum-macos")?,
        required(&arguments, "bundle-id")?,
        required(&arguments, "key-id")?,
        &artifacts,
        &seed,
    );
    seed.zeroize();
    let signed = signed?;
    write_signed_manifest(Path::new(required(&arguments, "output-dir")?), &signed)
}

fn parse_arguments(
    arguments: impl Iterator<Item = String>,
) -> Result<BTreeMap<String, String>, PublishError> {
    let mut parsed = BTreeMap::new();
    let mut arguments = arguments;
    let allowed = [
        "version",
        "channel",
        "minimum-macos",
        "bundle-id",
        "key-id",
        "base-url",
        "arm64",
        "x86-64",
        "output-dir",
    ];
    while let Some(flag) = arguments.next() {
        let name = flag
            .strip_prefix("--")
            .filter(|name| !name.is_empty())
            .ok_or(PublishError::Arguments)?;
        if !allowed.contains(&name) {
            return Err(PublishError::Arguments);
        }
        let value = arguments.next().ok_or(PublishError::Arguments)?;
        if value.starts_with("--") || parsed.insert(name.to_owned(), value).is_some() {
            return Err(PublishError::Arguments);
        }
    }
    Ok(parsed)
}

fn required<'a>(
    arguments: &'a BTreeMap<String, String>,
    name: &str,
) -> Result<&'a str, PublishError> {
    arguments
        .get(name)
        .map(String::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(PublishError::Arguments)
}

fn artifact_input(
    architecture: Architecture,
    path: PathBuf,
    base_url: &str,
) -> Result<ArtifactInput, PublishError> {
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| value.ends_with(".zip"))
        .ok_or(PublishError::Arguments)?;
    Ok(ArtifactInput {
        architecture,
        url: format!("{base_url}/{filename}"),
        path,
    })
}
