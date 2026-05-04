use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, RwLock};

use crate::config::profile::PointValue;
use crate::event::bus::{Event, EventBus};
use crate::node::NodeId;
use crate::store::node_store::NodeStore;
use crate::store::point_store::PointStore;

use super::{RawProtocolValue, ValueSink};

/// Per-point raw → canonical string mapping read from the entity's `enum`
/// tag. This duplicates `bms_store_bridges::normalize::value_map::ValueMap`
/// (storage cannot depend on bridges), kept minimal — see that module for
/// full semantics. Stored as a flat BTreeMap so the lookup is allocation-free.
#[derive(Debug, Clone, Default)]
pub struct ValueMap {
    pub entries: BTreeMap<String, String>,
}

impl ValueMap {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn from_json(json: &str) -> Option<Self> {
        let entries: BTreeMap<String, String> = serde_json::from_str(json).ok()?;
        Some(ValueMap { entries })
    }
    pub fn lookup(&self, raw: &PointValue) -> Option<String> {
        let key = match raw {
            PointValue::Bool(b) => {
                if *b {
                    "true".to_string()
                } else {
                    "false".to_string()
                }
            }
            PointValue::Integer(i) => i.to_string(),
            PointValue::Float(f) => f.to_string(),
        };
        self.entries.get(&key).cloned()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Shared cache of per-node ValueMap entries. Bridges hold an Arc clone and
/// look up canonical mappings on every value write. The cache is rebuilt by
/// a background task subscribed to the EntityStore version watch.
pub type ValueMapCache = Arc<RwLock<HashMap<NodeId, ValueMap>>>;

/// Construct an empty ValueMapCache. Use as a default when no entity-driven
/// cache is wired (canonical lookup becomes a no-op).
pub fn empty_value_map_cache() -> ValueMapCache {
    Arc::new(RwLock::new(HashMap::new()))
}

/// Rebuild the cache from a fresh entity snapshot. Iterates entities,
/// reads the `enum` tag (JSON ValueMap) and writes the parsed map into the
/// cache keyed by entity id. Entities without an `enum` tag get an empty
/// map (so a stale cache entry for a now-untagged point falls through to
/// raw-only writes). Call this on EntityStore version bumps.
pub fn rebuild_value_map_cache(
    cache: &ValueMapCache,
    entities: &[crate::store::entity_store::Entity],
) {
    let mut next: HashMap<NodeId, ValueMap> = HashMap::with_capacity(entities.len());
    for ent in entities {
        if let Some(Some(json)) = ent.tags.get("enum") {
            if let Some(vm) = ValueMap::from_json(json) {
                if !vm.is_empty() {
                    next.insert(ent.id.clone(), vm);
                }
            }
        }
    }
    if let Ok(mut guard) = cache.write() {
        *guard = next;
    }
}

/// Spawn a background task that subscribes to EntityStore version updates
/// and rebuilds the ValueMap cache on every change. Returns immediately.
/// The task ends when the EntityStore version watch channel is closed
/// (i.e. when the EntityStore is dropped).
pub fn spawn_value_map_refresher(
    entity_store: crate::store::entity_store::EntityStore,
    cache: ValueMapCache,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // Initial fill
        let initial = entity_store.list_entities(None, None).await;
        rebuild_value_map_cache(&cache, &initial);

        let mut rx = entity_store.subscribe();
        while rx.changed().await.is_ok() {
            let snapshot = entity_store.list_entities(None, None).await;
            rebuild_value_map_cache(&cache, &snapshot);
        }
    })
}

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
    /// Optional shared per-node ValueMap cache. When set, writes use
    /// `set_with_canonical` so consumers see the canonical string
    /// (e.g. "OFF"/"ON") alongside the raw value.
    value_maps: Option<ValueMapCache>,
}

impl PointStoreValueSink {
    pub fn new(normalizer: Arc<dyn Normalizer>, store: PointStore) -> Self {
        PointStoreValueSink {
            normalizer,
            store,
            event_bus: None,
            value_maps: None,
        }
    }

    pub fn with_event_bus(mut self, bus: EventBus) -> Self {
        self.event_bus = Some(bus);
        self
    }

