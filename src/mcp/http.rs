use super::server::McpCore;
use crate::mcp::McpRuntime;
use crate::observability;
use crate::odoo::ClientManager;
use crate::policy::Capability;
use crate::secret::{EnvironmentSecretProvider, SecretProvider, SecretReference};
use crate::tools::catalog::ToolName;
use crate::tools::result::ToolExecutionResult;
use axum::{
    Router,
    extract::{DefaultBodyLimit, Extension, Request, State},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::post,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::config::{Config, OAuthSettings, RbacSettings};

type SharedConfig = Arc<RwLock<Config>>;

const SESSION_HEADER: &str = "mcp-session-id";

#[derive(Clone, Debug, Eq, PartialEq)]
struct Principal {
    hash: [u8; 32],
    subject: Option<String>,
}

#[derive(Clone)]
struct RemoteState {
    config: SharedConfig,
    client_manager: ClientManager,
    runtime: McpRuntime,
    require_https: bool,
    expected_api_key_hashes: Vec<[u8; 32]>,
    jwt_hs256_secret: Option<Vec<u8>>,
    oauth: Option<Arc<OAuthValidator>>,
    rbac: Option<Arc<RbacPolicy>>,
    rate_limiter: Arc<RateLimiter>,
    sessions: Arc<Mutex<SessionStore>>,
}

struct RbacPolicy {
    default_role: Option<String>,
    roles: std::collections::BTreeMap<String, crate::config::RbacRole>,
    principals: std::collections::BTreeMap<String, Vec<String>>,
    api_key_subjects: HashMap<[u8; 32], String>,
    tenant_by_subject: std::collections::BTreeMap<String, String>,
    instances_by_subject: std::collections::BTreeMap<String, Vec<String>>,
}

impl RbacPolicy {
    fn new(settings: RbacSettings, api_key_subjects: HashMap<[u8; 32], String>) -> Self {
        Self {
            default_role: settings.default_role,
            roles: settings.roles,
            principals: settings.principals,
            api_key_subjects,
            tenant_by_subject: settings.principal_tenants,
            instances_by_subject: settings.principal_instances,
        }
    }

    fn subject_for_api_key(&self, hash: &[u8; 32]) -> Option<&str> {
        self.api_key_subjects.get(hash).map(String::as_str)
    }

    fn has_scopes(&self) -> bool {
        !self.tenant_by_subject.is_empty() || !self.instances_by_subject.is_empty()
    }

    fn instance_allowed(
        &self,
        subject: Option<&str>,
        instance_id: &str,
        instance_name: &str,
        instance_tenant: &str,
    ) -> bool {
        if !self.has_scopes() {
            return true;
        }
        let Some(subject) = subject else {
            return false;
        };
        if !self.tenant_by_subject.is_empty() {
            let Some(tenant) = self.tenant_by_subject.get(subject) else {
                return false;
            };
            if tenant != instance_tenant {
                return false;
            }
        }
        if !self.instances_by_subject.is_empty() {
            let Some(instances) = self.instances_by_subject.get(subject) else {
                return false;
            };
            if !instances.iter().any(|instance| {
                instance == instance_id || instance.eq_ignore_ascii_case(instance_name)
            }) {
                return false;
            }
        }
        true
    }

    fn authorize(&self, subject: Option<&str>, capability: Capability) -> bool {
        let roles = subject
            .and_then(|subject| self.principals.get(subject))
            .map(|roles| roles.as_slice());
        let roles = match roles {
            Some(roles) if !roles.is_empty() => roles,
            _ => match &self.default_role {
                Some(role) => std::slice::from_ref(role),
                None => return false,
            },
        };
        let mut allowed = false;
        for role in roles {
            let Some(role) = self.roles.get(role) else {
                continue;
            };
            if role.deny.contains(&capability) {
                return false;
            }
            if role.allow.contains(&capability) {
                allowed = true;
            }
        }
        allowed
    }
}

struct OAuthValidator {
    issuer: String,
    audience: String,
    jwks: JwkSet,
}

impl OAuthValidator {
    fn validate(&self, token: &str) -> Option<Principal> {
        let header = decode_header(token).ok()?;
        if !matches!(
            header.alg,
            Algorithm::RS256
                | Algorithm::RS384
                | Algorithm::RS512
                | Algorithm::ES256
                | Algorithm::ES384
        ) {
            return None;
        }
        let kid = header.kid.as_deref()?;
        let jwk = self.jwks.find(kid)?;
        let key = DecodingKey::from_jwk(jwk).ok()?;
        let mut validation = Validation::new(header.alg);
        validation.set_audience(&[&self.audience]);
        validation.set_issuer(&[&self.issuer]);
        validation.leeway = JWT_LEEWAY_SECS as u64;
        let claims = decode::<Value>(token, &key, &validation).ok()?.claims;
        let subject = claims.get("sub")?.as_str()?.trim();
        if subject.is_empty() {
            return None;
        }
        let issuer = claims
            .get("iss")
            .and_then(Value::as_str)
            .unwrap_or(&self.issuer);
        Some(Principal {
            hash: Sha256::digest(format!("oauth:{issuer}:{subject}").as_bytes()).into(),
            subject: Some(format!("{issuer}:{subject}")),
        })
    }
}

async fn fetch_oauth_validator(settings: &OAuthSettings) -> Result<OAuthValidator, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|error| format!("failed to build JWKS client: {error}"))?;
    let response = client
        .get(&settings.jwks_url)
        .send()
        .await
        .map_err(|error| format!("JWKS request failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("JWKS request returned HTTP {}", response.status()));
    }
    let jwks = response
        .json::<JwkSet>()
        .await
        .map_err(|error| format!("invalid JWKS document: {error}"))?;
    if jwks.keys.is_empty() {
        return Err("JWKS document contains no keys".to_string());
    }
    Ok(OAuthValidator {
        issuer: settings.issuer.clone(),
        audience: settings.audience.clone(),
        jwks,
    })
}

