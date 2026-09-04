pub(crate) fn validate_read_id_count(ids: &[i64], maximum: usize) -> Result<(), String> {
    if ids.len() > maximum {
        return Err(format!(
            "requested {} record IDs, exceeding the configured maximum of {maximum}",
            ids.len()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_read_id_lists_above_maximum() {
        assert!(validate_read_id_count(&[1, 2], 2).is_ok());
        assert_eq!(
            validate_read_id_count(&[1, 2, 3], 2),
            Err("requested 3 record IDs, exceeding the configured maximum of 2".to_string())
        );
    }
}
