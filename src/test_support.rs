use crate::config::{Config, GlobalSettings, OdooInstance};
use axum::{Json, Router, routing::post};
use serde_json::Value;
use std::sync::{Arc, RwLock};

pub(crate) struct MockOdooServer {
    base_url: String,
    task: tokio::task::JoinHandle<()>,
}

impl MockOdooServer {
    pub(crate) async fn start(response: Value) -> Self {
        let app = Router::new().route(
            "/jsonrpc",
            post(move || {
                let response = response.clone();
                async move { Json(response) }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("mock Odoo server should bind");
        let address = listener
            .local_addr()
            .expect("mock Odoo server should expose its address");
        let task = tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("mock Odoo server should run");
        });

        Self {
            base_url: format!("http://{address}"),
            task,
        }
    }

    pub(crate) fn base_url(&self) -> &str {
        &self.base_url
    }
}

impl Drop for MockOdooServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub(crate) fn multi_instance_config() -> Arc<RwLock<Config>> {
    Arc::new(RwLock::new(Config {
        global_settings: GlobalSettings {
            default_mode: "crud".to_string(),
        },
        instances: vec![
            OdooInstance {
                id: "1".into(),
                name: "CRUD Instance".into(),
                url: "https://odoo-crud.test".into(),
                db: "db1".into(),
                username: "admin".into(),
                password: "pass".into(),
                active: true,
                mode: Some("crud".into()),
                allowed_tools: None,
            },
            OdooInstance {
                id: "2".into(),
                name: "Read Only Instance".into(),
                url: "https://odoo-readonly.test".into(),
                db: "db2".into(),
                username: "admin".into(),
                password: "pass".into(),
                active: true,
                mode: Some("read_only".into()),
                allowed_tools: None,
            },
        ],
        prompts: vec![],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fixture_exposes_crud_and_read_only_instances() {
        let config = multi_instance_config();
        let config = config.read().unwrap();

        assert_eq!(config.instances.len(), 2);
        assert_eq!(config.instances[0].get_mode("crud"), "crud");
        assert_eq!(config.instances[1].get_mode("crud"), "read_only");
    }

    #[tokio::test]
    async fn mock_server_returns_configured_json_rpc_response() {
        let server = MockOdooServer::start(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": 7
        }))
        .await;

        let response: Value = reqwest::Client::new()
            .post(format!("{}/jsonrpc", server.base_url()))
            .json(&json!({"jsonrpc": "2.0", "id": 1, "method": "call"}))
            .send()
            .await
            .expect("mock request should succeed")
            .json()
            .await
            .expect("mock response should contain JSON");

        assert_eq!(response["result"], 7);
    }
}