struct SessionStore {
    entries: HashMap<String, SessionEntry>,
    max_sessions: usize,
    ttl: Duration,
}

struct SessionEntry {
    core: Arc<Mutex<McpCore>>,
    principal: [u8; 32],
    last_used: Instant,
}

impl SessionStore {
    fn new(max_sessions: usize, ttl: Duration) -> Self {
        Self {
            entries: HashMap::new(),
            max_sessions,
            ttl,
        }
    }

    fn prune(&mut self, now: Instant) {
        self.entries
            .retain(|_, entry| now.duration_since(entry.last_used) < self.ttl);
    }

    fn create(
        &mut self,
        core: Arc<Mutex<McpCore>>,
        principal: [u8; 32],
        now: Instant,
    ) -> Result<String, &'static str> {
        self.prune(now);
        if self.entries.len() >= self.max_sessions {
            return Err("maximum remote MCP sessions reached");
        }
        let id = Uuid::new_v4().to_string();
        self.entries.insert(
            id.clone(),
            SessionEntry {
                core,
                principal,
                last_used: now,
            },
        );
        Ok(id)
    }

    fn get(&mut self, id: &str, principal: [u8; 32], now: Instant) -> Option<Arc<Mutex<McpCore>>> {
        self.prune(now);
        let entry = self.entries.get_mut(id)?;
        if entry.principal != principal {
            return None;
        }
        entry.last_used = now;
        Some(entry.core.clone())
    }

    fn remove(&mut self, id: &str) -> bool {
        self.entries.remove(id).is_some()
    }
}

struct RateLimiter {
    limit: u32,
    window: Duration,
    entries: Mutex<HashMap<[u8; 32], RateWindow>>,
}

struct RateWindow {
    started: Instant,
    count: u32,
}

impl RateLimiter {
    fn new(limit: u32, window: Duration) -> Self {
        Self {
            limit,
            window,
            entries: Mutex::new(HashMap::new()),
        }
    }

    async fn check(&self, principal: [u8; 32]) -> Result<(), Duration> {
        self.check_at(principal, Instant::now()).await
    }

    async fn check_at(&self, principal: [u8; 32], now: Instant) -> Result<(), Duration> {
        let mut entries = self.entries.lock().await;
        if entries.len() > 4_096 {
            entries.retain(|_, window| now.duration_since(window.started) < self.window);
        }
        let window = entries.entry(principal).or_insert(RateWindow {
            started: now,
            count: 0,
        });
        if now.duration_since(window.started) >= self.window {
            *window = RateWindow {
                started: now,
                count: 0,
            };
        }
        if window.count >= self.limit {
            return Err(self
                .window
                .saturating_sub(now.duration_since(window.started)));
        }
        window.count += 1;
        Ok(())
    }
}

