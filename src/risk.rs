#![allow(dead_code)] // Risk evaluation is integrated incrementally through S4-19.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::operation::{Operation, OperationClass, OperationKind};

/// Ordered severity assigned to an operation before execution.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl RiskLevel {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RiskSource {
    OperationType,
    OperationClass,
    ModelOverride,
    FieldOverride,
    BulkThreshold,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct RiskAssessment {
    pub(crate) level: RiskLevel,
    pub(crate) source: RiskSource,
    pub(crate) reason: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub(crate) struct RiskPolicy {
    pub(crate) models: BTreeMap<String, RiskLevel>,
    pub(crate) fields: BTreeMap<String, BTreeMap<String, RiskLevel>>,
    pub(crate) bulk: BulkRiskThresholds,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub(crate) struct BulkRiskThresholds {
    pub(crate) medium: usize,
    pub(crate) high: usize,
    pub(crate) critical: usize,
}

impl Default for BulkRiskThresholds {
    fn default() -> Self {
        Self {
            medium: 10,
            high: 100,
            critical: 1_000,
        }
    }
}

impl BulkRiskThresholds {
    fn classify(&self, record_count: usize) -> RiskLevel {
        if record_count >= self.critical {
            RiskLevel::Critical
        } else if record_count >= self.high {
            RiskLevel::High
        } else if record_count >= self.medium {
            RiskLevel::Medium
        } else {
            RiskLevel::Low
        }
    }
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
        self.assess(operation).level
    }

    pub(crate) fn assess(&self, operation: &Operation) -> RiskAssessment {
        let operation_default = match operation.kind {
            OperationKind::Create | OperationKind::Copy => RiskLevel::Low,
            OperationKind::Update => RiskLevel::Medium,
            OperationKind::Delete => RiskLevel::High,
        };
        let mut assessment = RiskAssessment {
            level: operation_default,
            source: RiskSource::OperationType,
            reason: format!(
                "operation type '{}' defaults to {} risk",
                operation_kind_name(operation.kind),
                operation_default.as_str()
            ),
        };
        let class_level = match operation.class {
            OperationClass::Write => None,
            OperationClass::Read => Some(RiskLevel::Low),
            OperationClass::Workflow => Some(RiskLevel::High),
            OperationClass::Financial if operation.kind == OperationKind::Delete => {
                Some(RiskLevel::Critical)
            }
            OperationClass::Financial => Some(RiskLevel::High),
            OperationClass::Admin => Some(RiskLevel::Critical),
        };
        if let Some(level) = class_level {
            assessment = RiskAssessment {
                level,
                source: RiskSource::OperationClass,
                reason: format!(
                    "operation class '{}' requires {} risk",
                    operation_class_name(operation.class),
                    level.as_str()
                ),
            };
        }
        if let Some(level) = self.policy.models.get(&operation.model).copied() {
            assessment = RiskAssessment {
                level,
                source: RiskSource::ModelOverride,
                reason: format!(
                    "model override '{}' sets {} risk",
                    operation.model,
                    level.as_str()
                ),
            };
        }
        if let Some((field, level)) = self.field_override(operation) {
            assessment = RiskAssessment {
                level,
                source: RiskSource::FieldOverride,
                reason: format!(
                    "field override '{}.{}' sets {} risk",
                    operation.model,
                    field,
                    level.as_str()
                ),
            };
        }
        let record_count = affected_record_count(operation);
        let bulk_level = self.policy.bulk.classify(record_count);
        if bulk_level > assessment.level {
            assessment = RiskAssessment {
                level: bulk_level,
                source: RiskSource::BulkThreshold,
                reason: format!(
                    "bulk operation affecting {record_count} records requires {} risk",
                    bulk_level.as_str()
                ),
            };
        }

        assessment
    }

    fn field_override(&self, operation: &Operation) -> Option<(String, RiskLevel)> {
        let configured = self.policy.fields.get(&operation.model)?;
        operation
            .payload
            .as_value()
            .get("vals")?
            .as_object()?
            .keys()
            .filter_map(|field| configured.get(field).copied().map(|level| (field, level)))
            .max_by(|(left_field, left_level), (right_field, right_level)| {
                left_level
                    .cmp(right_level)
                    .then_with(|| right_field.cmp(left_field))
            })
            .map(|(field, level)| (field.clone(), level))
    }
}

const fn operation_kind_name(kind: OperationKind) -> &'static str {
    match kind {
        OperationKind::Create => "create",
        OperationKind::Copy => "copy",
        OperationKind::Update => "update",
        OperationKind::Delete => "delete",
    }
}

