use serde_json::{Value, json};

use crate::policy::Capability;

fn domain_schema() -> Value {
    json!({
        "type": "array",
        "description": "Odoo domain as an array of nested [field, operator, value] clauses and optional prefix logical operators. Example: [[\"name\", \"=\", \"S00027\"]].",
        "items": {
            "oneOf": [
                {
                    "type": "array",
                    "prefixItems": [
                        { "type": "string" },
                        {
                            "type": "string",
                            "enum": ["=", "!=", ">", ">=", "<", "<=", "=?", "=like", "like", "not like", "=ilike", "ilike", "not ilike", "in", "not in", "child_of", "parent_of", "any", "not any", "any!"]
                        },
                        {}
                    ],
                    "minItems": 3,
                    "maxItems": 3
                },
                {
                    "type": "string",
                    "enum": ["&", "|", "!"]
                }
            ]
        }
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ToolName {
    SearchRead,
    SearchCount,
    ReadGroup,
    Create,
    Copy,
    Update,
    Delete,
    GetMetadata,
    Search,
    Read,
    GetServerInfo,
    ListModels,
    CheckAccess,
    Onchange,
    NameSearch,
    GetViews,
    ListReports,
    RenderReport,
    CreateBatch,
    UpdateBatch,
    DeleteBatch,
    ListInstances,
}

impl ToolName {
    #[cfg(test)]
    pub(crate) const ALL: [Self; 22] = [
        Self::SearchRead,
        Self::SearchCount,
        Self::ReadGroup,
        Self::Create,
        Self::Copy,
        Self::Update,
        Self::Delete,
        Self::GetMetadata,
        Self::Search,
        Self::Read,
        Self::GetServerInfo,
        Self::ListModels,
        Self::CheckAccess,
        Self::Onchange,
        Self::NameSearch,
        Self::GetViews,
        Self::ListReports,
        Self::RenderReport,
        Self::CreateBatch,
        Self::UpdateBatch,
        Self::DeleteBatch,
        Self::ListInstances,
    ];

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::SearchRead => "odoo-search-read",
            Self::SearchCount => "odoo-search-count",
            Self::ReadGroup => "odoo-read-group",
            Self::Create => "odoo-create",
            Self::Copy => "odoo-copy",
            Self::Update => "odoo-update",
            Self::Delete => "odoo-delete",
            Self::GetMetadata => "odoo-get-metadata",
            Self::Search => "odoo-search",
            Self::Read => "odoo-read",
            Self::GetServerInfo => "odoo-get-server-info",
            Self::ListModels => "odoo-list-models",
            Self::CheckAccess => "odoo-check-access",
            Self::Onchange => "odoo-onchange",
            Self::NameSearch => "odoo-name-search",
            Self::GetViews => "odoo-get-views",
            Self::ListReports => "odoo-list-reports",
            Self::RenderReport => "odoo-render-report",
            Self::CreateBatch => "odoo-create-batch",
            Self::UpdateBatch => "odoo-update-batch",
            Self::DeleteBatch => "odoo-delete-batch",
            Self::ListInstances => "odoo-list-instances",
        }
    }

    pub(crate) const fn capability(self) -> Capability {
        match self {
            Self::Create | Self::Copy | Self::CreateBatch => Capability::Create,
            Self::Update | Self::UpdateBatch => Capability::Update,
            Self::Delete | Self::DeleteBatch => Capability::Delete,
            Self::SearchRead
            | Self::SearchCount
            | Self::ReadGroup
            | Self::GetMetadata
            | Self::Search
            | Self::Read
            | Self::GetServerInfo
            | Self::ListModels
            | Self::CheckAccess
            | Self::Onchange
            | Self::NameSearch
            | Self::GetViews
            | Self::ListReports
            | Self::RenderReport
            | Self::ListInstances => Capability::Read,
        }
    }

    pub(crate) const fn requires_model(self) -> bool {
        !matches!(
            self,
            Self::GetServerInfo | Self::ListModels | Self::ListReports | Self::ListInstances
        )
    }

    pub(crate) const fn requires_client(self) -> bool {
        !matches!(self, Self::ListInstances)
    }
}

impl TryFrom<&str> for ToolName {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "odoo-search-read" => Ok(Self::SearchRead),
            "odoo-search-count" => Ok(Self::SearchCount),
            "odoo-read-group" => Ok(Self::ReadGroup),
            "odoo-create" => Ok(Self::Create),
            "odoo-copy" => Ok(Self::Copy),
            "odoo-update" => Ok(Self::Update),
            "odoo-delete" => Ok(Self::Delete),
            "odoo-get-metadata" => Ok(Self::GetMetadata),
            "odoo-search" => Ok(Self::Search),
            "odoo-read" => Ok(Self::Read),
            "odoo-get-server-info" => Ok(Self::GetServerInfo),
            "odoo-list-models" => Ok(Self::ListModels),
            "odoo-check-access" => Ok(Self::CheckAccess),
            "odoo-onchange" => Ok(Self::Onchange),
            "odoo-name-search" => Ok(Self::NameSearch),
            "odoo-get-views" => Ok(Self::GetViews),
            "odoo-list-reports" => Ok(Self::ListReports),
            "odoo-render-report" => Ok(Self::RenderReport),
            "odoo-create-batch" => Ok(Self::CreateBatch),
            "odoo-update-batch" => Ok(Self::UpdateBatch),
            "odoo-delete-batch" => Ok(Self::DeleteBatch),
            "odoo-list-instances" => Ok(Self::ListInstances),
            _ => Err(()),
        }
    }
}

