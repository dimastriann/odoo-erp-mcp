#![allow(dead_code)] // Verification is integrated incrementally through Section 4.6.

use crate::error::AppError;
use crate::lifecycle::operation::{Operation, OperationKind};
use crate::odoo::OdooClient;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum VerificationStatus {
    Verified,
    Mismatch,
    Partial,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct VerificationResult {
    pub(crate) operation_id: String,
    pub(crate) status: VerificationStatus,
    pub(crate) checked_records: usize,
    pub(crate) mismatches: Vec<String>,
}

impl VerificationResult {
    pub(crate) fn new(operation: &Operation, status: VerificationStatus) -> Self {
        Self {
            operation_id: operation.id.to_string(),
            status,
            checked_records: 0,
            mismatches: Vec::new(),
        }
    }

    pub(crate) fn is_success(&self) -> bool {
        self.status == VerificationStatus::Verified
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ExecutionStatus {
    Succeeded,
    Failed,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct LifecycleOutcome {
    pub(crate) execution: ExecutionStatus,
    pub(crate) verification: VerificationResult,
}

impl LifecycleOutcome {
    pub(crate) fn new(execution: ExecutionStatus, verification: VerificationResult) -> Self {
        Self {
            execution,
            verification,
        }
    }
}

pub(crate) async fn verify_create(
    odoo: &OdooClient,
    operation: &Operation,
    created_id: i64,
) -> Result<VerificationResult, AppError> {
    let mut result = VerificationResult::new(operation, VerificationStatus::Verified);
    let records = odoo
        .read(&operation.model, vec![created_id], json!(["id"]))
        .await?;
    result.checked_records = 1;
    if !record_exists(&records) {
        result.status = VerificationStatus::Mismatch;
        result
            .mismatches
            .push(format!("Created record {created_id} was not found"));
    }
    Ok(result)
}

fn record_exists(records: &Value) -> bool {
    records.as_array().is_some_and(|items| !items.is_empty())
}

pub(crate) async fn verify_update(
    odoo: &OdooClient,
    operation: &Operation,
) -> Result<VerificationResult, AppError> {
    let payload = operation.payload.as_value();
    let ids = payload["ids"].as_array().cloned().unwrap_or_default();
    let vals = payload["vals"].as_object().cloned().unwrap_or_default();
    let fields = Value::Array(vals.keys().cloned().map(Value::String).collect());
    let records = odoo
        .read(
            &operation.model,
            ids.clone()
                .into_iter()
                .filter_map(|id| id.as_i64())
                .collect(),
            fields,
        )
        .await?;
    let mut result = VerificationResult::new(operation, VerificationStatus::Verified);
    result.checked_records = records.as_array().map_or(0, Vec::len);
    if let Some(status) = classify_coverage(result.checked_records, ids.len()) {
        result.status = status;
        result
            .mismatches
            .push("Not all updated records could be verified".to_string());
    } else if records.as_array().is_some_and(|items| {
        items
            .iter()
            .any(|record| !record_matches_values(record, &vals))
    }) {
        result.status = VerificationStatus::Mismatch;
        result
            .mismatches
            .push("Updated fields do not match the requested values".to_string());
    }
    Ok(result)
}

fn classify_coverage(checked: usize, expected: usize) -> Option<VerificationStatus> {
    (checked < expected).then_some(VerificationStatus::Partial)
}

fn record_matches_values(record: &Value, values: &serde_json::Map<String, Value>) -> bool {
    values
        .iter()
        .all(|(field, expected)| record.get(field) == Some(expected))
}

pub(crate) async fn verify_delete(
    odoo: &OdooClient,
    operation: &Operation,
) -> Result<VerificationResult, AppError> {
    let ids = operation.payload.as_value()["ids"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|id| id.as_i64())
        .collect::<Vec<_>>();
    let records = odoo
        .read(&operation.model, ids.clone(), json!(["id"]))
        .await?;
    let mut result = VerificationResult::new(operation, VerificationStatus::Verified);
    result.checked_records = ids.len();
    if !deleted_records_absent(&records) {
        result.status = VerificationStatus::Mismatch;
        result
            .mismatches
            .push("Deleted records are still present".to_string());
    }
    Ok(result)
}

fn deleted_records_absent(records: &Value) -> bool {
    records.as_array().is_none_or(Vec::is_empty)
}

pub(crate) fn operation_requires_verification(operation: &Operation) -> bool {
    matches!(
        operation.kind,
        OperationKind::Create | OperationKind::Update | OperationKind::Delete
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lifecycle::operation::{Operation, OperationPayload};

    #[test]
    fn verification_schema_exposes_operation_and_status() {
        let operation = Operation::new(
            OperationKind::Create,
            "res.partner",
            OperationPayload::new(json!({"vals": {"name": "A"}})).unwrap(),
        );
        let result = VerificationResult::new(&operation, VerificationStatus::Verified);
        assert!(result.is_success());
        assert_eq!(result.checked_records, 0);
        assert!(!result.operation_id.is_empty());
    }

    #[test]
    fn create_verification_requires_a_returned_record() {
        assert!(record_exists(&json!([{"id": 1}])));
        assert!(!record_exists(&json!([])));
    }

    #[test]
    fn update_verification_compares_requested_fields() {
        let values = serde_json::from_value(json!({"name": "Updated"})).unwrap();
        assert!(record_matches_values(&json!({"name": "Updated"}), &values));
        assert!(!record_matches_values(
            &json!({"name": "Original"}),
            &values
        ));
    }

    #[test]
    fn delete_verification_requires_record_absence() {
        assert!(deleted_records_absent(&json!([])));
        assert!(!deleted_records_absent(&json!([{"id": 1}])));
    }

    #[test]
    fn verification_reports_partial_coverage() {
        assert_eq!(classify_coverage(1, 2), Some(VerificationStatus::Partial));
        assert_eq!(classify_coverage(2, 2), None);
    }

    #[test]
    fn lifecycle_outcome_keeps_execution_and_verification_separate() {
        let operation = Operation::new(
            OperationKind::Delete,
            "res.partner",
            OperationPayload::new(json!({"ids": [1]})).unwrap(),
        );
        let verification = VerificationResult::new(&operation, VerificationStatus::Mismatch);
        let outcome = LifecycleOutcome::new(ExecutionStatus::Succeeded, verification);
        assert_eq!(outcome.execution, ExecutionStatus::Succeeded);
        assert_eq!(outcome.verification.status, VerificationStatus::Mismatch);
    }

    #[test]
    fn mismatch_and_unknown_statuses_are_distinct() {
        assert_ne!(VerificationStatus::Mismatch, VerificationStatus::Unknown);
        let encoded = serde_json::to_value(VerificationStatus::Unknown).unwrap();
        assert_eq!(encoded, json!("unknown"));
    }
}
