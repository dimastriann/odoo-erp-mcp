#![allow(dead_code)] // Preview generation is integrated incrementally through S4-28.

use crate::operation::{Operation, OperationKind};
use crate::{error::AppError, odoo::OdooClient};
use serde::Serialize;
use serde_json::Value;

/// A non-transactional description of a proposed mutation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct PreviewResponse {
    pub(crate) operation_id: String,
    pub(crate) operation: &'static str,
    pub(crate) model: String,
    pub(crate) affected_records: Vec<PreviewRecord>,
    pub(crate) summary: PreviewSummary,
    pub(crate) warnings: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct PreviewRecord {
    pub(crate) id: Option<i64>,
    pub(crate) current: Option<Value>,
    pub(crate) proposed: Option<Value>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct PreviewSummary {
    pub(crate) affected_count: usize,
    pub(crate) estimated: bool,
}

impl PreviewResponse {
    pub(crate) fn empty(operation: &Operation) -> Self {
        Self {
            operation_id: operation.id.to_string(),
            operation: operation_name(operation.kind),
            model: operation.model.clone(),
            affected_records: Vec::new(),
            summary: PreviewSummary {
                affected_count: 0,
                estimated: true,
            },
            warnings: vec![
                "Preview is not a transactional dry run; Odoo state may change before execution."
                    .to_string(),
            ],
        }
    }
}

pub(crate) fn preview_create(operation: &Operation) -> PreviewResponse {
    debug_assert_eq!(operation.kind, OperationKind::Create);
    let mut preview = PreviewResponse::empty(operation);
    let vals = operation
        .payload
        .as_value()
        .get("vals")
        .cloned()
        .expect("create operations always include vals");

    preview.affected_records.push(PreviewRecord {
        id: None,
        current: None,
        proposed: Some(vals),
    });
    preview.summary.affected_count = 1;
    preview
}

pub(crate) async fn preview_update(
    odoo: &OdooClient,
    operation: &Operation,
) -> Result<PreviewResponse, AppError> {
    debug_assert_eq!(operation.kind, OperationKind::Update);
    let payload = operation.payload.as_value();
    let ids = payload["ids"]
        .as_array()
        .expect("update operations always include ids")
        .iter()
        .map(|id| id.as_i64().expect("update operation IDs are integers"))
        .collect::<Vec<_>>();
    let vals = payload["vals"]
        .as_object()
        .expect("update operations always include vals");
    let fields = Value::Array(
        std::iter::once("id".to_string())
            .chain(vals.keys().cloned())
            .map(Value::String)
            .collect(),
    );
    let current = odoo.read(&operation.model, ids.clone(), fields).await?;
    let current = current.as_array().ok_or_else(|| {
        AppError::protocol("Odoo update preview returned a non-array record list")
    })?;
    let mut preview = PreviewResponse::empty(operation);

    for id in ids {
        let existing = current
            .iter()
            .find(|record| record.get("id").and_then(Value::as_i64) == Some(id))
            .cloned();
        preview.affected_records.push(PreviewRecord {
            id: Some(id),
            current: existing,
            proposed: Some(Value::Object(vals.clone())),
        });
    }
    preview.summary.affected_count = preview.affected_records.len();
    Ok(preview)
}

pub(crate) async fn preview_delete(
    odoo: &OdooClient,
    operation: &Operation,
) -> Result<PreviewResponse, AppError> {
    debug_assert_eq!(operation.kind, OperationKind::Delete);
    let ids = operation.payload.as_value()["ids"]
        .as_array()
        .expect("delete operations always include ids")
        .iter()
        .map(|id| id.as_i64().expect("delete operation IDs are integers"))
        .collect::<Vec<_>>();
    let current = odoo
        .read(
            &operation.model,
            ids.clone(),
            serde_json::json!(["id", "display_name"]),
        )
        .await?;
    let current = current.as_array().ok_or_else(|| {
        AppError::protocol("Odoo delete preview returned a non-array record list")
    })?;
    let mut preview = PreviewResponse::empty(operation);

    for id in ids {
        let existing = current
            .iter()
            .find(|record| record.get("id").and_then(Value::as_i64) == Some(id))
            .cloned();
        preview.affected_records.push(PreviewRecord {
            id: Some(id),
            current: existing,
            proposed: None,
        });
    }
    preview.summary.affected_count = preview.affected_records.len();
    Ok(preview)
}

pub(crate) const fn operation_name(kind: OperationKind) -> &'static str {
    match kind {
        OperationKind::Create => "create",
        OperationKind::Copy => "copy",
        OperationKind::Update => "update",
        OperationKind::Delete => "delete",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operation::OperationPayload;
    use crate::test_support::{MockOdooServer, authentication_success, json_rpc_success};
    use serde_json::json;

    #[test]
    fn preview_schema_exposes_stable_operation_metadata() {
        let operation = Operation::new(
            OperationKind::Create,
            "res.partner",
            OperationPayload::new(json!({"vals": {"name": "Alpha"}})).unwrap(),
        );

        let preview = PreviewResponse::empty(&operation);
        let value = serde_json::to_value(&preview).unwrap();

        assert_eq!(value["operation"], "create");
        assert_eq!(value["model"], "res.partner");
        assert_eq!(value["summary"]["affected_count"], 0);
        assert_eq!(value["summary"]["estimated"], true);
        assert!(!value["operation_id"].as_str().unwrap().is_empty());
        assert!(
            value["warnings"][0]
                .as_str()
                .unwrap()
                .contains("not a transactional dry run")
        );
    }

    #[test]
    fn create_preview_describes_values_without_a_record_id() {
        let operation = Operation::new(
            OperationKind::Create,
            "res.partner",
            OperationPayload::new(json!({"vals": {"name": "Alpha"}})).unwrap(),
        );

        let preview = preview_create(&operation);

        assert_eq!(preview.summary.affected_count, 1);
        assert_eq!(preview.affected_records[0].id, None);
        assert_eq!(preview.affected_records[0].current, None);
        assert_eq!(
            preview.affected_records[0].proposed,
            Some(json!({"name": "Alpha"}))
        );
    }

    #[tokio::test]
    async fn update_preview_reads_current_records_without_writing() {
        let server = MockOdooServer::start_with_responses(vec![
            authentication_success(7),
            json_rpc_success(json!([{"id": 7, "name": "Before"}])),
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
            OperationKind::Update,
            "res.partner",
            OperationPayload::new(json!({"ids": [7], "vals": {"name": "After"}})).unwrap(),
        );

        let preview = preview_update(&client, &operation).await.unwrap();

        assert_eq!(preview.affected_records[0].id, Some(7));
        assert_eq!(
            preview.affected_records[0].current,
            Some(json!({"id": 7, "name": "Before"}))
        );
        assert_eq!(
            preview.affected_records[0].proposed,
            Some(json!({"name": "After"}))
        );
        let requests = server.requests().await;
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[1]["params"]["args"][4], "read");
    }

    #[tokio::test]
    async fn delete_preview_reads_affected_records_without_writing() {
        let server = MockOdooServer::start_with_responses(vec![
            authentication_success(7),
            json_rpc_success(json!([{"id": 9, "display_name": "Alpha"}])),
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
            OperationKind::Delete,
            "res.partner",
            OperationPayload::new(json!({"ids": [9]})).unwrap(),
        );

        let preview = preview_delete(&client, &operation).await.unwrap();

        assert_eq!(preview.summary.affected_count, 1);
        assert_eq!(preview.affected_records[0].id, Some(9));
        assert_eq!(
            preview.affected_records[0].current,
            Some(json!({"id": 9, "display_name": "Alpha"}))
        );
        assert_eq!(preview.affected_records[0].proposed, None);
        let requests = server.requests().await;
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[1]["params"]["args"][4], "read");
    }
}