pub(crate) async fn start_remote_mcp(
    config: SharedConfig,
    client_manager: ClientManager,
    runtime: McpRuntime,
) {
    let (
        bind_address,
        api_key_envs,
        jwt_hs256_secret_env,
        oauth_settings,
        rbac_settings,
        rate_limit_per_minute,
        max_request_bytes,
        max_sessions,
        session_ttl_secs,
    ) = {
        let config = config.read().unwrap();
        let settings = &config.global_settings.remote_mcp;
        let mut names = settings.api_key_envs.clone();
        if let Some(api_key_env) = &settings.api_key_env {
            names.push(api_key_env.clone());
        }
        (
            settings.bind_address.clone(),
            names,
            settings.jwt_hs256_secret_env.clone().unwrap_or_default(),
            settings.oauth.clone(),
            settings.rbac.clone(),
            settings.rate_limit_per_minute,
            settings.max_request_bytes,
            settings.max_sessions,
            settings.session_ttl_secs,
        )
    };
    let require_https =
        !bind_address.starts_with("127.0.0.1:") && !bind_address.starts_with("[::1]:");
    let provider = EnvironmentSecretProvider::new();
    let mut expected_api_key_hashes = Vec::with_capacity(api_key_envs.len());
    let mut api_key_subjects = HashMap::new();
    for api_key_env in &api_key_envs {
        let api_key = match provider.resolve(&SecretReference::new(api_key_env)) {
            Ok(Some(secret)) if !secret.expose_secret().is_empty() => secret,
            Ok(_) => {
                eprintln!(
                    "Remote MCP transport disabled: environment variable {api_key_env:?} has no API key"
                );
                return;
            }
            Err(error) => {
                eprintln!("Remote MCP transport disabled: {error}");
                return;
            }
        };
        let hash: [u8; 32] = Sha256::digest(api_key.expose_secret().as_bytes()).into();
        if let Some(rbac) = &rbac_settings
            && let Some(subject) = rbac.api_key_subjects.get(api_key_env)
        {
            api_key_subjects.insert(hash, subject.clone());
        }
        expected_api_key_hashes.push(hash);
    }
    let jwt_hs256_secret = if jwt_hs256_secret_env.trim().is_empty() {
        None
    } else {
        match provider.resolve(&SecretReference::new(&jwt_hs256_secret_env)) {
            Ok(Some(secret)) if !secret.expose_secret().is_empty() => {
                Some(secret.expose_secret().as_bytes().to_vec())
            }
            Ok(_) => {
                eprintln!(
                    "Remote MCP transport disabled: environment variable {jwt_hs256_secret_env:?} has no JWT secret"
                );
                return;
            }
            Err(error) => {
                eprintln!("Remote MCP transport disabled: {error}");
                return;
            }
        }
    };
    let oauth = match oauth_settings {
        Some(settings) => match fetch_oauth_validator(&settings).await {
            Ok(validator) => Some(Arc::new(validator)),
            Err(error) => {
                eprintln!("Remote MCP transport disabled: OAuth JWKS unavailable: {error}");
                return;
            }
        },
        None => None,
    };
    let rbac = rbac_settings.map(|settings| Arc::new(RbacPolicy::new(settings, api_key_subjects)));
    let state = RemoteState {
        config,
        client_manager,
        runtime,
        require_https,
        expected_api_key_hashes,
        jwt_hs256_secret,
        oauth,
        rbac,
        rate_limiter: Arc::new(RateLimiter::new(
            rate_limit_per_minute,
            Duration::from_secs(60),
        )),
        sessions: Arc::new(Mutex::new(SessionStore::new(
            max_sessions,
            Duration::from_secs(session_ttl_secs),
        ))),
    };

    let app = Router::new()
        .route(
            "/mcp",
            post(handle_mcp_request).delete(handle_session_delete),
        )
        .layer(DefaultBodyLimit::max(max_request_bytes))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            authorize_remote,
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

async fn authorize_remote(
    State(state): State<RemoteState>,
    request: Request,
    next: Next,
) -> Response {
    if state.require_https && !request_is_https(&request) {
        return JsonRpcErrorBody::parse_error("HTTPS required for remote MCP").into_response();
    }
    let Some(mut principal) = authenticate_request(
        &request,
        &state.expected_api_key_hashes,
        state.jwt_hs256_secret.as_deref(),
        state.oauth.as_deref(),
    ) else {
        return unauthorized_response();
    };
    if principal.subject.is_none()
        && let Some(rbac) = &state.rbac
    {
        principal.subject = rbac
            .subject_for_api_key(&principal.hash)
            .map(str::to_string);
    }
    if let Err(retry_after) = state.rate_limiter.check(principal.hash).await {
        return rate_limited_response(retry_after);
    }
    let mut request = request;
    request.extensions_mut().insert(principal);
    next.run(request).await
}

fn authenticate_request(
    request: &Request,
    expected_api_keys: &[[u8; 32]],
    jwt_secret: Option<&[u8]>,
    oauth: Option<&OAuthValidator>,
) -> Option<Principal> {
    let token = bearer_token(request)?;
    if token.split('.').count() == 3 {
        if let Some(secret) = jwt_secret
            && let Some(principal) = validate_jwt(token, secret)
        {
            return Some(principal);
        }
        if let Some(oauth) = oauth
            && let Some(principal) = oauth.validate(token)
        {
            return Some(principal);
        }
    }
    let api_key_hash: [u8; 32] = Sha256::digest(token.as_bytes()).into();
    expected_api_keys
        .iter()
        .any(|expected| constant_time_eq(&api_key_hash, expected))
        .then_some(Principal {
            hash: api_key_hash,
            subject: None,
        })
}

fn bearer_token(request: &Request) -> Option<&str> {
    request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value
                .strip_prefix("Bearer ")
                .or_else(|| value.strip_prefix("bearer "))
        })
}

const JWT_LEEWAY_SECS: i64 = 60;

