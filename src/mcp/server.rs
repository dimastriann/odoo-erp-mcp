use crate::config::Config;
use crate::error::AppError;
use crate::odoo::ClientManager;
use crate::tools::catalog::{ToolName, tool_definitions};
use crate::tools::executor::execute_tool;
use crate::tools::protection::QueryLimits;
use crate::tools::result::ToolExecutionResult;
use serde_json::{Value, json};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};

fn tool_call_response(id: Value, result: ToolExecutionResult) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result.into_mcp_result()
    })
}

pub async fn run_server(config: Arc<RwLock<Config>>, client_manager: ClientManager) {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut reader = BufReader::new(stdin).lines();

    while let Some(line) = reader.next_line().await.unwrap_or(None) {
        if line.trim().is_empty() {
            continue;
        }

        if let Ok(request) = serde_json::from_str::<Value>(&line) {
            let response = handle_request(request, &config, &client_manager).await;
            if let Some(resp) = response {
                let resp_str = serde_json::to_string(&resp).unwrap();
                let _ = stdout.write_all(format!("{}\n", resp_str).as_bytes()).await;
                let _ = stdout.flush().await;
            }
        } else {
            let error_resp = json!({
                "jsonrpc": "2.0",
                "error": { "code": -32700, "message": "Parse error" },
                "id": Value::Null
            });
            let _ = stdout
                .write_all(format!("{}\n", error_resp).as_bytes())
                .await;
            let _ = stdout.flush().await;
        }
    }
}

