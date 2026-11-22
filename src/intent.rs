use crate::analytics::{parse_groupby, parse_metric};
use crate::domain::is_supported_operator;
use crate::tools::fields::is_valid_field_path;
use crate::tools::protection::validate_order;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum IntentOperation {
    #[default]
    SearchRead,
    Count,
    Aggregate,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct IntentFilter {
    pub(crate) field: String,
    pub(crate) operator: String,
    pub(crate) value: Value,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct QueryIntent {
    pub(crate) model: String,
    #[serde(default)]
    pub(crate) operation: IntentOperation,
    #[serde(default)]
    pub(crate) filters: Vec<IntentFilter>,
    #[serde(default)]
    pub(crate) fields: Vec<String>,
    #[serde(default)]
    pub(crate) group_by: Vec<String>,
    #[serde(default)]
    pub(crate) metrics: Vec<String>,
    #[serde(default)]
    pub(crate) order: Option<String>,
    #[serde(default)]
    pub(crate) limit: Option<u64>,
}

impl QueryIntent {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if !is_valid_field_path(&self.model) {
            return Err(format!("invalid model name {:?}", self.model));
        }
        for field in &self.fields {
            if !is_valid_field_path(field) {
                return Err(format!("invalid field name {field:?}"));
            }
        }
        for filter in &self.filters {
            if !is_valid_field_path(&filter.field) {
                return Err(format!("invalid filter field {:?}", filter.field));
            }
            if !is_supported_operator(&filter.operator) {
                return Err(format!("unsupported filter operator {:?}", filter.operator));
            }
        }
        for field in &self.group_by {
            parse_groupby(field).map_err(|error| error.to_string())?;
        }
        for metric in &self.metrics {
            parse_metric(metric).map_err(|error| error.to_string())?;
        }
        if let Some(order) = self.order.as_deref() {
            validate_order(order)?;
        }
        if self.limit == Some(0) {
            return Err("limit must be greater than zero".to_string());
        }
        self.validate_operation()?;
        Ok(())
    }

    fn validate_operation(&self) -> Result<(), String> {
        match self.operation {
            IntentOperation::SearchRead => {
                if self.fields.is_empty() {
                    return Err("search_read requires at least one field".to_string());
                }
                if !self.metrics.is_empty() || !self.group_by.is_empty() {
                    return Err(
                        "search_read cannot use metrics or group_by; use aggregate".to_string()
                    );
                }
            }
            IntentOperation::Count => {
                if !self.fields.is_empty() || !self.metrics.is_empty() || !self.group_by.is_empty()
                {
                    return Err("count cannot use fields, metrics, or group_by".to_string());
                }
            }
            IntentOperation::Aggregate => {
                if self.metrics.is_empty() {
                    return Err("aggregate requires at least one metric".to_string());
                }
                if !self.fields.is_empty() {
                    return Err("aggregate uses metrics, not fields".to_string());
                }
            }
        }
        Ok(())
    }

    pub(crate) fn domain(&self) -> Value {
        Value::Array(
            self.filters
                .iter()
                .map(|filter| json!([filter.field, filter.operator, filter.value]))
                .collect(),
        )
    }

    pub(crate) fn description(&self) -> String {
        let mut parts = vec![format!("{:?} {}", self.operation, self.model)];
        if !self.filters.is_empty() {
            let filters = self
                .filters
                .iter()
                .map(|filter| format!("{} {} {}", filter.field, filter.operator, filter.value))
                .collect::<Vec<_>>()
                .join(" and ");
            parts.push(format!("where {filters}"));
        }
        if !self.fields.is_empty() {
            parts.push(format!("fields {}", self.fields.join(", ")));
        }
        if !self.metrics.is_empty() {
            parts.push(format!("metrics {}", self.metrics.join(", ")));
        }
        if !self.group_by.is_empty() {
            parts.push(format!("grouped by {}", self.group_by.join(", ")));
        }
        if let Some(order) = &self.order {
            parts.push(format!("ordered by {order}"));
        }
        if let Some(limit) = self.limit {
            parts.push(format!("limited to {limit}"));
        }
        parts.join("; ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_intent() -> QueryIntent {
        QueryIntent {
            model: "res.partner".to_string(),
            operation: IntentOperation::SearchRead,
            filters: vec![IntentFilter {
                field: "name".to_string(),
                operator: "ilike".to_string(),
                value: json!("Acme"),
            }],
            fields: vec!["id".to_string(), "name".to_string()],
            group_by: Vec::new(),
            metrics: Vec::new(),
            order: Some("name asc".to_string()),
            limit: Some(25),
        }
    }

    #[test]
    fn typed_intent_validates_and_builds_a_domain() {
        let intent = valid_intent();
        assert!(intent.validate().is_ok());
        assert_eq!(intent.domain(), json!([["name", "ilike", "Acme"]]));
        assert!(intent.description().contains("res.partner"));
    }

    #[derive(Deserialize)]
    struct EvalCase {
        utterance: String,
        intent: QueryIntent,
        expected_operation: IntentOperation,
        expected_domain: Value,
    }

    #[test]
    fn evaluation_dataset_covers_common_queries() {
        let cases: Vec<EvalCase> = serde_json::from_str(include_str!("intent_eval.json")).unwrap();
        assert!(cases.len() >= 4);
        for case in cases {
            assert!(!case.utterance.trim().is_empty());
            assert!(case.intent.validate().is_ok(), "{}", case.utterance);
            assert_eq!(case.intent.operation, case.expected_operation);
            assert_eq!(case.intent.domain(), case.expected_domain);
        }
    }

    #[test]
    fn typed_intent_rejects_unsafe_or_conflicting_shapes() {
        let mut intent = valid_intent();
        intent.filters[0].operator = "drop".to_string();
        assert!(intent.validate().is_err());

        let mut intent = valid_intent();
        intent.metrics = vec!["amount_total:sum".to_string()];
        assert!(intent.validate().is_err());

        let mut intent = valid_intent();
        intent.operation = IntentOperation::Aggregate;
        intent.fields.clear();
        intent.metrics = vec!["amount_total:sum".to_string()];
        intent.group_by = vec!["date_order:month".to_string()];
        assert!(intent.validate().is_ok());
    }
}
