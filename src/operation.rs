#![allow(dead_code)] // The envelope is integrated incrementally through S4-06.

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

/// Stable, typed metadata shared by every stage of a mutation lifecycle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Operation {
    pub(crate) id: OperationId,
    pub(crate) kind: OperationKind,
    pub(crate) model: String,
}

impl Operation {
    pub(crate) fn new(kind: OperationKind, model: impl Into<String>) -> Self {
        Self {
            id: OperationId::new(),
            kind,
            model: model.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_preserves_typed_kind_and_model() {
        let operation = Operation::new(OperationKind::Create, "res.partner");

        assert_eq!(operation.kind, OperationKind::Create);
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
        let first = Operation::new(OperationKind::Update, "res.partner");
        let second = Operation::new(OperationKind::Update, "res.partner");

        assert_ne!(first.id, second.id);
    }
}