fn validate_jwt(token: &str, secret: &[u8]) -> Option<Principal> {
    let mut parts = token.split('.');
    let encoded_header = parts.next()?;
    let encoded_payload = parts.next()?;
    let signature = parts.next()?;
    if parts.next().is_some() {
        return None;
    }

    let header: Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(encoded_header).ok()?).ok()?;
    if header.get("alg").and_then(Value::as_str) != Some("HS256") {
        return None;
    }

    let signing_input = format!("{encoded_header}.{encoded_payload}");
    let expected_signature = hmac_sha256(secret, signing_input.as_bytes());
    let provided_signature = URL_SAFE_NO_PAD.decode(signature).ok()?;
    if !constant_time_eq(&provided_signature, &expected_signature) {
        return None;
    }

    let payload: Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(encoded_payload).ok()?).ok()?;
    let now = Utc::now().timestamp();
    let exp = numeric_date(payload.get("exp")?)?;
    if now > exp.saturating_add(JWT_LEEWAY_SECS) {
        return None;
    }
    if let Some(nbf) = payload.get("nbf").and_then(numeric_date)
        && now.saturating_add(JWT_LEEWAY_SECS) < nbf
    {
        return None;
    }
    let subject = payload.get("sub")?.as_str()?;
    if subject.trim().is_empty() {
        return None;
    }
    let issuer = payload
        .get("iss")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Some(Principal {
        hash: Sha256::digest(format!("jwt:{issuer}:{subject}").as_bytes()).into(),
        subject: Some(format!("{issuer}:{}", subject.trim())),
    })
}

fn numeric_date(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_f64().map(|value| value as i64))
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut key_block = [0u8; 64];
    if key.len() > key_block.len() {
        key_block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }

    let mut inner = Sha256::new();
    inner.update(key_block.map(|byte| byte ^ 0x36));
    inner.update(message);
    let inner = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(key_block.map(|byte| byte ^ 0x5c));
    outer.update(inner);
    outer.finalize().into()
}

fn unauthorized_response() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(
            header::WWW_AUTHENTICATE,
            header::HeaderValue::from_static("Bearer"),
        )],
        JsonRpcErrorBody::unauthorized().0.to_string(),
    )
        .into_response()
}

fn rate_limited_response(retry_after: Duration) -> Response {
    let retry_after_secs = retry_after.as_secs().max(1).to_string();
    (
        StatusCode::TOO_MANY_REQUESTS,
        [
            (header::RETRY_AFTER, retry_after_secs),
            (header::CONTENT_TYPE, "application/json".to_string()),
        ],
        JsonRpcErrorBody::rate_limited().0.to_string(),
    )
        .into_response()
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0u8, |difference, (left, right)| difference | (left ^ right))
        == 0
}

fn request_is_https(request: &Request) -> bool {
    request.uri().scheme_str() == Some("https")
        || request
            .headers()
            .get(header::HeaderName::from_static("x-forwarded-proto"))
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.eq_ignore_ascii_case("https"))
}

async fn handle_mcp_request(
    State(state): State<RemoteState>,
    Extension(principal): Extension<Principal>,
    headers: HeaderMap,
    body: String,
) -> Response {
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

    if method == "tools/call"
        && let (Some(rbac), Some(tool_name)) = (
            &state.rbac,
            tool.as_deref()
                .and_then(|name| ToolName::try_from(name).ok()),
        )
    {
        if !rbac.authorize(principal.subject.as_deref(), tool_name.capability()) {
            observability::metrics().record_request(true);
            observability::metrics().record_tool_call(true);
            return JsonRpcErrorBody::forbidden().into_response_with_status(StatusCode::FORBIDDEN);
        }
        if tool_name == ToolName::ListInstances {
            let revision = state.runtime.revision();
            let instances = {
                let config = state.config.read().unwrap();
                filtered_instances(&config, principal.subject.as_deref(), rbac)
            };
            let mut result = ToolExecutionResult::Success(json!({
                "revision": revision,
                "instances": instances
            }))
            .into_mcp_result();
            result["_meta"] = json!({"configRevision": revision});
            return json_rpc_response(Some(json!({
                "jsonrpc": "2.0",
                "id": request.get("id").cloned().unwrap_or(Value::Null),
                "result": result
            })));
        }
        if tool_name.requires_client() {
            let instance_target = request
                .pointer("/params/arguments/instance")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let instance = {
                let config = state.config.read().unwrap();
                config.find_instance(instance_target).map(|instance| {
                    (
                        instance.id.clone(),
                        instance.name.clone(),
                        instance.tenant_id().to_string(),
                    )
                })
            };
            if let Some((id, name, tenant)) = instance
                && !rbac.instance_allowed(principal.subject.as_deref(), &id, &name, &tenant)
            {
                observability::metrics().record_request(true);
                observability::metrics().record_tool_call(true);
                return JsonRpcErrorBody::forbidden()
                    .into_response_with_status(StatusCode::FORBIDDEN);
            }
        }
    }

    let (core, session_id) =
        if method == "initialize" {
            let core = Arc::new(Mutex::new(McpCore::new(
                state.config.clone(),
                state.client_manager.clone(),
                state.runtime.clone(),
            )));
            let session_id = match state.sessions.lock().await.create(
                core.clone(),
                principal.hash,
                Instant::now(),
            ) {
                Ok(session_id) => session_id,
                Err(message) => {
                    observability::metrics().record_request(true);
                    return JsonRpcErrorBody::session_error(message)
                        .into_response_with_status(StatusCode::SERVICE_UNAVAILABLE);
                }
            };
            (core, Some(session_id))
        } else {
            let Some(session_id) = session_header(&headers) else {
                observability::metrics().record_request(true);
                return JsonRpcErrorBody::session_error("Mcp-Session-Id header is required")
                    .into_response_with_status(StatusCode::BAD_REQUEST);
            };
            let core = state
                .sessions
                .lock()
                .await
                .get(session_id, principal.hash, Instant::now());
            let Some(core) = core else {
                observability::metrics().record_request(true);
                return JsonRpcErrorBody::session_error("Unknown or expired MCP session")
                    .into_response_with_status(StatusCode::NOT_FOUND);
            };
            (core, None)
        };

    let response = core.lock().await.handle_request(request).await;
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

    let mut response = json_rpc_response(response);
    if let Some(session_id) = session_id {
        insert_session_header(&mut response, &session_id);
    }
    response
}

