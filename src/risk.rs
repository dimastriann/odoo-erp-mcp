#![allow(dead_code)] // Risk evaluation is integrated incrementally through S4-19.

use serde::{Deserialize, Serialize};

/// Ordered severity assigned to an operation before execution.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn risk_levels_have_a_stable_severity_order() {
        assert!(RiskLevel::Low < RiskLevel::Medium);
        assert!(RiskLevel::Medium < RiskLevel::High);
        assert!(RiskLevel::High < RiskLevel::Critical);
    }

    #[test]
    fn risk_levels_use_stable_configuration_names() {
        assert_eq!(serde_json::to_value(RiskLevel::Low).unwrap(), "low");
        assert_eq!(
            serde_json::to_value(RiskLevel::Critical).unwrap(),
            "critical"
        );
        assert_eq!(
            serde_json::from_value::<RiskLevel>(serde_json::json!("high")).unwrap(),
            RiskLevel::High
        );
    }
}
