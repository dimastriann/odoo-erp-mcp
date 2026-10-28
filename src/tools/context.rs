use crate::tools::protection::QueryLimits;
use serde_json::{Map, Value};

const MAX_KEY_LENGTH: usize = 64;
const MAX_STRING_LENGTH: usize = 64;

pub(crate) fn validate_context(
    context: Option<&Map<String, Value>>,
    limits: QueryLimits,
) -> Result<(), String> {
    let Some(context) = context else {
        return Ok(());
    };
    if context.len() > limits.max_context_keys {
        return Err(format!(
            "context has {} keys, exceeding the configured maximum of {}",
            context.len(),
            limits.max_context_keys
        ));
    }
    let bytes = serde_json::to_vec(context)
        .map_err(|error| format!("context must be serializable JSON: {error}"))?;
    if bytes.len() > limits.max_context_bytes {
        return Err(format!(
            "context is {} bytes, exceeding the configured maximum of {}",
            bytes.len(),
            limits.max_context_bytes
        ));
    }

    for (key, value) in context {
        if key.is_empty() || key.len() > MAX_KEY_LENGTH {
            return Err(format!("invalid context key '{key}'"));
        }
        match key.as_str() {
            "lang" | "tz" => validate_string(key, value)?,
            "active_test" => {
                if !value.is_boolean() {
                    return Err(format!("context key '{key}' must be a boolean"));
                }
            }
            "company_id" => {
                if value.as_i64().is_none() && value.as_u64().is_none() {
                    return Err("context key 'company_id' must be an integer".to_string());
                }
            }
            "allowed_company_ids" => {
                let ids = value.as_array().ok_or_else(|| {
                    "context key 'allowed_company_ids' must be an array".to_string()
                })?;
                if ids
                    .iter()
                    .any(|id| id.as_i64().is_none() && id.as_u64().is_none())
                {
                    return Err(
                        "context key 'allowed_company_ids' must contain only integers".to_string(),
                    );
                }
            }
            key if key.starts_with("default_") => {}
            _ => return Err(format!("unsupported context key '{key}'")),
        }
    }
    Ok(())
}

fn validate_string(key: &str, value: &Value) -> Result<(), String> {
    let value = value
        .as_str()
        .ok_or_else(|| format!("context key '{key}' must be a string"))?;
    if value.is_empty() || value.len() > MAX_STRING_LENGTH {
        return Err(format!("context key '{key}' has an invalid length"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const LIMITS: QueryLimits = QueryLimits {
        max_query_limit: 1_000,
        max_requested_fields: 100,
        max_read_ids: 100,
        max_domain_depth: 8,
        max_domain_terms: 100,
        max_response_records: 1_000,
        max_domain_in_values: 1_000,
        max_context_keys: 5,
        max_context_bytes: 256,
        max_batch_size: 100,
        max_report_bytes: 10_485_760,
    };

    fn context(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn accepts_supported_context_values() {
        let value = context(json!({
            "lang": "en_US",
            "tz": "Asia/Jakarta",
            "active_test": false,
            "allowed_company_ids": [1, 2],
            "default_name": "Alpha"
        }));

        assert!(validate_context(Some(&value), LIMITS).is_ok());
    }

    #[test]
    fn rejects_unsupported_keys_and_wrong_types() {
        for value in [
            json!({"unknown": true}),
            json!({"lang": 42}),
            json!({"active_test": "no"}),
            json!({"company_id": "one"}),
            json!({"allowed_company_ids": [1, "two"]}),
        ] {
            let value = context(value);
            assert!(validate_context(Some(&value), LIMITS).is_err());
        }
    }

    #[test]
    fn enforces_key_and_byte_limits() {
        let too_many = context(json!({
            "lang": "en_US",
            "tz": "UTC",
            "active_test": true,
            "company_id": 1,
            "default_name": "Alpha",
            "default_code": "A"
        }));
        assert!(validate_context(Some(&too_many), LIMITS).is_err());

        let too_large = context(json!({"default_name": "x".repeat(300)}));
        assert!(validate_context(Some(&too_large), LIMITS).is_err());
    }

    #[test]
    fn absent_context_is_valid() {
        assert!(validate_context(None, LIMITS).is_ok());
    }
}
