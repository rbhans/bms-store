use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceProfile {
    pub profile: ProfileMeta,
    pub defaults: Option<DeviceDefaults>,
    pub points: Vec<Point>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileMeta {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub equipment_type: EquipmentType,
    pub version: String,
    pub description: Option<String>,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EquipmentType {
    Ahu,
    Vav,
    Rtu,
    Chiller,
    Boiler,
    Pump,
    CoolingTower,
    FanCoil,
    HeatExchanger,
    Generic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceDefaults {
    pub protocols: Option<ProtocolDefaults>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolDefaults {
    pub bacnet: Option<BacnetDefaults>,
    pub modbus: Option<ModbusDefaults>,
    /// Additional protocol defaults for unknown/future protocols.
    /// Captured via serde flatten — new protocols are stored as raw JSON.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacnetDefaults {
    pub device_id: Option<u32>,
    pub device_name: Option<String>,
    pub vendor_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModbusDefaults {
    pub unit_id: Option<u8>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub byte_order: Option<ByteOrder>,
    pub word_order: Option<ByteOrder>,
    pub response_timeout_ms: Option<u64>,
    pub retry_count: Option<u8>,
    pub throttle_delay_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ByteOrder {
    BigEndian,
    LittleEndian,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub kind: PointKind,
    pub access: PointAccess,
    pub units: Option<String>,
    pub initial_value: Option<PointValue>,
    pub constraints: Option<Constraints>,
    /// Simulation behavior — parsed but only used by simulation plugin.
    pub behavior: Option<serde_json::Value>,
    /// FMU mapping — parsed but only used by simulation plugin.
    pub fmu: Option<serde_json::Value>,
    pub ui: Option<UiConfig>,
    /// Legacy history config — kept for backward compat but ignored.
    /// All points now have history via COV recording.
    pub history: Option<HistoryConfig>,
    /// COV increment for history recording. Overrides the default
    /// (0.5% of range for analog, any-change for binary/multistate).
    /// Also read from protocols.bacnet.cov_increment if not set here.
    pub cov_increment: Option<f64>,
    /// If true, this point is excluded from history recording.
    #[serde(default)]
    pub history_exclude: bool,
    pub protocols: Option<ProtocolMappings>,
    pub haystack: Option<HaystackConfig>,
    /// Suggested alarms from the profile template. Not auto-enabled —
    /// the user must explicitly apply them.
    pub suggested_alarms: Option<Vec<SuggestedAlarm>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SuggestedAlarm {
    #[serde(rename = "type")]
    pub alarm_type: String,
    pub severity: String,
    #[serde(flatten)]
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryConfig {
    pub mode: HistoryMode,
    pub interval_secs: Option<u64>,
    pub cov_threshold: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HistoryMode {
    Interval,
    Cov,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PointKind {
    Analog,
    Binary,
    Multistate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PointAccess {
    Input,
    Output,
    Value,
}

// Re-exported from the bms-core crate — the canonical definition lives there.
pub use bms_core::PointValue;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Constraints {
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub states: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProtocolMappings {
    pub bacnet: Option<BacnetPointMapping>,
    pub modbus: Option<ModbusPointMapping>,
    /// Additional protocol mappings for unknown/future protocols.
    /// Captured via serde flatten — new protocols are stored as raw JSON.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BacnetPointMapping {
    pub object_type: BacnetObjectType,
    pub instance: u32,
    pub object_name: Option<String>,
    pub cov_increment: Option<f64>,
    pub priority: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BacnetObjectType {
    AnalogInput,
    AnalogOutput,
    AnalogValue,
    BinaryInput,
    BinaryOutput,
    BinaryValue,
    MultistateInput,
    MultistateOutput,
    MultistateValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModbusPointMapping {
    pub register_type: ModbusRegisterType,
    pub address: u16,
    pub data_type: Option<ModbusDataType>,
    pub scale: Option<f64>,
    pub register_count: Option<u16>,
    /// Extract a single bit from the register (0-15).
    pub bit_offset: Option<u8>,
    /// Extract masked bits from the register.
    pub bit_mask: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModbusRegisterType {
    Holding,
    Input,
    Coil,
    DiscreteInput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModbusDataType {
    Uint16,
    Int16,
    Uint32,
    Int32,
    Float32,
    Float64,
    Bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiConfig {
    pub group: Option<String>,
    pub color: Option<String>,
    pub precision: Option<u8>,
    pub trend: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HaystackConfig {
    pub tags: Option<Vec<String>>,
}

// ---------------------------------------------------------------------------
// Modbus Profile Library
// ---------------------------------------------------------------------------

/// Load all device profiles from a directory (e.g. `profiles/modbus-library/`).
/// Returns an empty Vec if the directory doesn't exist or has no valid JSON files.
pub fn load_profile_library(dir: &std::path::Path) -> Vec<DeviceProfile> {
    let read_dir = match std::fs::read_dir(dir) {
        Ok(d) => d,
        Err(_) => return vec![],
    };

    let mut profiles = Vec::new();
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        match std::fs::read_to_string(&path) {
            Ok(contents) => match serde_json::from_str::<DeviceProfile>(&contents) {
                Ok(profile) => profiles.push(profile),
                Err(e) => {
                    tracing::warn!(path = %path.display(), "Profile library: invalid JSON: {e}");
                }
            },
            Err(e) => {
                tracing::warn!(path = %path.display(), "Profile library: read error: {e}");
            }
        }
    }

    // Sort by name for deterministic ordering
    profiles.sort_by(|a, b| a.profile.name.cmp(&b.profile.name));
    profiles
}

/// Compute a similarity score (0.0–1.0) between a profile and a vendor/model string.
/// Used to auto-suggest the best matching profile for a discovered Modbus device.
pub fn match_profile(profile: &DeviceProfile, vendor: &str, model: &str) -> f64 {
    let mut score: f64 = 0.0;

    if let Some(ref mfr) = profile.profile.manufacturer {
        if !mfr.is_empty() && !vendor.is_empty() {
            let mfr_lower = mfr.to_lowercase();
            let vendor_lower = vendor.to_lowercase();
            if mfr_lower == vendor_lower {
                score += 0.5;
            } else if vendor_lower.contains(&mfr_lower) || mfr_lower.contains(&vendor_lower) {
                score += 0.3;
            }
        }
    }

    if let Some(ref m) = profile.profile.model {
        if !m.is_empty() && !model.is_empty() {
            let m_lower = m.to_lowercase();
            let model_lower = model.to_lowercase();
            if m_lower == model_lower {
                score += 0.5;
            } else if model_lower.contains(&m_lower) || m_lower.contains(&model_lower) {
                score += 0.3;
            }
        }
    }

    score.min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_ahu_profile() {
        let json = std::fs::read_to_string("profiles/ahu-single-duct.json").unwrap();
        let profile: DeviceProfile = serde_json::from_str(&json).unwrap();

        assert_eq!(profile.profile.id, "ahu-single-duct");
        assert_eq!(profile.points.len(), 35);
        assert!(matches!(profile.profile.equipment_type, EquipmentType::Ahu));
    }

    #[test]
    fn point_value_untagged_deser() {
        let bool_val: PointValue = serde_json::from_str("true").unwrap();
        assert!(matches!(bool_val, PointValue::Bool(true)));

        let int_val: PointValue = serde_json::from_str("12000").unwrap();
        assert!(matches!(int_val, PointValue::Integer(12000)));

        let float_val: PointValue = serde_json::from_str("85.0").unwrap();
        assert!(matches!(float_val, PointValue::Float(f) if (f - 85.0).abs() < f64::EPSILON));

        let one_val: PointValue = serde_json::from_str("1").unwrap();
        assert!(matches!(one_val, PointValue::Integer(1)));
    }

    #[test]
    fn point_value_as_f64() {
        assert!((PointValue::Bool(true).as_f64() - 1.0).abs() < f64::EPSILON);
        assert!((PointValue::Bool(false).as_f64() - 0.0).abs() < f64::EPSILON);
        assert!((PointValue::Integer(42).as_f64() - 42.0).abs() < f64::EPSILON);
        assert!((PointValue::Float(3.125).as_f64() - 3.125).abs() < f64::EPSILON);
    }
}
