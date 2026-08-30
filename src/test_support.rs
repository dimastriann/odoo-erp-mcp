use crate::config::{Config, GlobalSettings, OdooInstance};
use axum::{Json, Router, extract::State, routing::post};
use serde_json::Value;
use std::sync::{Arc, RwLock};

pub(crate) struct MockOdooServer {
    base_url: String,
    requests: Arc<tokio::sync::Mutex<Vec<Value>>>,
    task: tokio::task::JoinHandle<()>,
}

#[derive(Clone)]
struct MockOdooState {
    response: Value,
    requests: Arc<tokio::sync::Mutex<Vec<Value>>>,
}

async fn handle_mock_rpc(
    State(state): State<MockOdooState>,
    Json(request): Json<Value>,
) -> Json<Value> {
    state.requests.lock().await.push(request);
    Json(state.response)
}

impl MockOdooServer {
    pub(crate) async fn start(response: Value) -> Self {
        let requests = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let state = MockOdooState {
            response,
            requests: Arc::clone(&requests),
        };
        let app = Router::new()
            .route("/jsonrpc", post(handle_mock_rpc))
            .with_state(state);
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
            requests,
            task,
        }
    }

    pub(crate) fn base_url(&self) -> &str {
        &self.base_url
    }

    pub(crate) async fn requests(&self) -> Vec<Value> {
        self.requests.lock().await.clone()
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
        assert_eq!(
            server.requests().await,
            vec![json!({"jsonrpc": "2.0", "id": 1, "method": "call"})]
        );
    }
}
