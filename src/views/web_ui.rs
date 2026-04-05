use axum::{
    extract::{Path, State},
    http::{StatusCode},
    response::{Html},
    routing::{get, post, delete},
    Json, Router,
};
use std::sync::{Arc, RwLock};
use crate::config::{Config, OdooInstance, OdooPrompt, generate_id};
// use serde_json::json;
use tower_http::cors::CorsLayer;

type SharedConfig = Arc<RwLock<Config>>;

pub async fn start_ui(config: SharedConfig) {
    let app = Router::new()
        .route("/", get(index))
        .route("/api/config", get(get_config))
        .route("/api/instances", post(add_instance))
        .route("/api/instances/{id}", delete(delete_instance))
        .route("/api/instances/{id}/active", post(set_active))
        .route("/api/prompts", post(add_prompt))
        .route("/api/prompts/{id}", delete(delete_prompt))
        .layer(CorsLayer::permissive())
        .with_state(config);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3333").await.unwrap();
    eprintln!("Web UI started on http://localhost:3333");
    axum::serve(listener, app).await.unwrap();
}

async fn index() -> Html<&'static str> {
    Html(include_str!("index.html"))
}

async fn get_config(State(state): State<SharedConfig>) -> Json<Config> {
    let config = state.read().unwrap();
    Json(config.clone())
}

async fn add_instance(State(state): State<SharedConfig>, Json(mut instance): Json<OdooInstance>) -> StatusCode {
    if instance.id.is_empty() {
        instance.id = generate_id();
    }
    let mut config = state.write().unwrap();
    
    // If first instance, make it active
    if config.instances.is_empty() {
        instance.active = true;
    }

    // Update existing or add new
    if let Some(existing) = config.instances.iter_mut().find(|i| i.id == instance.id) {
        *existing = instance;
    } else {
        config.instances.push(instance);
    }
    
    config.save().unwrap();
    StatusCode::OK
}

async fn delete_instance(State(state): State<SharedConfig>, Path(id): Path<String>) -> StatusCode {
    let mut config = state.write().unwrap();
    config.instances.retain(|i| i.id != id);
    config.save().unwrap();
    StatusCode::OK
}

async fn set_active(State(state): State<SharedConfig>, Path(id): Path<String>) -> StatusCode {
    let mut config = state.write().unwrap();
    config.set_active_instance(&id);
    config.save().unwrap();
    StatusCode::OK
}

async fn add_prompt(State(state): State<SharedConfig>, Json(mut prompt): Json<OdooPrompt>) -> StatusCode {
    if prompt.id.is_empty() {
        prompt.id = generate_id();
    }
    let mut config = state.write().unwrap();
    
    if let Some(existing) = config.prompts.iter_mut().find(|p| p.id == prompt.id) {
        *existing = prompt;
    } else {
        config.prompts.push(prompt);
    }
    
    config.save().unwrap();
    StatusCode::OK
}

async fn delete_prompt(State(state): State<SharedConfig>, Path(id): Path<String>) -> StatusCode {
    let mut config = state.write().unwrap();
    config.prompts.retain(|p| p.id != id);
    config.save().unwrap();
    StatusCode::OK
}
