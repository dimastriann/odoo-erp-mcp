#![allow(dead_code)] // The envelope is integrated incrementally through S4-06.

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
    pub(crate) kind: OperationKind,
    pub(crate) model: String,
}

impl Operation {
    pub(crate) fn new(kind: OperationKind, model: impl Into<String>) -> Self {
        Self {
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
    }

    #[test]
    fn mutation_kinds_remain_distinct() {
        assert_ne!(OperationKind::Create, OperationKind::Copy);
        assert_ne!(OperationKind::Update, OperationKind::Delete);
    }
}
