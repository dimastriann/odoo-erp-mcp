use crate::error::AppError;
use serde_json::Value;

#[derive(Clone, Copy, Debug)]
pub(crate) struct QueryLimits {
    pub(crate) max_query_limit: u64,
    pub(crate) max_requested_fields: usize,
    pub(crate) max_read_ids: usize,
    pub(crate) max_domain_depth: usize,
    pub(crate) max_domain_terms: usize,
    pub(crate) max_response_records: usize,
    pub(crate) max_domain_in_values: usize,
    pub(crate) max_context_keys: usize,
    pub(crate) max_context_bytes: usize,
    pub(crate) max_batch_size: usize,
}

pub(crate) fn validate_response_record_count(
    result: Result<Value, AppError>,
    maximum: usize,
) -> Result<Value, AppError> {
    let result = result?;
    let records = result
        .get("items")
        .and_then(Value::as_array)
        .or_else(|| result.as_array());
    if records.is_some_and(|records| records.len() > maximum) {
        return Err(AppError::protocol(format!(
            "Odoo response exceeds the configured maximum of {maximum} records"
        )));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_raw_and_paginated_record_arrays_above_maximum() {
        assert!(validate_response_record_count(Ok(json!([1, 2])), 2).is_ok());
        assert!(validate_response_record_count(Ok(json!([1, 2, 3])), 2).is_err());
        assert!(validate_response_record_count(Ok(json!({"items": [1, 2, 3]})), 2).is_err());
    }
}

pub(crate) fn validate_order(value: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 256 {
        return Err("order clause must be between 1 and 256 characters".to_string());
    }
    for character in value.chars() {
        if !(character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | ',' | ' ')) {
            return Err("order clause contains unsupported characters".to_string());
        }
    }
    for token in value.split_whitespace() {
        let token = token.trim_matches(',').to_ascii_lowercase();
        if token == "asc" || token == "desc" {
            continue;
        }
        if token.is_empty()
            || !token.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | ',')
            })
        {
            return Err("order clause contains an unsupported token".to_string());
        }
    }
    Ok(())
}

#[cfg(test)]
mod order_tests {
    use super::*;

    #[test]
    fn accepts_identifier_order_clauses() {
        for value in ["name", "name asc", "name desc, id desc", "partner_id.name"] {
            assert!(validate_order(value).is_ok(), "{value}");
        }
    }

    #[test]
    fn rejects_injection_and_unsupported_tokens() {
        for value in [
            "name; drop table",
            "name --",
            "(select 1)",
            "name = 'x'",
            "",
        ] {
            assert!(validate_order(value).is_err(), "{value}");
        }
    }
}
