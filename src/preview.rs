#![allow(dead_code)] // Preview generation is integrated incrementally through S4-28.

use crate::operation::{Operation, OperationKind};
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
}
