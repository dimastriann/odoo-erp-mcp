use serde::Deserialize;
use serde_json::{Map, Value, json};

fn empty_array() -> Value {
    json!([])
}

#[derive(Debug, Deserialize)]
pub(crate) struct SearchReadArgs {
    pub(crate) model: String,
    #[serde(default = "empty_array")]
    pub(crate) domain: Value,
    #[serde(default = "empty_array")]
    pub(crate) fields: Value,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SearchDomainArgs {
    pub(crate) model: String,
    #[serde(default = "empty_array")]
    pub(crate) domain: Value,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ReadGroupArgs {
    pub(crate) model: String,
    #[serde(default = "empty_array")]
    pub(crate) domain: Value,
    #[serde(default = "empty_array")]
    pub(crate) fields: Value,
    #[serde(default = "empty_array")]
    pub(crate) groupby: Value,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ModelFieldsArgs {
    pub(crate) model: String,
    #[serde(default = "empty_array")]
    pub(crate) fields: Value,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ReadArgs {
    pub(crate) model: String,
    pub(crate) ids: Vec<i64>,
    #[serde(default = "empty_array")]
    pub(crate) fields: Value,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateArgs {
    pub(crate) model: String,
    pub(crate) vals: Map<String, Value>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CopyArgs {
    pub(crate) model: String,
    pub(crate) id: i64,
    pub(crate) vals: Map<String, Value>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UpdateArgs {
    pub(crate) model: String,
    pub(crate) ids: Vec<i64>,
    pub(crate) vals: Map<String, Value>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DeleteArgs {
    pub(crate) model: String,
    pub(crate) ids: Vec<i64>,
}
