use serde::{Deserialize, Serialize};
use std::fs;
// use std::path::Path;
use anyhow::Result;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OdooInstance {
    pub id: String,
    pub name: String,
    pub url: String,
    pub db: String,
    pub username: String,
    pub password: String,
    pub active: bool,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,
}

impl OdooInstance {
    pub fn get_mode<'a>(&'a self, global_default: &'a str) -> &'a str {
        match &self.mode {
            Some(m) if !m.trim().is_empty() && m != "inherit" => m.as_str(),
            _ => global_default,
        }
    }

    pub fn is_tool_allowed(&self, tool_name: &str, global_default_mode: &str) -> bool {
        if let Some(ref allowed) = self.allowed_tools
            && !allowed.is_empty()
        {
            return allowed.iter().any(|t| t == tool_name);
        }

        let mode = self.get_mode(global_default_mode);
        match mode {
            "read_only" => {
                let write_tools = ["odoo-create", "odoo-update", "odoo-delete", "odoo-copy"];
                !write_tools.contains(&tool_name)
            }
            _ => true,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GlobalSettings {
    #[serde(default = "default_global_mode")]
    pub default_mode: String,
    #[serde(default = "default_rpc_connection_timeout_secs")]
    pub rpc_connection_timeout_secs: u64,
    #[serde(default = "default_rpc_request_timeout_secs")]
    pub rpc_request_timeout_secs: u64,
    #[serde(default = "default_rpc_max_response_bytes")]
    pub rpc_max_response_bytes: usize,
    #[serde(default = "default_max_query_limit")]
    pub max_query_limit: u64,
    #[serde(default = "default_max_requested_fields")]
    pub max_requested_fields: usize,
    #[serde(default = "default_max_read_ids")]
    pub max_read_ids: usize,
}

fn default_global_mode() -> String {
    "crud".to_string()
}

fn default_rpc_connection_timeout_secs() -> u64 {
    10
}

fn default_rpc_request_timeout_secs() -> u64 {
    30
}

fn default_rpc_max_response_bytes() -> usize {
    10 * 1024 * 1024
}

fn default_max_query_limit() -> u64 {
    1_000
}

fn default_max_requested_fields() -> usize {
    100
}

fn default_max_read_ids() -> usize {
    100
}

impl Default for GlobalSettings {
    fn default() -> Self {
        Self {
            default_mode: default_global_mode(),
            rpc_connection_timeout_secs: default_rpc_connection_timeout_secs(),
            rpc_request_timeout_secs: default_rpc_request_timeout_secs(),
            rpc_max_response_bytes: default_rpc_max_response_bytes(),
            max_query_limit: default_max_query_limit(),
            max_requested_fields: default_max_requested_fields(),
            max_read_ids: default_max_read_ids(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OdooPrompt {
    pub id: String,
    pub name: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Config {
    #[serde(default)]
    pub global_settings: GlobalSettings,
    pub instances: Vec<OdooInstance>,
    pub prompts: Vec<OdooPrompt>,
}

impl Config {
    pub fn get_path() -> std::path::PathBuf {
        if cfg!(debug_assertions) {
            // In debug mode, use the project root
            let manifest_dir = env!("CARGO_MANIFEST_DIR");
            std::path::Path::new(manifest_dir).join("config.json")
        } else {
            // In release mode, look in the same folder as the executable
            let exe_path =
                std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let dir = exe_path.parent().unwrap_or(std::path::Path::new("."));
            dir.join("config.json")
        }
    }

    pub fn load() -> Result<Self> {
        let config_path = Self::get_path();
        if !config_path.exists() {
            let default_config = Config::default();
            default_config.save()?;
            return Ok(default_config);
        }

        let content = fs::read_to_string(config_path)?;
        let config: Config = serde_json::from_str(&content)?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let config_path = Self::get_path();
        let content = serde_json::to_string_pretty(self)?;
        fs::write(config_path, content)?;
        Ok(())
    }

    // pub fn get_active_instances(&self) -> Vec<&OdooInstance> {
    //     self.instances.iter().filter(|i| i.active).collect()
    // }

    pub fn get_active_instance(&self) -> Option<&OdooInstance> {
        self.instances.iter().find(|i| i.active)
    }

    pub fn find_instance(&self, identifier: &str) -> Option<&OdooInstance> {
        if identifier.trim().is_empty() {
            return self.get_active_instance();
        }
        self.instances
            .iter()
            .find(|i| i.id == identifier || i.name.eq_ignore_ascii_case(identifier))
    }

    pub fn toggle_active_instance(&mut self, id: &str) {
        if let Some(instance) = self.instances.iter_mut().find(|i| i.id == id) {
            instance.active = !instance.active;
        }
    }

    #[allow(dead_code)]
    pub fn set_instance_active(&mut self, id: &str, active: bool) {
        if let Some(instance) = self.instances.iter_mut().find(|i| i.id == id) {
            instance.active = active;
        }
    }
}

pub fn generate_id() -> String {
    Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crud_instance_permissions() {
        let instance = OdooInstance {
            id: "1".into(),
            name: "Prod".into(),
            url: "https://odoo.com".into(),
            db: "db".into(),
            username: "admin".into(),
            password: "pass".into(),
            active: true,
            mode: Some("crud".into()),
            allowed_tools: None,
        };

        assert!(instance.is_tool_allowed("odoo-search-read", "crud"));
        assert!(instance.is_tool_allowed("odoo-create", "crud"));
        assert!(instance.is_tool_allowed("odoo-update", "crud"));
        assert!(instance.is_tool_allowed("odoo-delete", "crud"));
    }

    #[test]
    fn test_read_only_instance_permissions() {
        let instance = OdooInstance {
            id: "2".into(),
            name: "ReadOnly".into(),
            url: "https://odoo.com".into(),
            db: "db".into(),
            username: "admin".into(),
            password: "pass".into(),
            active: false,
            mode: Some("read_only".into()),
            allowed_tools: None,
        };

        assert!(instance.is_tool_allowed("odoo-search-read", "crud"));
        assert!(instance.is_tool_allowed("odoo-search-count", "crud"));
        assert!(!instance.is_tool_allowed("odoo-create", "crud"));
        assert!(!instance.is_tool_allowed("odoo-update", "crud"));
        assert!(!instance.is_tool_allowed("odoo-delete", "crud"));
        assert!(!instance.is_tool_allowed("odoo-copy", "crud"));
    }

    #[test]
    fn test_inherit_global_mode() {
        let instance = OdooInstance {
            id: "3".into(),
            name: "Inherit".into(),
            url: "https://odoo.com".into(),
            db: "db".into(),
            username: "admin".into(),
            password: "pass".into(),
            active: false,
            mode: Some("inherit".into()),
            allowed_tools: None,
        };

        assert!(!instance.is_tool_allowed("odoo-create", "read_only"));
        assert!(instance.is_tool_allowed("odoo-create", "crud"));
    }
}