pub(crate) fn tool_definitions() -> Value {
    let mut definitions = json!([
        {
            "name": "odoo-search-read",
            "description": "Search and retrieve records from Odoo.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "instance": { "type": "string", "description": "Target Odoo instance ID or Name (optional, defaults to active instance)" },
                    "model": { "type": "string", "description": "The Odoo model name (e.g., res.partner)" },
                    "domain": domain_schema(),
                    "fields": { "type": "array", "minItems": 1, "items": { "type": "string" }, "description": "List of fields to return" },
                    "offset": { "type": "integer", "minimum": 0, "description": "Number of matching records to skip" },
                    "limit": { "type": "integer", "minimum": 1, "default": 100, "description": "Maximum number of records to return" },
                    "order": { "type": "string", "description": "Odoo order clause, for example 'name asc, id desc'" },
                    "include_total": { "type": "boolean", "default": false, "description": "Run an additional count query and include the total number of matches" }
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
                    "domain": domain_schema()
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
                    "domain": domain_schema(),
                    "fields": { "type": "array", "minItems": 1, "items": { "type": "string" } },
                    "groupby": { "type": "array", "items": { "type": "string" } },
                    "offset": { "type": "integer", "minimum": 0 },
                    "limit": { "type": "integer", "minimum": 1, "default": 100 },
                    "orderby": { "type": "string", "description": "Odoo order clause for grouped results" }
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
                    "vals": { "type": "object", "description": "Dictionary of fields to set" },
                    "idempotency_key": { "type": "string", "minLength": 1, "maxLength": 256, "description": "Client key used to safely retry this mutation" }
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
                    "vals": { "type": "object", "description": "Fields to override in the copy" },
                    "idempotency_key": { "type": "string", "minLength": 1, "maxLength": 256, "description": "Client key used to safely retry this mutation" }
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
                    "vals": { "type": "object", "description": "Dictionary of fields to update" },
                    "idempotency_key": { "type": "string", "minLength": 1, "maxLength": 256, "description": "Client key used to safely retry this mutation" }
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
                    "ids": { "type": "array", "items": { "type": "integer" } },
                    "idempotency_key": { "type": "string", "minLength": 1, "maxLength": 256, "description": "Client key used to safely retry this mutation" }
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
                    "fields": { "type": "array", "minItems": 1, "items": { "type": "string" }, "description": "Specific fields to inspect" }
                },
                "required": ["model", "fields"]
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
                    "domain": domain_schema(),
                    "offset": { "type": "integer", "minimum": 0, "description": "Number of matching records to skip" },
                    "limit": { "type": "integer", "minimum": 1, "default": 100, "description": "Maximum number of record IDs to return" },
                    "order": { "type": "string", "description": "Odoo order clause, for example 'name asc, id desc'" },
                    "include_total": { "type": "boolean", "default": false, "description": "Run an additional count query and include the total number of matches" }
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
                    "fields": { "type": "array", "minItems": 1, "items": { "type": "string" }, "description": "List of fields to return" }
                },
                "required": ["model", "ids", "fields"]
            }
        },
        {
            "name": "odoo-get-server-info",
            "description": "Return the Odoo server version and selected RPC transport.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "instance": { "type": "string", "description": "Target Odoo instance ID or Name (optional, defaults to active instance)" }
                },
                "required": []
            }
        },
        {
            "name": "odoo-list-models",
            "description": "List Odoo models with pagination.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "instance": { "type": "string", "description": "Target Odoo instance ID or Name (optional, defaults to active instance)" },
                    "domain": domain_schema(),
                    "offset": { "type": "integer", "minimum": 0 },
                    "limit": { "type": "integer", "minimum": 1, "default": 100 },
                    "include_total": { "type": "boolean", "default": false }
                },
                "required": []
            }
        },
        {
            "name": "odoo-check-access",
            "description": "Check model and record access for the current Odoo user.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "instance": { "type": "string", "description": "Target Odoo instance ID or Name (optional, defaults to active instance)" },
                    "model": { "type": "string" },
                    "operation": { "type": "string", "enum": ["read", "write", "create", "unlink"] },
                    "ids": { "type": "array", "items": { "type": "integer" }, "description": "Optional record IDs for record-rule checks" }
                },
                "required": ["model", "operation"]
            }
        },
        {
            "name": "odoo-onchange",
            "description": "Evaluate Odoo onchange logic without writing records.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "instance": { "type": "string", "description": "Target Odoo instance ID or Name (optional, defaults to active instance)" },
                    "model": { "type": "string" },
                    "values": { "type": "object" },
                    "field_names": { "type": "array", "items": { "type": "string" } },
                    "fields_spec": { "type": "object" }
                },
                "required": ["model", "values", "field_names", "fields_spec"]
            }
        },
        {
            "name": "odoo-name-search",
            "description": "Search record display names for relation pickers.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "instance": { "type": "string", "description": "Target Odoo instance ID or Name (optional, defaults to active instance)" },
                    "model": { "type": "string" },
                    "name": { "type": "string" },
                    "domain": domain_schema(),
                    "operator": { "type": "string", "enum": ["ilike", "like", "=", "=ilike", "=like"] },
                    "limit": { "type": "integer", "minimum": 1, "default": 20 }
                },
                "required": ["model", "name"]
            }
        },
        {
            "name": "odoo-get-views",
            "description": "Read model view definitions.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "instance": { "type": "string", "description": "Target Odoo instance ID or Name (optional, defaults to active instance)" },
                    "model": { "type": "string" },
                    "views": { "type": "array", "items": { "type": "array", "prefixItems": [{ "type": "integer" }, { "type": "string", "enum": ["form", "tree", "list", "search", "kanban", "graph", "pivot", "calendar"] }], "minItems": 2, "maxItems": 2 } },
                    "options": { "type": "object" }
                },
                "required": ["model", "views"]
            }
        },
        {
            "name": "odoo-list-reports",
            "description": "List available Odoo reports.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "instance": { "type": "string", "description": "Target Odoo instance ID or Name (optional, defaults to active instance)" },
                    "model": { "type": "string", "description": "Optional model filter" },
                    "domain": domain_schema(),
                    "offset": { "type": "integer", "minimum": 0 },
                    "limit": { "type": "integer", "minimum": 1, "default": 100 },
                    "include_total": { "type": "boolean", "default": false }
                },
                "required": []
            }
        },
        {
            "name": "odoo-render-report",
            "description": "Render an Odoo report as HTML or base64 PDF.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "instance": { "type": "string", "description": "Target Odoo instance ID or Name (optional, defaults to active instance)" },
                    "report_name": { "type": "string", "description": "Odoo report technical name, e.g. sale.report_saleorder" },
                    "ids": { "type": "array", "items": { "type": "integer" }, "minItems": 1 },
                    "format": { "type": "string", "enum": ["pdf", "html"], "default": "pdf" },
                    "data": { "type": "object", "description": "Optional report data" }
                },
                "required": ["report_name", "ids"]
            }
        },
        {
            "name": "odoo-create-batch",
            "description": "Create multiple records with per-item verification (CRUD mode only).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "instance": { "type": "string", "description": "Target Odoo instance ID or Name (optional, defaults to active instance)" },
                    "model": { "type": "string" },
                    "vals_list": { "type": "array", "items": { "type": "object" }, "minItems": 1 },
                    "idempotency_key": { "type": "string", "minLength": 1, "maxLength": 256 },
                    "approval_id": { "type": "string" }
                },
                "required": ["model", "vals_list"]
            }
        },
        {
            "name": "odoo-update-batch",
            "description": "Update multiple records with per-record results (CRUD mode only).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "instance": { "type": "string", "description": "Target Odoo instance ID or Name (optional, defaults to active instance)" },
                    "model": { "type": "string" },
                    "records": { "type": "array", "items": { "type": "object", "properties": { "id": { "type": "integer" }, "vals": { "type": "object" } }, "required": ["id", "vals"] }, "minItems": 1 },
                    "idempotency_key": { "type": "string", "minLength": 1, "maxLength": 256 },
                    "approval_id": { "type": "string" }
                },
                "required": ["model", "records"]
            }
        },
        {
            "name": "odoo-delete-batch",
            "description": "Delete multiple records with per-id results (CRUD mode only).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "instance": { "type": "string", "description": "Target Odoo instance ID or Name (optional, defaults to active instance)" },
                    "model": { "type": "string" },
                    "ids": { "type": "array", "items": { "type": "integer" }, "minItems": 1 },
                    "idempotency_key": { "type": "string", "minLength": 1, "maxLength": 256 },
                    "approval_id": { "type": "string" }
                },
                "required": ["model", "ids"]
            }
        },
        {
            "name": "odoo-list-instances",
            "description": "List configured Odoo instances, folders, and active state without contacting Odoo.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "required": []
            }
        }
    ]);

    for definition in definitions
        .as_array_mut()
        .expect("tool definitions are always an array")
    {
        definition["inputSchema"]["properties"]["context"] = context_schema();
    }
    definitions
}

