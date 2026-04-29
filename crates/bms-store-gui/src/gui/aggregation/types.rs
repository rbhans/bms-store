//! Cross-site aggregation DTOs.
//!
//! These types are the cross-site view of per-site alarm and energy data.
//! They are intentionally simpler than the storage-layer types so that
//! HTTP-backed remote stores can produce them without a storage dependency.

use bms_store_storage::store::alarm_store::AlarmSeverity;

// ----------------------------------------------------------------
// Shared error type
// ----------------------------------------------------------------

/// An error that occurred while querying a single site's data.
#[derive(Debug, thiserror::Error, Clone)]
pub enum AggregatorError {
    #[error("site unreachable: {0}")]
    Unreachable(String),
    #[error("query failed: {0}")]
    Query(String),
}

// ----------------------------------------------------------------
// Alarm DTOs
// ----------------------------------------------------------------

/// Simplified active alarm for cross-site aggregation views.
#[derive(Debug, Clone)]
pub struct SiteActiveAlarm {
    pub config_id: i64,
    pub device_id: String,
    pub point_id: String,
    pub severity: AlarmSeverity,
    pub trigger_value: f64,
    pub trigger_time_ms: i64,
    pub ack_time_ms: Option<i64>,
}

/// Simplified alarm history event for cross-site aggregation views.
#[derive(Debug, Clone)]
pub struct SiteAlarmEvent {
    pub config_id: i64,
    pub device_id: String,
    pub point_id: String,
    pub severity: AlarmSeverity,
    pub timestamp_ms: i64,
    pub from_state: String,
    pub to_state: String,
    pub value: f64,
}

// ----------------------------------------------------------------
// Energy DTOs
// ----------------------------------------------------------------

/// Simplified energy meter for cross-site aggregation views.
#[derive(Debug, Clone)]
pub struct SiteMeter {
    pub id: i64,
    pub name: String,
}

/// Simplified daily rollup for cross-site aggregation views.
#[derive(Debug, Clone)]
pub struct SiteDailyRollup {
    pub period_start_ms: i64,
    pub consumption_kwh: f64,
    pub peak_demand_kw: f64,
    pub avg_kw: f64,
    pub cost: f64,
}
