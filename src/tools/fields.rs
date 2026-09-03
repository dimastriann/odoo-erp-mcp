use serde_json::Value;

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
}
