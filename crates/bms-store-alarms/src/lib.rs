//! bms-store-alarms — threshold-based alarm engine that runs as a
//! sibling to bms-store rather than inside it.
//!
//! The engine consumes a stream of value-change events (the bms-store
//! event bus, MQTT, or any source that yields
//! [`crate::event::ValueChanged`]), evaluates configured
//! [`rule::AlarmRule`]s, and writes back to a pluggable
//! [`sink::AlarmSink`] — typically a small client that POSTs alarm
//! entities through the bms-store REST API.
//!
//! Why a sibling crate rather than in-process?
//!
//! - Forces the bms-store consumer API to be good (dogfood).
//! - Makes alarming optional — deployments that don't want it just
//!   don't run this crate.
//! - Lets the alarm rule schema evolve independently of the data
//!   layer.
//!
//! v1 surface (this scaffold):
//!
//! - [`rule::AlarmRule`] + [`rule::Condition`] — the wire shape for
//!   threshold rules
//! - [`state::AlarmState`] + lifecycle (Active → Ack → Cleared)
//! - [`engine::AlarmEngine`] — pure-function evaluator: takes a
//!   value, returns the resulting state transitions. Persistence and
//!   wire I/O are caller's responsibility for now.
//!
//! v1.1+: SQLite persistence, embedded HTTP server for ack endpoint,
//! built-in webhook fan-out, deadband / on-delay / off-delay timing.

pub mod engine;
pub mod event;
pub mod rule;
pub mod sink;
pub mod state;

pub use engine::AlarmEngine;
pub use event::ValueChanged;
pub use rule::{AlarmRule, Condition, Severity};
pub use sink::{AlarmSink, NullSink};
pub use state::{AlarmState, AlarmStatus};
