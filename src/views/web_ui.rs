use crate::config::{Config, OdooInstance, OdooPrompt, generate_id};
use axum::{
    Json, Router,
    extract::{Path, Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{delete, get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    sync::{Arc, RwLock},
};
use uuid::Uuid;

type SharedConfig = Arc<RwLock<Config>>;

#[derive(Clone)]
struct AppState {
    config: SharedConfig,
    username: Arc<String>,
    password: Arc<String>,
    sessions: Arc<RwLock<HashSet<String>>>,
}

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

pub async fn start_ui(config: SharedConfig) {
    let username = std::env::var("ODOO_MCP_UI_USERNAME").unwrap_or_else(|_| "admin".into());
    let password = std::env::var("ODOO_MCP_UI_PASSWORD").unwrap_or_else(|_| {
        let generated = Uuid::new_v4().to_string();
        eprintln!("ODOO_MCP_UI_PASSWORD is not set; generated one-time UI password: {generated}");
        generated
    });
    let state = AppState {
        config,
        username: Arc::new(username),
        password: Arc::new(password),
        sessions: Arc::new(RwLock::new(HashSet::new())),
    };

    let protected = Router::new()
        .route("/api/config", get(get_config))
        .route("/api/global-settings", post(update_global_settings))
        .route("/api/instances", post(add_instance))
        .route("/api/instances/{id}", delete(delete_instance))
        .route("/api/instances/{id}/active", post(toggle_active))
        .route("/api/prompts", post(add_prompt))
        .route("/api/prompts/{id}", delete(delete_prompt))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    let app = Router::new()
        .route("/", get(index))
        .route("/app.js", get(app_js))
        .route("/styles.css", get(styles_css))
        .route("/api/version", get(version))
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        .merge(protected)
        .with_state(state);

    let bind = std::env::var("ODOO_MCP_UI_BIND").unwrap_or_else(|_| "127.0.0.1:3333".into());
    let listener = tokio::net::TcpListener::bind(&bind).await.unwrap();
    eprintln!("Web UI started on http://localhost:3333");
    axum::serve(listener, app).await.unwrap();
}

fn session_token(request: &Request) -> Option<&str> {
    request
        .headers()
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .map(str::trim)
        .find_map(|cookie| cookie.strip_prefix("odoo_mcp_session="))
}

async fn require_auth(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let authenticated =
        session_token(&request).is_some_and(|token| state.sessions.read().unwrap().contains(token));
    if authenticated {
        next.run(request).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

async fn login(State(state): State<AppState>, Json(credentials): Json<LoginRequest>) -> Response {
    if credentials.username != *state.username || credentials.password != *state.password {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Invalid credentials"})),
        )
            .into_response();
    }
    let token = Uuid::new_v4().to_string();
    state.sessions.write().unwrap().insert(token.clone());
    let cookie = format!("odoo_mcp_session={token}; HttpOnly; SameSite=Strict; Path=/");
    (
        StatusCode::OK,
        [(header::SET_COOKIE, cookie)],
        Json(json!({"ok": true})),
    )
        .into_response()
}

async fn logout(State(state): State<AppState>, request: Request) -> Response {
    if let Some(token) = session_token(&request) {
        state.sessions.write().unwrap().remove(token);
    }
    (
        StatusCode::OK,
        [(
            header::SET_COOKIE,
            "odoo_mcp_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0",
        )],
    )
        .into_response()
}

async fn update_global_settings(
    State(state): State<AppState>,
    Json(settings): Json<crate::config::GlobalSettings>,
) -> StatusCode {
    let mut config = state.config.write().unwrap();
    config.global_settings = settings;
    config.save().unwrap();
    StatusCode::OK
}

async fn toggle_active(State(state): State<AppState>, Path(id): Path<String>) -> StatusCode {
    let mut config = state.config.write().unwrap();
    config.toggle_active_instance(&id);
    config.save().unwrap();
    StatusCode::OK
}

async fn index() -> Html<&'static str> {
    Html(include_str!("index.html"))
}

async fn app_js() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("app.js"),
    )
}

async fn styles_css() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("styles.css"),
    )
}

async fn version() -> Json<Value> {
    Json(json!({ "version": env!("CARGO_PKG_VERSION") }))
}

async fn get_config(State(state): State<AppState>) -> Json<Value> {
    let config = state.config.read().unwrap();
    let mut value = serde_json::to_value(&*config).unwrap();
    if let Some(instances) = value["instances"].as_array_mut() {
        for instance in instances {
            let has_password = instance["password"]
                .as_str()
                .is_some_and(|value| !value.is_empty());
            instance["password"] = Value::String(String::new());
            instance["has_password"] = Value::Bool(has_password);
        }
    }
    Json(value)
}

async fn add_instance(
    State(state): State<AppState>,
    Json(mut instance): Json<OdooInstance>,
) -> StatusCode {
    if instance.id.is_empty() {
        instance.id = generate_id();
    }
    let mut config = state.config.write().unwrap();

    // If first instance, make it active
    if config.instances.is_empty() {
        instance.active = true;
    }

    // Update existing or add new
    if let Some(existing) = config.instances.iter_mut().find(|i| i.id == instance.id) {
        if instance.password.is_empty() {
            instance.password = existing.password.clone();
        }
        *existing = instance;
    } else {
        if instance.password.is_empty() {
            return StatusCode::BAD_REQUEST;
        }
        config.instances.push(instance);
    }

    config.save().unwrap();
    StatusCode::OK
}

async fn delete_instance(State(state): State<AppState>, Path(id): Path<String>) -> StatusCode {
    let mut config = state.config.write().unwrap();
    config.instances.retain(|i| i.id != id);
    config.save().unwrap();
    StatusCode::OK
}

async fn add_prompt(
    State(state): State<AppState>,
    Json(mut prompt): Json<OdooPrompt>,
) -> StatusCode {
    if prompt.id.is_empty() {
        prompt.id = generate_id();
    }
    let mut config = state.config.write().unwrap();

    if let Some(existing) = config.prompts.iter_mut().find(|p| p.id == prompt.id) {
        *existing = prompt;
    } else {
        config.prompts.push(prompt);
    }

    config.save().unwrap();
    StatusCode::OK
}

async fn delete_prompt(State(state): State<AppState>, Path(id): Path<String>) -> StatusCode {
    let mut config = state.config.write().unwrap();
    config.prompts.retain(|p| p.id != id);
    config.save().unwrap();
    StatusCode::OK
}
