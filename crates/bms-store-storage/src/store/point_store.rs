use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::config::profile::{DeviceProfile, PointValue};
use crate::event::bus::{Event, EventBus};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PointKey {
    pub device_instance_id: String,
    pub point_id: String,
}

// Re-exported from the bms-core crate — the canonical definition lives there.
pub use bms_core::PointStatusFlags;

#[derive(Debug, Clone)]
pub struct TimestampedValue {
    pub value: PointValue,
    pub timestamp: Instant,
    pub status: PointStatusFlags,
}

#[derive(Clone)]
pub struct PointStore {
    data: Arc<RwLock<HashMap<PointKey, TimestampedValue>>>,
    version_tx: tokio::sync::watch::Sender<u64>,
    version_rx: tokio::sync::watch::Receiver<u64>,
    history_tx: tokio::sync::broadcast::Sender<(PointKey, PointValue)>,
    event_bus: Option<EventBus>,
}

impl Default for PointStore {
    fn default() -> Self {
        Self::new()
    }
}

impl PointStore {
    pub fn new() -> Self {
        let (version_tx, version_rx) = tokio::sync::watch::channel(0u64);
        let (history_tx, _) = tokio::sync::broadcast::channel(8192);
        PointStore {
            data: Arc::new(RwLock::new(HashMap::new())),
            version_tx,
            version_rx,
            history_tx,
            event_bus: None,
        }
    }

    pub fn with_event_bus(mut self, bus: EventBus) -> Self {
        self.event_bus = Some(bus);
        self
    }

    pub fn get(&self, key: &PointKey) -> Option<TimestampedValue> {
        self.data
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(key)
            .cloned()
    }

    pub fn set(&self, key: PointKey, value: PointValue) {
        let _ = self.history_tx.send((key.clone(), value.clone()));
        let mut data = self.data.write().unwrap_or_else(|e| e.into_inner());
        let existing_status = data.get(&key).map(|tv| tv.status).unwrap_or_default();
        data.insert(
            key.clone(),
            TimestampedValue {
                value: value.clone(),
                timestamp: Instant::now(),
                status: existing_status,
            },
        );
        drop(data);
        self.version_tx.send_modify(|v| *v += 1);

        if let Some(ref bus) = self.event_bus {
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;
            bus.publish(Event::ValueChanged {
                node_id: format!("{}/{}", key.device_instance_id, key.point_id),
                value,
                timestamp_ms: now_ms,
            });
        }
    }

    /// Like `set`, but only fires events/history if the value actually changed.
    /// Use in poll loops to avoid duplicate events when the device returns the same value.
    pub fn set_if_changed(&self, key: PointKey, value: PointValue) {
        let mut data = self.data.write().unwrap_or_else(|e| e.into_inner());
        let existing_status;
        if let Some(existing) = data.get(&key) {
            if existing.value == value {
                return;
            }
            existing_status = existing.status;
        } else {
            existing_status = PointStatusFlags::default();
        }
        data.insert(
            key.clone(),
            TimestampedValue {
                value: value.clone(),
                timestamp: Instant::now(),
                status: existing_status,
            },
        );
        drop(data);

        let _ = self.history_tx.send((key.clone(), value.clone()));
        self.version_tx.send_modify(|v| *v += 1);

        if let Some(ref bus) = self.event_bus {
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;
            bus.publish(Event::ValueChanged {
                node_id: format!("{}/{}", key.device_instance_id, key.point_id),
                value,
                timestamp_ms: now_ms,
            });
        }
    }

    pub fn get_all_for_device(
        &self,
        device_instance_id: &str,
    ) -> Vec<(PointKey, TimestampedValue)> {
        self.data
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|(k, _)| k.device_instance_id == device_instance_id)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    pub fn initialize_from_profile(&self, device_instance_id: &str, profile: &DeviceProfile) {
        for point in &profile.points {
            if let Some(initial) = &point.initial_value {
                let key = PointKey {
                    device_instance_id: device_instance_id.to_string(),
                    point_id: point.id.clone(),
                };
                let ts = TimestampedValue {
                    value: initial.clone(),
                    timestamp: Instant::now(),
                    status: PointStatusFlags::default(),
                };
                self.data
                    .write()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(key, ts);
            }
        }
        self.version_tx.send_modify(|v| *v += 1);
    }

