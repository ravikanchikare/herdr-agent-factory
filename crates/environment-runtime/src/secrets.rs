use std::collections::BTreeMap;
use std::fmt;

use platform_secrets::{SecretError, SecretRef, SecretStore, SecretValue};

use crate::error::{EnvironmentError, Result};
use crate::model::{EnvironmentDescriptor, EnvironmentValue};

/// Narrow injected boundary used by environment resolution. Production adapters read
/// platform storage; tests can supply a deterministic fake.
pub trait SecretResolver: Send + Sync {
    fn resolve(&self, reference: &SecretRef) -> std::result::Result<SecretValue, SecretError>;
}

pub struct StoredSecretResolver<'a, S: SecretStore + ?Sized> {
    store: &'a S,
}

impl<'a, S: SecretStore + ?Sized> StoredSecretResolver<'a, S> {
    pub fn new(store: &'a S) -> Self {
        Self { store }
    }
}

impl<S: SecretStore + ?Sized> SecretResolver for StoredSecretResolver<'_, S> {
    fn resolve(&self, reference: &SecretRef) -> std::result::Result<SecretValue, SecretError> {
        self.store.read(reference)
    }
}

enum ResolvedEnvironmentValue {
    Literal(String),
    Secret {
        reference: SecretRef,
        value: SecretValue,
    },
}

/// A non-serializable environment projection. Secret buffers zeroize on drop
/// and its `Debug` implementation reveals keys and value kinds only.
pub struct ResolvedEnvironment {
    values: BTreeMap<String, ResolvedEnvironmentValue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedEnvironmentValueRef<'a> {
    Literal(&'a str),
    Secret {
        reference: &'a SecretRef,
        value: &'a str,
    },
}

impl ResolvedEnvironment {
    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn get(&self, name: &str) -> Option<ResolvedEnvironmentValueRef<'_>> {
        match self.values.get(name)? {
            ResolvedEnvironmentValue::Literal(value) => {
                Some(ResolvedEnvironmentValueRef::Literal(value))
            }
            ResolvedEnvironmentValue::Secret { reference, value } => {
                let value = std::str::from_utf8(value.expose()).ok()?;
                Some(ResolvedEnvironmentValueRef::Secret { reference, value })
            }
        }
    }

    /// Gives a process builder temporary string views without copying secrets
    /// into a serializable intermediate representation.
    pub fn for_each(&self, mut visitor: impl FnMut(&str, &str)) {
        for (name, value) in &self.values {
            match value {
                ResolvedEnvironmentValue::Literal(value) => visitor(name, value),
                ResolvedEnvironmentValue::Secret { value, .. } => {
                    // UTF-8 was checked while resolving.
                    if let Ok(value) = std::str::from_utf8(value.expose()) {
                        visitor(name, value);
                    }
                }
            }
        }
    }
}

impl fmt::Debug for ResolvedEnvironment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let redacted = self
            .values
            .iter()
            .map(|(name, value)| {
                let kind = match value {
                    ResolvedEnvironmentValue::Literal(_) => "literal",
                    ResolvedEnvironmentValue::Secret { .. } => "secret:[REDACTED]",
                };
                (name, kind)
            })
            .collect::<BTreeMap<_, _>>();
        formatter
            .debug_struct("ResolvedEnvironment")
            .field("values", &redacted)
            .finish()
    }
}

impl EnvironmentDescriptor {
    pub fn resolve_environment(
        &self,
        resolver: &dyn SecretResolver,
    ) -> Result<ResolvedEnvironment> {
        let mut values = BTreeMap::new();
        for (name, configured) in &self.environment_variables {
            let resolved = match configured {
                EnvironmentValue::Literal(value) => {
                    ResolvedEnvironmentValue::Literal(value.literal.clone())
                }
                EnvironmentValue::Secret(value) => {
                    let secret = resolver.resolve(&value.secret_ref).map_err(|source| {
                        EnvironmentError::SecretResolution {
                            reference: value.secret_ref.clone(),
                            source,
                        }
                    })?;
                    if std::str::from_utf8(secret.expose()).is_err() {
                        return Err(EnvironmentError::SecretNotUtf8(value.secret_ref.clone()));
                    }
                    ResolvedEnvironmentValue::Secret {
                        reference: value.secret_ref.clone(),
                        value: secret,
                    }
                }
            };
            values.insert(name.clone(), resolved);
        }
        Ok(ResolvedEnvironment { values })
    }
}
