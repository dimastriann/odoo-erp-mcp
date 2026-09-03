#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OperationClass {
    Authentication,
    ReadOnly,
    Mutation,
}

impl OperationClass {
    pub(crate) const fn is_retry_safe(self) -> bool {
        matches!(self, Self::ReadOnly)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_read_only_operations_are_retry_safe() {
        assert!(OperationClass::ReadOnly.is_retry_safe());
        assert!(!OperationClass::Authentication.is_retry_safe());
        assert!(!OperationClass::Mutation.is_retry_safe());
    }
}
