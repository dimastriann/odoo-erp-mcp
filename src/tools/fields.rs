use serde_json::Value;
use std::collections::HashSet;

pub(crate) fn validate_field_count(fields: &Value, maximum: usize) -> Result<(), String> {
    let fields = fields
        .as_array()
        .ok_or_else(|| "fields must be an array".to_string())?;
    if fields.len() > maximum {
        return Err(format!(
            "requested {} fields, exceeding the configured maximum of {maximum}",
            fields.len()
        ));
    }
    Ok(())
}

pub(crate) fn validate_field_names(fields: &Value) -> Result<(), String> {
    let fields = fields
        .as_array()
        .ok_or_else(|| "fields must be an array".to_string())?;
    let mut unique = HashSet::with_capacity(fields.len());

    for field in fields {
        let field = field
            .as_str()
            .ok_or_else(|| "every requested field must be a string".to_string())?;
        if !is_valid_field_path(field) {
            return Err(format!("invalid field name {field:?}"));
        }
        if !unique.insert(field) {
            return Err(format!("duplicate requested field {field:?}"));
        }
    }
    Ok(())
}

fn is_valid_field_path(field: &str) -> bool {
    !field.is_empty() && field.split('.').all(is_valid_identifier)
}

fn is_valid_identifier(identifier: &str) -> bool {
    let mut characters = identifier.chars();
    characters
        .next()
        .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_field_lists_above_maximum() {
        assert!(validate_field_count(&json!(["id", "name"]), 2).is_ok());
        assert_eq!(
            validate_field_count(&json!(["id", "name", "email"]), 2),
            Err("requested 3 fields, exceeding the configured maximum of 2".to_string())
        );
    }

    #[test]
    fn accepts_unique_identifier_and_relational_field_names() {
        assert!(validate_field_names(&json!(["id", "x_code", "partner_id.name"])).is_ok());
    }

    #[test]
    fn rejects_duplicate_fields() {
        assert_eq!(
            validate_field_names(&json!(["name", "name"])),
            Err("duplicate requested field \"name\"".to_string())
        );
    }

    #[test]
    fn rejects_non_string_and_invalid_field_names() {
        assert!(validate_field_names(&json!(["name", 7])).is_err());
        assert!(validate_field_names(&json!(["partner_id..name"])).is_err());
        assert!(validate_field_names(&json!(["name;drop"])).is_err());
    }
}