async fn handle_session_delete(
    State(state): State<RemoteState>,
    Extension(principal): Extension<Principal>,
    headers: HeaderMap,
) -> Response {
    let Some(session_id) = session_header(&headers) else {
        return JsonRpcErrorBody::session_error("Mcp-Session-Id header is required")
            .into_response_with_status(StatusCode::BAD_REQUEST);
    };
    let removed = {
        let mut sessions = state.sessions.lock().await;
        sessions
            .get(session_id, principal.hash, Instant::now())
            .is_some()
            && sessions.remove(session_id)
    };
    if removed {
        StatusCode::NO_CONTENT.into_response()
    } else {
        JsonRpcErrorBody::session_error("Unknown or expired MCP session")
            .into_response_with_status(StatusCode::NOT_FOUND)
    }
}

fn filtered_instances(config: &Config, subject: Option<&str>, rbac: &RbacPolicy) -> Vec<Value> {
    config
        .instances
        .iter()
        .filter(|instance| {
            rbac.instance_allowed(subject, &instance.id, &instance.name, instance.tenant_id())
        })
        .map(|instance| {
            let folder = instance
                .folder_id
                .as_ref()
                .and_then(|folder_id| config.folders.iter().find(|folder| folder.id == *folder_id))
                .map(|folder| json!({"id": folder.id, "name": folder.name}));
            json!({
                "id": instance.id,
                "name": instance.name,
                "tenant_id": instance.tenant_id(),
                "active": instance.active,
                "mode": instance.get_mode(&config.global_settings.default_mode),
                "rpc_transport": instance.rpc_transport.as_str(),
                "folder": folder,
                "disabled_tools": instance.disabled_tools.clone().unwrap_or_default(),
                "has_credentials": !instance.password.is_empty()
                    || instance.password_env.is_some()
            })
        })
        .collect()
}

fn session_header(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(SESSION_HEADER)
        .and_then(|value| value.to_str().ok())
}

fn insert_session_header(response: &mut Response, session_id: &str) {
    if let Ok(value) = HeaderValue::from_str(session_id) {
        response
            .headers_mut()
            .insert(HeaderName::from_static(SESSION_HEADER), value);
    }
}

