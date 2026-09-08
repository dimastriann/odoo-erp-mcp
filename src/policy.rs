use std::fmt;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum Capability {
    Read,
    Create,
    Update,
    Delete,
}

impl Capability {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Create => "create",
            Self::Update => "update",
            Self::Delete => "delete",
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
    }
}