const fn operation_class_name(class: OperationClass) -> &'static str {
    match class {
        OperationClass::Read => "read",
        OperationClass::Write => "write",
        OperationClass::Workflow => "workflow",
        OperationClass::Financial => "financial",
        OperationClass::Admin => "admin",
    }
}

fn affected_record_count(operation: &Operation) -> usize {
    match operation.kind {
        OperationKind::Create | OperationKind::Copy => 1,
        OperationKind::Update | OperationKind::Delete => operation
            .payload
            .as_value()
            .get("ids")
            .and_then(|ids| ids.as_array())
            .map_or(0, Vec::len),
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
            ..RiskPolicy::default()
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

    #[test]
    fn bulk_thresholds_escalate_multi_record_mutations() {
        let evaluator = RiskEvaluator::new(RiskPolicy {
            bulk: BulkRiskThresholds {
                medium: 2,
                high: 5,
                critical: 10,
            },
            ..RiskPolicy::default()
        });
        let update = |count| {
            Operation::new(
                OperationKind::Update,
                "res.partner",
                OperationPayload::new(json!({
                    "ids": (1..=count).collect::<Vec<i64>>(),
                    "vals": {"active": false}
                }))
                .unwrap(),
            )
        };

        assert_eq!(evaluator.classify(&update(1)), RiskLevel::Medium);
        assert_eq!(evaluator.classify(&update(2)), RiskLevel::Medium);
        assert_eq!(evaluator.classify(&update(5)), RiskLevel::High);
        assert_eq!(evaluator.classify(&update(10)), RiskLevel::Critical);
    }

    #[test]
    fn bulk_risk_never_downgrades_a_scoped_override() {
        let evaluator = RiskEvaluator::new(RiskPolicy {
            fields: BTreeMap::from([(
                "res.partner".to_string(),
                BTreeMap::from([("credit_limit".to_string(), RiskLevel::Critical)]),
            )]),
            ..RiskPolicy::default()
        });
        let operation = Operation::new(
            OperationKind::Update,
            "res.partner",
            OperationPayload::new(json!({"ids": [7], "vals": {"credit_limit": 1}})).unwrap(),
        );

        assert_eq!(evaluator.classify(&operation), RiskLevel::Critical);
    }

    #[test]
    fn financial_workflows_are_high_or_critical_risk() {
        let evaluator = RiskEvaluator::default();
        let post = Operation::new(
            OperationKind::Update,
            "account.move",
            OperationPayload::new(json!({"ids": [7], "vals": {"state": "posted"}})).unwrap(),
        )
        .classified(OperationClass::Financial);
        let remove = Operation::new(
            OperationKind::Delete,
            "account.move",
            OperationPayload::new(json!({"ids": [7]})).unwrap(),
        )
        .classified(OperationClass::Financial);

        assert_eq!(evaluator.classify(&post), RiskLevel::High);
        assert_eq!(evaluator.classify(&remove), RiskLevel::Critical);
    }

    #[test]
    fn admin_operations_are_always_critical_by_default() {
        let operation = operation(OperationKind::Create).classified(OperationClass::Admin);

        assert_eq!(
            RiskEvaluator::default().classify(&operation),
            RiskLevel::Critical
        );
    }

    #[test]
    fn assessments_explain_the_rule_that_won() {
        let evaluator = RiskEvaluator::new(RiskPolicy {
            fields: BTreeMap::from([(
                "res.partner".to_string(),
                BTreeMap::from([("credit_limit".to_string(), RiskLevel::Critical)]),
            )]),
            ..RiskPolicy::default()
        });
        let operation = Operation::new(
            OperationKind::Update,
            "res.partner",
            OperationPayload::new(json!({"ids": [7], "vals": {"credit_limit": 5000}})).unwrap(),
        );

        let assessment = evaluator.assess(&operation);

        assert_eq!(assessment.level, RiskLevel::Critical);
        assert_eq!(assessment.source, RiskSource::FieldOverride);
        assert_eq!(
            assessment.reason,
            "field override 'res.partner.credit_limit' sets critical risk"
        );
    }
}
