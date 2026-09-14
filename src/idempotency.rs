#![allow(dead_code)] // Idempotency is integrated incrementally through Section 4.5.

use crate::error::AppError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub(crate) struct IdempotencyKey(String);

impl IdempotencyKey {
    pub(crate) fn new(value: impl Into<String>) -> Result<Self, AppError> {
        let value = value.into();
        if value.trim().is_empty() || value.len() > 256 {
            return Err(AppError::input_validation(
                "Idempotency key must contain 1 to 256 characters",
            ));
        }
        Ok(Self(value))
    }
}

impl std::fmt::Display for IdempotencyKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum IdempotencyState {
    Pending,
    Succeeded,
    Failed,
    Unknown,
    Expired,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct IdempotencyRecord {
    pub(crate) key: IdempotencyKey,
    pub(crate) actor_subject: Option<String>,
    pub(crate) instance: String,
    pub(crate) payload_hash: String,
    pub(crate) state: IdempotencyState,
    pub(crate) result: Option<Value>,
    pub(crate) created_at: i64,
    pub(crate) expires_at: i64,
}

impl IdempotencyRecord {
    pub(crate) fn is_expired(&self, now: i64) -> bool {
        now >= self.expires_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_preserves_state_scope_and_result() {
        let record = IdempotencyRecord {
            key: IdempotencyKey::new("request-1").unwrap(),
            actor_subject: Some("user-1".to_string()),
            instance: "production".to_string(),
            payload_hash: "abc123".to_string(),
            state: IdempotencyState::Succeeded,
            result: Some(serde_json::json!({"id": 42})),
            created_at: 100,
            expires_at: 200,
        };
        let encoded = serde_json::to_value(&record).unwrap();
        let decoded: IdempotencyRecord = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded, record);
        assert!(!record.is_expired(199));
        assert!(record.is_expired(200));
    }

    #[test]
    fn keys_reject_empty_and_overlong_values() {
        assert!(IdempotencyKey::new(" ").is_err());
        assert!(IdempotencyKey::new("x".repeat(257)).is_err());
        assert_eq!(
            IdempotencyKey::new("request-1").unwrap().to_string(),
            "request-1"
        );
    }
}
