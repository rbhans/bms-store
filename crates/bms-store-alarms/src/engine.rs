//! Alarm rule evaluation engine.
//!
//! Holds rules + active alarm state in-memory; consumes
//! [`crate::ValueChanged`] events; emits lifecycle transitions to a
//! [`crate::sink::AlarmSink`].
//!
//! Persistence (SQLite for rules + active alarms across restarts) is
//! v1.1+; this scaffold is in-memory for unit tests + the v1.0
//! self-test reference flow.

use std::collections::HashMap;
use std::sync::RwLock;

use crate::event::ValueChanged;
use crate::rule::AlarmRule;
use crate::sink::AlarmSink;
use crate::state::{AlarmState, AlarmStatus};

pub struct AlarmEngine {
    rules: RwLock<Vec<AlarmRule>>,
    /// Active alarms keyed by rule_id. v1 invariant: at most one
    /// active alarm per rule. A re-trip while still active is a no-op
    /// (no flapping notifications); ack flips status; return-to-normal
    /// fires on_cleared and removes the entry.
    active: RwLock<HashMap<String, AlarmState>>,
    /// Monotonic counter for synthesized alarm ids.
    next_id: RwLock<u64>,
}

impl Default for AlarmEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl AlarmEngine {
    pub fn new() -> Self {
        AlarmEngine {
            rules: RwLock::new(Vec::new()),
            active: RwLock::new(HashMap::new()),
            next_id: RwLock::new(1),
        }
    }

    pub fn add_rule(&self, rule: AlarmRule) {
        self.rules.write().unwrap().push(rule);
    }

    pub fn rules(&self) -> Vec<AlarmRule> {
        self.rules.read().unwrap().clone()
    }

    pub fn active_alarms(&self) -> Vec<AlarmState> {
        self.active.read().unwrap().values().cloned().collect()
    }

    /// Evaluate a value-change event against every enabled rule that
    /// targets the same node_id. Emits side-effects through `sink`.
    pub fn evaluate(&self, event: &ValueChanged, sink: &dyn AlarmSink) {
        let matching: Vec<AlarmRule> = self
            .rules
            .read()
            .unwrap()
            .iter()
            .filter(|r| r.enabled && r.node_id == event.node_id)
            .cloned()
            .collect();

        for rule in matching {
            let tripped = rule.condition.evaluate(event.value);
            let mut active = self.active.write().unwrap();
            let entry = active.get(&rule.id).cloned();
            match (entry, tripped) {
                (None, true) => {
                    let id = self.next_alarm_id();
                    let alarm = AlarmState {
                        id,
                        rule_id: rule.id.clone(),
                        node_id: rule.node_id.clone(),
                        status: AlarmStatus::Active,
                        triggered_ts_ms: event.ts_ms,
                        acknowledged_ts_ms: None,
                        cleared_ts_ms: None,
                        triggered_value: event.value,
                        message: rule.message_for(event.value),
                    };
                    active.insert(rule.id.clone(), alarm.clone());
                    drop(active);
                    sink.on_triggered(&alarm);
                }
                (Some(_existing), true) => {
                    // Already active and still tripped — suppress duplicate.
                }
                (Some(mut existing), false) => {
                    existing.status = AlarmStatus::Cleared;
                    existing.cleared_ts_ms = Some(event.ts_ms);
                    active.remove(&rule.id);
                    drop(active);
                    sink.on_cleared(&existing);
                }
                (None, false) => {
                    // Steady-state normal — nothing to do.
                }
            }
        }
    }

    /// Operator acknowledgement. Returns the updated state if found.
    pub fn acknowledge(&self, rule_id: &str, ts_ms: i64) -> Option<AlarmState> {
        let mut active = self.active.write().unwrap();
        let entry = active.get_mut(rule_id)?;
        entry.status = AlarmStatus::Acknowledged;
        entry.acknowledged_ts_ms = Some(ts_ms);
        Some(entry.clone())
    }

