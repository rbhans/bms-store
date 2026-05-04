//! Inbound value-change events the engine evaluates.
//!
//! Re-shaped from `bms_core` events (which the data layer produces)
//! so the alarms crate doesn't need to know the source — anything
//! that can construct a [`ValueChanged`] can drive the engine.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValueChanged {
    /// Fully-qualified node id, e.g. `"ahu-1/discharge-air-temp"`.
    pub node_id: String,
    /// Numeric value. Bools are pre-coerced to 0.0 / 1.0 by the caller.
    pub value: f64,
    /// Wall-clock timestamp the source measured the value (Unix ms).
    /// Falls back to ingest time when the protocol does not provide one.
    pub ts_ms: i64,
}