fn context_schema() -> Value {
    json!({
        "type": "object",
        "description": "Optional Odoo context. Supported keys: lang, tz, active_test, company_id, allowed_company_ids, and default_* values.",
        "additionalProperties": true
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_name_round_trips_and_has_one_definition() {
        let definitions = tool_definitions();
        let definitions = definitions.as_array().unwrap();

        assert_eq!(definitions.len(), ToolName::ALL.len());
        for tool_name in ToolName::ALL {
            assert_eq!(ToolName::try_from(tool_name.as_str()), Ok(tool_name));
            assert_eq!(
                definitions
                    .iter()
                    .filter(|definition| definition["name"] == tool_name.as_str())
                    .count(),
                1
            );
        }
    }

    #[test]
    fn tools_map_to_typed_core_capabilities() {
        assert_eq!(ToolName::SearchRead.capability(), Capability::Read);
        assert_eq!(ToolName::Create.capability(), Capability::Create);
        assert_eq!(ToolName::Copy.capability(), Capability::Create);
        assert_eq!(ToolName::Update.capability(), Capability::Update);
        assert_eq!(ToolName::Delete.capability(), Capability::Delete);
    }

    #[test]
    fn unknown_tool_name_is_rejected() {
        assert_eq!(ToolName::try_from("odoo-unknown"), Err(()));
    }

    #[test]
    fn search_tools_advertise_nested_domain_clauses() {
        let definitions = tool_definitions();
        let search_read = definitions
            .as_array()
            .unwrap()
            .iter()
            .find(|definition| definition["name"] == ToolName::SearchRead.as_str())
            .unwrap();
        let domain = &search_read["inputSchema"]["properties"]["domain"];

        assert_eq!(domain["type"], "array");
        assert!(domain["description"].as_str().unwrap().contains("nested"));
        assert!(domain["items"]["oneOf"].is_array());
        assert_eq!(domain["items"]["oneOf"][0]["minItems"], 3);
        assert_eq!(domain["items"]["oneOf"][0]["maxItems"], 3);
        assert!(domain["items"]["oneOf"][0]["prefixItems"][1]["enum"].is_array());
    }

    #[test]
    fn search_tools_advertise_consistent_pagination_contracts() {
        let definitions = tool_definitions();

        for tool_name in [ToolName::Search, ToolName::SearchRead] {
            let definition = definitions
                .as_array()
                .unwrap()
                .iter()
                .find(|definition| definition["name"] == tool_name.as_str())
                .unwrap();
            let properties = &definition["inputSchema"]["properties"];

            assert_eq!(properties["offset"]["minimum"], 0);
            assert_eq!(properties["limit"]["minimum"], 1);
            assert_eq!(properties["limit"]["default"], 100);
            assert_eq!(properties["include_total"]["default"], false);
        }
    }
}
