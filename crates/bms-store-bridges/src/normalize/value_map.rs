//! Per-point value mapping for BMS state normalization.
//!
//! Raw protocol values (e.g. BACnet binary `0`/`1`) are mapped to
//! canonical human-readable strings (`"OFF"`/`"ON"`) using a per-point
//! [`ValueMap`] stored in the entity's `enum` tag as a JSON object.
//!
//! # Storage convention
//!
//! The map is serialized as a JSON object and stored in the entity's `tags`
//! HashMap under the key `"enum"`:
//! ```json
//! {"0":"OFF","1":"ON","false":"OFF","true":"ON"}
//! ```
//!
//! This piggybacks the existing tag system — no schema change is required.
//!
//! # Usage
//!
//! ```rust
//! use bms_store_bridges::normalize::value_map::{ValueMap, BoolMap};
//! use bms_store_storage::config::profile::PointValue;
//!
//! let vm = BoolMap::on_off();
//! assert_eq!(vm.get("0"), Some("OFF"));
//! assert_eq!(vm.get("1"), Some("ON"));
//!
//! let raw = PointValue::Integer(0);
//! let canonical = vm.normalize_value(&raw);
//! // Canonical values are exposed as strings via PointValue::Integer for
//! // numeric inputs; string output is signaled by a tag on the entity.
//! ```

use std::collections::BTreeMap;
use std::collections::HashMap;

use bms_store_storage::config::profile::PointValue;
use bms_store_storage::store::entity_store::Entity;

// ---------------------------------------------------------------------------
// ValueMap
// ---------------------------------------------------------------------------

/// Maps raw protocol values to canonical display strings.
///
/// Both keys and values are strings; the normalization layer converts
/// the incoming [`PointValue`] to a string representation before looking
/// it up.  On a hit, the canonical string is returned as the
/// [`PointValue::text`] — callers must store the result in the canonical
/// field of the TimestampedValue.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ValueMap {
    entries: BTreeMap<String, String>,
}

impl ValueMap {
    /// Create an empty map.
    pub fn new() -> Self {
        ValueMap {
            entries: BTreeMap::new(),
        }
    }

    /// Insert a raw → canonical mapping.
    pub fn insert(&mut self, raw: impl Into<String>, canonical: impl Into<String>) {
        self.entries.insert(raw.into(), canonical.into());
    }

    /// Look up the canonical string for a raw value string.
    pub fn get(&self, raw: &str) -> Option<&str> {
        self.entries.get(raw).map(String::as_str)
    }

    /// Normalize a [`PointValue`] to its canonical string, if the map has an
    /// entry for it.  If no entry matches the raw value is returned unchanged.
    pub fn normalize_value(&self, raw: &PointValue) -> NormalizedValue {
        let key = point_value_to_string(raw);
        match self.entries.get(&key) {
            Some(canonical) => NormalizedValue::Mapped(canonical.clone()),
            None => NormalizedValue::Raw(raw.clone()),
        }
    }

    /// Serialize the map to a JSON string suitable for storage in the `enum` tag.
    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.entries).unwrap_or_else(|_| "{}".to_string())
    }

    /// Parse a JSON string (as stored in the `enum` tag) into a [`ValueMap`].
    pub fn from_json(json: &str) -> Option<Self> {
        let entries: BTreeMap<String, String> = serde_json::from_str(json).ok()?;
        Some(ValueMap { entries })
    }

    /// Return the number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Return true if the map has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Normalized value result
// ---------------------------------------------------------------------------

/// Result of a [`ValueMap::normalize_value`] call.
#[derive(Debug, Clone, PartialEq)]
pub enum NormalizedValue {
    /// A canonical string was found in the map.
    Mapped(String),
    /// No entry matched; the raw value is returned as-is.
    Raw(PointValue),
}

impl NormalizedValue {
    /// True if the value was mapped to a canonical string.
    pub fn is_mapped(&self) -> bool {
        matches!(self, NormalizedValue::Mapped(_))
    }

