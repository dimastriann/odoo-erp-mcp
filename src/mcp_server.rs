use crate::config::Config;
use crate::odoo_client::{ClientManager, OdooClient};
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
                "tools": [
                    {
                        "name": "odoo-search-read",
                        "description": "Search and retrieve records from Odoo.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "instance": { "type": "string", "description": "Target Odoo instance ID or Name (optional, defaults to active instance)" },
                                "model": { "type": "string", "description": "The Odoo model name (e.g., res.partner)" },
                                "domain": { "type": "array", "description": "Search domain (e.g., [['is_company', '=', true]])" },
                                "fields": { "type": "array", "items": { "type": "string" }, "description": "List of fields to return" }
                            },
                            "required": ["model", "domain", "fields"]
                        }
                    },
                    {
                        "name": "odoo-search-count",
                        "description": "Get the count of records matching a domain.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "instance": { "type": "string", "description": "Target Odoo instance ID or Name (optional, defaults to active instance)" },
                                "model": { "type": "string" },
                                "domain": { "type": "array" }
                            },
                            "required": ["model", "domain"]
                        }
                    },
                    {
                        "name": "odoo-read-group",
                        "description": "Get aggregated data from Odoo.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "instance": { "type": "string", "description": "Target Odoo instance ID or Name (optional, defaults to active instance)" },
                                "model": { "type": "string" },
                                "domain": { "type": "array" },
                                "fields": { "type": "array", "items": { "type": "string" } },
                                "groupby": { "type": "array", "items": { "type": "string" } }
                            },
                            "required": ["model", "domain", "fields", "groupby"]
                        }
                    },
                    {
                        "name": "odoo-create",
                        "description": "Create a new record in Odoo (CRUD mode only).",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "instance": { "type": "string", "description": "Target Odoo instance ID or Name (optional, defaults to active instance)" },
                                "model": { "type": "string", "description": "The Odoo model name" },
                                "vals": { "type": "object", "description": "Dictionary of fields to set" }
                            },
                            "required": ["model", "vals"]
                        }
                    },
                    {
                        "name": "odoo-copy",
                        "description": "Duplicate an existing record (CRUD mode only).",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "instance": { "type": "string", "description": "Target Odoo instance ID or Name (optional, defaults to active instance)" },
                                "model": { "type": "string" },
                                "id": { "type": "integer" },
                                "vals": { "type": "object", "description": "Fields to override in the copy" }
                            },
                            "required": ["model", "id", "vals"]
                        }
                    },
                    {
                        "name": "odoo-update",
                        "description": "Update an existing record in Odoo (CRUD mode only).",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "instance": { "type": "string", "description": "Target Odoo instance ID or Name (optional, defaults to active instance)" },
                                "model": { "type": "string", "description": "The Odoo model name" },
                                "ids": { "type": "array", "items": { "type": "integer" }, "description": "List of record IDs to update" },
                                "vals": { "type": "object", "description": "Dictionary of fields to update" }
                            },
                            "required": ["model", "ids", "vals"]
                        }
                    },
                    {
                        "name": "odoo-delete",
                        "description": "Delete records from Odoo (CRUD mode only).",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "instance": { "type": "string", "description": "Target Odoo instance ID or Name (optional, defaults to active instance)" },
                                "model": { "type": "string" },
                                "ids": { "type": "array", "items": { "type": "integer" } }
                            },
                            "required": ["model", "ids"]
                        }
                    },
                    {
                        "name": "odoo-get-metadata",
                        "description": "Get field definitions and metadata for a model.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "instance": { "type": "string", "description": "Target Odoo instance ID or Name (optional, defaults to active instance)" },
                                "model": { "type": "string" },
                                "fields": { "type": "array", "items": { "type": "string" }, "description": "Specific fields to inspect (optional)" }
                            },
                            "required": ["model"]
                        }
                    },
                    {
                        "name": "odoo-search",
                        "description": "Search for records and return only their IDs.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "instance": { "type": "string", "description": "Target Odoo instance ID or Name (optional, defaults to active instance)" },
                                "model": { "type": "string", "description": "The Odoo model name (e.g., res.partner)" },
                                "domain": { "type": "array", "description": "Search domain (e.g., [['is_company', '=', true]])" }
                            },
                            "required": ["model", "domain"]
                        }
                    },
                    {
                        "name": "odoo-read",
                        "description": "Read specific records by their IDs.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "instance": { "type": "string", "description": "Target Odoo instance ID or Name (optional, defaults to active instance)" },
                                "model": { "type": "string", "description": "The Odoo model name (e.g., res.partner)" },
                                "ids": { "type": "array", "items": { "type": "integer" }, "description": "List of record IDs to read" },
                                "fields": { "type": "array", "items": { "type": "string" }, "description": "List of fields to return (optional)" }
                            },
                            "required": ["model", "ids"]
                        }
                    }
                ]
            }
        })),
        "tools/call" => {
            let params = req.get("params").cloned().unwrap_or(json!({}));
            let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

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
            if !instance_obj.is_tool_allowed(name, &global_mode) {
                let mode_str = instance_obj.get_mode(&global_mode);
                let err_msg = format!(
                    "Error: Tool '{}' is restricted for Odoo instance '{}' (Mode: '{}'). Permission denied.",
                    name, instance_obj.name, mode_str
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

            let result = execute_tool(name, arguments, &odoo_client).await;

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

async fn execute_tool(name: &str, arguments: Value, odoo: &OdooClient) -> String {
    let model = arguments
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("");
    if model.is_empty() {
        return "Error: Missing required parameter 'model'".to_string();
    }

    match name {
        "odoo-search-read" => {
            let domain = arguments.get("domain").cloned().unwrap_or(json!([]));
            let fields = arguments.get("fields").cloned().unwrap_or(json!([]));
            match odoo.search_read(model, domain, fields).await {
                Ok(data) => serde_json::to_string_pretty(&data)
                    .unwrap_or_else(|e| format!("Error parsing result: {}", e)),
                Err(e) => format!("Error executing search_read: {}", e),
            }
        }
        "odoo-search-count" => {
            let domain = arguments.get("domain").cloned().unwrap_or(json!([]));
            match odoo.search_count(model, domain).await {
                Ok(data) => serde_json::to_string_pretty(&data)
                    .unwrap_or_else(|e| format!("Error parsing result: {}", e)),
                Err(e) => format!("Error executing search_count: {}", e),
            }
        }
        "odoo-read-group" => {
            let domain = arguments.get("domain").cloned().unwrap_or(json!([]));
            let fields = arguments.get("fields").cloned().unwrap_or(json!([]));
            let groupby = arguments.get("groupby").cloned().unwrap_or(json!([]));
            match odoo.read_group(model, domain, fields, groupby).await {
                Ok(data) => serde_json::to_string_pretty(&data)
                    .unwrap_or_else(|e| format!("Error parsing result: {}", e)),
                Err(e) => format!("Error executing read_group: {}", e),
            }
        }
        "odoo-create" => {
            let vals = arguments.get("vals").cloned().unwrap_or(json!({}));
            match odoo.create(model, vals).await {
                Ok(data) => serde_json::to_string_pretty(&data)
                    .unwrap_or_else(|e| format!("Error parsing result: {}", e)),
                Err(e) => format!("Error executing create: {}", e),
            }
        }
        "odoo-copy" => {
            let id = arguments.get("id").and_then(|i| i.as_i64()).unwrap_or(0);
            let vals = arguments.get("vals").cloned().unwrap_or(json!({}));
            match odoo.copy(model, id, vals).await {
                Ok(data) => serde_json::to_string_pretty(&data)
                    .unwrap_or_else(|e| format!("Error parsing result: {}", e)),
                Err(e) => format!("Error executing copy: {}", e),
            }
        }
        "odoo-update" => {
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
        "odoo-delete" => {
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
        "odoo-get-metadata" => {
            let fields = arguments.get("fields").cloned().unwrap_or(json!([]));
            match odoo.get_metadata(model, fields).await {
                Ok(data) => serde_json::to_string_pretty(&data)
                    .unwrap_or_else(|e| format!("Error parsing result: {}", e)),
                Err(e) => format!("Error executing get_metadata: {}", e),
            }
        }
        "odoo-search" => {
            let domain = arguments.get("domain").cloned().unwrap_or(json!([]));
            match odoo.search(model, domain).await {
                Ok(data) => serde_json::to_string_pretty(&data)
                    .unwrap_or_else(|e| format!("Error parsing result: {}", e)),
                Err(e) => format!("Error executing search: {}", e),
            }
        }
        "odoo-read" => {
            let ids_arr = arguments.get("ids").and_then(|ids| ids.as_array());
            let fields = arguments.get("fields").cloned().unwrap_or(json!([]));

            if let Some(arr) = ids_arr {
                let ids: Vec<i64> = arr.iter().filter_map(|v| v.as_i64()).collect();
                match odoo.read(model, ids, fields).await {
                    Ok(data) => serde_json::to_string_pretty(&data)
                        .unwrap_or_else(|e| format!("Error parsing result: {}", e)),
                    Err(e) => format!("Error executing read: {}", e),
                }
            } else {
                "Error: Missing required parameter 'ids' (array of ints)".to_string()
            }
        }
        _ => format!("Error: Unknown tool {}", name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, GlobalSettings, OdooInstance};

    fn create_test_config() -> Arc<RwLock<Config>> {
        let config = Config {
            global_settings: GlobalSettings {
                default_mode: "crud".to_string(),
            },
            instances: vec![
                OdooInstance {
                    id: "1".into(),
                    name: "CRUD Instance".into(),
                    url: "https://odoo-crud.com".into(),
                    db: "db1".into(),
                    username: "admin".into(),
                    password: "pass".into(),
                    active: true,
                    mode: Some("crud".into()),
                    allowed_tools: None,
                },
                OdooInstance {
                    id: "2".into(),
                    name: "Read Only Instance".into(),
                    url: "https://odoo-readonly.com".into(),
                    db: "db2".into(),
                    username: "admin".into(),
                    password: "pass".into(),
                    active: true,
                    mode: Some("read_only".into()),
                    allowed_tools: None,
                },
            ],
            prompts: vec![],
        };
        Arc::new(RwLock::new(config))
    }

    #[tokio::test]
    async fn test_initialize_request() {
        let config = create_test_config();
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
        let config = create_test_config();
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
        let config = create_test_config();
        let client_manager = ClientManager::new();
        let req = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list"
        });

        let resp = handle_request(req, &config, &client_manager).await.unwrap();
        let tools = resp["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();

        assert!(names.contains(&"odoo-search-read"));
        assert!(names.contains(&"odoo-create"));
        assert!(names.contains(&"odoo-update"));
        assert!(names.contains(&"odoo-delete"));
    }

    #[tokio::test]
    async fn test_tools_call_permission_denied_on_readonly() {
        let config = create_test_config();
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
        let config = create_test_config();
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
        let config = create_test_config();
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
