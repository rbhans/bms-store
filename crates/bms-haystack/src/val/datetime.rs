use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use serde::{Deserialize, Serialize};

/// Calendar date without time-of-day (`YYYY-MM-DD`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HDate(pub NaiveDate);

impl HDate {
    pub fn parse(s: &str) -> Option<Self> {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").ok().map(Self)
    }

    pub fn to_iso(&self) -> String {
        self.0.format("%Y-%m-%d").to_string()
    }
}

/// Time of day with optional fractional seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HTime(pub NaiveTime);

impl HTime {
    pub fn parse(s: &str) -> Option<Self> {
        NaiveTime::parse_from_str(s, "%H:%M:%S%.f")
            .or_else(|_| NaiveTime::parse_from_str(s, "%H:%M:%S"))
            .ok()
            .map(Self)
    }

    pub fn to_iso(&self) -> String {
        self.0.format("%H:%M:%S%.f").to_string()
    }
}

/// Timezone-aware date+time. Hayson carries `val` (ISO instant) and `tz`
/// (IANA zone name) separately; we store a `DateTime<Utc>` with a `tz`
/// string and let the caller convert when needed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HDateTime {
    pub val: DateTime<Utc>,
    pub tz: String,
}

impl HDateTime {
    pub fn new(val: DateTime<Utc>, tz: impl Into<String>) -> Self {
        Self {
            val,
            tz: tz.into(),
        }
    }

    pub fn parse(val: &str, tz: &str) -> Option<Self> {
        DateTime::parse_from_rfc3339(val)
            .ok()
            .map(|dt| Self {
                val: dt.with_timezone(&Utc),
                tz: tz.to_string(),
            })
    }

    pub fn to_iso(&self) -> String {
        self.val.to_rfc3339()
    }
}