async fn handle_request(
    req: Value,
    config: &Arc<RwLock<Config>>,
    client_manager: &ClientManager,
) -> Option<Value> {
    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let id = req.get("id").cloned().unwrap_or(Value::Null);

    match method {
        "initialize" => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2025-11-25",
                "serverInfo": {
                    "name": "odoo-erp-mcp",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "capabilities": {
                    "tools": {}
                }
            }
        })),
        "notifications/initialized" => None,
        "tools/list" => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "tools": tool_definitions()
            }
        })),
        "tools/call" => {
            let params = req.get("params").cloned().unwrap_or(json!({}));
            let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
            let tool_name = match ToolName::try_from(name) {
                Ok(tool_name) => tool_name,
                Err(()) => {
                    return Some(tool_call_response(
                        id,
                        ToolExecutionResult::Failure(AppError::input_validation(format!(
                            "Error: Unknown tool {name}"
                        ))),
                    ));
                }
            };

            let instance_target = arguments
                .get("instance")
                .and_then(|i| i.as_str())
                .unwrap_or("");

            let (
                target_instance,
                global_mode,
                connection_timeout,
                request_timeout,
                max_response_bytes,
                query_limits,
            ) = {
                let conf = config.read().unwrap();
                let inst = conf.find_instance(instance_target).cloned();
                let mode = conf.global_settings.default_mode.clone();
                let connection_timeout =
                    Duration::from_secs(conf.global_settings.rpc_connection_timeout_secs);
                let request_timeout =
                    Duration::from_secs(conf.global_settings.rpc_request_timeout_secs);
                let max_response_bytes = conf.global_settings.rpc_max_response_bytes;
                let query_limits = QueryLimits {
                    max_query_limit: conf.global_settings.max_query_limit,
                    max_requested_fields: conf.global_settings.max_requested_fields,
                    max_read_ids: conf.global_settings.max_read_ids,
                    max_domain_depth: conf.global_settings.max_domain_depth,
                    max_domain_terms: conf.global_settings.max_domain_terms,
                };
                (
                    inst,
                    mode,
                    connection_timeout,
                    request_timeout,
                    max_response_bytes,
                    query_limits,
                )
            };

            let instance_obj = match target_instance {
                Some(i) => i,
                None => {
                    let error = if instance_target.is_empty() {
                        AppError::configuration(
                            "Error: No active Odoo instance configured or selected.",
                        )
                    } else {
                        AppError::configuration(format!(
                            "Error: Specified Odoo instance '{}' not found.",
                            instance_target
                        ))
                    };
                    return Some(tool_call_response(id, ToolExecutionResult::Failure(error)));
                }
            };

            // Enforce Instance Tool Permissions
            if !instance_obj.is_tool_allowed(tool_name.as_str(), &global_mode) {
                let mode_str = instance_obj.get_mode(&global_mode);
                let error = AppError::authorization(format!(
                    "Error: Tool '{}' is restricted for Odoo instance '{}' (Mode: '{}'). Permission denied.",
                    tool_name.as_str(),
                    instance_obj.name,
                    mode_str
                ));
                return Some(tool_call_response(id, ToolExecutionResult::Failure(error)));
            }

            // Get or create OdooClient dynamically
            let odoo_client = match client_manager
                .get_client(
                    &instance_obj,
                    connection_timeout,
                    request_timeout,
                    max_response_bytes,
                )
                .await
            {
                Ok(client) => client,
                Err(err) => {
                    return Some(tool_call_response(id, ToolExecutionResult::Failure(err)));
                }
            };

            let result = execute_tool(tool_name, arguments, &odoo_client, query_limits)
                .await
                .into_mcp_result();

            Some(json!({"jsonrpc": "2.0", "id": id, "result": result}))
        }
        _ => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32601, "message": "Method not found" }
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::multi_instance_config;

    #[tokio::test]
    async fn test_initialize_request() {
        let config = multi_instance_config();
        let client_manager = ClientManager::new();
        let req = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize"
        });

        let resp = handle_request(req, &config, &client_manager).await.unwrap();
        assert_eq!(resp["result"]["protocolVersion"], "2025-11-25");
        assert_eq!(resp["result"]["serverInfo"]["name"], "odoo-erp-mcp");
        assert_eq!(
            resp["result"]["serverInfo"]["version"],
            env!("CARGO_PKG_VERSION")
        );
    }

    #[tokio::test]
    async fn test_initialized_notification() {
        let config = multi_instance_config();
        let client_manager = ClientManager::new();
        let req = json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });

        let resp = handle_request(req, &config, &client_manager).await;
        assert!(resp.is_none());
    }

    #[tokio::test]
    async fn test_tools_list_request() {
        let config = multi_instance_config();
        let client_manager = ClientManager::new();
        let req = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list"
        });

        let resp = handle_request(req, &config, &client_manager).await.unwrap();
        let tools = resp["result"]["tools"].as_array().unwrap();
        let expected_contracts = [
            ("odoo-search-read", &["model", "domain", "fields"][..]),
            ("odoo-search-count", &["model", "domain"][..]),
            (
                "odoo-read-group",
                &["model", "domain", "fields", "groupby"][..],
            ),
            ("odoo-create", &["model", "vals"][..]),
            ("odoo-copy", &["model", "id", "vals"][..]),
            ("odoo-update", &["model", "ids", "vals"][..]),
            ("odoo-delete", &["model", "ids"][..]),
            ("odoo-get-metadata", &["model", "fields"][..]),
            ("odoo-search", &["model", "domain"][..]),
            ("odoo-read", &["model", "ids", "fields"][..]),
        ];

        assert_eq!(tools.len(), expected_contracts.len());
        for (name, required) in expected_contracts {
            let tool = tools
                .iter()
                .find(|tool| tool["name"] == name)
                .unwrap_or_else(|| panic!("missing tool contract: {name}"));

            assert!(
                tool["description"]
                    .as_str()
                    .is_some_and(|text| !text.is_empty())
            );
            assert_eq!(tool["inputSchema"]["type"], "object");
            assert!(tool["inputSchema"]["properties"].is_object());
            assert_eq!(tool["inputSchema"]["required"], json!(required));
        }
    }

    #[tokio::test]
    async fn test_tools_call_permission_denied_on_readonly() {
        let config = multi_instance_config();
        let client_manager = ClientManager::new();
        let req = json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "odoo-create",
                "arguments": {
                    "instance": "2",
                    "model": "res.partner",
                    "vals": { "name": "Test Partner" }
                }
            }
        });

        let resp = handle_request(req, &config, &client_manager).await.unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("Permission denied"));
        assert_eq!(resp["result"]["isError"], true);
        assert_eq!(
            resp["result"]["structuredContent"]["error"]["code"],
            "PERMISSION_DENIED"
        );
    }

    #[tokio::test]
    async fn test_tools_call_unknown_instance() {
        let config = multi_instance_config();
        let client_manager = ClientManager::new();
        let req = json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "odoo-search-read",
                "arguments": {
                    "instance": "nonexistent_instance",
                    "model": "res.partner"
                }
            }
        });

        let resp = handle_request(req, &config, &client_manager).await.unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("not found"));
        assert_eq!(resp["result"]["isError"], true);
        assert_eq!(
            resp["result"]["structuredContent"]["error"]["code"],
            "CONFIGURATION_ERROR"
        );
    }

    #[tokio::test]
    async fn test_unknown_tool_returns_structured_input_error() {
        let config = multi_instance_config();
        let client_manager = ClientManager::new();
        let req = json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "tools/call",
            "params": {
                "name": "odoo-unknown",
                "arguments": {}
            }
        });

        let resp = handle_request(req, &config, &client_manager).await.unwrap();

        assert_eq!(resp["result"]["isError"], true);
        assert_eq!(
            resp["result"]["structuredContent"]["error"]["code"],
            "INVALID_ARGUMENTS"
        );
    }

    #[tokio::test]
    async fn test_unknown_method() {
        let config = multi_instance_config();
        let client_manager = ClientManager::new();
        let req = json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "unknown/method"
        });

        let resp = handle_request(req, &config, &client_manager).await.unwrap();
        assert_eq!(resp["error"]["code"], -32601);
    }
}
