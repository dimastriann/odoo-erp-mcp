#![allow(dead_code)] // The provider boundary is integrated by the following secret-handling items.

use std::fmt;

use serde::{Deserialize, Serialize};

const REDACTED_SECRET: &str = "[REDACTED]";

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
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
    fn resolve(&self, reference: &SecretReference) -> Result<Option<String>, SecretProviderError>;
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

    impl SecretProvider for FixedSecretProvider {
        fn resolve(
            &self,
            reference: &SecretReference,
        ) -> Result<Option<String>, SecretProviderError> {
            Ok((reference.as_str() == "odoo/prod").then(|| "resolved-value".to_string()))
        }
    }

    #[test]
    fn providers_resolve_opaque_references() {
        let provider = FixedSecretProvider;

        assert_eq!(
            provider
                .resolve(&SecretReference::new("odoo/prod"))
                .unwrap(),
            Some("resolved-value".to_string())
        );
        assert_eq!(
            provider
                .resolve(&SecretReference::new("odoo/missing"))
                .unwrap(),
            None
        );
    }
}
