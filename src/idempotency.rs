#![allow(dead_code)] // Idempotency is integrated incrementally through Section 4.5.

use crate::context::RequestContext;
use crate::error::AppError;
use crate::operation::Operation;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
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
    pub(crate) fn for_operation(
        key: IdempotencyKey,
        operation: &Operation,
        context: &RequestContext,
        now: i64,
        ttl_seconds: i64,
    ) -> Result<Self, AppError> {
        if ttl_seconds <= 0 {
            return Err(AppError::input_validation(
                "Idempotency TTL must be greater than zero",
            ));
        }
        Ok(Self {
            key,
            actor_subject: context.actor.subject.clone(),
            instance: context.instance.clone(),
            payload_hash: operation.payload_hash.to_string(),
            state: IdempotencyState::Pending,
            result: None,
            created_at: now,
            expires_at: now.saturating_add(ttl_seconds),
        })
    }

    pub(crate) fn scope_matches(&self, context: &RequestContext) -> bool {
        self.actor_subject == context.actor.subject && self.instance == context.instance
    }

    pub(crate) fn is_expired(&self, now: i64) -> bool {
        now >= self.expires_at
    }
}

pub(crate) trait IdempotencyStorage {
    fn insert(&mut self, record: IdempotencyRecord) -> Result<(), AppError>;
    fn get(&self, key: &IdempotencyKey) -> Result<Option<IdempotencyRecord>, AppError>;
    fn update(&mut self, record: IdempotencyRecord) -> Result<(), AppError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{ActorIdentity, ClientIdentity, IdentitySource};
    use crate::operation::{OperationKind, OperationPayload};
    use std::collections::BTreeMap;

    struct MemoryStorage(BTreeMap<IdempotencyKey, IdempotencyRecord>);

    impl IdempotencyStorage for MemoryStorage {
        fn insert(&mut self, record: IdempotencyRecord) -> Result<(), AppError> {
            self.0.insert(record.key.clone(), record);
            Ok(())
        }

        fn get(&self, key: &IdempotencyKey) -> Result<Option<IdempotencyRecord>, AppError> {
            Ok(self.0.get(key).cloned())
        }

        fn update(&mut self, record: IdempotencyRecord) -> Result<(), AppError> {
            self.0.insert(record.key.clone(), record);
            Ok(())
        }
    }

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

    #[test]
    fn storage_contract_round_trips_records() {
        let key = IdempotencyKey::new("request-1").unwrap();
        let record = IdempotencyRecord {
            key: key.clone(),
            actor_subject: None,
            instance: "test".to_string(),
            payload_hash: "hash".to_string(),
            state: IdempotencyState::Pending,
            result: None,
            created_at: 1,
            expires_at: 2,
        };
        let mut storage = MemoryStorage(BTreeMap::new());
        storage.insert(record.clone()).unwrap();
        assert_eq!(storage.get(&key).unwrap(), Some(record));
    }

    #[test]
    fn record_scope_is_bound_to_actor_and_instance() {
        let context = RequestContext::identified(
            ClientIdentity::default(),
            Default::default(),
            ActorIdentity::claimed(
                Some("user-1".to_string()),
                None,
                IdentitySource::AuthenticatedTransport,
            ),
            "production".to_string(),
        );
        let operation = Operation::new(
            OperationKind::Delete,
            "res.partner",
            OperationPayload::new(serde_json::json!({"ids": [1]})).unwrap(),
        );
        let record = IdempotencyRecord::for_operation(
            IdempotencyKey::new("request-1").unwrap(),
            &operation,
            &context,
            100,
            60,
        )
        .unwrap();
        assert!(record.scope_matches(&context));
        assert!(!record.scope_matches(&RequestContext::identified(
            ClientIdentity::default(),
            Default::default(),
            ActorIdentity::claimed(
                Some("other".to_string()),
                None,
                IdentitySource::AuthenticatedTransport
            ),
            "production".to_string(),
        )));
    }
}
