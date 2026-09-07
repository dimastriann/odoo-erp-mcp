#![allow(dead_code)] // The provider boundary is integrated by the following secret-handling items.

use std::env::VarError;
use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Serialize, Serializer};

const REDACTED_SECRET: &str = "[REDACTED]";

#[derive(Clone, Default, Deserialize, Eq, PartialEq)]
#[serde(transparent)]
pub(crate) struct SecretString(String);

impl SecretString {
    pub(crate) fn expose_secret(&self) -> &str {
        &self.0
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(REDACTED_SECRET)
    }
}

impl Serialize for SecretString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(REDACTED_SECRET)
    }
}

impl From<String> for SecretString {
    fn from(secret: String) -> Self {
        Self(secret)
    }
}

impl From<&str> for SecretString {
    fn from(secret: &str) -> Self {
        Self(secret.to_string())
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct SecretReference(String);

impl SecretReference {
    pub(crate) fn new(reference: impl Into<String>) -> Self {
        Self(reference.into())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum SecretProviderError {
    Unavailable(String),
}

impl fmt::Display for SecretProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(message) => {
                write!(formatter, "secret provider unavailable: {message}")
            }
        }
    }
}

impl std::error::Error for SecretProviderError {}

pub(crate) trait SecretProvider: Send + Sync {
    fn resolve(
        &self,
        reference: &SecretReference,
    ) -> Result<Option<SecretString>, SecretProviderError>;
}

type EnvironmentLookup = dyn Fn(&str) -> Result<String, VarError> + Send + Sync;

pub(crate) struct EnvironmentSecretProvider {
    lookup: Arc<EnvironmentLookup>,
}

impl EnvironmentSecretProvider {
    pub(crate) fn new() -> Self {
        Self {
            lookup: Arc::new(|name| std::env::var(name)),
        }
    }

    #[cfg(test)]
    fn with_lookup(
        lookup: impl Fn(&str) -> Result<String, VarError> + Send + Sync + 'static,
    ) -> Self {
        Self {
            lookup: Arc::new(lookup),
        }
    }
}

impl SecretProvider for EnvironmentSecretProvider {
    fn resolve(
        &self,
        reference: &SecretReference,
    ) -> Result<Option<SecretString>, SecretProviderError> {
        match (self.lookup)(reference.as_str()) {
            Ok(secret) => Ok(Some(secret.into())),
            Err(VarError::NotPresent) => Ok(None),
            Err(VarError::NotUnicode(_)) => Err(SecretProviderError::Unavailable(format!(
                "environment variable {:?} is not valid Unicode",
                reference.as_str()
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedSecretProvider;

    #[test]
    fn secret_strings_have_redacted_debug_output() {
        let secret = SecretString::from("do-not-print-me");

        assert_eq!(format!("{secret:?}"), REDACTED_SECRET);
        assert_eq!(secret.expose_secret(), "do-not-print-me");
    }

    #[test]
    fn secret_strings_have_redacted_serialized_output() {
        let serialized = serde_json::to_string(&SecretString::from("serialization-canary"))
            .expect("secret should serialize safely");

        assert_eq!(serialized, format!("\"{REDACTED_SECRET}\""));
        assert!(!serialized.contains("serialization-canary"));
    }

    impl SecretProvider for FixedSecretProvider {
        fn resolve(
            &self,
            reference: &SecretReference,
        ) -> Result<Option<SecretString>, SecretProviderError> {
            Ok((reference.as_str() == "odoo/prod").then(|| SecretString::from("resolved-value")))
        }
    }

    #[test]
    fn providers_resolve_opaque_references() {
        let provider = FixedSecretProvider;

        assert_eq!(
            provider
                .resolve(&SecretReference::new("odoo/prod"))
                .unwrap(),
            Some(SecretString::from("resolved-value"))
        );
        assert_eq!(
            provider
                .resolve(&SecretReference::new("odoo/missing"))
                .unwrap(),
            None
        );
    }

    #[test]
    fn environment_provider_resolves_variable_names() {
        let provider = EnvironmentSecretProvider::with_lookup(|name| match name {
            "ODOO_PROD_PASSWORD" => Ok("environment-secret".to_string()),
            _ => Err(VarError::NotPresent),
        });

        let resolved = provider
            .resolve(&SecretReference::new("ODOO_PROD_PASSWORD"))
            .unwrap()
            .unwrap();
        assert_eq!(resolved.expose_secret(), "environment-secret");
        assert_eq!(
            provider
                .resolve(&SecretReference::new("MISSING_PASSWORD"))
                .unwrap(),
            None
        );
    }
}
