use std::error::Error;
use std::fmt::{self, Display, Formatter};

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum AppError {
    Internal { message: String },
}

impl AppError {
    pub(crate) fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }
}

impl Display for AppError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Internal { message } => formatter.write_str(message),
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
}
