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

pub(crate) fn validate_domain_depth(domain: &Value, maximum: usize) -> Result<(), String> {
    let depth = value_depth(domain);
    if depth > maximum {
        return Err(format!(
            "domain depth {depth} exceeds the configured maximum of {maximum}"
        ));
    }
    Ok(())
}

pub(crate) fn validate_domain_term_count(domain: &Value, maximum: usize) -> Result<(), String> {
    let terms = domain
        .as_array()
        .ok_or_else(|| "expected an array of domain clauses".to_string())?
        .len();
    if terms > maximum {
        return Err(format!(
            "domain contains {terms} terms, exceeding the configured maximum of {maximum}"
        ));
    }
    Ok(())
}

fn value_depth(value: &Value) -> usize {
    match value {
        Value::Array(values) => 1 + values.iter().map(value_depth).max().unwrap_or(0),
        Value::Object(values) => 1 + values.values().map(value_depth).max().unwrap_or(0),
        _ => 0,
    }
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

    #[test]
    fn rejects_domains_above_maximum_depth() {
        let domain = json!([["id", "in", [[1, 2]]]]);

        assert!(validate_domain_depth(&domain, 4).is_ok());
        assert_eq!(
            validate_domain_depth(&domain, 3),
            Err("domain depth 4 exceeds the configured maximum of 3".to_string())
        );
    }

    #[test]
    fn rejects_domains_above_maximum_term_count() {
        let domain = json!(["|", ["name", "=", "Alpha"], ["name", "=", "Beta"]]);

        assert!(validate_domain_term_count(&domain, 3).is_ok());
        assert_eq!(
            validate_domain_term_count(&domain, 2),
            Err("domain contains 3 terms, exceeding the configured maximum of 2".to_string())
        );
    }
}
