mod config;
mod mcp_server;
mod odoo_client;
mod views;

use config::Config;
use odoo_client::ClientManager;
use std::sync::{Arc, RwLock};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();

    // Load configuration
    let config_path = config::Config::get_path();
    eprintln!("Loading config from: {:?}", config_path);
    let config = Config::load().expect("Failed to load config.json");
    let shared_config = Arc::new(RwLock::new(config));
    let client_manager = ClientManager::new();

    // Spawn Web UI Task
    let ui_config = Arc::clone(&shared_config);
    tokio::spawn(async move {
        views::web_ui::start_ui(ui_config).await;
    });

    eprintln!("Starting MCP server with multi-instance support & permission enforcement...");

    // Start MCP Server loop
    mcp_server::run_server(shared_config, client_manager).await;

    Ok(())
}
