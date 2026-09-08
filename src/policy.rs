use std::fmt;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum Capability {
    Read,
    Create,
    Update,
    Delete,
    #[allow(dead_code)] // Reserved for explicit workflow tools and method calls.
    Workflow,
    #[allow(dead_code)] // Reserved for financially sensitive operations.
    Financial,
    #[allow(dead_code)] // Reserved for administrative operations.
    Admin,
}

impl Capability {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Create => "create",
            Self::Update => "update",
            Self::Delete => "delete",
            Self::Workflow => "workflow",
            Self::Financial => "financial",
            Self::Admin => "admin",
        }
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_capabilities_have_stable_names() {
        assert_eq!(Capability::Read.as_str(), "read");
        assert_eq!(Capability::Create.as_str(), "create");
        assert_eq!(Capability::Update.as_str(), "update");
        assert_eq!(Capability::Delete.as_str(), "delete");
        assert_eq!(Capability::Workflow.as_str(), "workflow");
        assert_eq!(Capability::Financial.as_str(), "financial");
        assert_eq!(Capability::Admin.as_str(), "admin");
    }
}
