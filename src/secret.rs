#![allow(dead_code)] // The provider boundary is integrated by the following secret-handling items.

use std::fmt;

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
