use serde_json::Value;
use std::error::Error;
use std::fmt::{self, Display, Formatter};

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum AppError {
    Authentication { message: String },
    Authorization { message: String },
    Configuration { message: String },
    InputValidation { message: String },
    Internal { message: String },
    OdooAccess { message: String },
    OdooValidation { message: String },
    Protocol { message: String },
    Timeout { message: String },
    Transport { message: String },
}

impl AppError {
    pub(crate) fn authentication(message: impl Into<String>) -> Self {
        Self::Authentication {
            message: message.into(),
        }
    }

    pub(crate) fn authorization(message: impl Into<String>) -> Self {
        Self::Authorization {
            message: message.into(),
        }
    }

    pub(crate) fn configuration(message: impl Into<String>) -> Self {
        Self::Configuration {
            message: message.into(),
        }
    }

    pub(crate) fn input_validation(message: impl Into<String>) -> Self {
        Self::InputValidation {
            message: message.into(),
        }
    }

    pub(crate) fn invalid_arguments(operation: &str, error: impl Display) -> Self {
        Self::input_validation(format!("Error: Invalid {operation} arguments: {error}"))
    }

    pub(crate) fn from_odoo_rpc(error: &Value) -> Self {
        let message = format!("Odoo RPC Error: {error}");
        match error
            .pointer("/data/name")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "odoo.exceptions.AccessError" => Self::OdooAccess { message },
            "odoo.exceptions.ValidationError" => Self::OdooValidation { message },
            _ => Self::Internal { message },
        }
    }

    pub(crate) fn protocol(message: impl Into<String>) -> Self {
        Self::Protocol {
            message: message.into(),
        }
    }

    pub(crate) fn timeout(message: impl Into<String>) -> Self {
        Self::Timeout {
            message: message.into(),
        }
    }

    pub(crate) fn transport(message: impl Into<String>) -> Self {
        Self::Transport {
            message: message.into(),
        }
    }

    pub(crate) fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }
}

impl Display for AppError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Authentication { message }
            | Self::Authorization { message }
            | Self::Configuration { message }
            | Self::InputValidation { message }
            | Self::Internal { message }
            | Self::OdooAccess { message }
            | Self::OdooValidation { message }
            | Self::Protocol { message }
            | Self::Timeout { message }
            | Self::Transport { message } => formatter.write_str(message),
        }
    }
}

impl Error for AppError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_error_preserves_its_message() {
        let error = AppError::internal("unexpected failure");

        assert_eq!(error.to_string(), "unexpected failure");
        assert_eq!(
            error,
            AppError::Internal {
                message: "unexpected failure".to_string()
            }
        );
    }

    #[test]
    fn invalid_arguments_preserve_the_existing_wire_message() {
        let error = AppError::invalid_arguments("search", "domain must be nested");

        assert_eq!(
            error.to_string(),
            "Error: Invalid search arguments: domain must be nested"
        );
        assert!(matches!(error, AppError::InputValidation { .. }));
    }

    #[test]
    fn configuration_error_preserves_its_message() {
        let error = AppError::configuration("No active Odoo instance configured");

        assert_eq!(error.to_string(), "No active Odoo instance configured");
        assert!(matches!(error, AppError::Configuration { .. }));
    }

    #[test]
    fn identity_errors_have_distinct_categories() {
        let authentication = AppError::authentication("Authentication failed");
        let authorization = AppError::authorization("Permission denied");

        assert!(matches!(authentication, AppError::Authentication { .. }));
        assert!(matches!(authorization, AppError::Authorization { .. }));
    }

    #[test]
    fn classifies_validation_and_access_errors_from_odoo_payloads() {
        let validation = AppError::from_odoo_rpc(&serde_json::json!({
            "data": {"name": "odoo.exceptions.ValidationError"}
        }));
        let access = AppError::from_odoo_rpc(&serde_json::json!({
            "data": {"name": "odoo.exceptions.AccessError"}
        }));

        assert!(matches!(validation, AppError::OdooValidation { .. }));
        assert!(matches!(access, AppError::OdooAccess { .. }));
    }

    #[test]
    fn communication_errors_have_distinct_categories() {
        assert!(matches!(
            AppError::transport("connection refused"),
            AppError::Transport { .. }
        ));
        assert!(matches!(
            AppError::timeout("request timed out"),
            AppError::Timeout { .. }
        ));
        assert!(matches!(
            AppError::protocol("invalid response"),
            AppError::Protocol { .. }
        ));
    }
}
