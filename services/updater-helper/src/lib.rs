//! Privilege-separable macOS bundle swap helper.
//!
//! Installation is a top-level rename transaction. The helper never writes
//! files inside the running bundle and keeps exactly one prior bundle until a
//! later health-confirmed cleanup.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use thiserror::Error;

const REQUEST_SCHEMA_VERSION: u32 = 1;
const JOURNAL_SCHEMA_VERSION: u32 = 1;
const BACKUP_NAME: &str = ".agent-factory-rollback.app";
const JOURNAL_NAME: &str = ".agent-factory-update-journal.json";
const FAILED_NAME: &str = ".agent-factory-failed-update.app";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallRequest {
    pub schema_version: u32,
    pub current_bundle: PathBuf,
    pub staged_bundle: PathBuf,
    pub expected_bundle_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallOutcome {
    pub rollback_bundle: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwapPoint {
    JournalPrepared,
    CurrentBackedUp,
    NewBundleInstalled,
    MetadataSynced,
}

pub trait FaultInjector {
    fn checkpoint(&self, _point: SwapPoint) -> Result<(), HelperError> {
        Ok(())
    }
}

pub struct NoFault;
impl FaultInjector for NoFault {}

pub trait BundleValidator {
    fn validate(&self, bundle: &Path, expected_bundle_id: &str) -> Result<(), HelperError>;
}

pub struct MacOsBundleValidator;

impl BundleValidator for MacOsBundleValidator {
    fn validate(&self, bundle: &Path, expected_bundle_id: &str) -> Result<(), HelperError> {
        validate_bundle_id(expected_bundle_id)?;
        let info = plist::Value::from_file(bundle.join("Contents/Info.plist"))
            .map_err(|_| HelperError::InvalidBundle)?;
        let actual = info
            .as_dictionary()
            .and_then(|dictionary| dictionary.get("CFBundleIdentifier"))
            .and_then(plist::Value::as_string)
            .ok_or(HelperError::InvalidBundle)?;
        if actual != expected_bundle_id {
            return Err(HelperError::BundleIdMismatch);
        }
        if !cfg!(target_os = "macos") {
            return Err(HelperError::UnsupportedPlatform);
        }
        command_succeeds(
            "/usr/bin/codesign",
            &[
                "--verify",
                "--deep",
                "--strict",
                bundle.to_str().ok_or(HelperError::InvalidPath)?,
            ],
        )?;
        command_succeeds(
            "/usr/sbin/spctl",
            &[
                "--assess",
                "--type",
                "execute",
                bundle.to_str().ok_or(HelperError::InvalidPath)?,
            ],
        )
    }
}

fn command_succeeds(program: &str, arguments: &[&str]) -> Result<(), HelperError> {
    let status = Command::new(program)
        .args(arguments)
        .status()
        .map_err(|_| HelperError::SignatureValidationFailed)?;
    if status.success() {
        Ok(())
    } else {
        Err(HelperError::SignatureValidationFailed)
    }
}

pub fn install_bundle(
    request: &InstallRequest,
    validator: &dyn BundleValidator,
) -> Result<InstallOutcome, HelperError> {
    install_bundle_with_fault(request, validator, &NoFault)
}

pub fn install_bundle_with_fault(
    request: &InstallRequest,
    validator: &dyn BundleValidator,
    fault: &dyn FaultInjector,
) -> Result<InstallOutcome, HelperError> {
    validate_request_header(request)?;
    let current = normalize_current_path(&request.current_bundle)?;
    let parent = current.parent().ok_or(HelperError::InvalidPath)?;
    let journal_path = parent.join(JOURNAL_NAME);
    if journal_path.exists() {
        recover_from_journal(&journal_path, validator, &request.expected_bundle_id)?;
    }

    let current = validate_existing_bundle_path(&current)?;
    let staged = validate_existing_bundle_path(&request.staged_bundle)?;
    if current == staged {
        return Err(HelperError::InvalidPath);
    }
    ensure_same_volume(&current, &staged)?;
    validator.validate(&current, &request.expected_bundle_id)?;
    validator.validate(&staged, &request.expected_bundle_id)?;

    let parent = current.parent().ok_or(HelperError::InvalidPath)?;
    let backup = parent.join(BACKUP_NAME);
    if backup.exists() {
        return Err(HelperError::RollbackAlreadyExists);
    }
    let journal_path = parent.join(JOURNAL_NAME);
    let mut journal = Journal {
        schema_version: JOURNAL_SCHEMA_VERSION,
        state: JournalState::Prepared,
        current: current.clone(),
        staged: staged.clone(),
        backup: backup.clone(),
    };
    write_journal(&journal_path, &journal)?;

    let operation = (|| -> Result<(), HelperError> {
        fault.checkpoint(SwapPoint::JournalPrepared)?;
        fs::rename(&current, &backup).map_err(|_| HelperError::SwapFailed)?;
        sync_dir(parent)?;
        journal.state = JournalState::CurrentBackedUp;
        write_journal(&journal_path, &journal)?;
        fault.checkpoint(SwapPoint::CurrentBackedUp)?;

        fs::rename(&staged, &current).map_err(|_| HelperError::SwapFailed)?;
        sync_dir(parent)?;
        journal.state = JournalState::NewBundleInstalled;
        write_journal(&journal_path, &journal)?;
        fault.checkpoint(SwapPoint::NewBundleInstalled)?;

        sync_dir(parent)?;
        fault.checkpoint(SwapPoint::MetadataSynced)?;
        remove_journal(&journal_path)?;
        Ok(())
    })();

    if let Err(error) = operation {
        recover_from_journal(&journal_path, validator, &request.expected_bundle_id)
            .map_err(|_| HelperError::RollbackFailed)?;
        return Err(error);
    }

    Ok(InstallOutcome {
        rollback_bundle: backup,
    })
}

/// Restores the preserved rollback bundle, keeping the rejected update next
/// to it for diagnostics. This is also a top-level rename transaction.
pub fn rollback_bundle(
    current_bundle: &Path,
    expected_bundle_id: &str,
    validator: &dyn BundleValidator,
) -> Result<PathBuf, HelperError> {
    validate_bundle_id(expected_bundle_id)?;
    let current = validate_existing_bundle_path(current_bundle)?;
    let parent = current.parent().ok_or(HelperError::InvalidPath)?;
    let backup = validate_existing_bundle_path(&parent.join(BACKUP_NAME))?;
    validator.validate(&backup, expected_bundle_id)?;
    let failed = parent.join(FAILED_NAME);
    if failed.exists() {
        return Err(HelperError::FailedBundleAlreadyExists);
    }
    fs::rename(&current, &failed).map_err(|_| HelperError::SwapFailed)?;
    if fs::rename(&backup, &current).is_err() {
        let _ = fs::rename(&failed, &current);
        return Err(HelperError::RollbackFailed);
    }
    sync_dir(parent)?;
    Ok(failed)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Journal {
    schema_version: u32,
    state: JournalState,
    current: PathBuf,
    staged: PathBuf,
    backup: PathBuf,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum JournalState {
    Prepared,
    CurrentBackedUp,
    NewBundleInstalled,
}

fn recover_from_journal(
    journal_path: &Path,
    validator: &dyn BundleValidator,
    expected_bundle_id: &str,
) -> Result<(), HelperError> {
    let bytes = fs::read(journal_path).map_err(|_| HelperError::JournalCorrupt)?;
    if bytes.len() > 64 * 1024 {
        return Err(HelperError::JournalCorrupt);
    }
    let journal: Journal =
        serde_json::from_slice(&bytes).map_err(|_| HelperError::JournalCorrupt)?;
    if journal.schema_version != JOURNAL_SCHEMA_VERSION {
        return Err(HelperError::JournalCorrupt);
    }
    validate_journal_paths(journal_path, &journal)?;

    let current_exists = journal.current.is_dir();
    let staged_exists = journal.staged.is_dir();
    let backup_exists = journal.backup.is_dir();
    match (current_exists, staged_exists, backup_exists) {
        // No mutation occurred.
        (true, true, false) => {}
        // Current was backed up but new bundle was not installed.
        (false, true, true) => {
            validator.validate(&journal.backup, expected_bundle_id)?;
            fs::rename(&journal.backup, &journal.current)
                .map_err(|_| HelperError::RollbackFailed)?;
        }
        // Both renames occurred. Put the new bundle back at its staging path,
        // then restore the last-known-good bundle.
        (true, false, true) => {
            validator.validate(&journal.backup, expected_bundle_id)?;
            fs::rename(&journal.current, &journal.staged)
                .map_err(|_| HelperError::RollbackFailed)?;
            if fs::rename(&journal.backup, &journal.current).is_err() {
                let _ = fs::rename(&journal.staged, &journal.current);
                return Err(HelperError::RollbackFailed);
            }
        }
        _ => return Err(HelperError::JournalCorrupt),
    }
    let parent = journal.current.parent().ok_or(HelperError::InvalidPath)?;
    sync_dir(parent)?;
    remove_journal(journal_path)
}

fn validate_request_header(request: &InstallRequest) -> Result<(), HelperError> {
    if request.schema_version != REQUEST_SCHEMA_VERSION {
        return Err(HelperError::UnsupportedRequestVersion);
    }
    validate_bundle_id(&request.expected_bundle_id)
}

fn validate_bundle_id(value: &str) -> Result<(), HelperError> {
    if value.is_empty()
        || value.len() > 200
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
    {
        return Err(HelperError::InvalidBundleId);
    }
    Ok(())
}

fn normalize_current_path(path: &Path) -> Result<PathBuf, HelperError> {
    validate_absolute_app_path(path)?;
    let parent = path.parent().ok_or(HelperError::InvalidPath)?;
    let parent = fs::canonicalize(parent).map_err(|_| HelperError::InvalidPath)?;
    Ok(parent.join(path.file_name().ok_or(HelperError::InvalidPath)?))
}

fn validate_existing_bundle_path(path: &Path) -> Result<PathBuf, HelperError> {
    validate_absolute_app_path(path)?;
    let metadata = fs::symlink_metadata(path).map_err(|_| HelperError::InvalidPath)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(HelperError::InvalidPath);
    }
    fs::canonicalize(path).map_err(|_| HelperError::InvalidPath)
}

fn validate_absolute_app_path(path: &Path) -> Result<(), HelperError> {
    if !path.is_absolute()
        || path.extension().and_then(|value| value.to_str()) != Some("app")
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        return Err(HelperError::InvalidPath);
    }
    Ok(())
}

fn validate_journal_paths(journal_path: &Path, journal: &Journal) -> Result<(), HelperError> {
    let parent = journal_path.parent().ok_or(HelperError::InvalidPath)?;
    let current_parent = journal.current.parent().ok_or(HelperError::InvalidPath)?;
    if current_parent != parent
        || journal.backup != parent.join(BACKUP_NAME)
        || journal_path != parent.join(JOURNAL_NAME)
    {
        return Err(HelperError::JournalCorrupt);
    }
    validate_absolute_app_path(&journal.current)?;
    validate_absolute_app_path(&journal.staged)?;
    validate_absolute_app_path(&journal.backup)
}

#[cfg(unix)]
fn ensure_same_volume(left: &Path, right: &Path) -> Result<(), HelperError> {
    use std::os::unix::fs::MetadataExt;
    let left_device = fs::metadata(left)
        .map_err(|_| HelperError::InvalidPath)?
        .dev();
    let right_device = fs::metadata(right)
        .map_err(|_| HelperError::InvalidPath)?
        .dev();
    if left_device == right_device {
        Ok(())
    } else {
        Err(HelperError::DifferentVolume)
    }
}

#[cfg(not(unix))]
fn ensure_same_volume(_left: &Path, _right: &Path) -> Result<(), HelperError> {
    Err(HelperError::UnsupportedPlatform)
}

fn write_journal(path: &Path, journal: &Journal) -> Result<(), HelperError> {
    let temporary = path.with_extension("json.tmp");
    let _ = fs::remove_file(&temporary);
    let bytes = serde_json::to_vec(journal).map_err(|_| HelperError::JournalCorrupt)?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(|_| HelperError::JournalWriteFailed)?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| HelperError::JournalWriteFailed)?;
    fs::rename(&temporary, path).map_err(|_| HelperError::JournalWriteFailed)?;
    sync_dir(path.parent().ok_or(HelperError::InvalidPath)?)
}

fn remove_journal(path: &Path) -> Result<(), HelperError> {
    if path.exists() {
        fs::remove_file(path).map_err(|_| HelperError::JournalWriteFailed)?;
        sync_dir(path.parent().ok_or(HelperError::InvalidPath)?)?;
    }
    Ok(())
}

fn sync_dir(path: &Path) -> Result<(), HelperError> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|_| HelperError::MetadataSyncFailed)
}

