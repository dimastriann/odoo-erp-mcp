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
