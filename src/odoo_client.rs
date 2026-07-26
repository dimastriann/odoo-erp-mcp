use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Clone)]
pub struct OdooClient {
    base_url: String,
    db: String,
    uid: i64,
    password: String,
    client: Client,
}

#[derive(Serialize, Deserialize, Debug)]
struct RpcRequest {
    jsonrpc: String,
    method: String,
    params: Value,
    id: i64,
}

impl OdooClient {
    pub async fn new(
        base_url: String,
        db: String,
        username: String,
        password: String,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let client = Client::builder().cookie_store(true).build()?;

        let mut odoo = OdooClient {
            base_url,
            db,
            uid: 0,
            password: password.clone(),
            client,
        };

        let uid = odoo
            .authenticate(&odoo.db.clone(), &username, &password)
            .await?;
        odoo.uid = uid;
        Ok(odoo)
    }

    async fn call_rpc(&self, params: Value) -> Result<Value, Box<dyn std::error::Error>> {
        let req = RpcRequest {
            jsonrpc: "2.0".into(),
            method: "call".into(),
            params,
            id: 1,
        };

        let url = format!("{}/jsonrpc", self.base_url);
        let res = self.client.post(&url).json(&req).send().await?;
        let resp_json: Value = res.json().await?;

        if let Some(error) = resp_json.get("error") {
            return Err(format!("Odoo RPC Error: {}", error).into());
        }

        Ok(resp_json["result"].clone())
    }

    pub async fn authenticate(
        &self,
        db: &str,
        username: &str,
        password: &str,
    ) -> Result<i64, Box<dyn std::error::Error>> {
        let params = json!({
            "service": "common",
            "method": "authenticate",
            "args": [db, username, password, {}]
        });

        let result = self.call_rpc(params).await?;

        let uid = result
            .as_i64()
            .ok_or("Authentication failed or returned empty uid")?;
        Ok(uid)
    }

    pub async fn search_read(
        &self,
        model: &str,
        domain: Value,
        fields: Value,
    ) -> Result<Value, Box<dyn std::error::Error>> {
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

    pub async fn search_count(
        &self,
        model: &str,
        domain: Value,
    ) -> Result<Value, Box<dyn std::error::Error>> {
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
    ) -> Result<Value, Box<dyn std::error::Error>> {
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

    pub async fn create(
        &self,
        model: &str,
        vals: Value,
    ) -> Result<Value, Box<dyn std::error::Error>> {
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

    pub async fn copy(
        &self,
        model: &str,
        id: i64,
        vals: Value,
    ) -> Result<Value, Box<dyn std::error::Error>> {
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

    pub async fn update(
        &self,
        model: &str,
        ids: Vec<i64>,
        vals: Value,
    ) -> Result<Value, Box<dyn std::error::Error>> {
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

    pub async fn delete(
        &self,
        model: &str,
        ids: Vec<i64>,
    ) -> Result<Value, Box<dyn std::error::Error>> {
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

    pub async fn get_metadata(
        &self,
        model: &str,
        fields: Value,
    ) -> Result<Value, Box<dyn std::error::Error>> {
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

    pub async fn search(
        &self,
        model: &str,
        domain: Value,
    ) -> Result<Value, Box<dyn std::error::Error>> {
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

    pub async fn read(
        &self,
        model: &str,
        ids: Vec<i64>,
        fields: Value,
    ) -> Result<Value, Box<dyn std::error::Error>> {
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

    pub async fn get_client(&self, instance: &OdooInstance) -> Result<Arc<OdooClient>, String> {
        let mut map = self.clients.lock().await;
        if let Some(client) = map.get(&instance.id) {
            return Ok(Arc::clone(client));
        }

        eprintln!(
            "Connecting & authenticating Odoo instance '{}' (id: {})...",
            instance.name, instance.id
        );
        match OdooClient::new(
            instance.url.clone(),
            instance.db.clone(),
            instance.username.clone(),
            instance.password.clone(),
        )
        .await
        {
            Ok(client) => {
                let arc_client = Arc::new(client);
                map.insert(instance.id.clone(), Arc::clone(&arc_client));
                Ok(arc_client)
            }
            Err(e) => Err(format!(
                "Failed to connect to Odoo instance '{}': {}",
                instance.name, e
            )),
        }
    }

    #[allow(dead_code)]
    pub async fn remove_client(&self, instance_id: &str) {
        let mut map = self.clients.lock().await;
        map.remove(instance_id);
    }
}
