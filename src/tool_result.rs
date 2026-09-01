use crate::error::AppError;
use serde_json::Value;
use std::fmt::Display;

#[derive(Debug)]
pub(crate) enum ToolExecutionResult {
    Success(Value),
    Failure(AppError),
}

impl ToolExecutionResult {
    pub(crate) fn from_rpc<E>(operation: &str, result: Result<Value, E>) -> Self
    where
        E: Display,
    {
        match result {
            Ok(value) => Self::Success(value),
            Err(error) => Self::Failure(AppError::internal(format!(
                "Error executing {operation}: {error}"
            ))),
        }
    }

    pub(crate) fn invalid_arguments(operation: &str, error: impl Display) -> Self {
        Self::Failure(AppError::invalid_arguments(operation, error))
    }

    pub(crate) fn into_text(self) -> String {
        match self {
            Self::Success(value) => serde_json::to_string_pretty(&value)
                .unwrap_or_else(|error| format!("Error parsing result: {error}")),
            Self::Failure(error) => error.to_string(),
        }
    }
}