    pub fn with_value_maps(mut self, cache: ValueMapCache) -> Self {
        self.value_maps = Some(cache);
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
                let canonical = self
                    .value_maps
                    .as_ref()
                    .and_then(|cache| cache.read().ok().and_then(|guard| {
                        guard.get(&node_id).and_then(|vm| vm.lookup(&value))
                    }));
                if canonical.is_some() {
                    self.store.set_with_canonical(key, value, canonical);
                } else {
                    self.store.set(key, value);
                }
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

    // ---------------------------------------------------------------------
    // ValueMap + cache wiring
    // ---------------------------------------------------------------------

    #[test]
    fn value_map_lookup_bool_int() {
        let vm = ValueMap::from_json(r#"{"true":"ON","false":"OFF","0":"OFF","1":"ON"}"#).unwrap();
        assert_eq!(vm.lookup(&PointValue::Bool(true)).as_deref(), Some("ON"));
        assert_eq!(vm.lookup(&PointValue::Bool(false)).as_deref(), Some("OFF"));
        assert_eq!(vm.lookup(&PointValue::Integer(1)).as_deref(), Some("ON"));
        assert_eq!(vm.lookup(&PointValue::Integer(0)).as_deref(), Some("OFF"));
        assert!(vm.lookup(&PointValue::Integer(99)).is_none());
    }

    #[test]
    fn rebuild_value_map_cache_filters_empty_and_invalid() {
        use crate::store::entity_store::Entity;
        let cache = empty_value_map_cache();
        fn entity(id: &str) -> Entity {
            Entity {
                id: id.into(),
                entity_type: "point".into(),
                dis: id.into(),
                parent_id: None,
                tags: HashMap::new(),
                refs: HashMap::new(),
                created_ms: 0,
                updated_ms: 0,
            }
        }
        let mut e1 = entity("dev/p1");
        e1.tags.insert("enum".into(), Some(r#"{"0":"OFF","1":"ON"}"#.into()));
        let mut e2 = entity("dev/p2");
        e2.tags.insert("enum".into(), Some("not-json".into()));
        let e3 = entity("dev/p3");
        rebuild_value_map_cache(&cache, &[e1, e2, e3]);
        let guard = cache.read().unwrap();
        assert_eq!(guard.len(), 1, "only e1 has a valid non-empty enum map");
        assert!(guard.contains_key("dev/p1"));
    }

    #[test]
    fn point_store_value_sink_uses_cache_for_canonical() {
        use crate::store::point_store::{PointKey, PointStore};

        let store = PointStore::new();
        let cache = empty_value_map_cache();
        cache.write().unwrap().insert(
            "dev1/binary".to_string(),
            ValueMap::from_json(r#"{"0":"OFF","1":"ON"}"#).unwrap(),
        );

        let mut norm = ProfileNormalizer::new();
        norm.add_bacnet_mapping(7, "binary-input", 1, "dev1/binary");
        let sink = PointStoreValueSink::new(Arc::new(norm), store.clone())
            .with_value_maps(cache.clone());

        let raw = RawProtocolValue::new(
            "bacnet",
            "7",
            "binary-input-1",
            serde_json::json!({
                "device_instance": 7,
                "object_type": "binary-input",
                "object_instance": 1,
                "value": 1,
            }),
        );
        sink.on_value(raw);

        let key = PointKey {
            device_instance_id: "dev1".into(),
            point_id: "binary".into(),
        };
        let stored = store.get(&key).expect("stored value");
        assert_eq!(stored.canonical_value.as_deref(), Some("ON"));
    }

    #[test]
    fn point_store_value_sink_no_cache_writes_raw_only() {
        use crate::store::point_store::{PointKey, PointStore};

        let store = PointStore::new();
        let mut norm = ProfileNormalizer::new();
        norm.add_bacnet_mapping(7, "binary-input", 1, "dev1/binary");
        let sink = PointStoreValueSink::new(Arc::new(norm), store.clone());

        let raw = RawProtocolValue::new(
            "bacnet",
            "7",
            "binary-input-1",
            serde_json::json!({
                "device_instance": 7,
                "object_type": "binary-input",
                "object_instance": 1,
                "value": 1,
            }),
        );
        sink.on_value(raw);

        let key = PointKey {
            device_instance_id: "dev1".into(),
            point_id: "binary".into(),
        };
        let stored = store.get(&key).expect("stored value");
        assert!(stored.canonical_value.is_none());
    }
}
