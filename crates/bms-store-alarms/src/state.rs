//! Alarm lifecycle state.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AlarmStatus {
    /// Trip condition currently true and unacknowledged.
    Active,
    /// Operator acknowledged; condition may still be true.
    Acknowledged,
    /// Trip condition returned to normal — alarm closed.
    Cleared,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AlarmState {
    pub id: String,
    pub rule_id: String,
    pub node_id: String,
    pub status: AlarmStatus,
    pub triggered_ts_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acknowledged_ts_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cleared_ts_ms: Option<i64>,
    pub triggered_value: f64,
    pub message: String,
}
