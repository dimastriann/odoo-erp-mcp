use crate::config::RpcTransport;
use crate::error::AppError;
use crate::odoo::retry::{
    OperationClass, RetryBackoff, RetryEvent, RetryObserver, default_retry_observer,
};
use futures::StreamExt;
use reqwest::{Client, header};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU8, Ordering};
use std::time::Duration;

const TRANSPORT_JSONRPC: u8 = 1;
const TRANSPORT_JSON2: u8 = 2;

#[derive(Clone)]
pub struct OdooClient {
    base_url: String,
    db: String,
    uid: i64,
    password: String,
    client: Client,
    next_request_id: Arc<AtomicI64>,
    max_response_bytes: usize,
    retry_observer: Arc<dyn RetryObserver>,
    configured_transport: RpcTransport,
    transport: Arc<AtomicU8>,
    context: Option<Map<String, Value>>,
}

#[derive(Serialize, Deserialize, Debug)]
struct RpcRequest {
    jsonrpc: String,
    method: String,
    params: Value,
    id: i64,
}

const MAX_READ_RETRIES: u32 = 2;

struct OdooClientOptions {
    base_url: String,
    db: String,
    username: String,
    password: String,
    connection_timeout: Duration,
    request_timeout: Duration,
    max_response_bytes: usize,
    transport: RpcTransport,
}

impl OdooClient {
    #[cfg(test)]
    pub async fn new(
        base_url: String,
        db: String,
        username: String,
        password: String,
    ) -> Result<Self, AppError> {
        Self::new_with_timeouts(
            base_url,
            db,
            username,
            password,
            Duration::from_secs(10),
            Duration::from_secs(30),
            10 * 1024 * 1024,
        )
        .await
    }

    #[allow(dead_code)] // Retained for tests and compatibility with embedders.
    pub async fn new_with_timeouts(
        base_url: String,
        db: String,
        username: String,
        password: String,
        connection_timeout: Duration,
        request_timeout: Duration,
        max_response_bytes: usize,
    ) -> Result<Self, AppError> {
        Self::new_with_transport(
            base_url,
            db,
            username,
            password,
            connection_timeout,
            request_timeout,
            max_response_bytes,
            RpcTransport::JsonRpc,
        )
        .await
    }

    /// Creates a client with an explicit transport selection. The argument
    /// shape matches `new_with_timeouts` for compatibility with callers.
    #[allow(clippy::too_many_arguments)]
    pub async fn new_with_transport(
        base_url: String,
        db: String,
        username: String,
        password: String,
        connection_timeout: Duration,
        request_timeout: Duration,
        max_response_bytes: usize,
        transport: RpcTransport,
    ) -> Result<Self, AppError> {
        Self::new_with_retry_observer(
            OdooClientOptions {
                base_url,
                db,
                username,
                password,
                connection_timeout,
                request_timeout,
                max_response_bytes,
                transport,
            },
            default_retry_observer(),
        )
        .await
    }

    async fn new_with_retry_observer(
        options: OdooClientOptions,
        retry_observer: Arc<dyn RetryObserver>,
    ) -> Result<Self, AppError> {
        let OdooClientOptions {
            base_url,
            db,
            username,
            password,
            connection_timeout,
            request_timeout,
            max_response_bytes,
            transport,
        } = options;
        let client = Client::builder()
            .cookie_store(true)
            .connect_timeout(connection_timeout)
            .timeout(request_timeout)
            .build()
            .map_err(|_| AppError::transport("Failed to initialize Odoo HTTP client"))?;

        let initial_transport = match transport {
            RpcTransport::Json2 => TRANSPORT_JSON2,
            RpcTransport::Auto | RpcTransport::JsonRpc => TRANSPORT_JSONRPC,
        };
        let mut odoo = OdooClient {
            base_url,
            db,
            uid: 0,
            password: password.clone(),
            client,
            next_request_id: Arc::new(AtomicI64::new(1)),
            max_response_bytes,
            retry_observer,
            configured_transport: transport,
            transport: Arc::new(AtomicU8::new(initial_transport)),
            context: None,
        };

        if transport == RpcTransport::Auto && odoo.server_version_major().await.unwrap_or(0) >= 19 {
            odoo.set_transport(TRANSPORT_JSON2);
        }

        let uid = odoo
            .authenticate(&odoo.db.clone(), &username, &password)
            .await?;
        odoo.uid = uid;
        Ok(odoo)
    }

