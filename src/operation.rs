#![allow(dead_code)] // The envelope is integrated incrementally through S4-06.

use crate::error::AppError;
use crate::odoo::OdooClient;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::fmt;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct OperationId(Uuid);

impl OperationId {
    pub(crate) fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl fmt::Display for OperationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// A mutation understood by the safe operation lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OperationKind {
    Create,
    Copy,
    Update,
    Delete,
}

/// Security and risk category applied to an operation before execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OperationClass {
    Read,
    Write,
    Workflow,
    Financial,
    Admin,
}

/// Canonical JSON data used by lifecycle checks and execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OperationPayload(Value);

impl OperationPayload {
    pub(crate) fn new(value: Value) -> Result<Self, &'static str> {
        if !value.is_object() {
            return Err("operation payload must be a JSON object");
        }

        Ok(Self(normalize_json(value)))
    }

    pub(crate) fn as_value(&self) -> &Value {
        &self.0
    }

    fn hash(&self) -> PayloadHash {
        let bytes = serde_json::to_vec(&self.0).expect("normalized JSON must serialize");
        PayloadHash(Sha256::digest(bytes).into())
    }
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub(crate) struct PayloadHash([u8; 32]);

impl fmt::Debug for PayloadHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "PayloadHash({self})")
    }
}

impl fmt::Display for PayloadHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

fn normalize_json(value: Value) -> Value {
    match value {
        Value::Object(entries) => {
            let mut keys: Vec<_> = entries.keys().cloned().collect();
            keys.sort_unstable();
            let normalized = keys
                .into_iter()
                .map(|key| {
                    let value = entries[&key].clone();
                    (key, normalize_json(value))
                })
                .collect::<Map<_, _>>();
            Value::Object(normalized)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(normalize_json).collect()),
        scalar => scalar,
    }
}

/// Stable, typed metadata shared by every stage of a mutation lifecycle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Operation {
    pub(crate) id: OperationId,
    pub(crate) kind: OperationKind,
    pub(crate) class: OperationClass,
    pub(crate) model: String,
    pub(crate) payload: OperationPayload,
    pub(crate) payload_hash: PayloadHash,
}

impl Operation {
    pub(crate) fn new(
        kind: OperationKind,
        model: impl Into<String>,
        payload: OperationPayload,
    ) -> Self {
        let payload_hash = payload.hash();
        Self {
            id: OperationId::new(),
            kind,
            class: OperationClass::Write,
            model: model.into(),
            payload,
            payload_hash,
        }
    }
}

/// Single execution boundary for mutations that have passed lifecycle checks.
pub(crate) struct OperationExecutor<'a> {
    odoo: &'a OdooClient,
}

impl<'a> OperationExecutor<'a> {
    pub(crate) fn new(odoo: &'a OdooClient) -> Self {
        Self { odoo }
    }

    pub(crate) async fn execute(&self, operation: &Operation) -> Result<Value, AppError> {
        let payload = operation
            .payload
            .as_value()
            .as_object()
            .expect("operation payloads always have an object root");

        match operation.kind {
            OperationKind::Create => {
                self.odoo
                    .create(&operation.model, required_value(payload, "vals")?.clone())
                    .await
            }
            OperationKind::Copy => {
                self.odoo
                    .copy(
                        &operation.model,
                        required_i64(payload, "id")?,
                        required_value(payload, "vals")?.clone(),
                    )
                    .await
            }
            OperationKind::Update => {
                self.odoo
                    .update(
                        &operation.model,
                        required_ids(payload)?,
                        required_value(payload, "vals")?.clone(),
                    )
                    .await
            }
            OperationKind::Delete => {
                self.odoo
                    .delete(&operation.model, required_ids(payload)?)
                    .await
            }
        }
    }
}

fn required_value<'a>(payload: &'a Map<String, Value>, key: &str) -> Result<&'a Value, AppError> {
    payload
        .get(key)
        .ok_or_else(|| invalid_operation_payload(format!("missing '{key}'")))
}

fn required_i64(payload: &Map<String, Value>, key: &str) -> Result<i64, AppError> {
    required_value(payload, key)?
        .as_i64()
        .ok_or_else(|| invalid_operation_payload(format!("'{key}' must be an integer")))
}

fn required_ids(payload: &Map<String, Value>) -> Result<Vec<i64>, AppError> {
    required_value(payload, "ids")?
        .as_array()
        .ok_or_else(|| invalid_operation_payload("'ids' must be an array"))?
        .iter()
        .map(|id| {
            id.as_i64()
                .ok_or_else(|| invalid_operation_payload("'ids' must contain only integers"))
        })
        .collect()
}

