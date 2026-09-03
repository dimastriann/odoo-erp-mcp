use crate::config::{Config, GlobalSettings, OdooInstance};
use axum::http::StatusCode;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router, extract::State, routing::post};
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::{Arc, RwLock};
use std::time::Duration;

pub(crate) fn json_rpc_success(result: Value) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "result": result
    })
}

pub(crate) fn authentication_success(uid: i64) -> Value {
    json_rpc_success(Value::from(uid))
}

fn odoo_error(name: &str, message: &str) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "error": {
            "code": 200,
            "message": "Odoo Server Error",
            "data": {
                "name": name,
                "message": message
            }
        }
    })
}

pub(crate) fn validation_error(message: &str) -> Value {
    odoo_error("odoo.exceptions.ValidationError", message)
}

pub(crate) fn access_error(message: &str) -> Value {
    odoo_error("odoo.exceptions.AccessError", message)
}

pub(crate) fn connection_failure_url() -> &'static str {
    "http://127.0.0.1:0"
}

pub(crate) struct MockOdooServer {
    base_url: String,
    requests: Arc<tokio::sync::Mutex<Vec<Value>>>,
    task: tokio::task::JoinHandle<()>,
}

#[derive(Clone)]
struct MockOdooState {
    responses: Arc<tokio::sync::Mutex<VecDeque<MockReply>>>,
    fallback: MockReply,
    requests: Arc<tokio::sync::Mutex<Vec<Value>>>,
}

#[derive(Clone)]
enum MockResponse {
    Json(Value),
    Raw(String),
}

#[derive(Clone)]
struct MockReply {
    response: MockResponse,
    response_delay: Duration,
    status: StatusCode,
}

async fn handle_mock_rpc(
    State(state): State<MockOdooState>,
    Json(request): Json<Value>,
) -> Response {
    state.requests.lock().await.push(request);
    let reply = state
        .responses
        .lock()
        .await
        .pop_front()
        .unwrap_or_else(|| state.fallback.clone());
    tokio::time::sleep(reply.response_delay).await;
    match reply.response {
        MockResponse::Json(value) => (reply.status, Json(value)).into_response(),
        MockResponse::Raw(body) => (
            reply.status,
            [(header::CONTENT_TYPE, "application/json")],
            body,
        )
            .into_response(),
    }
}

impl MockOdooServer {
    pub(crate) async fn start(response: Value) -> Self {
        Self::start_with_options(MockResponse::Json(response), StatusCode::OK, Duration::ZERO).await
    }

    pub(crate) async fn start_delayed(response: Value, response_delay: Duration) -> Self {
        Self::start_with_options(MockResponse::Json(response), StatusCode::OK, response_delay).await
    }

    pub(crate) async fn start_with_delays(response: Value, delays: Vec<Duration>) -> Self {
        let fallback_delay = delays.last().copied().unwrap_or(Duration::ZERO);
        let responses = delays
            .into_iter()
            .map(|response_delay| MockReply {
                response: MockResponse::Json(response.clone()),
                response_delay,
                status: StatusCode::OK,
            })
            .collect();
        let fallback = MockReply {
            response: MockResponse::Json(response),
            response_delay: fallback_delay,
            status: StatusCode::OK,
        };
        Self::start_with_replies(responses, fallback).await
    }

    pub(crate) async fn start_with_status(response: Value, status: StatusCode) -> Self {
        Self::start_with_options(MockResponse::Json(response), status, Duration::ZERO).await
    }

    pub(crate) async fn start_raw(response: impl Into<String>) -> Self {
        Self::start_with_options(
            MockResponse::Raw(response.into()),
            StatusCode::OK,
            Duration::ZERO,
        )
        .await
    }

    async fn start_with_options(
        response: MockResponse,
        status: StatusCode,
        response_delay: Duration,
    ) -> Self {
        let fallback = MockReply {
            response,
            response_delay,
            status,
        };
        Self::start_with_replies(VecDeque::new(), fallback).await
    }

    async fn start_with_replies(responses: VecDeque<MockReply>, fallback: MockReply) -> Self {
        let requests = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let state = MockOdooState {
            responses: Arc::new(tokio::sync::Mutex::new(responses)),
            fallback,
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
            rpc_connection_timeout_secs: 10,
            rpc_request_timeout_secs: 30,
            rpc_max_response_bytes: 10 * 1024 * 1024,
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
        let server = MockOdooServer::start(authentication_success(7)).await;

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

    #[test]
    fn success_fixture_wraps_arbitrary_results() {
        assert_eq!(
            json_rpc_success(json!([{"id": 42, "name": "Example"}])),
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": [{"id": 42, "name": "Example"}]
            })
        );
    }

    #[test]
    fn error_fixtures_identify_odoo_exception_types() {
        let validation = validation_error("Invalid quantity");
        let access = access_error("Access denied");

        assert_eq!(
            validation["error"]["data"]["name"],
            "odoo.exceptions.ValidationError"
        );
        assert_eq!(
            access["error"]["data"]["name"],
            "odoo.exceptions.AccessError"
        );
    }

    #[tokio::test]
    async fn delayed_server_and_unreachable_url_simulate_transport_failures() {
        let server =
            MockOdooServer::start_delayed(authentication_success(7), Duration::from_millis(100))
                .await;
        let timeout_client = reqwest::Client::builder()
            .timeout(Duration::from_millis(10))
            .build()
            .unwrap();

        let timeout = timeout_client
            .post(format!("{}/jsonrpc", server.base_url()))
            .json(&json!({"jsonrpc": "2.0", "id": 1, "method": "call"}))
            .send()
            .await
            .unwrap_err();
        let connection = reqwest::get(connection_failure_url()).await.unwrap_err();

        assert!(timeout.is_timeout());
        assert!(connection.is_connect());
    }
}
