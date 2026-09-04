use serde_json::Value;

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct Domain {
    terms: Vec<DomainTerm>,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum DomainTerm {
    Logical(LogicalOperator),
    Clause(DomainClause),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LogicalOperator {
    And,
    Or,
    Not,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct DomainClause {
    pub(crate) field: String,
    pub(crate) operator: String,
    pub(crate) value: Value,
}

const ALLOWED_DOMAIN_OPERATORS: &[&str] = &[
    "=",
    "!=",
    ">",
    ">=",
    "<",
    "<=",
    "=?",
    "=like",
    "like",
    "not like",
    "=ilike",
    "ilike",
    "not ilike",
    "in",
    "not in",
    "child_of",
    "parent_of",
    "any",
    "not any",
    "any!",
];

impl Domain {
    fn validate_operators(&self) -> Result<(), String> {
        for term in &self.terms {
            match term {
                DomainTerm::Logical(operator) => {
                    let _ = operator;
                }
                DomainTerm::Clause(clause) => {
                    if !ALLOWED_DOMAIN_OPERATORS.contains(&clause.operator.as_str()) {
                        return Err(format!("unsupported domain operator {:?}", clause.operator));
                    }
                    let _ = (&clause.field, &clause.value);
                }
            }
        }
        Ok(())
    }
}

impl TryFrom<&Value> for Domain {
    type Error = String;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        let items = value
            .as_array()
            .ok_or_else(|| "expected an array of domain clauses".to_string())?;
        let terms = items
            .iter()
            .map(DomainTerm::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { terms })
    }
}

impl TryFrom<&Value> for DomainTerm {
    type Error = String;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        if let Some(operator) = value.as_str() {
            let logical = match operator {
                "&" => LogicalOperator::And,
                "|" => LogicalOperator::Or,
                "!" => LogicalOperator::Not,
                _ => {
                    return Err(format!(
                        "invalid domain item {operator:?}; filters must be nested, for example [[\"name\", \"=\", \"S00027\"]]"
                    ));
                }
            };
            return Ok(Self::Logical(logical));
        }

        let clause = value.as_array().ok_or_else(|| {
            "each domain item must be a three-element clause or a logical operator".to_string()
        })?;
        if clause.len() != 3 {
            return Err(format!(
                "domain clauses must contain exactly three elements, received {}",
                clause.len()
            ));
        }
        let field = clause[0]
            .as_str()
            .ok_or_else(|| "domain clause fields and operators must be strings".to_string())?;
        let operator = clause[1]
            .as_str()
            .ok_or_else(|| "domain clause fields and operators must be strings".to_string())?;
        if field.is_empty() || operator.is_empty() {
            return Err("domain clause fields and operators cannot be empty".to_string());
        }
        Ok(Self::Clause(DomainClause {
            field: field.to_string(),
            operator: operator.to_string(),
            value: clause[2].clone(),
        }))
    }
}

pub(crate) fn validate_domain(domain: &Value) -> Result<(), String> {
    let parsed = Domain::try_from(domain)?;
    parsed.validate_operators()
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
    fn parses_typed_clauses_and_logical_operators() {
        let domain =
            Domain::try_from(&json!(["|", ["name", "=", "Alpha"], ["active", "=", true]])).unwrap();

        assert_eq!(domain.terms.len(), 3);
        assert_eq!(domain.terms[0], DomainTerm::Logical(LogicalOperator::Or));
        let DomainTerm::Clause(clause) = &domain.terms[1] else {
            panic!("second term must be a clause");
        };
        assert_eq!(clause.field, "name");
        assert_eq!(clause.operator, "=");
        assert_eq!(clause.value, json!("Alpha"));
    }

    #[test]
    fn accepts_supported_odoo_domain_operators() {
        for operator in ALLOWED_DOMAIN_OPERATORS {
            let domain = json!([["name", operator, "value"]]);
            assert!(validate_domain(&domain).is_ok(), "operator {operator}");
        }
    }

    #[test]
    fn rejects_unsupported_domain_operator() {
        assert_eq!(
            validate_domain(&json!([["name", "contains", "value"]])),
            Err("unsupported domain operator \"contains\"".to_string())
        );
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
