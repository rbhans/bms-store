//! Core value types used across the BMS domain model.

use serde::{Deserialize, Serialize};

/// Type alias for node identifiers.
///
/// Convention: `"{device_instance_id}/{point_id}"` for points,
/// `"{device_instance_id}"` for equipment.
pub type NodeId = String;

/// A point value — the fundamental data carrier in the BAS.
///
/// Variant order matters for `serde(untagged)` deserialization:
/// Bool is tried first, then Integer, then Float.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PointValue {
    Bool(bool),
    Integer(i64),
    Float(f64),
}

impl PointValue {
    /// Convert any variant to f64 (bool: 0.0/1.0).
    pub fn as_f64(&self) -> f64 {
        match self {
            PointValue::Bool(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            PointValue::Integer(i) => *i as f64,
            PointValue::Float(f) => *f,
        }
    }
}

/// Bitfield status flags for a point.
///
/// Multiple flags can be active simultaneously. Use the associated constants
/// and helper methods to query and manipulate flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PointStatusFlags(pub u8);

impl PointStatusFlags {
    pub const ALARM: u8 = 0b0000_0001;
    pub const STALE: u8 = 0b0000_0010;
    pub const FAULT: u8 = 0b0000_0100;
    pub const OVERRIDDEN: u8 = 0b0000_1000;
    pub const DOWN: u8 = 0b0001_0000;
    pub const DISABLED: u8 = 0b0010_0000;

    pub fn has(self, flag: u8) -> bool {
        self.0 & flag != 0
    }
    pub fn set(&mut self, flag: u8) {
        self.0 |= flag;
    }
    pub fn clear(&mut self, flag: u8) {
        self.0 &= !flag;
    }
    pub fn is_normal(self) -> bool {
        self.0 == 0
    }

    /// Returns the highest-priority active flag name for display.
    pub fn worst_status(self) -> Option<&'static str> {
        if self.has(Self::DOWN) {
            Some("down")
        } else if self.has(Self::FAULT) {
            Some("fault")
        } else if self.has(Self::ALARM) {
            Some("alarm")
        } else if self.has(Self::OVERRIDDEN) {
            Some("overridden")
        } else if self.has(Self::STALE) {
            Some("stale")
        } else if self.has(Self::DISABLED) {
            Some("disabled")
        } else {
            None
        }
    }

    /// Returns all active flag names.
    pub fn active_flags(self) -> Vec<&'static str> {
        let mut flags = Vec::new();
        if self.has(Self::DOWN) {
            flags.push("down");
        }
        if self.has(Self::FAULT) {
            flags.push("fault");
        }
        if self.has(Self::ALARM) {
            flags.push("alarm");
        }
        if self.has(Self::OVERRIDDEN) {
            flags.push("overridden");
        }
        if self.has(Self::STALE) {
            flags.push("stale");
        }
        if self.has(Self::DISABLED) {
            flags.push("disabled");
        }
        flags
    }
}
