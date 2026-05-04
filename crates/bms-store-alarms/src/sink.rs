//! Pluggable alarm output. The engine produces transitions; the sink
//! decides where they go (REST POST, log file, in-memory buffer for
//! tests).

use crate::state::AlarmState;

/// Side-effect target for alarm transitions. Implementations should
/// be cheap to call from the engine's hot path — push notifications
/// to a channel rather than blocking on network I/O when possible.
pub trait AlarmSink: Send + Sync {
    /// Called when a rule trips and a new active alarm is born.
    fn on_triggered(&self, alarm: &AlarmState);
    /// Called when an active alarm's trip condition returns to normal.
    fn on_cleared(&self, alarm: &AlarmState);
}

/// Drops every event. Useful as a default in tests / benchmarks.
pub struct NullSink;

impl AlarmSink for NullSink {
    fn on_triggered(&self, _alarm: &AlarmState) {}
    fn on_cleared(&self, _alarm: &AlarmState) {}
}

/// In-memory recorder — collects every transition. Useful in tests.
#[derive(Default)]
pub struct VecSink {
    pub triggered: std::sync::Mutex<Vec<AlarmState>>,
    pub cleared: std::sync::Mutex<Vec<AlarmState>>,
}

impl AlarmSink for VecSink {
    fn on_triggered(&self, alarm: &AlarmState) {
        self.triggered.lock().unwrap().push(alarm.clone());
    }
    fn on_cleared(&self, alarm: &AlarmState) {
        self.cleared.lock().unwrap().push(alarm.clone());
    }
}
