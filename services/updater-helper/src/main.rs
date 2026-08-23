use std::io::{self, Read};

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use updater_helper::{
    HelperError, InstallRequest, MacOsBundleValidator, install_bundle, rollback_bundle,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HelperResponse<T: Serialize> {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<HelperError>,
}

fn main() {
    let response = run();
    let exit_code = if response.ok { 0 } else { 1 };
    match serde_json::to_writer(io::stdout().lock(), &response) {
        Ok(()) => std::process::exit(exit_code),
        Err(_) => std::process::exit(74),
    }
}

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
enum HelperCommand {
    Install {
        schema_version: u32,
        current_bundle: PathBuf,
        staged_bundle: PathBuf,
        expected_bundle_id: String,
    },
    Rollback {
        schema_version: u32,
        current_bundle: PathBuf,
        expected_bundle_id: String,
    },
}

fn run() -> HelperResponse<serde_json::Value> {
    let mut input = Vec::new();
    if io::stdin()
        .take(64 * 1024 + 1)
        .read_to_end(&mut input)
        .is_err()
        || input.len() > 64 * 1024
    {
        return failure(HelperError::InvalidPath);
    }
    let request: HelperCommand = match serde_json::from_slice(&input) {
        Ok(request) => request,
        Err(_) => return failure(HelperError::InvalidPath),
    };
    let result = match request {
        HelperCommand::Install {
            schema_version,
            current_bundle,
            staged_bundle,
            expected_bundle_id,
        } => install_bundle(
            &InstallRequest {
                schema_version,
                current_bundle,
                staged_bundle,
                expected_bundle_id,
            },
            &MacOsBundleValidator,
        )
        .and_then(|outcome| serde_json::to_value(outcome).map_err(|_| HelperError::InvalidPath)),
        HelperCommand::Rollback {
            schema_version,
            current_bundle,
            expected_bundle_id,
        } => {
            if schema_version != 1 {
                Err(HelperError::UnsupportedRequestVersion)
            } else {
                rollback_bundle(&current_bundle, &expected_bundle_id, &MacOsBundleValidator)
                    .and_then(|path| {
                        serde_json::to_value(path).map_err(|_| HelperError::InvalidPath)
                    })
            }
        }
    };
    match result {
        Ok(result) => HelperResponse {
            ok: true,
            result: Some(result),
            error: None,
        },
        Err(error) => failure(error),
    }
}

fn failure(error: HelperError) -> HelperResponse<serde_json::Value> {
    HelperResponse {
        ok: false,
        result: None,
        error: Some(error),
    }
}
