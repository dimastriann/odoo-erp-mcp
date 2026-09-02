use crate::error::AppError;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

#[derive(Clone)]
pub struct OdooClient {
    base_url: String,
    db: String,
    uid: i64,
    password: String,
    client: Client,
    next_request_id: Arc<AtomicI64>,
}

#[derive(Serialize, Deserialize, Debug)]
struct RpcRequest {
    jsonrpc: String,
    method: String,
    params: Value,
    id: i64,
}

impl OdooClient {
    #[cfg(test)]
    pub async fn new(
        base_url: String,
        db: String,
        username: String,
        password: String,
    ) -> Result<Self, AppError> {
        Self::new_with_connection_timeout(base_url, db, username, password, Duration::from_secs(10))
            .await
    }

    pub async fn new_with_connection_timeout(
        base_url: String,
        db: String,
        username: String,
        password: String,
        connection_timeout: Duration,
    ) -> Result<Self, AppError> {
        let client = Client::builder()
            .cookie_store(true)
            .connect_timeout(connection_timeout)
            .build()
            .map_err(|_| AppError::transport("Failed to initialize Odoo HTTP client"))?;

        let mut odoo = OdooClient {
            base_url,
            db,
            uid: 0,
            password: password.clone(),
            client,
            next_request_id: Arc::new(AtomicI64::new(1)),
        };

        let uid = odoo
            .authenticate(&odoo.db.clone(), &username, &password)
            .await?;
        odoo.uid = uid;
        Ok(odoo)
    }

    async fn call_rpc(&self, params: Value) -> Result<Value, AppError> {
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
        let resp_json: Value = res
            .json()
            .await
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

        let result = self.call_rpc(params).await?;

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
    ) -> Result<Value, AppError> {
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
                { "fields": fields }
            ]
        });

        self.call_rpc(params).await
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

        self.call_rpc(params).await
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

        self.call_rpc(params).await
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

        self.call_rpc(params).await
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

        self.call_rpc(params).await
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

        self.call_rpc(params).await
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

        self.call_rpc(params).await
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
                { "attributes": ["string", "help", "type", "relation", "selection", "required"] }
            ]
        });

        self.call_rpc(params).await
    }

    pub async fn search(&self, model: &str, domain: Value) -> Result<Value, AppError> {
        let params = json!({
            "service": "object",
            "method": "execute_kw",
            "args": [
                self.db,
                self.uid,
                self.password,
                model,
                "search",
                [domain]
            ]
        });

        self.call_rpc(params).await
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

        self.call_rpc(params).await
    }
}

use crate::config::OdooInstance;
use std::collections::HashMap;
use tokio::sync::Mutex;

#[derive(Clone, Default)]
pub struct ClientManager {
    clients: Arc<Mutex<HashMap<String, Arc<OdooClient>>>>,
}

impl ClientManager {
    pub fn new() -> Self {
        Self {
            clients: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn get_client(
        &self,
        instance: &OdooInstance,
        connection_timeout: Duration,
    ) -> Result<Arc<OdooClient>, AppError> {
        let mut map = self.clients.lock().await;
        if let Some(client) = map.get(&instance.id) {
            return Ok(Arc::clone(client));
        }

        eprintln!(
            "Connecting & authenticating Odoo instance '{}' (id: {})...",
            instance.name, instance.id
        );
        match OdooClient::new_with_connection_timeout(
            instance.url.clone(),
            instance.db.clone(),
            instance.username.clone(),
            instance.password.clone(),
            connection_timeout,
        )
        .await
        {
            Ok(client) => {
                let arc_client = Arc::new(client);
                map.insert(instance.id.clone(), Arc::clone(&arc_client));
                Ok(arc_client)
            }
            Err(e) => Err(AppError::authentication(format!(
                "Failed to connect to Odoo instance '{}': {}",
                instance.name, e
            ))),
        }
    }

    #[allow(dead_code)]
    pub async fn remove_client(&self, instance_id: &str) {
        let mut map = self.clients.lock().await;
        map.remove(instance_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{MockOdooServer, authentication_success};
    use axum::http::StatusCode;

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
}
