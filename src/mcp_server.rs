use crate::config::Config;
use crate::odoo_client::{ClientManager, OdooClient};
use crate::tool_arguments::{
    ModelFieldsArgs, ReadArgs, ReadGroupArgs, SearchDomainArgs, SearchReadArgs,
};
use crate::tool_catalog::{ToolName, tool_definitions};
use serde_json::{Value, json};
use std::sync::{Arc, RwLock};
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};

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
                    return Some(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [{ "type": "text", "text": format!("Error: Unknown tool {name}") }]
                        }
                    }));
                }
            };

            let instance_target = arguments
                .get("instance")
                .and_then(|i| i.as_str())
                .unwrap_or("");

            let (target_instance, global_mode) = {
                let conf = config.read().unwrap();
                let inst = conf.find_instance(instance_target).cloned();
                let mode = conf.global_settings.default_mode.clone();
                (inst, mode)
            };

            let instance_obj = match target_instance {
                Some(i) => i,
                None => {
                    let err_msg = if instance_target.is_empty() {
                        "Error: No active Odoo instance configured or selected.".to_string()
                    } else {
                        format!(
                            "Error: Specified Odoo instance '{}' not found.",
                            instance_target
                        )
                    };
                    return Some(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [{ "type": "text", "text": err_msg }]
                        }
                    }));
                }
            };

            // Enforce Instance Tool Permissions
            if !instance_obj.is_tool_allowed(tool_name.as_str(), &global_mode) {
                let mode_str = instance_obj.get_mode(&global_mode);
                let err_msg = format!(
                    "Error: Tool '{}' is restricted for Odoo instance '{}' (Mode: '{}'). Permission denied.",
                    tool_name.as_str(),
                    instance_obj.name,
                    mode_str
                );
                return Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": [{ "type": "text", "text": err_msg }]
                    }
                }));
            }

            // Get or create OdooClient dynamically
            let odoo_client = match client_manager.get_client(&instance_obj).await {
                Ok(client) => client,
                Err(err) => {
                    return Some(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [{ "type": "text", "text": format!("Error: {}", err) }]
                        }
                    }));
                }
            };

            let result = execute_tool(tool_name, arguments, &odoo_client).await;

            Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [
                        {
                            "type": "text",
                            "text": result
                        }
                    ]
                }
            }))
        }
        _ => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32601, "message": "Method not found" }
        })),
    }
}

async fn execute_tool(name: ToolName, arguments: Value, odoo: &OdooClient) -> String {
    let model = arguments
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("");
    if model.is_empty() {
        return "Error: Missing required parameter 'model'".to_string();
    }

    match name {
        ToolName::SearchRead => {
            let args: SearchReadArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return format!("Error: Invalid search-read arguments: {error}"),
            };
            match odoo
                .search_read(&args.model, args.domain, args.fields)
                .await
            {
                Ok(data) => serde_json::to_string_pretty(&data)
                    .unwrap_or_else(|e| format!("Error parsing result: {}", e)),
                Err(e) => format!("Error executing search_read: {}", e),
            }
        }
        ToolName::SearchCount => {
            let args: SearchDomainArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return format!("Error: Invalid search-count arguments: {error}"),
            };
            match odoo.search_count(&args.model, args.domain).await {
                Ok(data) => serde_json::to_string_pretty(&data)
                    .unwrap_or_else(|e| format!("Error parsing result: {}", e)),
                Err(e) => format!("Error executing search_count: {}", e),
            }
        }
        ToolName::ReadGroup => {
            let args: ReadGroupArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return format!("Error: Invalid read-group arguments: {error}"),
            };
            match odoo
                .read_group(&args.model, args.domain, args.fields, args.groupby)
                .await
            {
                Ok(data) => serde_json::to_string_pretty(&data)
                    .unwrap_or_else(|e| format!("Error parsing result: {}", e)),
                Err(e) => format!("Error executing read_group: {}", e),
            }
        }
        ToolName::Create => {
            let vals = arguments.get("vals").cloned().unwrap_or(json!({}));
            match odoo.create(model, vals).await {
                Ok(data) => serde_json::to_string_pretty(&data)
                    .unwrap_or_else(|e| format!("Error parsing result: {}", e)),
                Err(e) => format!("Error executing create: {}", e),
            }
        }
        ToolName::Copy => {
            let id = arguments.get("id").and_then(|i| i.as_i64()).unwrap_or(0);
            let vals = arguments.get("vals").cloned().unwrap_or(json!({}));
            match odoo.copy(model, id, vals).await {
                Ok(data) => serde_json::to_string_pretty(&data)
                    .unwrap_or_else(|e| format!("Error parsing result: {}", e)),
                Err(e) => format!("Error executing copy: {}", e),
            }
        }
        ToolName::Update => {
            let ids_arr = arguments.get("ids").and_then(|ids| ids.as_array());
            let vals = arguments.get("vals").cloned().unwrap_or(json!({}));

            if let Some(arr) = ids_arr {
                let ids: Vec<i64> = arr.iter().filter_map(|v| v.as_i64()).collect();
                match odoo.update(model, ids, vals).await {
                    Ok(data) => serde_json::to_string_pretty(&data)
                        .unwrap_or_else(|e| format!("Error parsing result: {}", e)),
                    Err(e) => format!("Error executing update: {}", e),
                }
            } else {
                "Error: Missing required parameter 'ids' (array of ints)".to_string()
            }
        }
        ToolName::Delete => {
            let ids_arr = arguments.get("ids").and_then(|ids| ids.as_array());

            if let Some(arr) = ids_arr {
                let ids: Vec<i64> = arr.iter().filter_map(|v| v.as_i64()).collect();
                match odoo.delete(model, ids).await {
                    Ok(data) => serde_json::to_string_pretty(&data)
                        .unwrap_or_else(|e| format!("Error parsing result: {}", e)),
                    Err(e) => format!("Error executing delete: {}", e),
                }
            } else {
                "Error: Missing required parameter 'ids' (array of ints)".to_string()
            }
        }
        ToolName::GetMetadata => {
            let args: ModelFieldsArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return format!("Error: Invalid metadata arguments: {error}"),
            };
            match odoo.get_metadata(&args.model, args.fields).await {
                Ok(data) => serde_json::to_string_pretty(&data)
                    .unwrap_or_else(|e| format!("Error parsing result: {}", e)),
                Err(e) => format!("Error executing get_metadata: {}", e),
            }
        }
        ToolName::Search => {
            let args: SearchDomainArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return format!("Error: Invalid search arguments: {error}"),
            };
            match odoo.search(&args.model, args.domain).await {
                Ok(data) => serde_json::to_string_pretty(&data)
                    .unwrap_or_else(|e| format!("Error parsing result: {}", e)),
                Err(e) => format!("Error executing search: {}", e),
            }
        }
        ToolName::Read => {
            let args: ReadArgs = match serde_json::from_value(arguments) {
                Ok(args) => args,
                Err(error) => return format!("Error: Invalid read arguments: {error}"),
            };
            match odoo.read(&args.model, args.ids, args.fields).await {
                Ok(data) => serde_json::to_string_pretty(&data)
                    .unwrap_or_else(|e| format!("Error parsing result: {}", e)),
                Err(e) => format!("Error executing read: {}", e),
            }
        }
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
            ("odoo-get-metadata", &["model"][..]),
            ("odoo-search", &["model", "domain"][..]),
            ("odoo-read", &["model", "ids"][..]),
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
