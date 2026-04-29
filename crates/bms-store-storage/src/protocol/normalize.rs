use std::collections::HashMap;
use std::sync::Arc;

use crate::config::profile::PointValue;
use crate::event::bus::{Event, EventBus};
use crate::node::NodeId;
use crate::store::node_store::NodeStore;
use crate::store::point_store::PointStore;

use super::{RawProtocolValue, ValueSink};

/// Trait for converting raw protocol values to (NodeId, PointValue) pairs.
pub trait Normalizer: Send + Sync {
    fn normalize(&self, raw: &RawProtocolValue) -> Option<(NodeId, PointValue)>;
}

/// Normalizer that uses profile-based mappings to convert raw values.
/// Maps BACnet (device_instance, object_type, object_instance) → NodeId.
/// Maps Modbus (host, unit_id, register) → NodeId.
pub struct ProfileNormalizer {
    /// BACnet: (device_instance, object_type, object_instance) → node_id
    bacnet_map: HashMap<(u32, String, u32), NodeId>,
    /// Modbus: (host, unit_id, register) → (node_id, scale)
    modbus_map: HashMap<(String, u8, u16), (NodeId, f64)>,
}

impl ProfileNormalizer {
    pub fn new() -> Self {
        ProfileNormalizer {
            bacnet_map: HashMap::new(),
            modbus_map: HashMap::new(),
        }
    }

    pub fn add_bacnet_mapping(
        &mut self,
        device_instance: u32,
        object_type: &str,
        object_instance: u32,
        node_id: &str,
    ) {
        self.bacnet_map.insert(
            (device_instance, object_type.to_string(), object_instance),
            node_id.to_string(),
        );
    }

    pub fn add_modbus_mapping(
        &mut self,
        host: &str,
        unit_id: u8,
        register: u16,
        node_id: &str,
        scale: f64,
    ) {
        self.modbus_map.insert(
            (host.to_string(), unit_id, register),
            (node_id.to_string(), scale),
        );
    }
}

impl Default for ProfileNormalizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Normalizer for ProfileNormalizer {
    fn normalize(&self, raw: &RawProtocolValue) -> Option<(NodeId, PointValue)> {
        match raw.protocol.as_str() {
            "bacnet" => self.normalize_bacnet(raw),
            "modbus" => self.normalize_modbus(raw),
            _ => None,
        }
    }
}

impl ProfileNormalizer {
    fn normalize_bacnet(&self, raw: &RawProtocolValue) -> Option<(NodeId, PointValue)> {
        let data = &raw.raw_data;
        let device_instance = data.get("device_instance")?.as_u64()? as u32;
        let object_type = data.get("object_type")?.as_str()?;
        let object_instance = data.get("object_instance")?.as_u64()? as u32;
        let value = data.get("value")?;

        let key = (device_instance, object_type.to_string(), object_instance);
        let node_id = self.bacnet_map.get(&key)?;
        let pv = json_to_point_value(value)?;
        Some((node_id.clone(), pv))
    }

    fn normalize_modbus(&self, raw: &RawProtocolValue) -> Option<(NodeId, PointValue)> {
        let data = &raw.raw_data;
        let host = data.get("host")?.as_str()?;
        let unit_id = data.get("unit_id")?.as_u64()? as u8;
        let register = data.get("register")?.as_u64()? as u16;

        let key = (host.to_string(), unit_id, register);
        let (node_id, scale) = self.modbus_map.get(&key)?;

        // Raw bytes as JSON array
        let raw_bytes: Vec<u8> = data
            .get("raw_bytes")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_u64().map(|n| n as u8))
                    .collect()
            })
            .unwrap_or_default();

        if raw_bytes.len() >= 2 {
            let raw_val = u16::from_be_bytes([raw_bytes[0], raw_bytes[1]]) as f64;
            let scaled = if *scale != 0.0 {
                raw_val / scale
            } else {
                raw_val
            };
            Some((node_id.clone(), PointValue::Float(scaled)))
        } else {
            None
        }
    }
}

fn json_to_point_value(v: &serde_json::Value) -> Option<PointValue> {
    match v {
        serde_json::Value::Bool(b) => Some(PointValue::Bool(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(PointValue::Integer(i))
            } else {
                n.as_f64().map(PointValue::Float)
            }
        }
        _ => None,
    }
}

/// A ValueSink that normalizes raw values and writes to PointStore (compat bridge).
/// Used during migration — bridges that still use the old PointSource trait.
pub struct PointStoreValueSink {
    normalizer: Arc<dyn Normalizer>,
    store: PointStore,
    event_bus: Option<EventBus>,
}

