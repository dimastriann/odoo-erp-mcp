use std::error::Error;
use std::fmt::{self, Display, Formatter};

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum AppError {
    InputValidation { message: String },
    Internal { message: String },
}

impl AppError {
    pub(crate) fn input_validation(message: impl Into<String>) -> Self {
        Self::InputValidation {
            message: message.into(),
        }
    }

    pub(crate) fn invalid_arguments(operation: &str, error: impl Display) -> Self {
        Self::input_validation(format!("Error: Invalid {operation} arguments: {error}"))
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
            Self::InputValidation { message } | Self::Internal { message } => {
                formatter.write_str(message)
            }
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
}
