use crate::odoo_client::OdooClient;
use serde_json::{json, Value};
use tokio::io::{self, AsyncBufReadExt, BufReader, AsyncWriteExt};

pub async fn run_server(odoo_client: Option<OdooClient>) {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut reader = BufReader::new(stdin).lines();

    while let Some(line) = reader.next_line().await.unwrap_or(None) {
        if line.trim().is_empty() {
            continue;
        }

        if let Ok(request) = serde_json::from_str::<Value>(&line) {
            let response = handle_request(request, &odoo_client).await;
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
            let _ = stdout.write_all(format!("{}\n", error_resp).as_bytes()).await;
            let _ = stdout.flush().await;
        }
    }
}

async fn handle_request(req: Value, odoo_client: &Option<OdooClient>) -> Option<Value> {
    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let id = req.get("id").cloned().unwrap_or(Value::Null);

    match method {
        "initialize" => {
            Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": "2025-11-25",
                    "serverInfo": {
                        "name": "odoo-erp-mcp",
                        "version": "1.0.0"
                    },
                    "capabilities": {
                        "tools": {}
                    }
                }
            }))
        }
        "notifications/initialized" => None,
        "tools/list" => {
            Some(json!({
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
                            "description": "Create a new record in Odoo.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "model": { "type": "string", "description": "The Odoo model name" },
                                    "vals": { "type": "object", "description": "Dictionary of fields to set" }
                                },
                                "required": ["model", "vals"]
                            }
                        },
                        {
                            "name": "odoo-copy",
                            "description": "Duplicate an existing record.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "model": { "type": "string" },
                                    "id": { "type": "integer" },
                                    "vals": { "type": "object", "description": "Fields to override in the copy" }
                                },
                                "required": ["model", "id", "vals"]
                            }
                        },
                        {
                            "name": "odoo-update",
                            "description": "Update an existing record in Odoo.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "model": { "type": "string", "description": "The Odoo model name" },
                                    "ids": { "type": "array", "items": { "type": "integer" }, "description": "List of record IDs to update" },
                                    "vals": { "type": "object", "description": "Dictionary of fields to update" }
                                },
                                "required": ["model", "ids", "vals"]
                            }
                        },
                        {
                            "name": "odoo-delete",
                            "description": "Delete records from Odoo.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
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
                                    "model": { "type": "string" },
                                    "fields": { "type": "array", "items": { "type": "string" }, "description": "Specific fields to inspect (optional)" }
                                },
                                "required": ["model"]
                            }
                        }
                    ]
                }
            }))
        }
        "tools/call" => {
            let params = req.get("params").cloned().unwrap_or(json!({}));
            let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

            let result = execute_tool(name, arguments, odoo_client).await;

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
        _ => {
            // Method not found
            Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": "Method not found" }
            }))
        }
    }
}

async fn execute_tool(name: &str, arguments: Value, odoo_opt: &Option<OdooClient>) -> String {
    let odoo = match odoo_opt {
        Some(c) => c,
        None => return "Error: No active Odoo instance found. Please configure one at http://localhost:3333 and restart the server.".to_string(),
    };

    let model = arguments.get("model").and_then(|m| m.as_str()).unwrap_or("");
    if model.is_empty() {
        return "Error: Missing required parameter 'model'".to_string();
    }

    match name {
        "odoo-search-read" => {
            let domain = arguments.get("domain").cloned().unwrap_or(json!([]));
            let fields = arguments.get("fields").cloned().unwrap_or(json!([]));
            match odoo.search_read(model, domain, fields).await {
                Ok(data) => serde_json::to_string_pretty(&data).unwrap_or_else(|e| format!("Error parsing result: {}", e)),
                Err(e) => format!("Error executing search_read: {}", e),
            }
        }
        "odoo-search-count" => {
            let domain = arguments.get("domain").cloned().unwrap_or(json!([]));
            match odoo.search_count(model, domain).await {
                Ok(data) => serde_json::to_string_pretty(&data).unwrap_or_else(|e| format!("Error parsing result: {}", e)),
                Err(e) => format!("Error executing search_count: {}", e),
            }
        }
        "odoo-read-group" => {
            let domain = arguments.get("domain").cloned().unwrap_or(json!([]));
            let fields = arguments.get("fields").cloned().unwrap_or(json!([]));
            let groupby = arguments.get("groupby").cloned().unwrap_or(json!([]));
            match odoo.read_group(model, domain, fields, groupby).await {
                Ok(data) => serde_json::to_string_pretty(&data).unwrap_or_else(|e| format!("Error parsing result: {}", e)),
                Err(e) => format!("Error executing read_group: {}", e),
            }
        }
        "odoo-create" => {
            let vals = arguments.get("vals").cloned().unwrap_or(json!({}));
            match odoo.create(model, vals).await {
                Ok(data) => serde_json::to_string_pretty(&data).unwrap_or_else(|e| format!("Error parsing result: {}", e)),
                Err(e) => format!("Error executing create: {}", e),
            }
        }
        "odoo-copy" => {
            let id = arguments.get("id").and_then(|i| i.as_i64()).unwrap_or(0);
            let vals = arguments.get("vals").cloned().unwrap_or(json!({}));
            match odoo.copy(model, id, vals).await {
                Ok(data) => serde_json::to_string_pretty(&data).unwrap_or_else(|e| format!("Error parsing result: {}", e)),
                Err(e) => format!("Error executing copy: {}", e),
            }
        }
        "odoo-update" => {
            let ids_arr = arguments.get("ids").and_then(|ids| ids.as_array());
            let vals = arguments.get("vals").cloned().unwrap_or(json!({}));
            
            if let Some(arr) = ids_arr {
                let ids: Vec<i64> = arr.iter().filter_map(|v| v.as_i64()).collect();
                match odoo.update(model, ids, vals).await {
                    Ok(data) => serde_json::to_string_pretty(&data).unwrap_or_else(|e| format!("Error parsing result: {}", e)),
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
                    Ok(data) => serde_json::to_string_pretty(&data).unwrap_or_else(|e| format!("Error parsing result: {}", e)),
                    Err(e) => format!("Error executing delete: {}", e),
                }
            } else {
                "Error: Missing required parameter 'ids' (array of ints)".to_string()
            }
        }
        "odoo-get-metadata" => {
            let fields = arguments.get("fields").cloned().unwrap_or(json!([]));
            match odoo.get_metadata(model, fields).await {
                Ok(data) => serde_json::to_string_pretty(&data).unwrap_or_else(|e| format!("Error parsing result: {}", e)),
                Err(e) => format!("Error executing get_metadata: {}", e),
            }
        }
        _ => format!("Error: Unknown tool {}", name),
    }
}
