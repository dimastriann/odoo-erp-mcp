use crate::domain::validate_domain;
use crate::odoo_client::OdooClient;
use crate::tool_arguments::{
    CopyArgs, CreateArgs, DeleteArgs, ModelFieldsArgs, ReadArgs, ReadGroupArgs, SearchDomainArgs,
    SearchReadArgs, UpdateArgs,
};
use crate::tool_catalog::ToolName;
use crate::tool_result::ToolExecutionResult;
use serde_json::Value;

pub(crate) async fn execute_tool(
    name: ToolName,
    arguments: Value,
    odoo: &OdooClient,
) -> ToolExecutionResult {
    let model = arguments
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("");
    if model.is_empty() {
        return ToolExecutionResult::Failure(AppError::internal(
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
            ToolExecutionResult::from_rpc(
                "search_read",
                odoo.search_read(&args.model, args.domain, args.fields)
                    .await,
            )
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
            ToolExecutionResult::from_rpc(
                "search_count",
                odoo.search_count(&args.model, args.domain).await,
            )
        }
        ToolName::ReadGroup => {
            let args: ReadGroupArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return ToolExecutionResult::invalid_arguments("read-group", error),
            };
            if let Err(error) = validate_domain(&args.domain) {
                return ToolExecutionResult::invalid_arguments("read-group domain", error);
            }
            ToolExecutionResult::from_rpc(
                "read_group",
                odoo.read_group(&args.model, args.domain, args.fields, args.groupby)
                    .await,
            )
        }
        ToolName::Create => {
            let args: CreateArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return ToolExecutionResult::invalid_arguments("create", error),
            };
            ToolExecutionResult::from_rpc(
                "create",
                odoo.create(&args.model, Value::Object(args.vals)).await,
            )
        }
        ToolName::Copy => {
            let args: CopyArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return ToolExecutionResult::invalid_arguments("copy", error),
            };
            ToolExecutionResult::from_rpc(
                "copy",
                odoo.copy(&args.model, args.id, Value::Object(args.vals))
                    .await,
            )
        }
        ToolName::Update => {
            let args: UpdateArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return ToolExecutionResult::invalid_arguments("update", error),
            };
            ToolExecutionResult::from_rpc(
                "update",
                odoo.update(&args.model, args.ids, Value::Object(args.vals))
                    .await,
            )
        }
        ToolName::Delete => {
            let args: DeleteArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return ToolExecutionResult::invalid_arguments("delete", error),
            };
            ToolExecutionResult::from_rpc("delete", odoo.delete(&args.model, args.ids).await)
        }
        ToolName::GetMetadata => {
            let args: ModelFieldsArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return ToolExecutionResult::invalid_arguments("metadata", error),
            };
            ToolExecutionResult::from_rpc(
                "get_metadata",
                odoo.get_metadata(&args.model, args.fields).await,
            )
        }
        ToolName::Search => {
            let args: SearchDomainArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return ToolExecutionResult::invalid_arguments("search", error),
            };
            if let Err(error) = validate_domain(&args.domain) {
                return ToolExecutionResult::invalid_arguments("search domain", error);
            }
            ToolExecutionResult::from_rpc("search", odoo.search(&args.model, args.domain).await)
        }
        ToolName::Read => {
            let args: ReadArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return ToolExecutionResult::invalid_arguments("read", error),
            };
            ToolExecutionResult::from_rpc(
                "read",
                odoo.read(&args.model, args.ids, args.fields).await,
            )
        }
    }
}
use crate::error::AppError;
