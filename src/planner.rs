use crate::analytics::{parse_groupby, parse_metric};
use crate::domain::{DomainLimits, validate_domain_security};
use crate::error::AppError;
use crate::intent::{IntentOperation, QueryIntent};
use crate::metadata;
use crate::odoo::OdooClient;
use crate::tools::protection::QueryLimits;
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(crate) async fn plan(
    odoo: &OdooClient,
    intent: &QueryIntent,
    limits: QueryLimits,
) -> Result<Value, AppError> {
    intent.validate().map_err(AppError::input_validation)?;
    if intent.fields.len() > limits.max_requested_fields {
        return Err(AppError::input_validation(format!(
            "intent requests {} fields, exceeding the configured maximum of {}",
            intent.fields.len(),
            limits.max_requested_fields
        )));
    }
    if intent.group_by.len() > 3 {
        return Err(AppError::input_validation(
            "intent group_by supports at most 3 fields",
        ));
    }
    let domain = intent.domain();
    validate_domain_security(
        &domain,
        DomainLimits {
            max_depth: limits.max_domain_depth,
            max_terms: limits.max_domain_terms,
            max_in_values: limits.max_domain_in_values,
        },
    )
    .map_err(AppError::input_validation)?;

    let mut inspected_models = BTreeSet::new();
    metadata::schema(
        odoo,
        &intent.model,
        limits.metadata_cache_ttl_secs,
        limits.max_metadata_fields,
        false,
    )
    .await?;
    inspected_models.insert(intent.model.clone());

    let mut filters = Vec::with_capacity(intent.filters.len());
    for filter in &intent.filters {
        let field = resolve_field(
            odoo,
            &intent.model,
            &filter.field,
            limits,
            &mut inspected_models,
        )
        .await?;
        filters.push(json!({
            "field": filter.field,
            "operator": filter.operator,
            "value": filter.value,
            "resolved": field
        }));
    }

    let mut fields = Vec::with_capacity(intent.fields.len());
    for field in &intent.fields {
        fields
            .push(resolve_field(odoo, &intent.model, field, limits, &mut inspected_models).await?);
    }

    let mut group_by = Vec::with_capacity(intent.group_by.len());
    for field in &intent.group_by {
        let groupby = parse_groupby(field)?;
        group_by.push(json!({
            "field": groupby.field,
            "granularity": groupby.granularity,
            "resolved": resolve_field(
                odoo,
                &intent.model,
                &groupby.field,
                limits,
                &mut inspected_models,
            )
            .await?
        }));
    }

    let mut metrics = Vec::with_capacity(intent.metrics.len());
    for raw in &intent.metrics {
        let metric = parse_metric(raw)?;
        metrics.push(json!({
            "field": metric.field,
            "function": metric.function.as_str(),
            "resolved": resolve_field(
                odoo,
                &intent.model,
                &metric.field,
                limits,
                &mut inspected_models,
            )
            .await?
        }));
    }

    let limit = intent
        .limit
        .unwrap_or(100)
        .min(limits.max_query_limit)
        .max(1);
    let cost_level = if limit <= 100 && intent.filters.len() <= 2 {
        "low"
    } else if limit <= 500 {
        "medium"
    } else {
        "high"
    };
    let operation = match intent.operation {
        IntentOperation::SearchRead => "search_read",
        IntentOperation::Count => "count",
        IntentOperation::Aggregate => "aggregate",
    };

    Ok(json!({
        "read_only": true,
        "model": intent.model,
        "operation": operation,
        "domain": domain,
        "fields": fields,
        "filters": filters,
        "group_by": group_by,
        "metrics": metrics,
        "order": intent.order,
        "limit": limit,
        "explanation": intent.description(),
        "cost": {
            "level": cost_level,
            "filter_terms": intent.filters.len(),
            "estimated_rows": limit,
            "metadata_models": inspected_models.into_iter().collect::<Vec<_>>()
        }
    }))
}

