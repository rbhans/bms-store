use serde::{Deserialize, Serialize};

use crate::config::profile::PointValue;

// ----------------------------------------------------------------
// Public types
// ----------------------------------------------------------------

pub type ScheduleId = i64;
pub type ExceptionGroupId = i64;
pub type AssignmentId = i64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TimeOfDay {
    pub hour: u8,
    pub minute: u8,
}

impl TimeOfDay {
    pub fn new(hour: u8, minute: u8) -> Self {
        Self { hour, minute }
    }

    pub fn total_minutes(&self) -> u16 {
        self.hour as u16 * 60 + self.minute as u16
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimeSlot {
    pub time: TimeOfDay,
    pub value: PointValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DaySlots(pub Vec<TimeSlot>);

impl DaySlots {
    /// Ensure slots are sorted by time ascending.
    pub fn sort(&mut self) {
        self.0.sort_by_key(|s| s.time);
    }
}

/// 7-element array: 0=Monday .. 6=Sunday.
pub type WeeklySchedule = [DaySlots; 7];

pub fn empty_weekly() -> WeeklySchedule {
    std::array::from_fn(|_| DaySlots::default())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleValueType {
    Binary,
    Analog,
    Multistate,
}

impl ScheduleValueType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Binary => "binary",
            Self::Analog => "analog",
            Self::Multistate => "multistate",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "binary" => Some(Self::Binary),
            "analog" => Some(Self::Analog),
            "multistate" => Some(Self::Multistate),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Binary => "Binary",
            Self::Analog => "Analog",
            Self::Multistate => "Multistate",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DateSpec {
    /// Recurring annual date (e.g. Jan 1).
    Fixed { month: u8, day: u8 },
    /// One-off date in a specific year.
    FixedYear { year: u16, month: u8, day: u8 },
    /// Relative date (e.g. fourth Thursday in November).
    Relative {
        ordinal: Ordinal,
        weekday: u8,
        month: u8,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ordinal {
    First,
    Second,
    Third,
    Fourth,
    Last,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Schedule {
    pub id: ScheduleId,
    pub name: String,
    pub description: String,
    pub value_type: ScheduleValueType,
    pub default_value: PointValue,
    pub enabled: bool,
    pub weekly: WeeklySchedule,
    pub created_ms: i64,
    pub updated_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExceptionGroup {
    pub id: ExceptionGroupId,
    pub name: String,
    pub description: String,
    pub entries: Vec<DateSpec>,
    pub created_ms: i64,
    pub updated_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleException {
    pub id: i64,
    pub schedule_id: ScheduleId,
    pub group_id: Option<ExceptionGroupId>,
    pub name: String,
    pub date_spec: DateSpec,
    pub slots: DaySlots,
    pub use_default: bool,
    pub created_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleAssignment {
    pub id: AssignmentId,
    pub schedule_id: ScheduleId,
    pub device_id: String,
    pub point_id: String,
    pub priority: i32,
    pub enabled: bool,
    pub created_ms: i64,
}

#[derive(Debug, Clone)]
pub struct ScheduleLogEntry {
    pub id: i64,
    pub assignment_id: AssignmentId,
    pub device_id: String,
    pub point_id: String,
    pub value_json: String,
    pub reason: String,
    pub timestamp_ms: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum ScheduleError {
    #[error("database error: {0}")]
    Db(String),
    #[error("channel closed")]
    ChannelClosed,
    #[error("not found")]
    NotFound,
}

#[derive(Debug, Clone)]
pub struct ScheduleConflict {
    pub device_id: String,
    pub point_id: String,
    pub assignments: Vec<ScheduleAssignment>,
}