    fn transport(&self) -> u8 {
        self.transport.load(Ordering::Relaxed)
    }

    fn set_transport(&self, transport: u8) {
        self.transport.store(transport, Ordering::Relaxed);
    }

    fn is_auto_transport(&self) -> bool {
        self.configured_transport == RpcTransport::Auto
    }

    pub(crate) fn with_context(&self, context: Option<Map<String, Value>>) -> Self {
        let mut client = self.clone();
        client.context = context;
        client
    }

    fn apply_context(&self, mut params: Value) -> Value {
        let Some(context) = &self.context else {
            return params;
        };
        if params.get("service").and_then(Value::as_str) != Some("object") {
            return params;
        }
        let Some(args) = params.get_mut("args").and_then(Value::as_array_mut) else {
            return params;
        };
        if args.len() == 6 {
            args.push(json!({"context": context}));
        } else if let Some(kwargs) = args.get_mut(6).and_then(Value::as_object_mut) {
            kwargs.insert("context".to_string(), Value::Object(context.clone()));
        }
        params
    }

    async fn server_version_major(&self) -> Option<u64> {
        let result = self
            .call_rpc(
                json!({"service": "common", "method": "version", "args": []}),
                OperationClass::ReadOnly,
            )
            .await
            .ok()?;
        result
            .get("server_version_info")
            .and_then(Value::as_array)
            .and_then(|values| values.first())
            .and_then(Value::as_u64)
            .or_else(|| {
                result
                    .get("server_version")
                    .and_then(Value::as_str)
                    .and_then(|version| version.split('.').next())
                    .and_then(|major| major.parse().ok())
            })
    }

    async fn call_rpc(
        &self,
        params: Value,
        operation_class: OperationClass,
    ) -> Result<Value, AppError> {
        let params = self.apply_context(params);
        let backoff = RetryBackoff::default();
        let mut retry_index = 0;

        loop {
            let mut result = self.call_rpc_once(params.clone()).await;
            if matches!(result, Err(AppError::Authentication { .. }))
                && self.is_auto_transport()
                && self.transport() == TRANSPORT_JSON2
            {
                self.set_transport(TRANSPORT_JSONRPC);
                result = self.call_rpc_once(params.clone()).await;
            }
            let retryable_error = result
                .as_ref()
                .err()
                .filter(|error| operation_class.should_retry(error));

            if retryable_error.is_none() || retry_index >= MAX_READ_RETRIES {
                return result;
            }

            let entropy = self.next_request_id.load(Ordering::Relaxed) as u64;
            let delay = backoff.delay_for(retry_index, entropy);
            self.retry_observer.on_retry(RetryEvent {
                operation_class,
                attempt: retry_index + 1,
                delay,
                error_code: retryable_error.expect("retryable error was checked").code(),
            });
            tokio::time::sleep(delay).await;
            retry_index += 1;
        }
    }

    async fn call_rpc_once(&self, params: Value) -> Result<Value, AppError> {
        let service = params.get("service").and_then(Value::as_str);
        let method = params.get("method").and_then(Value::as_str);
        if self.transport() == TRANSPORT_JSON2
            && service == Some("object")
            && method == Some("execute_kw")
        {
            return self.call_json2_once(&params).await;
        }
        self.call_jsonrpc_once(params).await
    }