fn invalid_operation_payload(message: impl fmt::Display) -> AppError {
    AppError::internal(format!("Invalid operation payload: {message}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        MockOdooServer, authentication_success, json_rpc_success, validation_error,
    };
    use serde_json::json;

    fn empty_payload() -> OperationPayload {
        OperationPayload::new(json!({})).unwrap()
    }

    #[test]
    fn operation_preserves_typed_kind_and_model() {
        let operation = Operation::new(OperationKind::Create, "res.partner", empty_payload());

        assert_eq!(operation.kind, OperationKind::Create);
        assert_eq!(operation.class, OperationClass::Write);
        assert_eq!(operation.model, "res.partner");
        assert!(!operation.id.to_string().is_empty());
    }

    #[test]
    fn mutation_kinds_remain_distinct() {
        assert_ne!(OperationKind::Create, OperationKind::Copy);
        assert_ne!(OperationKind::Update, OperationKind::Delete);
    }

    #[test]
    fn every_operation_has_a_unique_id() {
        let first = Operation::new(OperationKind::Update, "res.partner", empty_payload());
        let second = Operation::new(OperationKind::Update, "res.partner", empty_payload());

        assert_ne!(first.id, second.id);
    }

    #[test]
    fn operation_classes_cover_lifecycle_risk_boundaries() {
        let classes = [
            OperationClass::Read,
            OperationClass::Write,
            OperationClass::Workflow,
            OperationClass::Financial,
            OperationClass::Admin,
        ];

        assert_eq!(classes.len(), 5);
        assert_ne!(OperationClass::Read, OperationClass::Write);
        assert_ne!(OperationClass::Workflow, OperationClass::Financial);
    }

    #[test]
    fn payloads_are_normalized_recursively() {
        let first = OperationPayload::new(json!({
            "vals": {"z": 1, "a": 2},
            "ids": [3, 1]
        }))
        .unwrap();
        let second = OperationPayload::new(json!({
            "ids": [3, 1],
            "vals": {"a": 2, "z": 1}
        }))
        .unwrap();

        assert_eq!(first, second);
        assert_eq!(first.as_value()["ids"], json!([3, 1]));
    }

    #[test]
    fn payloads_reject_non_object_roots() {
        assert_eq!(
            OperationPayload::new(json!([1, 2, 3])),
            Err("operation payload must be a JSON object")
        );
    }

    #[test]
    fn normalized_payloads_have_stable_sha256_hashes() {
        let first = OperationPayload::new(json!({"vals": {"name": "Alpha"}, "ids": [7]})).unwrap();
        let reordered =
            OperationPayload::new(json!({"ids": [7], "vals": {"name": "Alpha"}})).unwrap();
        let changed =
            OperationPayload::new(json!({"ids": [8], "vals": {"name": "Alpha"}})).unwrap();

        assert_eq!(first.hash(), reordered.hash());
        assert_ne!(first.hash(), changed.hash());
        assert_eq!(first.hash().to_string().len(), 64);
    }

    #[test]
    fn operation_captures_hash_when_it_is_created() {
        let operation = Operation::new(
            OperationKind::Delete,
            "res.partner",
            OperationPayload::new(json!({"ids": [42]})).unwrap(),
        );

        assert_eq!(operation.payload_hash, operation.payload.hash());
    }

    #[test]
    fn executor_rejects_incomplete_internal_payloads_before_rpc() {
        let payload = OperationPayload::new(json!({})).unwrap();
        let payload = payload.as_value().as_object().unwrap();

        let error = required_ids(payload).unwrap_err();

        assert!(matches!(error, AppError::Internal { .. }));
        assert!(error.to_string().contains("missing 'ids'"));
    }

    #[tokio::test]
    async fn invalid_lifecycle_payloads_never_reach_odoo() {
        let server = MockOdooServer::start(authentication_success(7)).await;
        let client = OdooClient::new(
            server.base_url().to_string(),
            "test-db".to_string(),
            "admin".to_string(),
            "secret".to_string(),
        )
        .await
        .unwrap();
        let operations = [
            Operation::new(OperationKind::Create, "res.partner", empty_payload()),
            Operation::new(
                OperationKind::Copy,
                "res.partner",
                OperationPayload::new(json!({"vals": {}})).unwrap(),
            ),
            Operation::new(
                OperationKind::Update,
                "res.partner",
                OperationPayload::new(json!({"ids": ["invalid"], "vals": {}})).unwrap(),
            ),
            Operation::new(
                OperationKind::Delete,
                "res.partner",
                OperationPayload::new(json!({"ids": "invalid"})).unwrap(),
            ),
        ];
        let executor = OperationExecutor::new(&client);

        for operation in &operations {
            let result = executor.execute(operation).await;
            assert!(matches!(result, Err(AppError::Internal { .. })));
        }

        assert_eq!(server.requests().await.len(), 1);
    }

    #[tokio::test]
    async fn lifecycle_preserves_envelope_and_odoo_error_category() {
        let server = MockOdooServer::start_with_responses(vec![
            authentication_success(7),
            validation_error("Duplicate reference"),
        ])
        .await;
        let client = OdooClient::new(
            server.base_url().to_string(),
            "test-db".to_string(),
            "admin".to_string(),
            "secret".to_string(),
        )
        .await
        .unwrap();
        let operation = Operation::new(
            OperationKind::Create,
            "res.partner",
            OperationPayload::new(json!({"vals": {"name": "Alpha"}})).unwrap(),
        );
        let before_execution = operation.clone();

        let result = OperationExecutor::new(&client).execute(&operation).await;

        assert!(matches!(result, Err(AppError::OdooValidation { .. })));
        assert_eq!(operation, before_execution);
        assert_eq!(server.requests().await.len(), 2);
    }

    #[tokio::test]
    async fn lifecycle_returns_success_without_rewriting_the_envelope() {
        let server = MockOdooServer::start_with_responses(vec![
            authentication_success(7),
            json_rpc_success(json!(42)),
        ])
        .await;
        let client = OdooClient::new(
            server.base_url().to_string(),
            "test-db".to_string(),
            "admin".to_string(),
            "secret".to_string(),
        )
        .await
        .unwrap();
        let operation = Operation::new(
            OperationKind::Create,
            "res.partner",
            OperationPayload::new(json!({"vals": {"name": "Alpha"}})).unwrap(),
        );
        let before_execution = operation.clone();

        let result = OperationExecutor::new(&client)
            .execute(&operation)
            .await
            .unwrap();

        assert_eq!(result, json!(42));
        assert_eq!(operation, before_execution);
    }
}
