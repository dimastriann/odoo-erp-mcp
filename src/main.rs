mod config;
mod mcp_server;
mod odoo_client;
#[cfg(test)]
mod test_support;
mod views;

use config::Config;
use odoo_client::ClientManager;
use std::sync::{Arc, RwLock};

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
