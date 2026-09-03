use crate::error::AppError;
use serde_json::{Value, json};

pub(crate) const DEFAULT_QUERY_LIMIT: u64 = 100;

pub(crate) const fn fetch_limit(limit: u64) -> u64 {
    limit.saturating_add(1)
}

pub(crate) fn resolve_limit(limit: Option<u64>, maximum: u64) -> Result<u64, String> {
    match limit {
        Some(0) => Err("query limit must be greater than zero".to_string()),
        Some(limit) if limit > maximum => Err(format!(
            "query limit {limit} exceeds the configured maximum of {maximum}"
        )),
        Some(limit) => Ok(limit),
        None => Ok(DEFAULT_QUERY_LIMIT.min(maximum)),
    }
}

pub(crate) fn paginated_result(
    result: Result<Value, AppError>,
    offset: Option<u64>,
    limit: u64,
) -> Result<Value, AppError> {
    let mut items = result?;
    let items_array = items
        .as_array_mut()
        .ok_or_else(|| AppError::protocol("Odoo search result must be an array"))?;
    let requested_limit = usize::try_from(limit).unwrap_or(usize::MAX);
    let has_more = items_array.len() > requested_limit;
    items_array.truncate(requested_limit);
    let returned = items_array.len();

    Ok(json!({
        "items": items,
        "pagination": {
            "offset": offset.unwrap_or(0),
            "limit": limit,
            "returned": returned,
            "has_more": has_more
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omitted_limit_uses_safe_default() {
        assert_eq!(resolve_limit(None, 1_000), Ok(100));
        assert_eq!(resolve_limit(Some(25), 1_000), Ok(25));
    }

    #[test]
    fn requested_limit_cannot_exceed_configured_maximum() {
        assert_eq!(resolve_limit(None, 50), Ok(50));
        assert_eq!(
            resolve_limit(Some(1_001), 1_000),
            Err("query limit 1001 exceeds the configured maximum of 1000".to_string())
        );
    }

    #[test]
    fn zero_limit_is_rejected() {
        assert_eq!(
            resolve_limit(Some(0), 1_000),
            Err("query limit must be greater than zero".to_string())
        );
    }

    #[test]
    fn pagination_metadata_describes_returned_items() {
        let response = paginated_result(Ok(json!([10, 11])), Some(20), 10).unwrap();

        assert_eq!(response["items"], json!([10, 11]));
        assert_eq!(response["pagination"]["offset"], 20);
        assert_eq!(response["pagination"]["limit"], 10);
        assert_eq!(response["pagination"]["returned"], 2);
        assert_eq!(response["pagination"]["has_more"], false);
    }

    #[test]
    fn extra_item_sets_has_more_and_is_not_returned() {
        let response = paginated_result(Ok(json!([10, 11, 12])), None, 2).unwrap();

        assert_eq!(response["items"], json!([10, 11]));
        assert_eq!(response["pagination"]["returned"], 2);
        assert_eq!(response["pagination"]["has_more"], true);
        assert_eq!(fetch_limit(2), 3);
    }

    #[test]
    fn pagination_rejects_non_array_odoo_results() {
        let error = paginated_result(Ok(json!({"unexpected": true})), None, 100).unwrap_err();

        assert!(matches!(error, AppError::Protocol { .. }));
    }
}