    /// Return the canonical string if mapped, or `None`.
    pub fn canonical(&self) -> Option<&str> {
        match self {
            NormalizedValue::Mapped(s) => Some(s.as_str()),
            NormalizedValue::Raw(_) => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Entity helper
// ---------------------------------------------------------------------------

/// Read the `enum` tag from an entity and parse it as a [`ValueMap`].
///
/// Returns `None` if the tag is absent, empty, or not valid JSON.
pub fn value_map_for_entity(entity: &Entity) -> Option<ValueMap> {
    let tag_val = entity.tags.get("enum")?;
    let json = tag_val.as_deref()?;
    if json.is_empty() {
        return None;
    }
    ValueMap::from_json(json)
}

/// Build a lookup table: entity_id → ValueMap from a slice of entities.
///
/// Useful in bridges that iterate over many points at once to avoid
/// per-point `entity_store` round-trips.
pub fn build_entity_value_maps(entities: &[Entity]) -> HashMap<String, ValueMap> {
    entities
        .iter()
        .filter_map(|e| value_map_for_entity(e).map(|vm| (e.id.clone(), vm)))
        .collect()
}

// ---------------------------------------------------------------------------
// Pre-built value maps
// ---------------------------------------------------------------------------

/// Factory methods for common BMS boolean state maps.
pub struct BoolMap;

impl BoolMap {
    /// Binary output / binary input: `0/false` → `OFF`, `1/true` → `ON`.
    pub fn on_off() -> ValueMap {
        let mut vm = ValueMap::new();
        vm.insert("0", "OFF");
        vm.insert("1", "ON");
        vm.insert("false", "OFF");
        vm.insert("true", "ON");
        vm
    }

    /// Door / damper / valve position: `0/false` → `CLOSED`, `1/true` → `OPEN`.
    pub fn open_closed() -> ValueMap {
        let mut vm = ValueMap::new();
        vm.insert("0", "CLOSED");
        vm.insert("1", "OPEN");
        vm.insert("false", "CLOSED");
        vm.insert("true", "OPEN");
        vm
    }

    /// Occupancy sensor or schedule state.
    pub fn occupied_unoccupied() -> ValueMap {
        let mut vm = ValueMap::new();
        vm.insert("0", "UNOCCUPIED");
        vm.insert("1", "OCCUPIED");
        vm.insert("false", "UNOCCUPIED");
        vm.insert("true", "OCCUPIED");
        vm
    }

    /// Control mode (common in AHU/VAV setpoints).
    pub fn auto_manual() -> ValueMap {
        let mut vm = ValueMap::new();
        vm.insert("0", "AUTO");
        vm.insert("1", "MANUAL");
        vm.insert("false", "AUTO");
        vm.insert("true", "MANUAL");
        vm
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn point_value_to_string(v: &PointValue) -> String {
    match v {
        PointValue::Bool(b) => (if *b { "true" } else { "false" }).to_string(),
        PointValue::Integer(i) => i.to_string(),
        PointValue::Float(f) => {
            // Round-trip floats: try integer first to match "0"/"1" keys
            if *f == f.floor() && f.abs() < 1e15_f64 {
                (*f as i64).to_string()
            } else {
                format!("{f}")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_map_insert_get() {
        let mut vm = ValueMap::new();
        vm.insert("0", "OFF");
        vm.insert("1", "ON");
        assert_eq!(vm.get("0"), Some("OFF"));
        assert_eq!(vm.get("1"), Some("ON"));
        assert_eq!(vm.get("2"), None);
    }

    #[test]
    fn normalize_integer_matched() {
        let vm = BoolMap::on_off();
        let result = vm.normalize_value(&PointValue::Integer(0));
        assert_eq!(result, NormalizedValue::Mapped("OFF".into()));
    }

    #[test]
    fn normalize_integer_on() {
        let vm = BoolMap::on_off();
        let result = vm.normalize_value(&PointValue::Integer(1));
        assert_eq!(result, NormalizedValue::Mapped("ON".into()));
    }

    #[test]
    fn normalize_bool_true() {
        let vm = BoolMap::on_off();
        let result = vm.normalize_value(&PointValue::Bool(true));
        assert_eq!(result, NormalizedValue::Mapped("ON".into()));
    }

    #[test]
    fn normalize_bool_false() {
        let vm = BoolMap::on_off();
        let result = vm.normalize_value(&PointValue::Bool(false));
        assert_eq!(result, NormalizedValue::Mapped("OFF".into()));
    }

    #[test]
    fn normalize_float_zero() {
        let vm = BoolMap::on_off();
        let result = vm.normalize_value(&PointValue::Float(0.0));
        assert_eq!(result, NormalizedValue::Mapped("OFF".into()));
    }

    #[test]
    fn normalize_unmatched_returns_raw() {
        let vm = BoolMap::on_off();
        let raw = PointValue::Integer(42);
        let result = vm.normalize_value(&raw);
        assert_eq!(result, NormalizedValue::Raw(raw));
    }

    #[test]
    fn round_trip_json() {
        let vm = BoolMap::on_off();
        let json = vm.to_json();
        let recovered = ValueMap::from_json(&json).unwrap();
        assert_eq!(vm, recovered);
    }

    #[test]
    fn open_closed_map() {
        let vm = BoolMap::open_closed();
        assert_eq!(vm.get("0"), Some("CLOSED"));
        assert_eq!(vm.get("1"), Some("OPEN"));
        assert_eq!(vm.get("false"), Some("CLOSED"));
        assert_eq!(vm.get("true"), Some("OPEN"));
    }

    #[test]
    fn occupied_unoccupied_map() {
        let vm = BoolMap::occupied_unoccupied();
        assert_eq!(vm.get("0"), Some("UNOCCUPIED"));
        assert_eq!(vm.get("1"), Some("OCCUPIED"));
        let r = vm.normalize_value(&PointValue::Bool(true));
        assert_eq!(r, NormalizedValue::Mapped("OCCUPIED".into()));
    }

    #[test]
    fn auto_manual_map() {
        let vm = BoolMap::auto_manual();
        assert_eq!(vm.get("0"), Some("AUTO"));
        assert_eq!(vm.get("1"), Some("MANUAL"));
        let r = vm.normalize_value(&PointValue::Bool(false));
        assert_eq!(r, NormalizedValue::Mapped("AUTO".into()));
    }

    #[test]
    fn value_map_for_entity_parses_tag() {
        use std::collections::HashMap;
        use bms_store_storage::store::entity_store::Entity;

        let mut tags: HashMap<String, Option<String>> = HashMap::new();
        tags.insert(
            "enum".to_string(),
            Some(r#"{"0":"OFF","1":"ON"}"#.to_string()),
        );

        let entity = Entity {
            id: "test-point".to_string(),
            entity_type: "point".to_string(),
            dis: "Test Point".to_string(),
            parent_id: None,
            tags,
            refs: HashMap::new(),
            created_ms: 0,
            updated_ms: 0,
        };

        let vm = value_map_for_entity(&entity).unwrap();
        assert_eq!(vm.get("0"), Some("OFF"));
        assert_eq!(vm.get("1"), Some("ON"));
    }

    #[test]
    fn value_map_for_entity_absent_tag() {
        use std::collections::HashMap;
        use bms_store_storage::store::entity_store::Entity;

        let entity = Entity {
            id: "no-enum".to_string(),
            entity_type: "point".to_string(),
            dis: "No Enum".to_string(),
            parent_id: None,
            tags: HashMap::new(),
            refs: HashMap::new(),
            created_ms: 0,
            updated_ms: 0,
        };

        assert!(value_map_for_entity(&entity).is_none());
    }

    #[test]
    fn value_map_len_and_is_empty() {
        let mut vm = ValueMap::new();
        assert!(vm.is_empty());
        vm.insert("0", "OFF");
        assert_eq!(vm.len(), 1);
        assert!(!vm.is_empty());
    }
}