async fn resolve_field(
    odoo: &OdooClient,
    model: &str,
    path: &str,
    limits: QueryLimits,
    inspected_models: &mut BTreeSet<String>,
) -> Result<Value, AppError> {
    let mut current_model = model.to_string();
    let segments = path.split('.').collect::<Vec<_>>();
    if segments.len() > limits.max_domain_depth {
        return Err(AppError::input_validation(format!(
            "field path {path:?} exceeds the configured relationship depth of {}",
            limits.max_domain_depth
        )));
    }
    for (index, segment) in segments.iter().enumerate() {
        let schema = metadata::schema(
            odoo,
            &current_model,
            limits.metadata_cache_ttl_secs,
            limits.max_metadata_fields,
            false,
        )
        .await?;
        inspected_models.insert(current_model.clone());
        let field = schema.fields.get(*segment).ok_or_else(|| {
            AppError::input_validation(format!("unknown field {path:?} on model {current_model:?}"))
        })?;
        if index + 1 == segments.len() {
            return Ok(json!({
                "path": path,
                "model": current_model,
                "name": field.name,
                "type": field.field_type,
                "relation": field.relation,
                "readonly": field.readonly
            }));
        }
        current_model = field.relation.clone().ok_or_else(|| {
            AppError::input_validation(format!(
                "field {segment:?} on model {current_model:?} is not relational"
            ))
        })?;
    }
    Err(AppError::input_validation(format!(
        "invalid field path {path:?}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::{IntentFilter, IntentOperation};
    use crate::test_support::{MockOdooServer, authentication_success, json_rpc_success};

    fn limits() -> QueryLimits {
        QueryLimits {
            max_query_limit: 1_000,
            max_requested_fields: 100,
            max_read_ids: 100,
            max_domain_depth: 8,
            max_domain_terms: 100,
            max_response_records: 1_000,
            max_domain_in_values: 1_000,
            max_context_keys: 32,
            max_context_bytes: 8_192,
            max_batch_size: 100,
            max_report_bytes: 10_485_760,
            metadata_cache_ttl_secs: 300,
            max_metadata_fields: 500,
        }
    }

    async fn client() -> (MockOdooServer, OdooClient) {
        let server = MockOdooServer::start_with_responses(vec![
            authentication_success(7),
            json_rpc_success(json!({
                "id": {"type": "integer"},
                "name": {"type": "char", "string": "Name"},
                "partner_id": {"type": "many2one", "relation": "res.partner"},
                "amount_total": {"type": "monetary"}
            })),
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
        (server, client)
    }

    fn search_intent(field: &str) -> QueryIntent {
        QueryIntent {
            model: "res.partner".to_string(),
            operation: IntentOperation::SearchRead,
            filters: vec![IntentFilter {
                field: field.to_string(),
                operator: "ilike".to_string(),
                value: json!("Acme"),
            }],
            fields: vec!["id".to_string(), "name".to_string()],
            group_by: Vec::new(),
            metrics: Vec::new(),
            order: Some("name asc".to_string()),
            limit: Some(10),
        }
    }

    #[tokio::test]
    async fn plan_resolves_models_fields_and_cost() {
        let (_server, client) = client().await;
        let plan = plan(&client, &search_intent("name"), limits())
            .await
            .unwrap();
        assert_eq!(plan["read_only"], true);
        assert_eq!(plan["model"], "res.partner");
        assert_eq!(plan["operation"], "search_read");
        assert_eq!(plan["fields"][1]["name"], "name");
        assert_eq!(plan["cost"]["level"], "low");
    }

    #[tokio::test]
    async fn plan_rejects_unknown_fields_before_execution() {
        let (_server, client) = client().await;
        let error = plan(&client, &search_intent("missing_field"), limits())
            .await
            .unwrap_err();
        assert!(matches!(error, AppError::InputValidation { .. }));
    }
}
