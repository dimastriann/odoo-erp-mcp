use serde_json::{Value, json};

pub(crate) fn tool_definitions() -> Value {
    json!([
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
    ])
}
