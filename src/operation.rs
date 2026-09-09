#![allow(dead_code)] // The envelope is integrated incrementally through S4-06.

use serde_json::{Map, Value};
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
}

impl Operation {
    pub(crate) fn new(
        kind: OperationKind,
        model: impl Into<String>,
        payload: OperationPayload,
    ) -> Self {
        Self {
            id: OperationId::new(),
            kind,
            class: OperationClass::Write,
            model: model.into(),
            payload,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
