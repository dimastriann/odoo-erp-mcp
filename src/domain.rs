use serde_json::Value;

pub(crate) fn validate_domain(domain: &Value) -> Result<(), String> {
    let items = domain
        .as_array()
        .ok_or_else(|| "expected an array of domain clauses".to_string())?;

    for item in items {
        if let Some(operator) = item.as_str() {
            if matches!(operator, "&" | "|" | "!") {
                continue;
            }
            return Err(format!(
                "invalid domain item {operator:?}; filters must be nested, for example [[\"name\", \"=\", \"S00027\"]]"
            ));
        }

        let clause = item.as_array().ok_or_else(|| {
            "each domain item must be a three-element clause or a logical operator".to_string()
        })?;
        if clause.len() != 3 {
            return Err(format!(
                "domain clauses must contain exactly three elements, received {}",
                clause.len()
            ));
        }
        if !clause[0].is_string() || !clause[1].is_string() {
            return Err("domain clause fields and operators must be strings".to_string());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn accepts_nested_filter_clauses() {
        assert!(validate_domain(&json!([["name", "=", "S00027"]])).is_ok());
    }

    #[test]
    fn accepts_prefix_logical_operators() {
        assert!(
            validate_domain(&json!([
                "|",
                ["name", "=", "S00027"],
                ["state", "=", "sale"]
            ]))
            .is_ok()
        );
    }

    #[test]
    fn rejects_flat_filter_from_manual_smoke_test() {
        let error = validate_domain(&json!(["name", "=", "S00027"])).unwrap_err();

        assert!(error.contains("filters must be nested"));
        assert!(error.contains("[[\"name\", \"=\", \"S00027\"]]"));
    }
}
