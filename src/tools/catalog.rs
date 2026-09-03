use serde_json::{Value, json};

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
                        { "type": "string" },
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
}

impl ToolName {
    #[cfg(test)]
    pub(crate) const ALL: [Self; 10] = [
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
        }
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
            _ => Err(()),
        }
    }
}

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
                    "domain": domain_schema(),
                    "fields": { "type": "array", "items": { "type": "string" }, "description": "List of fields to return" },
                    "offset": { "type": "integer", "minimum": 0, "description": "Number of matching records to skip" },
                    "limit": { "type": "integer", "minimum": 1, "default": 100, "description": "Maximum number of records to return" },
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
                    "domain": domain_schema(),
                    "offset": { "type": "integer", "minimum": 0, "description": "Number of matching records to skip" },
                    "limit": { "type": "integer", "minimum": 1, "default": 100, "description": "Maximum number of record IDs to return" },
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
                    "fields": { "type": "array", "items": { "type": "string" }, "description": "List of fields to return (optional)" }
                },
                "required": ["model", "ids"]
            }
        }
    ])
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
