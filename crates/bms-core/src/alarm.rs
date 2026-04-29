//! Alarm types used by the [`AlarmEvaluator`](crate::plugin::AlarmEvaluator) trait.

use serde::{Deserialize, Serialize};

/// Alarm configuration ID (database primary key).
pub type AlarmConfigId = i64;

/// The kind of alarm condition being monitored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlarmType {
    HighLimit,
    LowLimit,
    StateFault,
    Stale,
    Deviation,
    StateChange,
    MultiStateAlarm,
    CommandMismatch,
}

impl AlarmType {
    pub fn label(&self) -> &'static str {
        match self {
            Self::HighLimit => "High Limit",
            Self::LowLimit => "Low Limit",
            Self::StateFault => "State Fault",
            Self::Stale => "Stale",
            Self::Deviation => "Deviation",
            Self::StateChange => "State Change",
            Self::MultiStateAlarm => "Multi-State",
            Self::CommandMismatch => "Cmd Mismatch",
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::HighLimit => "high_limit",
            Self::LowLimit => "low_limit",
            Self::StateFault => "state_fault",
            Self::Stale => "stale",
            Self::Deviation => "deviation",
            Self::StateChange => "state_change",
            Self::MultiStateAlarm => "multi_state_alarm",
            Self::CommandMismatch => "command_mismatch",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "high_limit" => Some(Self::HighLimit),
            "low_limit" => Some(Self::LowLimit),
            "state_fault" => Some(Self::StateFault),
            "stale" => Some(Self::Stale),
            "deviation" => Some(Self::Deviation),
            "state_change" => Some(Self::StateChange),
            "multi_state_alarm" => Some(Self::MultiStateAlarm),
            "command_mismatch" => Some(Self::CommandMismatch),
            _ => None,
        }
    }
}

/// Alarm severity level, ordered from least to most severe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlarmSeverity {
    Info,
    Warning,
    Critical,
    LifeSafety,
}

impl AlarmSeverity {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Info => "Info",
            Self::Warning => "Warning",
            Self::Critical => "Critical",
            Self::LifeSafety => "Life Safety",
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Critical => "critical",
            Self::LifeSafety => "life_safety",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "info" => Some(Self::Info),
            "warning" => Some(Self::Warning),
            "critical" => Some(Self::Critical),
            "life_safety" => Some(Self::LifeSafety),
            _ => None,
        }
    }

    pub fn all() -> &'static [AlarmSeverity] {
        &[Self::Info, Self::Warning, Self::Critical, Self::LifeSafety]
    }
}

/// Parameters for an alarm condition. Tagged by alarm type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AlarmParams {
    HighLimit {
        limit: f64,
        #[serde(default)]
        deadband: f64,
        #[serde(default)]
        delay_secs: u64,
    },
    LowLimit {
        limit: f64,
        #[serde(default)]
        deadband: f64,
        #[serde(default)]
        delay_secs: u64,
    },
    StateFault {
        fault_value: f64,
        #[serde(default)]
        delay_secs: u64,
    },
    Stale {
        timeout_secs: u64,
    },
    Deviation {
        ref_device_id: String,
        ref_point_id: String,
        threshold: f64,
        #[serde(default)]
        deadband: f64,
        #[serde(default)]
        delay_secs: u64,
    },
    StateChange {
        alarm_value: bool,
        #[serde(default)]
        delay_secs: u64,
    },
    MultiStateAlarm {
        alarm_states: Vec<i64>,
        #[serde(default)]
        delay_secs: u64,
    },
    CommandMismatch {
        feedback_device_id: String,
        feedback_point_id: String,
        delay_secs: u64,
    },
}

impl AlarmParams {
    pub fn delay_secs(&self) -> u64 {
        match self {
            Self::HighLimit { delay_secs, .. } => *delay_secs,
            Self::LowLimit { delay_secs, .. } => *delay_secs,
            Self::StateFault { delay_secs, .. } => *delay_secs,
            Self::Stale { .. } => 0,
            Self::Deviation { delay_secs, .. } => *delay_secs,
            Self::StateChange { delay_secs, .. } => *delay_secs,
            Self::MultiStateAlarm { delay_secs, .. } => *delay_secs,
            Self::CommandMismatch { delay_secs, .. } => *delay_secs,
        }
    }

    pub fn alarm_type(&self) -> AlarmType {
        match self {
            Self::HighLimit { .. } => AlarmType::HighLimit,
            Self::LowLimit { .. } => AlarmType::LowLimit,
            Self::StateFault { .. } => AlarmType::StateFault,
            Self::Stale { .. } => AlarmType::Stale,
            Self::Deviation { .. } => AlarmType::Deviation,
            Self::StateChange { .. } => AlarmType::StateChange,
            Self::MultiStateAlarm { .. } => AlarmType::MultiStateAlarm,
            Self::CommandMismatch { .. } => AlarmType::CommandMismatch,
        }
    }
}

/// A complete alarm configuration — identifies what to monitor and how.
#[derive(Debug, Clone, PartialEq)]
pub struct AlarmConfig {
    pub id: AlarmConfigId,
    pub device_id: String,
    pub point_id: String,
    pub alarm_type: AlarmType,
    pub severity: AlarmSeverity,
    pub enabled: bool,
    pub params: AlarmParams,
}

impl AlarmConfig {
    pub fn new(
        id: AlarmConfigId,
        device_id: String,
        point_id: String,
        alarm_type: AlarmType,
        severity: AlarmSeverity,
        enabled: bool,
        params: AlarmParams,
    ) -> Self {
        Self {
            id,
            device_id,
            point_id,
            alarm_type,
            severity,
            enabled,
            params,
        }
    }
}

/// The state machine for an alarm instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlarmState {
    Normal,
    Offnormal,
    Acknowledged,
}

impl AlarmState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Offnormal => "offnormal",
            Self::Acknowledged => "acknowledged",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "normal" => Some(Self::Normal),
            "offnormal" => Some(Self::Offnormal),
            "acknowledged" => Some(Self::Acknowledged),
            _ => None,
        }
    }
}
