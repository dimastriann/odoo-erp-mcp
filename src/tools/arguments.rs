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
pub(crate) struct SearchArgs {
    pub(crate) model: String,
    #[serde(default = "empty_array")]
    pub(crate) domain: Value,
    pub(crate) offset: Option<u64>,
    pub(crate) limit: Option<u64>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_read_arguments_default_to_empty_arrays() {
        let search: SearchReadArgs =
            serde_json::from_value(json!({"model": "res.partner"})).unwrap();

        assert_eq!(search.domain, json!([]));
        assert_eq!(search.fields, json!([]));
    }

    #[test]
    fn search_accepts_optional_pagination() {
        let search: SearchArgs = serde_json::from_value(json!({
            "model": "res.partner",
            "offset": 20,
            "limit": 10
        }))
        .unwrap();

        assert_eq!(search.domain, json!([]));
        assert_eq!(search.offset, Some(20));
        assert_eq!(search.limit, Some(10));
    }

    #[test]
    fn read_ids_must_be_an_integer_array() {
        let result = serde_json::from_value::<ReadArgs>(json!({
            "model": "res.partner",
            "ids": [1, "invalid"]
        }));

        assert!(result.is_err());
    }

    #[test]
    fn write_values_must_be_an_object() {
        let result = serde_json::from_value::<CreateArgs>(json!({
            "model": "res.partner",
            "vals": ["not", "an", "object"]
        }));

        assert!(result.is_err());
    }

    #[test]
    fn required_write_arguments_cannot_be_omitted() {
        let result = serde_json::from_value::<UpdateArgs>(json!({
            "model": "res.partner",
            "vals": {"name": "Updated"}
        }));

        assert!(result.is_err());
    }
}
