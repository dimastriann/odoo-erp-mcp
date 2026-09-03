pub(crate) const DEFAULT_QUERY_LIMIT: u64 = 100;

pub(crate) const fn resolve_limit(limit: Option<u64>) -> u64 {
    match limit {
        Some(limit) => limit,
        None => DEFAULT_QUERY_LIMIT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omitted_limit_uses_safe_default() {
        assert_eq!(resolve_limit(None), 100);
        assert_eq!(resolve_limit(Some(25)), 25);
    }
}
