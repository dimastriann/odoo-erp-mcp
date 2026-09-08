use crate::config::Config;
use crate::context::{
    ActorIdentity, AgentIdentity, ClientIdentity, IdentitySource, RequestContext,
};
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

fn request_context(client: &ClientIdentity, params: &Value, instance: String) -> RequestContext {
    let metadata = params.get("_meta");
    let agent = metadata
        .and_then(|meta| meta.get("agent"))
        .map(|identity| {
            AgentIdentity::claimed(
                identity
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                identity
                    .get("version")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                IdentitySource::RequestMetadata,
            )
        })
        .unwrap_or_default();
    let actor = metadata
        .and_then(|meta| meta.get("actor"))
        .map(|identity| {
            ActorIdentity::claimed(
                identity
                    .get("subject")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                identity
                    .get("displayName")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                IdentitySource::RequestMetadata,
            )
        })
        .unwrap_or_default();

    RequestContext::identified(client.clone(), agent, actor, instance)
}

pub async fn run_server(config: Arc<RwLock<Config>>, client_manager: ClientManager) {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut reader = BufReader::new(stdin).lines();
    let mut client_identity = ClientIdentity::default();

    while let Some(line) = reader.next_line().await.unwrap_or(None) {
        if line.trim().is_empty() {
            continue;
        }

        if let Ok(request) = serde_json::from_str::<Value>(&line) {
            let response = handle_request_with_identity(
                request,
                &config,
                &client_manager,
                &mut client_identity,
            )
            .await;
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

async fn handle_request_with_identity(
    req: Value,
    config: &Arc<RwLock<Config>>,
    client_manager: &ClientManager,
    client_identity: &mut ClientIdentity,
) -> Option<Value> {
    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let id = req.get("id").cloned().unwrap_or(Value::Null);

    match method {
        "initialize" => {
            let client_info = req
                .get("params")
                .and_then(|params| params.get("clientInfo"));
            *client_identity = client_info
                .map(|info| {
                    ClientIdentity::claimed(
                        info.get("name").and_then(Value::as_str).map(str::to_owned),
                        info.get("version")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        IdentitySource::McpInitialize,
                    )
                })
                .unwrap_or_default();
            Some(json!({
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
            }))
        }
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
                global_settings,
            ) = {
                let conf = config.read().unwrap();
                let inst = conf.find_instance(instance_target).cloned();
                let mode = conf.global_settings.default_mode.clone();
                let connection_timeout =
                    Duration::from_secs(conf.global_settings.rpc_connection_timeout_secs);
                let request_timeout =
                    Duration::from_secs(conf.global_settings.rpc_request_timeout_secs);
                let max_response_bytes = conf.global_settings.rpc_max_response_bytes;
                let global_settings = conf.global_settings.clone();
                (
                    inst,
                    mode,
                    connection_timeout,
                    request_timeout,
                    max_response_bytes,
                    global_settings,
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
            let effective_settings = instance_obj.effective_query_settings(&global_settings);
            let query_limits = QueryLimits {
                max_query_limit: effective_settings.max_query_limit,
                max_requested_fields: effective_settings.max_requested_fields,
                max_read_ids: effective_settings.max_read_ids,
                max_domain_depth: effective_settings.max_domain_depth,
                max_domain_terms: effective_settings.max_domain_terms,
                max_response_records: effective_settings.max_response_records,
                max_domain_in_values: effective_settings.max_domain_in_values,
            };

            // Enforce Instance Tool Permissions
            let model = arguments
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let requested_fields = requested_fields(&arguments);
            let method = arguments.get("method").and_then(Value::as_str);
            let policy_evaluation = instance_obj.policy_evaluation_for_request(
                tool_name,
                model,
                method,
                &requested_fields,
                &global_mode,
            );
            if policy_evaluation.decision.is_denied() {
                let mode_str = instance_obj.get_mode(&global_mode);
                let error = AppError::authorization(format!(
                    "Error: Tool '{}' requires '{}' capability, which is restricted for Odoo instance '{}' (Mode: '{}'). Permission denied: {}.",
                    tool_name.as_str(),
                    tool_name.capability(),
                    instance_obj.name,
                    mode_str,
                    policy_evaluation.explanation
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

            let request_context =
                request_context(client_identity, &params, instance_obj.name.clone());
            let result = execute_tool(
                tool_name,
                arguments,
                &request_context,
                &odoo_client,
                query_limits,
            )
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

fn requested_fields(arguments: &Value) -> Vec<String> {
    let mut fields = arguments
        .get("fields")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>();
    if let Some(values) = arguments.get("vals").and_then(Value::as_object) {
        fields.extend(values.keys().cloned());
    }
    fields
}

#[cfg(test)]
async fn handle_request(
    req: Value,
    config: &Arc<RwLock<Config>>,
    client_manager: &ClientManager,
) -> Option<Value> {
    handle_request_with_identity(req, config, client_manager, &mut ClientIdentity::default()).await
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
    async fn initialize_captures_untrusted_client_identity_for_the_session() {
        let config = multi_instance_config();
        let client_manager = ClientManager::new();
        let mut client = ClientIdentity::default();
        let req = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "clientInfo": {"name": "codex", "version": "1.2.3"}
            }
        });

        handle_request_with_identity(req, &config, &client_manager, &mut client).await;

        assert_eq!(client.name.as_deref(), Some("codex"));
        assert_eq!(client.version.as_deref(), Some("1.2.3"));
        assert_eq!(client.source, IdentitySource::McpInitialize);
        assert_eq!(
            client.source.trust(),
            crate::context::IdentityTrust::Untrusted
        );
    }

    #[test]
    fn tool_request_context_propagates_all_identity_dimensions() {
        let client = ClientIdentity::claimed(
            Some("desktop-host".to_string()),
            Some("2.0".to_string()),
            IdentitySource::McpInitialize,
        );
        let params = json!({
            "_meta": {
                "agent": {"name": "finance-agent", "version": "4.1"},
                "actor": {"subject": "user-42", "displayName": "Ada"}
            }
        });

        let context = request_context(&client, &params, "production".to_string());

        assert_eq!(context.client, client);
        assert_eq!(context.agent.name.as_deref(), Some("finance-agent"));
        assert_eq!(context.agent.version.as_deref(), Some("4.1"));
        assert_eq!(context.agent.source, IdentitySource::RequestMetadata);
        assert_eq!(context.actor.subject.as_deref(), Some("user-42"));
        assert_eq!(context.actor.display_name.as_deref(), Some("Ada"));
        assert_eq!(context.actor.source, IdentitySource::RequestMetadata);
        assert_eq!(context.instance, "production");
    }

    #[test]
    fn absent_request_claims_remain_explicitly_unavailable() {
        let context = request_context(
            &ClientIdentity::default(),
            &json!({}),
            "sandbox".to_string(),
        );

        assert_eq!(context.client.source, IdentitySource::Unavailable);
        assert_eq!(context.agent.source, IdentitySource::Unavailable);
        assert_eq!(context.actor.source, IdentitySource::Unavailable);
        assert_eq!(context.instance, "sandbox");
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
