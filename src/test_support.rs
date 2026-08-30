use crate::config::{Config, GlobalSettings, OdooInstance};
use std::sync::{Arc, RwLock};

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

    #[test]
    fn fixture_exposes_crud_and_read_only_instances() {
        let config = multi_instance_config();
        let config = config.read().unwrap();

        assert_eq!(config.instances.len(), 2);
        assert_eq!(config.instances[0].get_mode("crud"), "crud");
        assert_eq!(config.instances[1].get_mode("crud"), "read_only");
    }
}