impl PointStoreValueSink {
    pub fn new(normalizer: Arc<dyn Normalizer>, store: PointStore) -> Self {
        PointStoreValueSink {
            normalizer,
            store,
            event_bus: None,
        }
    }

    pub fn with_event_bus(mut self, bus: EventBus) -> Self {
        self.event_bus = Some(bus);
        self
    }
}

impl ValueSink for PointStoreValueSink {
    fn on_value(&self, raw: RawProtocolValue) {
        if let Some((node_id, value)) = self.normalizer.normalize(&raw) {
            // Convert node_id "device/point" → PointKey
            if let Some((dev, pt)) = node_id.split_once('/') {
                let key = crate::store::point_store::PointKey {
                    device_instance_id: dev.to_string(),
                    point_id: pt.to_string(),
                };
                self.store.set(key, value);
            }
        }
    }

    fn on_device_status(&self, device_key: &str, online: bool) {
        if let Some(ref bus) = self.event_bus {
            if online {
                bus.publish(Event::DeviceDiscovered {
                    bridge_type: "protocol".into(),
                    device_key: device_key.to_string(),
                });
            } else {
                bus.publish(Event::DeviceDown {
                    bridge_type: "protocol".into(),
                    device_key: device_key.to_string(),
                });
            }
        }
    }
}

/// A ValueSink that normalizes raw values and writes to NodeStore.
pub struct NodeStoreValueSink {
    normalizer: Arc<dyn Normalizer>,
    node_store: NodeStore,
}

impl NodeStoreValueSink {
    pub fn new(normalizer: Arc<dyn Normalizer>, node_store: NodeStore) -> Self {
        NodeStoreValueSink {
            normalizer,
            node_store,
        }
    }
}

impl ValueSink for NodeStoreValueSink {
    fn on_value(&self, raw: RawProtocolValue) {
        if let Some((node_id, value)) = self.normalizer.normalize(&raw) {
            self.node_store.update_value(&node_id, value);
        }
    }

    fn on_device_status(&self, _device_key: &str, _online: bool) {
        // NodeStore event publishing handled internally
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_normalizer_bacnet() {
        let mut norm = ProfileNormalizer::new();
        norm.add_bacnet_mapping(1000, "analog-input", 1, "ahu-1/dat");

        let raw = RawProtocolValue::new(
            "bacnet",
            "1000",
            "analog-input-1",
            serde_json::json!({
                "device_instance": 1000,
                "object_type": "analog-input",
                "object_instance": 1,
                "value": 72.5,
            }),
        );

        let result = norm.normalize(&raw);
        assert!(result.is_some());
        let (id, val) = result.unwrap();
        assert_eq!(id, "ahu-1/dat");
        assert!(matches!(val, PointValue::Float(f) if (f - 72.5).abs() < f64::EPSILON));
    }

    #[test]
    fn profile_normalizer_modbus() {
        let mut norm = ProfileNormalizer::new();
        norm.add_modbus_mapping("192.168.1.100", 1, 100, "ahu-1/oat", 10.0);

        let raw = RawProtocolValue::new(
            "modbus",
            "192.168.1.100:1",
            "100",
            serde_json::json!({
                "host": "192.168.1.100",
                "unit_id": 1,
                "register": 100,
                "raw_bytes": [0x03, 0x20],
            }),
        );

        let result = norm.normalize(&raw);
        assert!(result.is_some());
        let (id, val) = result.unwrap();
        assert_eq!(id, "ahu-1/oat");
        assert!(matches!(val, PointValue::Float(f) if (f - 80.0).abs() < f64::EPSILON));
    }

    #[test]
    fn unmapped_value_returns_none() {
        let norm = ProfileNormalizer::new();
        let raw = RawProtocolValue::new(
            "bacnet",
            "999",
            "analog-input-1",
            serde_json::json!({
                "device_instance": 999,
                "object_type": "analog-input",
                "object_instance": 1,
                "value": 42,
            }),
        );
        assert!(norm.normalize(&raw).is_none());
    }

    #[test]
    fn unknown_protocol_returns_none() {
        let norm = ProfileNormalizer::new();
        let raw = RawProtocolValue::new(
            "knx",
            "1.2.3",
            "switch-1",
            serde_json::json!({"value": true}),
        );
        assert!(norm.normalize(&raw).is_none());
    }
}
