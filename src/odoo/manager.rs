use super::client::OdooClient;
use crate::config::OdooInstance;
use crate::error::AppError;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

#[derive(Clone, Default)]
pub(crate) struct ClientManager {
    clients: Arc<Mutex<HashMap<String, Arc<OdooClient>>>>,
}

impl ClientManager {
    pub(crate) fn new() -> Self {
        Self {
            clients: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(crate) async fn get_client(
        &self,
        instance: &OdooInstance,
        connection_timeout: Duration,
        request_timeout: Duration,
        max_response_bytes: usize,
    ) -> Result<Arc<OdooClient>, AppError> {
        let mut map = self.clients.lock().await;
        if let Some(client) = map.get(&instance.id) {
            return Ok(Arc::clone(client));
        }

        eprintln!(
            "Connecting & authenticating Odoo instance '{}' (id: {})...",
            instance.name, instance.id
        );
        let client = OdooClient::new_with_timeouts(
            instance.url.clone(),
            instance.db.clone(),
            instance.username.clone(),
            instance.password.clone(),
            connection_timeout,
            request_timeout,
            max_response_bytes,
        )
        .await?;
        let arc_client = Arc::new(client);
        map.insert(instance.id.clone(), Arc::clone(&arc_client));
        Ok(arc_client)
    }

    #[allow(dead_code)]
    pub(crate) async fn remove_client(&self, instance_id: &str) {
        let mut map = self.clients.lock().await;
        map.remove(instance_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::connection_failure_url;

    #[tokio::test]
    async fn preserves_transport_failure_category() {
        let manager = ClientManager::new();
        let instance = OdooInstance {
            id: "unreachable".to_string(),
            name: "Unreachable Odoo".to_string(),
            url: connection_failure_url().to_string(),
            db: "test-db".to_string(),
            username: "admin".to_string(),
            password: "secret".to_string(),
            active: true,
            mode: None,
            allowed_tools: None,
            query_limits: None,
        };

        let result = manager
            .get_client(
                &instance,
                Duration::from_secs(1),
                Duration::from_secs(1),
                1024,
            )
            .await;
        let error = match result {
            Ok(_) => panic!("unreachable Odoo must not create a cached client"),
            Err(error) => error,
        };

        assert!(matches!(error, AppError::Transport { .. }));
        assert_eq!(error.to_string(), "Failed to communicate with Odoo");
    }
}
