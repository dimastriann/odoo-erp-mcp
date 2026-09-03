pub(crate) const DEFAULT_QUERY_LIMIT: u64 = 100;

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
}
