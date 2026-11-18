use super::server::McpCore;
use crate::mcp::McpRuntime;
use crate::observability;
use crate::odoo::ClientManager;
use axum::{
    Router,
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::post,
};
use serde_json::{Value, json};
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tokio::sync::Mutex;

use crate::config::Config;

type SharedConfig = Arc<RwLock<Config>>;

#[derive(Clone)]
struct RemoteState {
    core: Arc<Mutex<McpCore>>,
    require_https: bool,
}

pub(crate) async fn start_remote_mcp(
    config: SharedConfig,
    client_manager: ClientManager,
    runtime: McpRuntime,
) {
    let bind_address = {
        let config = config.read().unwrap();
        config.global_settings.remote_mcp.bind_address.clone()
    };
    let require_https =
        !bind_address.starts_with("127.0.0.1:") && !bind_address.starts_with("[::1]:");
    let state = RemoteState {
        core: Arc::new(Mutex::new(McpCore::new(config, client_manager, runtime))),
        require_https,
    };

    let app = Router::new()
        .route("/mcp", post(handle_mcp_request))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_https_middleware,
        ))
        .with_state(state);

    match tokio::net::TcpListener::bind(&bind_address).await {
        Ok(listener) => {
            eprintln!("Remote MCP transport started on http://{bind_address}/mcp");
            if let Err(error) = axum::serve(listener, app).await {
                eprintln!("Remote MCP transport failed: {error}");
            }
        }
        Err(error) => eprintln!("Failed to bind remote MCP transport {bind_address}: {error}"),
    }
}

async fn require_https_middleware(
    State(state): State<RemoteState>,
    request: Request,
    next: Next,
) -> Response {
    if state.require_https && !request_is_https(&request) {
        return JsonRpcErrorBody::parse_error("HTTPS required for remote MCP").into_response();
    }
    next.run(request).await
}

fn request_is_https(request: &Request) -> bool {
    request.uri().scheme_str() == Some("https")
        || request
            .headers()
            .get(header::HeaderName::from_static("x-forwarded-proto"))
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.eq_ignore_ascii_case("https"))
}

async fn handle_mcp_request(State(state): State<RemoteState>, body: String) -> Response {
    let request = match serde_json::from_str::<Value>(&body) {
        Ok(request) => request,
        Err(_) => {
            observability::metrics().record_request(true);
            return JsonRpcErrorBody::parse_error("Parse error").into_response();
        }
    };

    let method = request
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let tool = request
        .pointer("/params/name")
        .and_then(Value::as_str)
        .map(str::to_string);
    let started = Instant::now();
    let response = state.core.lock().await.handle_request(request).await;
    let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let failed = response
        .as_ref()
        .and_then(|response| response.pointer("/result/isError"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    observability::metrics().record_request(failed);
    if method == "tools/call" {
        observability::metrics().record_tool_call(failed);
    }
    observability::log_event(
        "info",
        "mcp_request",
        json!({
            "method": method,
            "tool": tool,
            "duration_ms": duration_ms,
            "failed": failed,
            "transport": "http"
        }),
    );

    match response {
        Some(response) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json")],
            serde_json::to_string(&response).unwrap_or_else(|_| {
                JsonRpcErrorBody::parse_error("Response serialization failed")
                    .0
                    .to_string()
            }),
        )
            .into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    }
}

struct JsonRpcErrorBody(Value);

impl JsonRpcErrorBody {
    fn parse_error(message: &str) -> Self {
        Self(json!({
            "jsonrpc": "2.0",
            "error": {"code": -32700, "message": message},
            "id": Value::Null
        }))
    }

    fn into_response(self) -> Response {
        (
            StatusCode::BAD_REQUEST,
            [(header::CONTENT_TYPE, "application/json")],
            serde_json::to_string(&self.0).unwrap_or_else(|_| {
                r#"{"jsonrpc":"2.0","error":{"code":-32700,"message":"Parse error"},"id":null}"#
                    .to_string()
            }),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::multi_instance_config;
    use axum::body::Body;
    use axum::http::Uri;

    async fn response_body(response: Response) -> Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn http_transport_returns_json_responses_for_requests() {
        let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel();
        let runtime = McpRuntime::new(sender);
        let state = RemoteState {
            core: Arc::new(Mutex::new(McpCore::new(
                multi_instance_config(),
                ClientManager::new(),
                runtime,
            ))),
            require_https: false,
        };

        let response = handle_mcp_request(
            State(state),
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize"
            })
            .to_string(),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = response_body(response).await;
        assert_eq!(body["result"]["protocolVersion"], "2025-11-25");
    }

    #[tokio::test]
    async fn http_transport_accepts_notifications_without_a_response() {
        let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel();
        let runtime = McpRuntime::new(sender);
        let state = RemoteState {
            core: Arc::new(Mutex::new(McpCore::new(
                multi_instance_config(),
                ClientManager::new(),
                runtime,
            ))),
            require_https: false,
        };

        let response = handle_mcp_request(
            State(state),
            json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized"
            })
            .to_string(),
        )
        .await;

        assert_eq!(response.status(), StatusCode::ACCEPTED);
    }

    fn request(uri: &str, forwarded_proto: Option<&str>) -> Request<Body> {
        let mut builder = Request::builder().uri(uri.parse::<Uri>().unwrap());
        if let Some(proto) = forwarded_proto {
            builder = builder.header("x-forwarded-proto", proto);
        }
        builder.body(axum::body::Body::empty()).unwrap()
    }

    #[test]
    fn https_requires_scheme_or_trusted_forwarded_proto() {
        assert!(request_is_https(&request("https://gateway/mcp", None)));
        assert!(request_is_https(&request(
            "http://gateway/mcp",
            Some("HTTPS")
        )));
        assert!(!request_is_https(&request("http://gateway/mcp", None)));
        assert!(!request_is_https(&request(
            "http://gateway/mcp",
            Some("http")
        )));
    }
}
