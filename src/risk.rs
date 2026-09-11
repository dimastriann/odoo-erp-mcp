#![allow(dead_code)] // Risk evaluation is integrated incrementally through S4-19.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::operation::{Operation, OperationKind};

/// Ordered severity assigned to an operation before execution.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub(crate) struct RiskPolicy {
    pub(crate) models: BTreeMap<String, RiskLevel>,
    pub(crate) fields: BTreeMap<String, BTreeMap<String, RiskLevel>>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RiskEvaluator {
    policy: RiskPolicy,
}

impl RiskEvaluator {
    pub(crate) fn new(policy: RiskPolicy) -> Self {
        Self { policy }
    }

    pub(crate) fn classify(&self, operation: &Operation) -> RiskLevel {
        let default = self
            .policy
            .models
            .get(&operation.model)
            .copied()
            .unwrap_or(match operation.kind {
                OperationKind::Create | OperationKind::Copy => RiskLevel::Low,
                OperationKind::Update => RiskLevel::Medium,
                OperationKind::Delete => RiskLevel::High,
            });

        self.field_override(operation).unwrap_or(default)
    }

    fn field_override(&self, operation: &Operation) -> Option<RiskLevel> {
        let configured = self.policy.fields.get(&operation.model)?;
        operation
            .payload
            .as_value()
            .get("vals")?
            .as_object()?
            .keys()
            .filter_map(|field| configured.get(field).copied())
            .max()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operation::OperationPayload;
    use serde_json::json;

    fn operation(kind: OperationKind) -> Operation {
        Operation::new(
            kind,
            "res.partner",
            OperationPayload::new(json!({})).unwrap(),
        )
    }

    #[test]
    fn risk_levels_have_a_stable_severity_order() {
        assert!(RiskLevel::Low < RiskLevel::Medium);
        assert!(RiskLevel::Medium < RiskLevel::High);
        assert!(RiskLevel::High < RiskLevel::Critical);
    }

    #[test]
    fn risk_levels_use_stable_configuration_names() {
        assert_eq!(serde_json::to_value(RiskLevel::Low).unwrap(), "low");
        assert_eq!(
            serde_json::to_value(RiskLevel::Critical).unwrap(),
            "critical"
        );
        assert_eq!(
            serde_json::from_value::<RiskLevel>(serde_json::json!("high")).unwrap(),
            RiskLevel::High
        );
    }

    #[test]
    fn mutation_types_have_conservative_default_risk() {
        let evaluator = RiskEvaluator::default();

        assert_eq!(
            evaluator.classify(&operation(OperationKind::Create)),
            RiskLevel::Low
        );
        assert_eq!(
            evaluator.classify(&operation(OperationKind::Copy)),
            RiskLevel::Low
        );
        assert_eq!(
            evaluator.classify(&operation(OperationKind::Update)),
            RiskLevel::Medium
        );
        assert_eq!(
            evaluator.classify(&operation(OperationKind::Delete)),
            RiskLevel::High
        );
    }

    #[test]
    fn model_overrides_replace_operation_defaults() {
        let evaluator = RiskEvaluator::new(RiskPolicy {
            models: BTreeMap::from([
                ("res.partner".to_string(), RiskLevel::High),
                ("mail.message".to_string(), RiskLevel::Low),
            ]),
            ..RiskPolicy::default()
        });
        let create = operation(OperationKind::Create);
        let delete = Operation::new(
            OperationKind::Delete,
            "mail.message",
            OperationPayload::new(json!({})).unwrap(),
        );

        assert_eq!(evaluator.classify(&create), RiskLevel::High);
        assert_eq!(evaluator.classify(&delete), RiskLevel::Low);
    }

    #[test]
    fn highest_matching_field_override_wins() {
        let evaluator = RiskEvaluator::new(RiskPolicy {
            models: BTreeMap::from([("res.partner".to_string(), RiskLevel::Low)]),
            fields: BTreeMap::from([(
                "res.partner".to_string(),
                BTreeMap::from([
                    ("name".to_string(), RiskLevel::Medium),
                    ("credit_limit".to_string(), RiskLevel::Critical),
                ]),
            )]),
        });
        let operation = Operation::new(
            OperationKind::Update,
            "res.partner",
            OperationPayload::new(json!({
                "ids": [7],
                "vals": {"name": "Alpha", "credit_limit": 5000}
            }))
            .unwrap(),
        );

        assert_eq!(evaluator.classify(&operation), RiskLevel::Critical);
    }
}
