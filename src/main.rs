mod audit;
mod config;
mod context;
mod domain;
mod error;
mod lifecycle;
mod mcp;
mod odoo;
mod policy;
mod risk;
mod secret;
#[cfg(test)]
mod test_support;
mod tools;
mod views;

use config::Config;
use mcp::McpRuntime;
use odoo::ClientManager;
use std::sync::{Arc, RwLock};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = config::Config::get_path();
    // Load .env from the same directory as config.json. In release builds this is
    // the executable directory, independent of the MCP client's working directory.
    if let Some(config_dir) = config_path.parent() {
        let env_path = config_dir.join(".env");
        match dotenvy::from_path(&env_path) {
            Ok(_) => eprintln!("Loaded environment from: {:?}", env_path),
            Err(dotenvy::Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => eprintln!("Warning: failed to load {:?}: {}", env_path, error),
        }
    }

    // Load configuration
    eprintln!("Loading config from: {:?}", config_path);
    let config = Config::load().expect("Failed to load config.json");
    audit::initialize(config.global_settings.audit_retention_events);
    let shared_config = Arc::new(RwLock::new(config));
    let client_manager = ClientManager::new();
    let (sender, mut receiver) = mpsc::unbounded_channel::<serde_json::Value>();
    tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        while let Some(message) = receiver.recv().await {
            let Ok(line) = serde_json::to_string(&message) else {
                continue;
            };
            let _ = stdout.write_all(format!("{line}\n").as_bytes()).await;
            let _ = stdout.flush().await;
        }
    });
    let runtime = McpRuntime::new(sender);

    // Spawn Web UI Task
    let ui_config = Arc::clone(&shared_config);
    let ui_runtime = runtime.clone();
    let ui_client_manager = client_manager.clone();
    tokio::spawn(async move {
        views::web_ui::start_ui(ui_config, ui_runtime, ui_client_manager).await;
    });

    eprintln!("Starting MCP server with multi-instance support & permission enforcement...");

    // Start MCP Server loop
    mcp::run_server(shared_config, client_manager, runtime).await;

    Ok(())
}
