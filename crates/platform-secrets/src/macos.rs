use std::collections::BTreeMap;
use std::sync::Mutex;

use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password,
};
use zeroize::Zeroize;

use super::{
    SecretError, SecretMetadata, SecretRef, SecretStore, SecretValue, now_unix_ms, validate_label,
};

const CATALOG_ACCOUNT: &str = "__metadata_catalog_v1";
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

/// macOS Keychain implementation. Both values and the metadata catalog remain
/// Keychain items; SQLite and application logs only ever see opaque refs.
pub struct MacOsKeychain {
    value_service: String,
    metadata_service: String,
    mutation_lock: Mutex<()>,
}

impl MacOsKeychain {
    pub fn new(bundle_id: &str) -> Result<Self, SecretError> {
        if bundle_id.is_empty()
            || bundle_id.len() > 200
            || !bundle_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
        {
            return Err(SecretError::Backend);
        }
        Ok(Self {
            value_service: format!("{bundle_id}.secrets.v1"),
            metadata_service: format!("{bundle_id}.secret-metadata.v1"),
            mutation_lock: Mutex::new(()),
        })
    }

    fn load_catalog(&self) -> Result<BTreeMap<SecretRef, SecretMetadata>, SecretError> {
        let mut bytes = match get_generic_password(&self.metadata_service, CATALOG_ACCOUNT) {
            Ok(bytes) => bytes,
            Err(error) if error.code() == ERR_SEC_ITEM_NOT_FOUND => return Ok(BTreeMap::new()),
            Err(_) => return Err(SecretError::Backend),
        };
        let catalog = serde_json::from_slice(&bytes).map_err(|_| SecretError::CorruptMetadata);
        bytes.zeroize();
        let catalog: BTreeMap<SecretRef, SecretMetadata> = catalog?;
        if catalog
            .iter()
            .any(|(reference, metadata)| reference != &metadata.reference)
        {
            return Err(SecretError::CorruptMetadata);
        }
        Ok(catalog)
    }

    fn store_catalog(
        &self,
        catalog: &BTreeMap<SecretRef, SecretMetadata>,
    ) -> Result<(), SecretError> {
        let mut bytes = serde_json::to_vec(catalog).map_err(|_| SecretError::Backend)?;
        let result = set_generic_password(&self.metadata_service, CATALOG_ACCOUNT, &bytes)
            .map_err(|_| SecretError::Backend);
        bytes.zeroize();
        result
    }
}

impl SecretStore for MacOsKeychain {
    fn create(&self, label: &str, value: SecretValue) -> Result<SecretMetadata, SecretError> {
        validate_label(label)?;
        let _guard = self
            .mutation_lock
            .lock()
            .map_err(|_| SecretError::Backend)?;
        let mut catalog = self.load_catalog()?;
        let reference = (0..10)
            .find_map(|_| {
                let candidate = SecretRef::generate();
                if catalog.contains_key(&candidate) {
                    return None;
                }
                match get_generic_password(&self.value_service, candidate.as_str()) {
                    Err(error) if error.code() == ERR_SEC_ITEM_NOT_FOUND => Some(Ok(candidate)),
                    Ok(mut orphaned_value) => {
                        orphaned_value.zeroize();
                        None
                    }
                    Err(_) => Some(Err(SecretError::Backend)),
                }
            })
            .ok_or(SecretError::Backend)??;
        let timestamp = now_unix_ms();
        let metadata = SecretMetadata {
            reference: reference.clone(),
            label: label.to_owned(),
            created_at_unix_ms: timestamp,
            updated_at_unix_ms: timestamp,
        };
        set_generic_password(&self.value_service, reference.as_str(), value.expose())
            .map_err(|_| SecretError::Backend)?;
        catalog.insert(reference.clone(), metadata.clone());
        if let Err(error) = self.store_catalog(&catalog) {
            let _ = delete_generic_password(&self.value_service, reference.as_str());
            return Err(error);
        }
        Ok(metadata)
    }

    fn read(&self, reference: &SecretRef) -> Result<SecretValue, SecretError> {
        let bytes =
            get_generic_password(&self.value_service, reference.as_str()).map_err(|error| {
                if error.code() == ERR_SEC_ITEM_NOT_FOUND {
                    SecretError::NotFound
                } else {
                    SecretError::Backend
                }
            })?;
        SecretValue::new(bytes)
    }

    fn replace(&self, reference: &SecretRef, value: SecretValue) -> Result<(), SecretError> {
        let _guard = self
            .mutation_lock
            .lock()
            .map_err(|_| SecretError::Backend)?;
        let mut catalog = self.load_catalog()?;
        let metadata = catalog.get_mut(reference).ok_or(SecretError::NotFound)?;
        let mut previous = get_generic_password(&self.value_service, reference.as_str())
            .map_err(|_| SecretError::Backend)?;
        set_generic_password(&self.value_service, reference.as_str(), value.expose())
            .map_err(|_| SecretError::Backend)?;
        metadata.updated_at_unix_ms = now_unix_ms();
        let result = self.store_catalog(&catalog);
        if result.is_err() {
            let _ = set_generic_password(&self.value_service, reference.as_str(), &previous);
        }
        previous.zeroize();
        result
    }

    fn delete(&self, reference: &SecretRef) -> Result<(), SecretError> {
        let _guard = self
            .mutation_lock
            .lock()
            .map_err(|_| SecretError::Backend)?;
        let mut catalog = self.load_catalog()?;
        let metadata = catalog.remove(reference).ok_or(SecretError::NotFound)?;
        self.store_catalog(&catalog)?;
        if let Err(error) = delete_generic_password(&self.value_service, reference.as_str()) {
            if error.code() == ERR_SEC_ITEM_NOT_FOUND {
                return Ok(());
            }
            catalog.insert(reference.clone(), metadata);
            let _ = self.store_catalog(&catalog);
            return Err(SecretError::Backend);
        }
        Ok(())
    }

    fn list_metadata(&self) -> Result<Vec<SecretMetadata>, SecretError> {
        Ok(self.load_catalog()?.into_values().collect())
    }
}