    async fn call_jsonrpc_once(&self, params: Value) -> Result<Value, AppError> {
        let req = RpcRequest {
            jsonrpc: "2.0".into(),
            method: "call".into(),
            params,
            id: self.next_request_id.fetch_add(1, Ordering::Relaxed),
        };

        let url = format!("{}/jsonrpc", self.base_url);
        let res = self
            .client
            .post(&url)
            .json(&req)
            .send()
            .await
            .map_err(|error| {
                if error.is_timeout() {
                    AppError::timeout("Odoo request timed out")
                } else {
                    AppError::transport("Failed to communicate with Odoo")
                }
            })?;
        if !res.status().is_success() {
            return Err(AppError::protocol(format!(
                "Odoo returned HTTP status {}",
                res.status().as_u16()
            )));
        }
        let response_body = self.read_response_body(res).await?;
        let resp_json: Value = serde_json::from_slice(&response_body)
            .map_err(|_| AppError::protocol("Odoo returned an invalid JSON response"))?;

        if resp_json.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Err(AppError::protocol(
                "Odoo returned an invalid JSON-RPC version",
            ));
        }

        match (resp_json.get("result"), resp_json.get("error")) {
            (Some(result), None) => Ok(result.clone()),
            (None, Some(error)) => Err(AppError::from_odoo_rpc(error)),
            (Some(_), Some(_)) => Err(AppError::protocol(
                "Odoo returned both JSON-RPC result and error",
            )),
            (None, None) => Err(AppError::protocol(
                "Odoo response is missing JSON-RPC result or error",
            )),
        }
    }

    async fn read_response_body(&self, res: reqwest::Response) -> Result<Vec<u8>, AppError> {
        if res
            .content_length()
            .is_some_and(|length| length > self.max_response_bytes as u64)
        {
            return Err(AppError::protocol(
                "Odoo response exceeds configured size limit",
            ));
        }

        let mut response_body = Vec::new();
        let mut response_stream = res.bytes_stream();
        while let Some(chunk) = response_stream.next().await {
            let chunk = chunk.map_err(|error| {
                if error.is_timeout() {
                    AppError::timeout("Odoo request timed out")
                } else {
                    AppError::transport("Failed to read Odoo response")
                }
            })?;
            if response_body.len().saturating_add(chunk.len()) > self.max_response_bytes {
                return Err(AppError::protocol(
                    "Odoo response exceeds configured size limit",
                ));
            }
            response_body.extend_from_slice(&chunk);
        }
        Ok(response_body)
    }

    async fn call_json2_once(&self, params: &Value) -> Result<Value, AppError> {
        let args = params
            .get("args")
            .and_then(Value::as_array)
            .ok_or_else(|| AppError::protocol("JSON-2 call is missing execute_kw arguments"))?;
        if args.len() < 5 {
            return Err(AppError::protocol(
                "JSON-2 call has an incomplete execute_kw argument list",
            ));
        }
        let model = args[3]
            .as_str()
            .ok_or_else(|| AppError::protocol("JSON-2 call is missing a model name"))?;
        let method = args[4]
            .as_str()
            .ok_or_else(|| AppError::protocol("JSON-2 call is missing a method name"))?;
        let positional = args
            .get(5)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let kwargs = args
            .get(6)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let (body, normalize_single_create) =
            json2_request_body(model, method, &positional, &kwargs)?;

        let url = format!("{}/json/2/{model}/{method}", self.base_url);
        let res = self
            .client
            .post(&url)
            .header(header::AUTHORIZATION, format!("Bearer {}", self.password))
            .header("X-Odoo-Database", &self.db)
            .json(&body)
            .send()
            .await
            .map_err(|error| {
                if error.is_timeout() {
                    AppError::timeout("Odoo request timed out")
                } else {
                    AppError::transport("Failed to communicate with Odoo")
                }
            })?;

        let status = res.status();
        let response_body = self.read_response_body(res).await?;
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(AppError::authentication(
                "Odoo rejected the JSON-2 API key; create an API key with the rpc scope",
            ));
        }
        if !status.is_success() {
            let parsed = serde_json::from_slice::<Value>(&response_body).unwrap_or(Value::Null);
            let name = parsed
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if name.is_empty() {
                return Err(AppError::protocol(format!(
                    "Odoo JSON-2 request failed with HTTP status {}",
                    status.as_u16()
                )));
            }
            let error = json!({
                "data": {
                    "name": name,
                    "message": parsed
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("Odoo JSON-2 request failed")
                }
            });
            return Err(AppError::from_odoo_rpc(&error));
        }

        let mut result: Value = serde_json::from_slice(&response_body)
            .map_err(|_| AppError::protocol("Odoo returned an invalid JSON-2 response"))?;
        if normalize_single_create
            && let Some(values) = result.as_array()
            && values.len() == 1
        {
            result = values[0].clone();
        }
        Ok(result)
    }

    pub async fn authenticate(
        &self,
        db: &str,
        username: &str,
        password: &str,
    ) -> Result<i64, AppError> {
        let params = json!({
            "service": "common",
            "method": "authenticate",
            "args": [db, username, password, {}]
        });

        let result = self
            .call_rpc(params, OperationClass::Authentication)
            .await?;

        let uid = result.as_i64().ok_or_else(|| {
            AppError::authentication("Authentication failed or returned empty uid")
        })?;
        Ok(uid)
    }

    pub async fn search_read(
        &self,
        model: &str,
        domain: Value,
        fields: Value,
        offset: Option<u64>,
        limit: u64,
    ) -> Result<Value, AppError> {
        let mut options = serde_json::Map::new();
        options.insert("fields".to_string(), fields);
        if let Some(offset) = offset {
            options.insert("offset".to_string(), Value::from(offset));
        }
        options.insert("limit".to_string(), Value::from(limit));
        let params = json!({
            "service": "object",
            "method": "execute_kw",
            "args": [
                self.db,
                self.uid,
                self.password,
                model,
                "search_read",
                [domain],
                options
            ]
        });

        self.call_rpc(params, OperationClass::ReadOnly).await
    }

    pub async fn search_count(&self, model: &str, domain: Value) -> Result<Value, AppError> {
        let params = json!({
            "service": "object",
            "method": "execute_kw",
            "args": [
                self.db,
                self.uid,
                self.password,
                model,
                "search_count",
                [domain]
            ]
        });

        self.call_rpc(params, OperationClass::ReadOnly).await
    }

    pub async fn read_group(
        &self,
        model: &str,
        domain: Value,
        fields: Value,
        groupby: Value,
    ) -> Result<Value, AppError> {
        let params = json!({
            "service": "object",
            "method": "execute_kw",
            "args": [
                self.db,
                self.uid,
                self.password,
                model,
                "read_group",
                [domain],
                { "fields": fields, "groupby": groupby }
            ]
        });

        self.call_rpc(params, OperationClass::ReadOnly).await
    }

    pub async fn create(&self, model: &str, vals: Value) -> Result<Value, AppError> {
        let params = json!({
            "service": "object",
            "method": "execute_kw",
            "args": [
                self.db,
                self.uid,
                self.password,
                model,
                "create",
                [vals]
            ]
        });

        self.call_rpc(params, OperationClass::Mutation).await
    }

    pub async fn copy(&self, model: &str, id: i64, vals: Value) -> Result<Value, AppError> {
        let params = json!({
            "service": "object",
            "method": "execute_kw",
            "args": [
                self.db,
                self.uid,
                self.password,
                model,
                "copy",
                [id, vals]
            ]
        });

        self.call_rpc(params, OperationClass::Mutation).await
    }

    pub async fn update(&self, model: &str, ids: Vec<i64>, vals: Value) -> Result<Value, AppError> {
        let params = json!({
            "service": "object",
            "method": "execute_kw",
            "args": [
                self.db,
                self.uid,
                self.password,
                model,
                "write",
                [ids, vals]
            ]
        });

        self.call_rpc(params, OperationClass::Mutation).await
    }

    pub async fn delete(&self, model: &str, ids: Vec<i64>) -> Result<Value, AppError> {
        let params = json!({
            "service": "object",
            "method": "execute_kw",
            "args": [
                self.db,
                self.uid,
                self.password,
                model,
                "unlink",
                [ids]
            ]
        });

        self.call_rpc(params, OperationClass::Mutation).await
    }

    pub async fn get_metadata(&self, model: &str, fields: Value) -> Result<Value, AppError> {
        let params = json!({
            "service": "object",
            "method": "execute_kw",
            "args": [
                self.db,
                self.uid,
                self.password,
                model,
                "fields_get",
                [fields],
                { "attributes": ["string", "help", "type", "relation", "selection", "required", "readonly", "compute", "default"] }
            ]
        });

        self.call_rpc(params, OperationClass::ReadOnly).await
    }

    pub async fn search(
        &self,
        model: &str,
        domain: Value,
        offset: Option<u64>,
        limit: u64,
    ) -> Result<Value, AppError> {
        let mut options = serde_json::Map::new();
        if let Some(offset) = offset {
            options.insert("offset".to_string(), Value::from(offset));
        }
        options.insert("limit".to_string(), Value::from(limit));
        let params = json!({
            "service": "object",
            "method": "execute_kw",
            "args": [
                self.db,
                self.uid,
                self.password,
                model,
                "search",
                [domain],
                options
            ]
        });

        self.call_rpc(params, OperationClass::ReadOnly).await
    }

    pub async fn read(&self, model: &str, ids: Vec<i64>, fields: Value) -> Result<Value, AppError> {
        let params = json!({
            "service": "object",
            "method": "execute_kw",
            "args": [
                self.db,
                self.uid,
                self.password,
                model,
                "read",
                [ids],
                { "fields": fields }
            ]
        });

        self.call_rpc(params, OperationClass::ReadOnly).await
    }
}