fn json_rpc_response(response: Option<Value>) -> Response {
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

    fn unauthorized() -> Self {
        Self(json!({
            "jsonrpc": "2.0",
            "error": {"code": -32001, "message": "Unauthorized"},
            "id": Value::Null
        }))
    }

    fn rate_limited() -> Self {
        Self(json!({
            "jsonrpc": "2.0",
            "error": {"code": -32002, "message": "Rate limit exceeded"},
            "id": Value::Null
        }))
    }

    fn forbidden() -> Self {
        Self(json!({
            "jsonrpc": "2.0",
            "error": {"code": -32004, "message": "Forbidden"},
            "id": Value::Null
        }))
    }

    fn session_error(message: &str) -> Self {
        Self(json!({
            "jsonrpc": "2.0",
            "error": {"code": -32003, "message": message},
            "id": Value::Null
        }))
    }

    fn into_response(self) -> Response {
        self.into_response_with_status(StatusCode::BAD_REQUEST)
    }

    fn into_response_with_status(self, status: StatusCode) -> Response {
        (
            status,
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

    async fn handle(state: RemoteState, headers: HeaderMap, body: String) -> Response {
        handle_mcp_request(
            State(state),
            Extension(Principal {
                hash: [0u8; 32],
                subject: None,
            }),
            headers,
            body,
        )
        .await
    }

    async fn delete_session(state: RemoteState, headers: HeaderMap) -> Response {
        handle_session_delete(
            State(state),
            Extension(Principal {
                hash: [0u8; 32],
                subject: None,
            }),
            headers,
        )
        .await
    }

    fn remote_state(max_sessions: usize, ttl: Duration) -> RemoteState {
        let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel();
        RemoteState {
            config: multi_instance_config(),
            client_manager: ClientManager::new(),
            runtime: McpRuntime::new(sender),
            require_https: false,
            expected_api_key_hashes: vec![Sha256::digest(b"valid-api-key").into()],
            jwt_hs256_secret: None,
            oauth: None,
            rbac: None,
            rate_limiter: Arc::new(RateLimiter::new(120, Duration::from_secs(60))),
            sessions: Arc::new(Mutex::new(SessionStore::new(max_sessions, ttl))),
        }
    }

    fn session_headers(session_id: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static(SESSION_HEADER),
            HeaderValue::from_str(session_id).unwrap(),
        );
        headers
    }

    async fn initialize_session(state: &RemoteState) -> String {
        let response = handle(
            state.clone(),
            HeaderMap::new(),
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize"
            })
            .to_string(),
        )
        .await;
        response
            .headers()
            .get(SESSION_HEADER)
            .and_then(|value| value.to_str().ok())
            .expect("initialize should issue a session")
            .to_string()
    }

    #[tokio::test]
    async fn http_transport_returns_json_responses_for_requests() {
        let state = remote_state(8, Duration::from_secs(60));
        let response = handle(
            state,
            HeaderMap::new(),
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize"
            })
            .to_string(),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.headers().get(SESSION_HEADER).is_some());
        let body = response_body(response).await;
        assert_eq!(body["result"]["protocolVersion"], "2025-11-25");
    }

    #[tokio::test]
    async fn http_transport_accepts_notifications_without_a_response() {
        let state = remote_state(8, Duration::from_secs(60));
        let session_id = initialize_session(&state).await;

        let response = handle(
            state,
            session_headers(&session_id),
            json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized"
            })
            .to_string(),
        )
        .await;

        assert_eq!(response.status(), StatusCode::ACCEPTED);
    }

    #[tokio::test]
    async fn http_transport_rejects_missing_unknown_and_terminated_sessions() {
        let state = remote_state(8, Duration::from_secs(60));
        let missing = handle(
            state.clone(),
            HeaderMap::new(),
            json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}).to_string(),
        )
        .await;
        assert_eq!(missing.status(), StatusCode::BAD_REQUEST);

        let unknown = handle(
            state,
            session_headers("unknown-session"),
            json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}).to_string(),
        )
        .await;
        assert_eq!(unknown.status(), StatusCode::NOT_FOUND);

        let state = remote_state(8, Duration::from_secs(60));
        let session_id = initialize_session(&state).await;
        let deleted = delete_session(state.clone(), session_headers(&session_id)).await;
        assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

        let terminated = handle(
            state,
            session_headers(&session_id),
            json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}).to_string(),
        )
        .await;
        assert_eq!(terminated.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn sessions_are_bound_to_the_authenticated_principal() {
        let state = remote_state(8, Duration::from_secs(60));
        let init = handle_mcp_request(
            State(state.clone()),
            Extension(Principal {
                hash: [1u8; 32],
                subject: Some("issuer:user-1".to_string()),
            }),
            HeaderMap::new(),
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"}).to_string(),
        )
        .await;
        let session_id = init
            .headers()
            .get(SESSION_HEADER)
            .and_then(|value| value.to_str().ok())
            .unwrap()
            .to_string();

        let response = handle_mcp_request(
            State(state),
            Extension(Principal {
                hash: [2u8; 32],
                subject: Some("issuer:user-2".to_string()),
            }),
            session_headers(&session_id),
            json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}).to_string(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn http_transport_enforces_maximum_sessions() {
        let state = remote_state(1, Duration::from_secs(60));
        initialize_session(&state).await;

        let response = handle(
            state,
            HeaderMap::new(),
            json!({"jsonrpc": "2.0", "id": 2, "method": "initialize"}).to_string(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn session_store_expires_idle_sessions() {
        let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel();
        let core = Arc::new(Mutex::new(McpCore::new(
            multi_instance_config(),
            ClientManager::new(),
            McpRuntime::new(sender),
        )));
        let mut store = SessionStore::new(1, Duration::from_secs(60));
        let now = Instant::now();
        let session_id = store.create(core, [0u8; 32], now).unwrap();
        assert!(store.get(&session_id, [0u8; 32], now).is_some());
        assert!(
            store
                .get(&session_id, [0u8; 32], now + Duration::from_secs(61))
                .is_none()
        );
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

    #[test]
    fn api_keys_are_verified_with_constant_time_hash_comparison() {
        let valid: [u8; 32] = Sha256::digest(b"valid-api-key").into();
        let replacement: [u8; 32] = Sha256::digest(b"replacement-api-key").into();
        let rotated = vec![valid, replacement];
        assert_eq!(
            authenticate_request(&request_with_bearer("valid-api-key"), &rotated, None, None)
                .map(|principal| principal.hash),
            Some(valid)
        );
        assert_eq!(
            authenticate_request(
                &request_with_bearer("replacement-api-key"),
                &rotated,
                None,
                None
            )
            .map(|principal| principal.hash),
            Some(replacement)
        );
        assert!(
            authenticate_request(&request_with_bearer("wrong-api-key"), &rotated, None, None)
                .is_none()
        );
        assert!(
            authenticate_request(&request("http://gateway/mcp", None), &rotated, None, None)
                .is_none()
        );
    }

    #[test]
    fn jwt_validation_checks_signature_and_time_claims() {
        let now = Utc::now().timestamp();
        let secret = b"jwt-test-secret";
        let valid = test_jwt(secret, "user-42", now + 60, None);
        assert!(validate_jwt(&valid, secret).is_some());
        assert!(validate_jwt(&test_jwt(secret, "user-42", now - 120, None), secret).is_none());
        assert!(
            validate_jwt(
                &test_jwt(secret, "user-42", now + 60, Some(now + 120)),
                secret
            )
            .is_none()
        );
        assert!(
            validate_jwt(
                &test_jwt(b"wrong-secret", "user-42", now + 60, None),
                secret
            )
            .is_none()
        );
        assert!(validate_jwt("not.a.jwt", secret).is_none());
    }

    #[test]
    fn jwt_credentials_authenticate_requests() {
        let now = Utc::now().timestamp();
        let secret = b"jwt-test-secret";
        let token = test_jwt(secret, "user-42", now + 60, None);
        let principal = authenticate_request(&request_with_bearer(token), &[], Some(secret), None);
        assert!(principal.is_some());

        let expired = test_jwt(secret, "user-42", now - 120, None);
        assert!(
            authenticate_request(&request_with_bearer(expired), &[], Some(secret), None).is_none()
        );
    }

    #[test]
    fn oauth_rejects_symmetric_and_unknown_key_tokens() {
        let validator = OAuthValidator {
            issuer: "https://issuer.example.com".to_string(),
            audience: "odoo-mcp".to_string(),
            jwks: JwkSet { keys: Vec::new() },
        };
        let now = Utc::now().timestamp();
        let hs_token = test_jwt(b"secret", "user-42", now + 60, None);
        assert!(validator.validate(&hs_token).is_none());
        assert!(validator.validate("not.a.jwt").is_none());
    }

    #[test]
    fn rbac_roles_apply_allow_deny_and_default_roles() {
        let policy = RbacPolicy::new(
            RbacSettings {
                default_role: Some("viewer".to_string()),
                roles: std::collections::BTreeMap::from([
                    (
                        "viewer".to_string(),
                        crate::config::RbacRole {
                            allow: vec![Capability::Read],
                            deny: Vec::new(),
                        },
                    ),
                    (
                        "editor".to_string(),
                        crate::config::RbacRole {
                            allow: vec![Capability::Read, Capability::Delete],
                            deny: vec![Capability::Delete],
                        },
                    ),
                ]),
                principals: std::collections::BTreeMap::from([(
                    "issuer:editor".to_string(),
                    vec!["editor".to_string()],
                )]),
                api_key_subjects: std::collections::BTreeMap::new(),
                principal_tenants: std::collections::BTreeMap::new(),
                principal_instances: std::collections::BTreeMap::new(),
            },
            HashMap::new(),
        );

        assert!(policy.authorize(Some("issuer:editor"), Capability::Read));
        assert!(!policy.authorize(Some("issuer:editor"), Capability::Delete));
        assert!(policy.authorize(Some("issuer:unknown"), Capability::Read));
        assert!(!policy.authorize(Some("issuer:unknown"), Capability::Update));
    }

    #[test]
    fn rbac_enforces_tenant_and_instance_scopes() {
        let policy = RbacPolicy::new(
            RbacSettings {
                default_role: None,
                roles: std::collections::BTreeMap::new(),
                principals: std::collections::BTreeMap::new(),
                api_key_subjects: std::collections::BTreeMap::new(),
                principal_tenants: std::collections::BTreeMap::from([(
                    "issuer:tenant-a".to_string(),
                    "tenant-a".to_string(),
                )]),
                principal_instances: std::collections::BTreeMap::from([(
                    "issuer:tenant-a".to_string(),
                    vec!["1".to_string()],
                )]),
            },
            HashMap::new(),
        );

        assert!(policy.instance_allowed(Some("issuer:tenant-a"), "1", "Instance One", "tenant-a"));
        assert!(!policy.instance_allowed(Some("issuer:tenant-a"), "2", "Instance Two", "tenant-a"));
        assert!(!policy.instance_allowed(Some("issuer:tenant-a"), "1", "Instance One", "tenant-b"));
        assert!(!policy.instance_allowed(None, "1", "Instance One", "tenant-a"));
    }

    #[tokio::test]
    async fn http_transport_enforces_cross_tenant_instance_isolation() {
        let mut state = remote_state(8, Duration::from_secs(60));
        {
            let mut config = state.config.write().unwrap();
            config.tenants = vec![
                crate::config::Tenant {
                    id: "tenant-a".to_string(),
                    name: "Tenant A".to_string(),
                    secret_envs: Vec::new(),
                },
                crate::config::Tenant {
                    id: "tenant-b".to_string(),
                    name: "Tenant B".to_string(),
                    secret_envs: Vec::new(),
                },
            ];
            config.instances[0].tenant_id = Some("tenant-a".to_string());
            config.instances[1].tenant_id = Some("tenant-b".to_string());
        }
        state.rbac = Some(Arc::new(RbacPolicy::new(
            RbacSettings {
                default_role: None,
                roles: std::collections::BTreeMap::from([(
                    "viewer".to_string(),
                    crate::config::RbacRole {
                        allow: vec![Capability::Read],
                        deny: Vec::new(),
                    },
                )]),
                principals: std::collections::BTreeMap::from([(
                    "issuer:tenant-a".to_string(),
                    vec!["viewer".to_string()],
                )]),
                api_key_subjects: std::collections::BTreeMap::new(),
                principal_tenants: std::collections::BTreeMap::from([(
                    "issuer:tenant-a".to_string(),
                    "tenant-a".to_string(),
                )]),
                principal_instances: std::collections::BTreeMap::new(),
            },
            HashMap::new(),
        )));

        let response = handle_mcp_request(
            State(state),
            Extension(Principal {
                hash: [8u8; 32],
                subject: Some("issuer:tenant-a".to_string()),
            }),
            HeaderMap::new(),
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {
                    "name": "odoo-search",
                    "arguments": {"model": "res.partner", "instance": "2"}
                }
            })
            .to_string(),
        )
        .await;

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn http_transport_rejects_rbac_denied_tool_calls() {
        let mut state = remote_state(8, Duration::from_secs(60));
        state.rbac = Some(Arc::new(RbacPolicy::new(
            RbacSettings {
                default_role: None,
                roles: std::collections::BTreeMap::from([(
                    "viewer".to_string(),
                    crate::config::RbacRole {
                        allow: vec![Capability::Read],
                        deny: Vec::new(),
                    },
                )]),
                principals: std::collections::BTreeMap::from([(
                    "issuer:viewer".to_string(),
                    vec!["viewer".to_string()],
                )]),
                api_key_subjects: std::collections::BTreeMap::new(),
                principal_tenants: std::collections::BTreeMap::new(),
                principal_instances: std::collections::BTreeMap::new(),
            },
            HashMap::new(),
        )));

        let response = handle_mcp_request(
            State(state),
            Extension(Principal {
                hash: [7u8; 32],
                subject: Some("issuer:viewer".to_string()),
            }),
            HeaderMap::new(),
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {"name": "odoo-delete", "arguments": {}}
            })
            .to_string(),
        )
        .await;

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn hmac_sha256_matches_known_vector() {
        let digest = hmac_sha256(b"key", b"The quick brown fox jumps over the lazy dog");
        assert_eq!(
            digest
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
        );
    }

    fn test_jwt(secret: &[u8], subject: &str, exp: i64, nbf: Option<i64>) -> String {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
        let mut payload = json!({
            "sub": subject,
            "iss": "test-issuer",
            "exp": exp
        });
        if let Some(nbf) = nbf {
            payload["nbf"] = json!(nbf);
        }
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap());
        let signing_input = format!("{header}.{payload}");
        let signature = URL_SAFE_NO_PAD.encode(hmac_sha256(secret, signing_input.as_bytes()));
        format!("{signing_input}.{signature}")
    }

    #[test]
    fn auth_and_rate_limit_responses_are_well_formed() {
        let unauthorized = unauthorized_response();
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            unauthorized
                .headers()
                .get(header::WWW_AUTHENTICATE)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer")
        );

        let limited = rate_limited_response(Duration::from_secs(3));
        assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            limited
                .headers()
                .get(header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok()),
            Some("3")
        );
    }

    #[tokio::test]
    async fn rate_limiter_enforces_per_principal_windows() {
        let limiter = RateLimiter::new(2, Duration::from_secs(60));
        let now = Instant::now();
        let first = [1u8; 32];
        let second = [2u8; 32];

        assert!(limiter.check_at(first, now).await.is_ok());
        assert!(limiter.check_at(first, now).await.is_ok());
        let retry_after = limiter.check_at(first, now).await.unwrap_err();
        assert!(retry_after > Duration::ZERO);
        assert!(retry_after <= Duration::from_secs(60));
        assert!(limiter.check_at(second, now).await.is_ok());
        assert!(
            limiter
                .check_at(first, now + Duration::from_secs(61))
                .await
                .is_ok()
        );
    }

    fn request_with_bearer(token: impl Into<String>) -> Request<Body> {
        let mut request = request("http://gateway/mcp", None);
        request.headers_mut().insert(
            header::AUTHORIZATION,
            header::HeaderValue::from_str(&format!("Bearer {}", token.into())).unwrap(),
        );
        request
    }
}
