use crate::error::AppError;
use serde_json::{Value, json};
use std::fmt::Display;

#[derive(Debug)]
pub(crate) enum ToolExecutionResult {
    Success(Value),
    Failure(AppError),
}

impl ToolExecutionResult {
    pub(crate) fn from_app_error(result: Result<Value, AppError>) -> Self {
        match result {
            Ok(value) => Self::Success(value),
            Err(error) => Self::Failure(error),
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

    pub(crate) fn into_mcp_result(self) -> Value {
        match self {
            Self::Failure(error) => {
                let structured = serde_json::to_value(error.structured_response())
                    .expect("structured errors must be serializable");
                let text = serde_json::to_string_pretty(&structured)
                    .expect("structured errors must be serializable");
                json!({
                    "content": [{"type": "text", "text": text}],
                    "structuredContent": structured,
                    "isError": true
                })
            }
            result => json!({
                "content": [{"type": "text", "text": result.into_text()}]
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_failure_sets_mcp_error_and_content() {
        let result = ToolExecutionResult::Failure(AppError::input_validation("invalid domain"))
            .into_mcp_result();

        assert_eq!(result["isError"], true);
        assert_eq!(
            result["structuredContent"]["error"]["code"],
            "INVALID_ARGUMENTS"
        );
        assert_eq!(
            result["structuredContent"]["error"]["message"],
            "invalid domain"
        );
        assert!(
            result["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("INVALID_ARGUMENTS")
        );
    }
}