    pub fn point_count(&self) -> usize {
        self.data.read().unwrap_or_else(|e| e.into_inner()).len()
    }

    pub fn device_ids(&self) -> Vec<String> {
        let data = self.data.read().unwrap_or_else(|e| e.into_inner());
        let mut ids: Vec<String> = data
            .keys()
            .map(|k| k.device_instance_id.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        ids.sort();
        ids
    }

    /// Set a status flag on a point (additive — does not clear other flags)
    pub fn set_status(&self, key: &PointKey, flag: u8) {
        let mut data = self.data.write().unwrap_or_else(|e| e.into_inner());
        let changed = if let Some(tv) = data.get_mut(key) {
            let old = tv.status.0;
            tv.status.set(flag);
            if tv.status.0 != old {
                Some(tv.status.0)
            } else {
                None
            }
        } else {
            None
        };
        drop(data);

        if let Some(flags) = changed {
            self.version_tx.send_modify(|v| *v += 1);
            if let Some(ref bus) = self.event_bus {
                bus.publish(Event::StatusChanged {
                    node_id: format!("{}/{}", key.device_instance_id, key.point_id),
                    flags,
                });
            }
        }
    }

    /// Clear a status flag on a point
    pub fn clear_status(&self, key: &PointKey, flag: u8) {
        let mut data = self.data.write().unwrap_or_else(|e| e.into_inner());
        let changed = if let Some(tv) = data.get_mut(key) {
            let old = tv.status.0;
            tv.status.clear(flag);
            if tv.status.0 != old {
                Some(tv.status.0)
            } else {
                None
            }
        } else {
            None
        };
        drop(data);

        if let Some(flags) = changed {
            self.version_tx.send_modify(|v| *v += 1);
            if let Some(ref bus) = self.event_bus {
                bus.publish(Event::StatusChanged {
                    node_id: format!("{}/{}", key.device_instance_id, key.point_id),
                    flags,
                });
            }
        }
    }

    /// Remove all points belonging to a specific device.
    /// Used during rescan to clear stale points before repopulating.
    pub fn remove_device_points(&self, device_instance_id: &str) {
        let mut data = self.data.write().unwrap_or_else(|e| e.into_inner());
        let before = data.len();
        data.retain(|k, _| k.device_instance_id != device_instance_id);
        if data.len() != before {
            drop(data);
            self.version_tx.send_modify(|v| *v += 1);
        }
    }

    /// Get all point keys (for status sync iteration)
    pub fn all_keys(&self) -> Vec<PointKey> {
        self.data
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .keys()
            .cloned()
            .collect()
    }

    /// Insert a default value for a point if it doesn't already exist.
    /// Does NOT fire events, history, or version bumps.
    /// Use this for hydration/acceptance to avoid spurious notifications.
    pub fn insert_default(&self, key: PointKey, value: PointValue) {
        let mut data = self.data.write().unwrap_or_else(|e| e.into_inner());
        data.entry(key).or_insert_with(|| TimestampedValue {
            value,
            timestamp: Instant::now(),
            status: PointStatusFlags::default(),
        });
    }

    /// Bump the version counter. Call after a batch of `insert_default` calls.
    pub fn bump_version(&self) {
        self.version_tx.send_modify(|v| *v += 1);
    }

    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<u64> {
        self.version_rx.clone()
    }

    pub fn subscribe_history(&self) -> tokio::sync::broadcast::Receiver<(PointKey, PointValue)> {
        self.history_tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_set_roundtrip() {
        let store = PointStore::new();
        let key = PointKey {
            device_instance_id: "ahu-1".to_string(),
            point_id: "dat".to_string(),
        };

        store.set(key.clone(), PointValue::Float(55.0));

        let result = store.get(&key).unwrap();
        assert!(matches!(result.value, PointValue::Float(f) if (f - 55.0).abs() < f64::EPSILON));
    }

    #[test]
    fn get_all_for_device() {
        let store = PointStore::new();
        store.set(
            PointKey {
                device_instance_id: "ahu-1".to_string(),
                point_id: "dat".to_string(),
            },
            PointValue::Float(55.0),
        );
        store.set(
            PointKey {
                device_instance_id: "ahu-1".to_string(),
                point_id: "oat".to_string(),
            },
            PointValue::Float(85.0),
        );
        store.set(
            PointKey {
                device_instance_id: "vav-1".to_string(),
                point_id: "zat".to_string(),
            },
            PointValue::Float(72.0),
        );

        let ahu_points = store.get_all_for_device("ahu-1");
        assert_eq!(ahu_points.len(), 2);

        let vav_points = store.get_all_for_device("vav-1");
        assert_eq!(vav_points.len(), 1);
    }

    #[test]
    fn initialize_from_profile() {
        let json = std::fs::read_to_string("profiles/ahu-single-duct.json").unwrap();
        let profile: DeviceProfile = serde_json::from_str(&json).unwrap();

        let store = PointStore::new();
        store.initialize_from_profile("ahu-1", &profile);

        assert_eq!(store.point_count(), 35);

        let dat = store.get(&PointKey {
            device_instance_id: "ahu-1".to_string(),
            point_id: "dat".to_string(),
        });
        assert!(dat.is_some());
    }

    #[test]
    fn status_flags_basics() {
        let mut flags = PointStatusFlags::default();
        assert!(flags.is_normal());
        assert_eq!(flags.worst_status(), None);

        flags.set(PointStatusFlags::ALARM);
        assert!(!flags.is_normal());
        assert!(flags.has(PointStatusFlags::ALARM));
        assert_eq!(flags.worst_status(), Some("alarm"));

        flags.set(PointStatusFlags::DOWN);
        assert_eq!(flags.worst_status(), Some("down"));
        assert_eq!(flags.active_flags(), vec!["down", "alarm"]);

        flags.clear(PointStatusFlags::DOWN);
        assert!(!flags.has(PointStatusFlags::DOWN));
        assert_eq!(flags.worst_status(), Some("alarm"));
    }

    #[test]
    fn set_preserves_status_flags() {
        let store = PointStore::new();
        let key = PointKey {
            device_instance_id: "ahu-1".to_string(),
            point_id: "dat".to_string(),
        };

        store.set(key.clone(), PointValue::Float(55.0));
        store.set_status(&key, PointStatusFlags::ALARM);

        // Update value — status should be preserved
        store.set(key.clone(), PointValue::Float(60.0));
        let result = store.get(&key).unwrap();
        assert!(result.status.has(PointStatusFlags::ALARM));
        assert!(matches!(result.value, PointValue::Float(f) if (f - 60.0).abs() < f64::EPSILON));
    }

    #[test]
    fn set_if_changed_skips_duplicate() {
        let store = PointStore::new();
        let key = PointKey {
            device_instance_id: "ahu-1".to_string(),
            point_id: "dat".to_string(),
        };

        store.set(key.clone(), PointValue::Float(55.0));
        let v1 = store.get(&key).unwrap().timestamp;

        // Same value — timestamp should NOT change (set_if_changed is a no-op)
        std::thread::sleep(std::time::Duration::from_millis(10));
        store.set_if_changed(key.clone(), PointValue::Float(55.0));
        let v2 = store.get(&key).unwrap().timestamp;
        assert_eq!(v1, v2);

        // Different value — should update
        store.set_if_changed(key.clone(), PointValue::Float(60.0));
        let result = store.get(&key).unwrap();
        assert!(matches!(result.value, PointValue::Float(f) if (f - 60.0).abs() < f64::EPSILON));
    }

    #[test]
    fn all_keys() {
        let store = PointStore::new();
        store.set(
            PointKey {
                device_instance_id: "a".into(),
                point_id: "1".into(),
            },
            PointValue::Float(1.0),
        );
        store.set(
            PointKey {
                device_instance_id: "b".into(),
                point_id: "2".into(),
            },
            PointValue::Float(2.0),
        );
        assert_eq!(store.all_keys().len(), 2);
    }
}
