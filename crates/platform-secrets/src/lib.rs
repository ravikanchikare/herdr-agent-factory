//! Opaque secret references backed by a platform credential store.
//!
//! Secret values deliberately do not implement serialization, cloning, or a
//! revealing `Debug`. Callers persist only [`SecretRef`] values.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop};

const REF_PREFIX: &str = "secret_";
const MAX_LABEL_BYTES: usize = 128;
const MAX_SECRET_BYTES: usize = 64 * 1024;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SecretRef(String);

impl SecretRef {
    pub fn generate() -> Self {
        Self(format!("{REF_PREFIX}{}", Uuid::new_v4().simple()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("SecretRef").field(&self.0).finish()
    }
}

impl fmt::Display for SecretRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for SecretRef {
    type Err = SecretError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let suffix = value
            .strip_prefix(REF_PREFIX)
            .ok_or(SecretError::InvalidReference)?;
        if suffix.len() != 32
            || !suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
            || Uuid::parse_str(suffix).is_err()
        {
            return Err(SecretError::InvalidReference);
        }
        Ok(Self(value.to_ascii_lowercase()))
    }
}

impl TryFrom<String> for SecretRef {
    type Error = SecretError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_str(&value)
    }
}

impl From<SecretRef> for String {
    fn from(value: SecretRef) -> Self {
        value.0
    }
}

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct SecretValue(Vec<u8>);

impl SecretValue {
    pub fn new(value: impl Into<Vec<u8>>) -> Result<Self, SecretError> {
        let mut value = value.into();
        if value.is_empty() || value.len() > MAX_SECRET_BYTES {
            value.zeroize();
            return Err(SecretError::InvalidValue);
        }
        Ok(Self(value))
    }

