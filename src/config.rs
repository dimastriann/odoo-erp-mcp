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
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OdooPrompt {
    pub id: String,
    pub name: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Config {
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
            let exe_path = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("."));
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

    pub fn get_active_instance(&self) -> Option<&OdooInstance> {
        self.instances.iter().find(|i| i.active)
    }

    pub fn set_active_instance(&mut self, id: &str) {
        for instance in &mut self.instances {
            instance.active = instance.id == id;
        }
    }
}

pub fn generate_id() -> String {
    Uuid::new_v4().to_string()
}
