use crate::domain::validate_domain;
use crate::odoo::OdooClient;
use crate::tools::arguments::{
    CopyArgs, CreateArgs, DeleteArgs, ModelFieldsArgs, ReadArgs, ReadGroupArgs, SearchArgs,
    SearchDomainArgs, SearchReadArgs, UpdateArgs,
};
use crate::tools::catalog::ToolName;
use crate::tools::pagination::{add_total_count, fetch_limit, paginated_result, resolve_limit};
use crate::tools::result::ToolExecutionResult;
use serde_json::Value;

pub(crate) async fn execute_tool(
    name: ToolName,
    arguments: Value,
    odoo: &OdooClient,
    max_query_limit: u64,
) -> ToolExecutionResult {
    let model = arguments
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("");
    if model.is_empty() {
        return ToolExecutionResult::Failure(AppError::input_validation(
            "Error: Missing required parameter 'model'",
        ));
    }

    match name {
        ToolName::SearchRead => {
            let args: SearchReadArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return ToolExecutionResult::invalid_arguments("search-read", error),
            };
            if let Err(error) = validate_domain(&args.domain) {
                return ToolExecutionResult::invalid_arguments("search-read domain", error);
            }
            let limit = match resolve_limit(args.limit, max_query_limit) {
                Ok(limit) => limit,
                Err(error) => {
                    return ToolExecutionResult::invalid_arguments("search-read limit", error);
                }
            };
            let count_domain = args.include_total.then(|| args.domain.clone());
            let result = odoo
                .search_read(
                    &args.model,
                    args.domain,
                    args.fields,
                    args.offset,
                    fetch_limit(limit),
                )
                .await;
            let mut page = paginated_result(result, args.offset, limit);
            if page.is_ok()
                && let Some(domain) = count_domain
            {
                page = add_total_count(page, odoo.search_count(&args.model, domain).await);
            }
            ToolExecutionResult::from_app_error(page)
        }
        ToolName::SearchCount => {
            let args: SearchDomainArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => {
                    return ToolExecutionResult::invalid_arguments("search-count", error);
                }
            };
            if let Err(error) = validate_domain(&args.domain) {
                return ToolExecutionResult::invalid_arguments("search-count domain", error);
            }
            ToolExecutionResult::from_app_error(odoo.search_count(&args.model, args.domain).await)
        }
        ToolName::ReadGroup => {
            let args: ReadGroupArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return ToolExecutionResult::invalid_arguments("read-group", error),
            };
            if let Err(error) = validate_domain(&args.domain) {
                return ToolExecutionResult::invalid_arguments("read-group domain", error);
            }
            ToolExecutionResult::from_app_error(
                odoo.read_group(&args.model, args.domain, args.fields, args.groupby)
                    .await,
            )
        }
        ToolName::Create => {
            let args: CreateArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return ToolExecutionResult::invalid_arguments("create", error),
            };
            ToolExecutionResult::from_app_error(
                odoo.create(&args.model, Value::Object(args.vals)).await,
            )
        }
        ToolName::Copy => {
            let args: CopyArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return ToolExecutionResult::invalid_arguments("copy", error),
            };
            ToolExecutionResult::from_app_error(
                odoo.copy(&args.model, args.id, Value::Object(args.vals))
                    .await,
            )
        }
        ToolName::Update => {
            let args: UpdateArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return ToolExecutionResult::invalid_arguments("update", error),
            };
            ToolExecutionResult::from_app_error(
                odoo.update(&args.model, args.ids, Value::Object(args.vals))
                    .await,
            )
        }
        ToolName::Delete => {
            let args: DeleteArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return ToolExecutionResult::invalid_arguments("delete", error),
            };
            ToolExecutionResult::from_app_error(odoo.delete(&args.model, args.ids).await)
        }
        ToolName::GetMetadata => {
            let args: ModelFieldsArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return ToolExecutionResult::invalid_arguments("metadata", error),
            };
            ToolExecutionResult::from_app_error(odoo.get_metadata(&args.model, args.fields).await)
        }
        ToolName::Search => {
            let args: SearchArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return ToolExecutionResult::invalid_arguments("search", error),
            };
            if let Err(error) = validate_domain(&args.domain) {
                return ToolExecutionResult::invalid_arguments("search domain", error);
            }
            let limit = match resolve_limit(args.limit, max_query_limit) {
                Ok(limit) => limit,
                Err(error) => {
                    return ToolExecutionResult::invalid_arguments("search limit", error);
                }
            };
            let count_domain = args.include_total.then(|| args.domain.clone());
            let result = odoo
                .search(&args.model, args.domain, args.offset, fetch_limit(limit))
                .await;
            let mut page = paginated_result(result, args.offset, limit);
            if page.is_ok()
                && let Some(domain) = count_domain
            {
                page = add_total_count(page, odoo.search_count(&args.model, domain).await);
            }
            ToolExecutionResult::from_app_error(page)
        }
        ToolName::Read => {
            let args: ReadArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return ToolExecutionResult::invalid_arguments("read", error),
            };
            ToolExecutionResult::from_app_error(odoo.read(&args.model, args.ids, args.fields).await)
        }
    }
}
use crate::error::AppError;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{MockOdooServer, authentication_success, json_rpc_success};
    use serde_json::json;

    #[tokio::test]
    async fn search_pagination_contract_includes_has_more_without_count() {
        let server = MockOdooServer::start_with_responses(vec![
            authentication_success(7),
            json_rpc_success(json!([10, 11, 12])),
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

        let result = execute_tool(
            ToolName::Search,
            json!({"model": "res.partner", "limit": 2}),
            &client,
            1_000,
        )
        .await;
        let ToolExecutionResult::Success(response) = result else {
            panic!("search should return a paginated response");
        };

        assert_eq!(response["items"], json!([10, 11]));
        assert_eq!(response["pagination"]["has_more"], true);
        assert!(response["pagination"].get("total").is_none());
        let requests = server.requests().await;
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[1]["params"]["args"][6]["limit"], 3);
    }

    #[tokio::test]
    async fn total_count_is_opt_in_and_uses_the_same_domain() {
        let domain = json!([["active", "=", true]]);
        let server = MockOdooServer::start_with_responses(vec![
            authentication_success(7),
            json_rpc_success(json!([10])),
            json_rpc_success(json!(25)),
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

        let result = execute_tool(
            ToolName::Search,
            json!({
                "model": "res.partner",
                "domain": domain,
                "include_total": true
            }),
            &client,
            1_000,
        )
        .await;
        let ToolExecutionResult::Success(response) = result else {
            panic!("search should return a paginated response");
        };

        assert_eq!(response["pagination"]["total"], 25);
        let requests = server.requests().await;
        assert_eq!(requests.len(), 3);
        assert_eq!(requests[1]["params"]["args"][5][0], domain);
        assert_eq!(requests[2]["params"]["args"][5][0], domain);
    }
}
