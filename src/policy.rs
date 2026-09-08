use std::collections::BTreeMap;
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
    #[serde(default)]
    pub(crate) models: BTreeMap<String, CapabilityRule>,
    #[serde(default)]
    pub(crate) operations: BTreeMap<String, PolicyDecision>,
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

    pub(crate) fn decision_for_model(
        &self,
        model: &str,
        capability: Capability,
    ) -> Option<PolicyDecision> {
        self.models
            .get(model)
            .and_then(|rule| rule.decision_for(capability))
            .or_else(|| self.decision_for(capability))
    }

    pub(crate) fn decision_for_operation(
        &self,
        operation: &str,
        model: &str,
        capability: Capability,
    ) -> Option<PolicyDecision> {
        self.operations
            .get(operation)
            .copied()
            .or_else(|| self.decision_for_model(model, capability))
    }

    /// Evaluates rules from most to least specific: field denial, method,
    /// operation, model capability, then instance capability. Within a single
    /// capability rule, deny takes precedence over allow.
    pub(crate) fn decision_for_request(
        &self,
        operation: &str,
        model: &str,
        method: Option<&str>,
        capability: Capability,
        fields: &[String],
    ) -> Option<PolicyDecision> {
        let field_denied = self
            .models
            .get(model)
            .is_some_and(|rule| rule.fields.denies(fields));
        if field_denied {
            Some(PolicyDecision::Deny)
        } else if let Some(decision) = method
            .and_then(|method| self.models.get(model)?.methods.get(method))
            .copied()
        {
            Some(decision)
        } else {
            self.decision_for_operation(operation, model, capability)
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct CapabilityRule {
    #[serde(default)]
    pub(crate) allow: Vec<Capability>,
    #[serde(default)]
    pub(crate) deny: Vec<Capability>,
    #[serde(default)]
    pub(crate) fields: FieldPermissions,
    #[serde(default)]
    pub(crate) methods: BTreeMap<String, PolicyDecision>,
}

impl CapabilityRule {
    fn decision_for(&self, capability: Capability) -> Option<PolicyDecision> {
        if self.deny.contains(&capability) {
            Some(PolicyDecision::Deny)
        } else if self.allow.contains(&capability) {
            Some(PolicyDecision::Allow)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct FieldPermissions {
    #[serde(default)]
    pub(crate) allow: Vec<String>,
    #[serde(default)]
    pub(crate) deny: Vec<String>,
}

impl FieldPermissions {
    fn denies(&self, requested: &[String]) -> bool {
        requested.iter().any(|field| self.deny.contains(field))
            || (!self.allow.is_empty() && requested.iter().any(|field| !self.allow.contains(field)))
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
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
            models: BTreeMap::new(),
            operations: BTreeMap::new(),
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
    fn model_permissions_override_instance_capabilities() {
        let permissions: CapabilityPermissions = serde_json::from_value(serde_json::json!({
            "allow": ["read"],
            "models": {
                "account.move": { "deny": ["read"] }
            }
        }))
        .unwrap();

        assert_eq!(
            permissions.decision_for_model("account.move", Capability::Read),
            Some(PolicyDecision::Deny)
        );
        assert_eq!(
            permissions.decision_for_model("res.partner", Capability::Read),
            Some(PolicyDecision::Allow)
        );
    }

    #[test]
    fn operation_permissions_override_broader_rules() {
        let permissions: CapabilityPermissions = serde_json::from_value(serde_json::json!({
            "allow": ["read"],
            "operations": {
                "odoo-search-read": "deny"
            }
        }))
        .unwrap();

        assert_eq!(
            permissions.decision_for_operation("odoo-search-read", "res.partner", Capability::Read),
            Some(PolicyDecision::Deny)
        );
        assert_eq!(
            permissions.decision_for_operation(
                "odoo-search-count",
                "res.partner",
                Capability::Read
            ),
            Some(PolicyDecision::Allow)
        );
    }

    #[test]
    fn field_rules_reject_denied_and_unlisted_fields() {
        let permissions: CapabilityPermissions = serde_json::from_value(serde_json::json!({
            "allow": ["read"],
            "models": {
                "res.partner": {
                    "fields": {
                        "allow": ["name", "email"],
                        "deny": ["bank_ids"]
                    }
                }
            }
        }))
        .unwrap();

        for fields in [vec!["bank_ids".into()], vec!["phone".into()]] {
            assert_eq!(
                permissions.decision_for_request(
                    "odoo-search-read",
                    "res.partner",
                    None,
                    Capability::Read,
                    &fields
                ),
                Some(PolicyDecision::Deny)
            );
        }
        assert_eq!(
            permissions.decision_for_request(
                "odoo-search-read",
                "res.partner",
                None,
                Capability::Read,
                &["name".into(), "email".into()]
            ),
            Some(PolicyDecision::Allow)
        );
    }

    #[test]
    fn method_and_workflow_rules_are_explicit() {
        let permissions: CapabilityPermissions = serde_json::from_value(serde_json::json!({
            "models": {
                "sale.order": {
                    "allow": ["workflow"],
                    "methods": {
                        "action_confirm": "allow",
                        "action_cancel": "deny"
                    }
                }
            }
        }))
        .unwrap();

        assert_eq!(
            permissions.decision_for_request(
                "odoo-method",
                "sale.order",
                Some("action_confirm"),
                Capability::Workflow,
                &[]
            ),
            Some(PolicyDecision::Allow)
        );
        assert_eq!(
            permissions.decision_for_request(
                "odoo-method",
                "sale.order",
                Some("action_cancel"),
                Capability::Workflow,
                &[]
            ),
            Some(PolicyDecision::Deny)
        );
    }

    #[test]
    fn policy_precedence_is_narrowest_scope_first() {
        let permissions: CapabilityPermissions = serde_json::from_value(serde_json::json!({
            "deny": ["read"],
            "operations": { "odoo-search-read": "deny" },
            "models": {
                "res.partner": {
                    "allow": ["read"],
                    "fields": { "deny": ["bank_ids"] },
                    "methods": { "export_data": "allow" }
                }
            }
        }))
        .unwrap();

        assert_eq!(
            permissions.decision_for_request(
                "odoo-search-read",
                "res.partner",
                Some("export_data"),
                Capability::Read,
                &["bank_ids".into()]
            ),
            Some(PolicyDecision::Deny),
            "field denial must override a method allow"
        );
        assert_eq!(
            permissions.decision_for_request(
                "odoo-search-read",
                "res.partner",
                Some("export_data"),
                Capability::Read,
                &[]
            ),
            Some(PolicyDecision::Allow),
            "method rules must override operation rules"
        );
        assert_eq!(
            permissions.decision_for_request(
                "odoo-search-read",
                "res.partner",
                None,
                Capability::Read,
                &[]
            ),
            Some(PolicyDecision::Deny),
            "operation rules must override model rules"
        );
        assert_eq!(
            permissions.decision_for_request(
                "odoo-search-count",
                "res.partner",
                None,
                Capability::Read,
                &[]
            ),
            Some(PolicyDecision::Allow),
            "model rules must override instance rules"
        );
    }

    #[test]
    fn deny_wins_over_allow_within_one_scope() {
        let permissions: CapabilityPermissions = serde_json::from_value(serde_json::json!({
            "allow": ["delete"],
            "deny": ["delete"]
        }))
        .unwrap();

        assert_eq!(
            permissions.decision_for(Capability::Delete),
            Some(PolicyDecision::Deny)
        );
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