    pub fn expose(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretValue([REDACTED])")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SecretMetadata {
    pub reference: SecretRef,
    pub label: String,
    pub created_at_unix_ms: u64,
    pub updated_at_unix_ms: u64,
}

pub trait SecretStore: Send + Sync {
    fn create(&self, label: &str, value: SecretValue) -> Result<SecretMetadata, SecretError>;
    fn read(&self, reference: &SecretRef) -> Result<SecretValue, SecretError>;
    fn replace(&self, reference: &SecretRef, value: SecretValue) -> Result<(), SecretError>;
    fn delete(&self, reference: &SecretRef) -> Result<(), SecretError>;
    fn list_metadata(&self) -> Result<Vec<SecretMetadata>, SecretError>;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SecretError {
    #[error("secret reference is invalid")]
    InvalidReference,
    #[error("secret label is invalid")]
    InvalidLabel,
    #[error("secret value is empty or exceeds 64 KiB")]
    InvalidValue,
    #[error("secret was not found")]
    NotFound,
    #[error("secret already exists")]
    AlreadyExists,
    #[error("platform credential store is unavailable")]
    PlatformUnavailable,
    #[error("platform credential store operation failed")]
    Backend,
    #[error("secret metadata is corrupt")]
    CorruptMetadata,
}

fn validate_label(label: &str) -> Result<(), SecretError> {
    if label.trim() != label
        || label.is_empty()
        || label.len() > MAX_LABEL_BYTES
        || label.chars().any(char::is_control)
    {
        return Err(SecretError::InvalidLabel);
    }
    Ok(())
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

struct MemoryRecord {
    metadata: SecretMetadata,
    value: SecretValue,
}

type MemoryEntries = BTreeMap<SecretRef, MemoryRecord>;

#[derive(Clone, Default)]
pub struct InMemorySecretStore {
    inner: Arc<RwLock<MemoryEntries>>,
}

impl SecretStore for InMemorySecretStore {
    fn create(&self, label: &str, value: SecretValue) -> Result<SecretMetadata, SecretError> {
        validate_label(label)?;
        let reference = SecretRef::generate();
        let timestamp = now_unix_ms();
        let metadata = SecretMetadata {
            reference: reference.clone(),
            label: label.to_owned(),
            created_at_unix_ms: timestamp,
            updated_at_unix_ms: timestamp,
        };
        let mut entries = self.inner.write().map_err(|_| SecretError::Backend)?;
        entries.insert(
            reference,
            MemoryRecord {
                metadata: metadata.clone(),
                value,
            },
        );
        Ok(metadata)
    }

    fn read(&self, reference: &SecretRef) -> Result<SecretValue, SecretError> {
        let entries = self.inner.read().map_err(|_| SecretError::Backend)?;
        let record = entries.get(reference).ok_or(SecretError::NotFound)?;
        SecretValue::new(record.value.expose().to_vec())
    }

    fn replace(&self, reference: &SecretRef, value: SecretValue) -> Result<(), SecretError> {
        let mut entries = self.inner.write().map_err(|_| SecretError::Backend)?;
        let record = entries.get_mut(reference).ok_or(SecretError::NotFound)?;
        record.value = value;
        record.metadata.updated_at_unix_ms = now_unix_ms();
        Ok(())
    }

    fn delete(&self, reference: &SecretRef) -> Result<(), SecretError> {
        let mut entries = self.inner.write().map_err(|_| SecretError::Backend)?;
        entries.remove(reference).ok_or(SecretError::NotFound)?;
        Ok(())
    }

    fn list_metadata(&self) -> Result<Vec<SecretMetadata>, SecretError> {
        let entries = self.inner.read().map_err(|_| SecretError::Backend)?;
        Ok(entries
            .values()
            .map(|record| record.metadata.clone())
            .collect())
    }
}

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
pub use macos::MacOsKeychain;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_are_opaque_and_strictly_validated() {
        let reference = SecretRef::generate();
        assert_eq!(SecretRef::from_str(reference.as_str()).unwrap(), reference);
        for invalid in [
            "",
            "secret_",
            "secret_not-a-uuid",
            "other_550e8400e29b41d4a716446655440000",
            "secret_550e8400e29b41d4a716446655440000/../x",
        ] {
            assert_eq!(
                SecretRef::from_str(invalid),
                Err(SecretError::InvalidReference)
            );
        }
    }

    #[test]
    fn memory_backend_crud_lists_only_metadata() {
        let store = InMemorySecretStore::default();
        let metadata = store
            .create(
                "Anthropic API key",
                SecretValue::new(b"first".to_vec()).unwrap(),
            )
            .unwrap();
        assert_eq!(store.read(&metadata.reference).unwrap().expose(), b"first");
        store
            .replace(
                &metadata.reference,
                SecretValue::new(b"second".to_vec()).unwrap(),
            )
            .unwrap();
        assert_eq!(store.read(&metadata.reference).unwrap().expose(), b"second");

        let serialized = serde_json::to_string(&store.list_metadata().unwrap()).unwrap();
        assert!(serialized.contains("Anthropic API key"));
        assert!(!serialized.contains("second"));

        store.delete(&metadata.reference).unwrap();
        assert!(matches!(
            store.read(&metadata.reference),
            Err(SecretError::NotFound),
        ));
    }

    #[test]
    fn values_and_labels_are_bounded_and_debug_is_redacted() {
        assert!(matches!(
            SecretValue::new(Vec::new()),
            Err(SecretError::InvalidValue),
        ));
        assert!(matches!(
            SecretValue::new(vec![0; MAX_SECRET_BYTES + 1]),
            Err(SecretError::InvalidValue),
        ));
        let value = SecretValue::new(b"do-not-print".to_vec()).unwrap();
        assert_eq!(format!("{value:?}"), "SecretValue([REDACTED])");

        let store = InMemorySecretStore::default();
        assert_eq!(
            store.create(" padded ", SecretValue::new(b"x".to_vec()).unwrap()),
            Err(SecretError::InvalidLabel)
        );
    }
}
