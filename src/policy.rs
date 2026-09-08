use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
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

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct CapabilityPermissions {
    #[serde(default)]
    pub(crate) allow: Vec<Capability>,
    #[serde(default)]
    pub(crate) deny: Vec<Capability>,
}

impl CapabilityPermissions {
    pub(crate) fn decision_for(&self, capability: Capability) -> Option<PolicyDecision> {
        if self.deny.contains(&capability) {
            Some(PolicyDecision::Deny)
        } else if self.allow.contains(&capability) {
            Some(PolicyDecision::Allow)
        } else {
            None
        }
    }
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PolicyDecision {
    Allow,
    Deny,
}

impl PolicyDecision {
    pub(crate) const fn from_allowed(allowed: bool) -> Self {
        if allowed { Self::Allow } else { Self::Deny }
    }

    pub(crate) const fn is_denied(self) -> bool {
        matches!(self, Self::Deny)
    }
}

pub(crate) fn evaluate_legacy_mode(mode: &str, capability: Capability) -> PolicyDecision {
    let allowed = match mode {
        "read_only" => capability == Capability::Read,
        "crud" => matches!(
            capability,
            Capability::Read | Capability::Create | Capability::Update | Capability::Delete
        ),
        _ => false,
    };
    PolicyDecision::from_allowed(allowed)
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

    #[test]
    fn policy_decisions_are_explicit() {
        assert_eq!(PolicyDecision::from_allowed(true), PolicyDecision::Allow);
        assert_eq!(PolicyDecision::from_allowed(false), PolicyDecision::Deny);
        assert!(!PolicyDecision::Allow.is_denied());
        assert!(PolicyDecision::Deny.is_denied());
    }

    #[test]
    fn instance_permissions_return_explicit_matches() {
        let permissions = CapabilityPermissions {
            allow: vec![Capability::Read],
            deny: vec![Capability::Delete],
        };

        assert_eq!(
            permissions.decision_for(Capability::Read),
            Some(PolicyDecision::Allow)
        );
        assert_eq!(
            permissions.decision_for(Capability::Delete),
            Some(PolicyDecision::Deny)
        );
        assert_eq!(permissions.decision_for(Capability::Update), None);
    }

    #[test]
    fn legacy_modes_map_to_capabilities() {
        assert_eq!(
            evaluate_legacy_mode("read_only", Capability::Read),
            PolicyDecision::Allow
        );
        assert_eq!(
            evaluate_legacy_mode("read_only", Capability::Create),
            PolicyDecision::Deny
        );
        assert_eq!(
            evaluate_legacy_mode("crud", Capability::Delete),
            PolicyDecision::Allow
        );
        assert_eq!(
            evaluate_legacy_mode("unknown", Capability::Read),
            PolicyDecision::Deny
        );
    }
}