#[derive(Debug, Error, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HelperError {
    #[error("helper request schema version is unsupported")]
    UnsupportedRequestVersion,
    #[error("bundle path is invalid")]
    InvalidPath,
    #[error("bundle identifier is invalid")]
    InvalidBundleId,
    #[error("application bundle is invalid")]
    InvalidBundle,
    #[error("application bundle identifier does not match")]
    BundleIdMismatch,
    #[error("bundle signature validation failed")]
    SignatureValidationFailed,
    #[error("platform is unsupported")]
    UnsupportedPlatform,
    #[error("staged and current bundles are on different volumes")]
    DifferentVolume,
    #[error("a rollback bundle already exists")]
    RollbackAlreadyExists,
    #[error("a failed update bundle already exists")]
    FailedBundleAlreadyExists,
    #[error("update journal is corrupt")]
    JournalCorrupt,
    #[error("update journal could not be written durably")]
    JournalWriteFailed,
    #[error("bundle swap failed")]
    SwapFailed,
    #[error("bundle rollback failed")]
    RollbackFailed,
    #[error("directory metadata could not be synchronized")]
    MetadataSyncFailed,
    #[error("injected test failure")]
    InjectedFailure,
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUNDLE_ID: &str = "app.agentfactory.desktop";

    struct MarkerValidator;
    impl BundleValidator for MarkerValidator {
        fn validate(&self, bundle: &Path, expected_bundle_id: &str) -> Result<(), HelperError> {
            let actual = fs::read_to_string(bundle.join("bundle-id"))
                .map_err(|_| HelperError::InvalidBundle)?;
            if actual == expected_bundle_id {
                Ok(())
            } else {
                Err(HelperError::BundleIdMismatch)
            }
        }
    }

    struct RejectSignature;
    impl BundleValidator for RejectSignature {
        fn validate(&self, _bundle: &Path, _expected_bundle_id: &str) -> Result<(), HelperError> {
            Err(HelperError::SignatureValidationFailed)
        }
    }

    struct FailAt(SwapPoint);
    impl FaultInjector for FailAt {
        fn checkpoint(&self, point: SwapPoint) -> Result<(), HelperError> {
            if point == self.0 {
                Err(HelperError::InjectedFailure)
            } else {
                Ok(())
            }
        }
    }

    fn bundle(path: &Path, content: &str, bundle_id: &str) {
        fs::create_dir(path).unwrap();
        fs::write(path.join("bundle-id"), bundle_id).unwrap();
        fs::write(path.join("content"), content).unwrap();
    }

    fn setup() -> (tempfile::TempDir, InstallRequest) {
        let temp = tempfile::tempdir().unwrap();
        let current = temp.path().join("Agent Factory.app");
        let staged = temp.path().join("Staged.app");
        bundle(&current, "old", BUNDLE_ID);
        bundle(&staged, "new", BUNDLE_ID);
        (
            temp,
            InstallRequest {
                schema_version: REQUEST_SCHEMA_VERSION,
                current_bundle: current,
                staged_bundle: staged,
                expected_bundle_id: BUNDLE_ID.to_owned(),
            },
        )
    }

    #[test]
    fn swaps_whole_bundle_and_preserves_one_rollback() {
        let (_temp, request) = setup();
        let outcome = install_bundle(&request, &MarkerValidator).unwrap();
        assert_eq!(
            fs::read_to_string(request.current_bundle.join("content")).unwrap(),
            "new",
        );
        assert_eq!(
            fs::read_to_string(outcome.rollback_bundle.join("content")).unwrap(),
            "old"
        );
        assert!(!request.staged_bundle.exists());
    }

    #[test]
    fn injected_failures_roll_back_both_rename_boundaries() {
        for point in [
            SwapPoint::JournalPrepared,
            SwapPoint::CurrentBackedUp,
            SwapPoint::NewBundleInstalled,
            SwapPoint::MetadataSynced,
        ] {
            let (_temp, request) = setup();
            assert_eq!(
                install_bundle_with_fault(&request, &MarkerValidator, &FailAt(point)),
                Err(HelperError::InjectedFailure),
            );
            assert_eq!(
                fs::read_to_string(request.current_bundle.join("content")).unwrap(),
                "old",
            );
            assert_eq!(
                fs::read_to_string(request.staged_bundle.join("content")).unwrap(),
                "new",
            );
            assert!(
                !request
                    .current_bundle
                    .parent()
                    .unwrap()
                    .join(BACKUP_NAME)
                    .exists()
            );
            assert!(
                !request
                    .current_bundle
                    .parent()
                    .unwrap()
                    .join(JOURNAL_NAME)
                    .exists()
            );
        }
    }

    #[test]
    fn next_install_recovers_an_interrupted_swap_before_proceeding() {
        let (_temp, mut request) = setup();
        request.current_bundle = fs::canonicalize(&request.current_bundle).unwrap();
        request.staged_bundle = fs::canonicalize(&request.staged_bundle).unwrap();
        let parent = request.current_bundle.parent().unwrap();
        let backup = parent.join(BACKUP_NAME);
        fs::rename(&request.current_bundle, &backup).unwrap();
        let journal = Journal {
            schema_version: JOURNAL_SCHEMA_VERSION,
            state: JournalState::Prepared,
            current: request.current_bundle.clone(),
            staged: request.staged_bundle.clone(),
            backup,
        };
        write_journal(&parent.join(JOURNAL_NAME), &journal).unwrap();

        let outcome = install_bundle(&request, &MarkerValidator).unwrap();
        assert_eq!(
            fs::read_to_string(request.current_bundle.join("content")).unwrap(),
            "new",
        );
        assert_eq!(
            fs::read_to_string(outcome.rollback_bundle.join("content")).unwrap(),
            "old"
        );
    }

    #[test]
    fn rejects_bad_identity_paths_and_existing_rollback() {
        let (_temp, mut request) = setup();
        request.expected_bundle_id = "other.bundle".to_owned();
        assert_eq!(
            install_bundle(&request, &MarkerValidator),
            Err(HelperError::BundleIdMismatch),
        );

        let (_temp, mut request) = setup();
        request.staged_bundle = PathBuf::from("relative.app");
        assert_eq!(
            install_bundle(&request, &MarkerValidator),
            Err(HelperError::InvalidPath),
        );

        let (_temp, request) = setup();
        bundle(
            &request.current_bundle.parent().unwrap().join(BACKUP_NAME),
            "older",
            BUNDLE_ID,
        );
        assert_eq!(
            install_bundle(&request, &MarkerValidator),
            Err(HelperError::RollbackAlreadyExists),
        );
    }

    #[test]
    fn explicit_rollback_restores_prior_bundle() {
        let (_temp, request) = setup();
        install_bundle(&request, &MarkerValidator).unwrap();
        let failed = rollback_bundle(&request.current_bundle, BUNDLE_ID, &MarkerValidator).unwrap();
        assert_eq!(
            fs::read_to_string(request.current_bundle.join("content")).unwrap(),
            "old",
        );
        assert_eq!(fs::read_to_string(failed.join("content")).unwrap(), "new");
    }

    #[test]
    fn bad_signature_and_traversal_are_rejected_before_mutation() {
        let (_temp, request) = setup();
        assert_eq!(
            install_bundle(&request, &RejectSignature),
            Err(HelperError::SignatureValidationFailed),
        );
        assert_eq!(
            fs::read_to_string(request.current_bundle.join("content")).unwrap(),
            "old",
        );

        let (_temp, mut request) = setup();
        request.staged_bundle = request
            .staged_bundle
            .parent()
            .unwrap()
            .join("nested/../Staged.app");
        assert_eq!(
            install_bundle(&request, &MarkerValidator),
            Err(HelperError::InvalidPath),
        );
    }
}