fn json2_request_body(
    model: &str,
    method: &str,
    positional: &[Value],
    kwargs: &Map<String, Value>,
) -> Result<(Value, bool), AppError> {
    let mut body = kwargs.clone();
    let positional_value = |index: usize| positional.get(index).cloned().unwrap_or(Value::Null);
    let mut normalize_single_create = false;

    match method {
        "search" | "search_read" | "search_count" => {
            body.insert("domain".to_string(), positional_value(0));
        }
        "read" => {
            body.insert("ids".to_string(), positional_value(0));
        }
        "read_group" | "formatted_read_group" => {
            body.insert("domain".to_string(), positional_value(0));
            body.insert("fields".to_string(), positional_value(1));
            body.insert("groupby".to_string(), positional_value(2));
        }
        "create" => {
            let vals = positional_value(0);
            if vals.is_array() {
                body.insert("vals_list".to_string(), vals);
            } else {
                body.insert("vals_list".to_string(), json!([vals]));
                normalize_single_create = true;
            }
        }
        "write" => {
            body.insert("ids".to_string(), positional_value(0));
            body.insert("vals".to_string(), positional_value(1));
        }
        "unlink" => {
            body.insert("ids".to_string(), positional_value(0));
        }
        "copy" => {
            body.insert("ids".to_string(), json!([positional_value(0)]));
            if let Some(default) = positional.get(1) {
                body.insert("default".to_string(), default.clone());
            }
        }
        "fields_get" => {
            body.insert("allfields".to_string(), positional_value(0));
        }
        "name_search" => {
            body.insert("name".to_string(), positional_value(0));
        }
        "onchange" => {
            body.insert("values".to_string(), positional_value(0));
            body.insert("field_names".to_string(), positional_value(1));
            body.insert("fields_spec".to_string(), positional_value(2));
        }
        "has_access" => {
            body.insert("operation".to_string(), positional_value(0));
            body.entry("ids".to_string()).or_insert_with(|| json!([]));
        }
        "check_access_rights" => {
            body.insert("operation".to_string(), positional_value(0));
        }
        "get_views" => {
            body.insert("views".to_string(), positional_value(0));
        }
        _ => {
            return Err(AppError::protocol(format!(
                "JSON-2 transport does not support {model}.{method}"
            )));
        }
    }

    Ok((Value::Object(body), normalize_single_create))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::odoo::retry::RetryEvent;
    use crate::test_support::{MockOdooServer, authentication_success, json_rpc_success};
    use axum::http::StatusCode;
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingRetryObserver {
        events: Mutex<Vec<RetryEvent>>,
    }

    impl RetryObserver for RecordingRetryObserver {
        fn on_retry(&self, event: RetryEvent) {
            self.events.lock().unwrap().push(event);
        }
    }

    async fn client_with_observer(
        server: &MockOdooServer,
        observer: Arc<RecordingRetryObserver>,
    ) -> OdooClient {
        OdooClient::new_with_retry_observer(
            OdooClientOptions {
                base_url: server.base_url().to_string(),
                db: "test-db".to_string(),
                username: "admin".to_string(),
                password: "secret".to_string(),
                connection_timeout: Duration::from_secs(1),
                request_timeout: Duration::from_millis(100),
                max_response_bytes: 10 * 1024 * 1024,
                transport: RpcTransport::JsonRpc,
            },
            observer,
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn assigns_a_unique_id_to_each_json_rpc_request() {
        let server = MockOdooServer::start(authentication_success(7)).await;
        let client = OdooClient::new(
            server.base_url().to_string(),
            "test-db".to_string(),
            "admin".to_string(),
            "secret".to_string(),
        )
        .await
        .unwrap();

        client.search_count("res.partner", json!([])).await.unwrap();

        let requests = server.requests().await;
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0]["id"], 1);
        assert_eq!(requests[1]["id"], 2);
    }

    #[tokio::test]
    async fn rejects_unsuccessful_http_status_before_parsing_rpc_result() {
        let server = MockOdooServer::start_with_status(
            authentication_success(7),
            StatusCode::SERVICE_UNAVAILABLE,
        )
        .await;

        let result = OdooClient::new(
            server.base_url().to_string(),
            "test-db".to_string(),
            "admin".to_string(),
            "secret".to_string(),
        )
        .await;
        let error = match result {
            Ok(_) => panic!("HTTP failure must not create an Odoo client"),
            Err(error) => error,
        };

        assert!(matches!(error, AppError::Protocol { .. }));
        assert_eq!(error.to_string(), "Odoo returned HTTP status 503");
    }

    #[tokio::test]
    async fn classifies_rejected_credentials_as_authentication_failure() {
        let server = MockOdooServer::start(json_rpc_success(Value::Bool(false))).await;

        let result = OdooClient::new(
            server.base_url().to_string(),
            "test-db".to_string(),
            "admin".to_string(),
            "wrong-secret".to_string(),
        )
        .await;
        let error = match result {
            Ok(_) => panic!("rejected credentials must not create an Odoo client"),
            Err(error) => error,
        };

        assert!(matches!(error, AppError::Authentication { .. }));
        assert_eq!(
            error.to_string(),
            "Authentication failed or returned empty uid"
        );
    }

    #[tokio::test]
    async fn rejects_json_rpc_envelope_without_result_or_error() {
        let server = MockOdooServer::start(json!({
            "jsonrpc": "2.0",
            "id": 1
        }))
        .await;

        let result = OdooClient::new(
            server.base_url().to_string(),
            "test-db".to_string(),
            "admin".to_string(),
            "secret".to_string(),
        )
        .await;
        let error = match result {
            Ok(_) => panic!("malformed RPC envelope must not create a client"),
            Err(error) => error,
        };

        assert!(matches!(error, AppError::Protocol { .. }));
        assert_eq!(
            error.to_string(),
            "Odoo response is missing JSON-RPC result or error"
        );
    }

    #[tokio::test]
    async fn rejects_json_rpc_envelope_with_result_and_error() {
        let server = MockOdooServer::start(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": 7,
            "error": {"message": "ambiguous"}
        }))
        .await;

        let result = OdooClient::new(
            server.base_url().to_string(),
            "test-db".to_string(),
            "admin".to_string(),
            "secret".to_string(),
        )
        .await;
        let error = match result {
            Ok(_) => panic!("ambiguous RPC envelope must not create a client"),
            Err(error) => error,
        };

        assert!(matches!(error, AppError::Protocol { .. }));
        assert_eq!(
            error.to_string(),
            "Odoo returned both JSON-RPC result and error"
        );
    }

    #[tokio::test]
    async fn classifies_malformed_json_response_as_protocol_error() {
        let server = MockOdooServer::start_raw("{not-valid-json").await;

        let result = OdooClient::new(
            server.base_url().to_string(),
            "test-db".to_string(),
            "admin".to_string(),
            "secret".to_string(),
        )
        .await;
        let error = match result {
            Ok(_) => panic!("malformed JSON must not create an Odoo client"),
            Err(error) => error,
        };

        assert!(matches!(error, AppError::Protocol { .. }));
        assert_eq!(error.to_string(), "Odoo returned an invalid JSON response");
    }

    #[tokio::test]
    async fn classifies_request_timeout() {
        let server =
            MockOdooServer::start_delayed(authentication_success(7), Duration::from_millis(100))
                .await;

        let result = OdooClient::new_with_timeouts(
            server.base_url().to_string(),
            "test-db".to_string(),
            "admin".to_string(),
            "secret".to_string(),
            Duration::from_secs(1),
            Duration::from_millis(10),
            10 * 1024 * 1024,
        )
        .await;
        let error = match result {
            Ok(_) => panic!("delayed response must time out"),
            Err(error) => error,
        };

        assert!(matches!(error, AppError::Timeout { .. }));
        assert_eq!(error.to_string(), "Odoo request timed out");
    }

    #[tokio::test]
    async fn rejects_response_larger_than_configured_limit() {
        let server = MockOdooServer::start(authentication_success(7)).await;

        let result = OdooClient::new_with_timeouts(
            server.base_url().to_string(),
            "test-db".to_string(),
            "admin".to_string(),
            "secret".to_string(),
            Duration::from_secs(1),
            Duration::from_secs(1),
            16,
        )
        .await;
        let error = match result {
            Ok(_) => panic!("oversized response must not create an Odoo client"),
            Err(error) => error,
        };

        assert!(matches!(error, AppError::Protocol { .. }));
        assert_eq!(
            error.to_string(),
            "Odoo response exceeds configured size limit"
        );
    }

    #[tokio::test]
    async fn read_succeeds_after_retryable_failures() {
        let server = MockOdooServer::start_with_delays(
            authentication_success(7),
            vec![
                Duration::ZERO,
                Duration::from_millis(500),
                Duration::from_millis(500),
                Duration::ZERO,
            ],
        )
        .await;
        let observer = Arc::new(RecordingRetryObserver::default());
        let client = client_with_observer(&server, Arc::clone(&observer)).await;

        let result = client.search_count("res.partner", json!([])).await;

        assert!(result.is_ok());
        assert_eq!(server.requests().await.len(), 4);
        let events = observer.events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].attempt, 1);
        assert_eq!(events[1].attempt, 2);
    }

    #[tokio::test]
    async fn read_returns_last_error_after_retries_are_exhausted() {
        let server = MockOdooServer::start_with_delays(
            authentication_success(7),
            vec![
                Duration::ZERO,
                Duration::from_millis(500),
                Duration::from_millis(500),
                Duration::from_millis(500),
            ],
        )
        .await;
        let observer = Arc::new(RecordingRetryObserver::default());
        let client = client_with_observer(&server, Arc::clone(&observer)).await;

        let error = client
            .search_count("res.partner", json!([]))
            .await
            .expect_err("all delayed read attempts must time out");

        assert!(matches!(error, AppError::Timeout { .. }));
        assert_eq!(server.requests().await.len(), 4);
        assert_eq!(observer.events.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn mutation_is_not_retried_after_retryable_failure() {
        let server = MockOdooServer::start_with_delays(
            authentication_success(7),
            vec![Duration::ZERO, Duration::from_millis(500), Duration::ZERO],
        )
        .await;
        let observer = Arc::new(RecordingRetryObserver::default());
        let client = client_with_observer(&server, Arc::clone(&observer)).await;

        let error = client
            .create("res.partner", json!({"name": "Do not duplicate"}))
            .await
            .expect_err("timed-out mutation must not be retried");

        assert!(matches!(error, AppError::Timeout { .. }));
        assert_eq!(server.requests().await.len(), 2);
        assert!(observer.events.lock().unwrap().is_empty());
    }

    #[test]
    fn json2_body_uses_named_arguments() {
        let (body, normalize) = json2_request_body(
            "res.partner",
            "search_read",
            &[json!([["active", "=", true]])],
            &Map::from_iter([
                ("fields".to_string(), json!(["id", "name"])),
                ("limit".to_string(), json!(10)),
            ]),
        )
        .unwrap();

        assert_eq!(body["domain"], json!([["active", "=", true]]));
        assert_eq!(body["fields"], json!(["id", "name"]));
        assert_eq!(body["limit"], 10);
        assert!(!normalize);
    }

    #[tokio::test]
    async fn json2_transport_uses_bearer_headers_and_named_arguments() {
        let server = MockOdooServer::start_with_responses(vec![
            authentication_success(7),
            json!([{"id": 1, "name": "Alpha"}]),
        ])
        .await;
        let client = OdooClient::new_with_transport(
            server.base_url().to_string(),
            "test-db".to_string(),
            "admin".to_string(),
            "api-key".to_string(),
            Duration::from_secs(1),
            Duration::from_secs(1),
            10 * 1024 * 1024,
            RpcTransport::Json2,
        )
        .await
        .unwrap();

        let result = client
            .search_read(
                "res.partner",
                json!([["active", "=", true]]),
                json!(["id", "name"]),
                Some(5),
                10,
            )
            .await
            .unwrap();

        assert_eq!(result, json!([{"id": 1, "name": "Alpha"}]));
        let requests = server.full_requests().await;
        assert_eq!(requests.len(), 2);
        let request = &requests[1];
        assert_eq!(request.path, "/json/2/res.partner/search_read");
        assert_eq!(request.authorization.as_deref(), Some("Bearer api-key"));
        assert_eq!(request.database.as_deref(), Some("test-db"));
        assert_eq!(request.body["domain"], json!([["active", "=", true]]));
        assert_eq!(request.body["fields"], json!(["id", "name"]));
        assert_eq!(request.body["limit"], 10);
        assert_eq!(request.body["offset"], 5);
    }

    #[tokio::test]
    async fn json2_transport_normalizes_single_create_results() {
        let server =
            MockOdooServer::start_with_responses(vec![authentication_success(7), json!([42])])
                .await;
        let client = OdooClient::new_with_transport(
            server.base_url().to_string(),
            "test-db".to_string(),
            "admin".to_string(),
            "api-key".to_string(),
            Duration::from_secs(1),
            Duration::from_secs(1),
            10 * 1024 * 1024,
            RpcTransport::Json2,
        )
        .await
        .unwrap();

        let result = client
            .create("res.partner", json!({"name": "Alpha"}))
            .await
            .unwrap();

        assert_eq!(result, json!(42));
        let requests = server.full_requests().await;
        assert_eq!(requests[1].path, "/json/2/res.partner/create");
        assert_eq!(requests[1].body["vals_list"], json!([{"name": "Alpha"}]));
    }

    #[tokio::test]
    async fn auto_transport_selects_json2_for_odoo_19_and_newer() {
        let server = MockOdooServer::start_with_responses(vec![
            json_rpc_success(json!({
                "server_version": "19.0",
                "server_version_info": [19, 0, 0, "final", 0]
            })),
            authentication_success(7),
            json!(25),
        ])
        .await;
        let client = OdooClient::new_with_transport(
            server.base_url().to_string(),
            "test-db".to_string(),
            "admin".to_string(),
            "api-key".to_string(),
            Duration::from_secs(1),
            Duration::from_secs(1),
            10 * 1024 * 1024,
            RpcTransport::Auto,
        )
        .await
        .unwrap();

        let result = client.search_count("res.partner", json!([])).await.unwrap();

        assert_eq!(result, json!(25));
        let paths = server
            .full_requests()
            .await
            .into_iter()
            .map(|request| request.path)
            .collect::<Vec<_>>();
        assert_eq!(
            paths,
            vec![
                "/jsonrpc".to_string(),
                "/jsonrpc".to_string(),
                "/json/2/res.partner/search_count".to_string()
            ]
        );
    }
}
