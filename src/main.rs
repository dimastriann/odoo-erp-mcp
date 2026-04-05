mod odoo_client;
mod mcp_server;
mod config;
mod views;

use odoo_client::OdooClient;
use config::Config;
use std::sync::{Arc, RwLock};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();

    // Load configuration
    let config_path = config::Config::get_path();
    eprintln!("Loading config from: {:?}", config_path);
    let config = Config::load().expect("Failed to load config.json");
    let shared_config = Arc::new(RwLock::new(config));

    // Spawn Web UI Task
    let ui_config = Arc::clone(&shared_config);
    tokio::spawn(async move {
        views::web_ui::start_ui(ui_config).await;
    });

    // Initialize Odoo Client from active instance
    let odoo_client = {
        let conf = shared_config.read().unwrap();
        if let Some(inst) = conf.get_active_instance() {
            eprintln!("Connecting to Odoo instance: {}...", inst.name);
            match OdooClient::new(
                inst.url.clone(),
                inst.db.clone(),
                inst.username.clone(),
                inst.password.clone()
            ).await {
                Ok(client) => Some(client),
                Err(e) => {
                    eprintln!("Failed to initialize Odoo client: {}. Tools will fail.", e);
                    None
                }
            }
        } else {
            eprintln!("No active Odoo instance found in config.json. Tools will fail until configured via http://localhost:3333");
            None
        }
    };

    // Start MCP Server loop
    mcp_server::run_server(odoo_client).await;

    Ok(())
}
