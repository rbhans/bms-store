use serde::{Deserialize, Serialize};

/// Haystack `Number` — float value with optional unit string.
///
/// Units carry the canonical xeto unit code (e.g. `"degF"`, `"kW"`, `"%"`).
/// Cross-unit comparison is the caller's responsibility; this type does not
/// auto-convert between units. `unit: None` is unitless.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Number {
    pub val: f64,
    pub unit: Option<String>,
}

impl Number {
    pub const fn unitless(val: f64) -> Self {
        Self { val, unit: None }
    }

    pub fn with_unit(val: f64, unit: impl Into<String>) -> Self {
        Self {
            val,
            unit: Some(unit.into()),
        }
    }

    pub fn is_nan(&self) -> bool {
        self.val.is_nan()
    }

    pub fn is_inf(&self) -> bool {
        self.val.is_infinite()
    }
}

impl From<f64> for Number {
    fn from(v: f64) -> Self {
        Self::unitless(v)
    }
}

impl From<i64> for Number {
    fn from(v: i64) -> Self {
        Self::unitless(v as f64)
    }
}
