//! Alarm rule wire shape.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case", tag = "op")]
pub enum Condition {
    /// Trip when value > threshold.
    Gt { threshold: f64 },
    /// Trip when value >= threshold.
    Gte { threshold: f64 },
    /// Trip when value < threshold.
    Lt { threshold: f64 },
    /// Trip when value <= threshold.
    Lte { threshold: f64 },
    /// Trip when value == threshold (within `epsilon` for floats).
    Eq { threshold: f64, epsilon: f64 },
    /// Trip when value != threshold (within `epsilon` for floats).
    Ne { threshold: f64, epsilon: f64 },
}

impl Condition {
    /// True when the supplied value satisfies the trip condition.
    pub fn evaluate(&self, value: f64) -> bool {
        match self {
            Condition::Gt { threshold } => value > *threshold,
            Condition::Gte { threshold } => value >= *threshold,
            Condition::Lt { threshold } => value < *threshold,
            Condition::Lte { threshold } => value <= *threshold,
            Condition::Eq { threshold, epsilon } => (value - *threshold).abs() <= *epsilon,
            Condition::Ne { threshold, epsilon } => (value - *threshold).abs() > *epsilon,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AlarmRule {
    /// Stable identifier, used in [`crate::AlarmState::rule_id`].
    pub id: String,
    /// Fully-qualified point id this rule watches.
    pub node_id: String,
    pub condition: Condition,
    pub severity: Severity,
    /// Operator-readable message; format string with `{value}`.
    pub message_template: String,
    /// Disabled rules are skipped by [`crate::AlarmEngine::evaluate`].
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

impl AlarmRule {
    pub fn message_for(&self, value: f64) -> String {
        self.message_template.replace("{value}", &format!("{value}"))
    }
}