    fn next_alarm_id(&self) -> String {
        let mut n = self.next_id.write().unwrap();
        let id = format!("alarm-{:08}", *n);
        *n += 1;
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::{Condition, Severity};
    use crate::sink::VecSink;

    fn rule(id: &str, node_id: &str, cond: Condition) -> AlarmRule {
        AlarmRule {
            id: id.into(),
            node_id: node_id.into(),
            condition: cond,
            severity: Severity::Warning,
            message_template: "value={value}".into(),
            enabled: true,
        }
    }

    fn ev(node_id: &str, value: f64, ts: i64) -> ValueChanged {
        ValueChanged {
            node_id: node_id.into(),
            value,
            ts_ms: ts,
        }
    }

    #[test]
    fn trip_once_then_suppress_until_cleared() {
        let eng = AlarmEngine::new();
        eng.add_rule(rule(
            "high-temp",
            "ahu/dat",
            Condition::Gt { threshold: 80.0 },
        ));
        let sink = VecSink::default();

        // Below threshold — no trip.
        eng.evaluate(&ev("ahu/dat", 75.0, 1), &sink);
        assert_eq!(sink.triggered.lock().unwrap().len(), 0);

        // Cross threshold — trip once.
        eng.evaluate(&ev("ahu/dat", 85.0, 2), &sink);
        assert_eq!(sink.triggered.lock().unwrap().len(), 1);

        // Stays high — suppressed.
        eng.evaluate(&ev("ahu/dat", 90.0, 3), &sink);
        assert_eq!(sink.triggered.lock().unwrap().len(), 1);

        // Returns to normal — clear fires.
        eng.evaluate(&ev("ahu/dat", 70.0, 4), &sink);
        assert_eq!(sink.cleared.lock().unwrap().len(), 1);

        // Trips again later — counts as new alarm.
        eng.evaluate(&ev("ahu/dat", 95.0, 5), &sink);
        assert_eq!(sink.triggered.lock().unwrap().len(), 2);
    }

    #[test]
    fn other_nodes_dont_trip() {
        let eng = AlarmEngine::new();
        eng.add_rule(rule(
            "high",
            "ahu/dat",
            Condition::Gt { threshold: 80.0 },
        ));
        let sink = VecSink::default();
        eng.evaluate(&ev("vav/dat", 999.0, 1), &sink);
        assert_eq!(sink.triggered.lock().unwrap().len(), 0);
    }

    #[test]
    fn disabled_rule_skipped() {
        let eng = AlarmEngine::new();
        let mut r = rule("off", "x", Condition::Gt { threshold: 0.0 });
        r.enabled = false;
        eng.add_rule(r);
        let sink = VecSink::default();
        eng.evaluate(&ev("x", 100.0, 1), &sink);
        assert!(sink.triggered.lock().unwrap().is_empty());
    }

    #[test]
    fn acknowledge_changes_status() {
        let eng = AlarmEngine::new();
        eng.add_rule(rule(
            "r1",
            "n1",
            Condition::Gt { threshold: 0.0 },
        ));
        let sink = VecSink::default();
        eng.evaluate(&ev("n1", 5.0, 10), &sink);
        let acked = eng.acknowledge("r1", 20).expect("ack");
        assert_eq!(acked.status, AlarmStatus::Acknowledged);
        assert_eq!(acked.acknowledged_ts_ms, Some(20));
    }

    #[test]
    fn message_template_substitutes_value() {
        let eng = AlarmEngine::new();
        eng.add_rule(AlarmRule {
            id: "r".into(),
            node_id: "n".into(),
            condition: Condition::Gt { threshold: 0.0 },
            severity: Severity::Critical,
            message_template: "temp too high: {value}".into(),
            enabled: true,
        });
        let sink = VecSink::default();
        eng.evaluate(&ev("n", 99.5, 1), &sink);
        let triggered = sink.triggered.lock().unwrap();
        assert_eq!(triggered.len(), 1);
        assert_eq!(triggered[0].message, "temp too high: 99.5");
    }

    #[test]
    fn condition_eq_within_epsilon() {
        let cond = Condition::Eq {
            threshold: 1.0,
            epsilon: 0.01,
        };
        assert!(cond.evaluate(1.005));
        assert!(!cond.evaluate(1.5));
    }
}
