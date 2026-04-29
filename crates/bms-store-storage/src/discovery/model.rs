use serde::{Deserialize, Serialize};

use crate::node::ProtocolBinding;

/// Well-known protocol identifiers (constants for convenience).
pub const PROTOCOL_BACNET: &str = "bacnet";
pub const PROTOCOL_MODBUS: &str = "modbus";

/// Device approval state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceState {
    Discovered,
    Accepted,
    Ignored,
}

impl DeviceState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Discovered => "discovered",
            Self::Accepted => "accepted",
            Self::Ignored => "ignored",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "discovered" => Some(Self::Discovered),
            "accepted" => Some(Self::Accepted),
            "ignored" => Some(Self::Ignored),
            _ => None,
        }
    }
}

/// Device connectivity status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnStatus {
    Online,
    Offline,
    Unknown,
}

impl ConnStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Online => "online",
            Self::Offline => "offline",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "online" => Some(Self::Online),
            "offline" => Some(Self::Offline),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

/// Hint for the kind of point (analog, binary, multistate).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointKindHint {
    Analog,
    Binary,
    Multistate,
}

impl PointKindHint {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Analog => "analog",
            Self::Binary => "binary",
            Self::Multistate => "multistate",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "analog" => Some(Self::Analog),
            "binary" => Some(Self::Binary),
            "multistate" => Some(Self::Multistate),
            _ => None,
        }
    }
}

/// A device discovered by a protocol bridge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredDevice {
    pub id: String,
    /// Protocol identifier string (e.g. "bacnet", "modbus", "knx")
    pub protocol: String,
    pub state: DeviceState,
    pub conn_status: ConnStatus,
    pub display_name: String,
    pub vendor: Option<String>,
    pub model: Option<String>,
    pub address: String,
    pub point_count: usize,
    pub discovered_at_ms: i64,
    pub accepted_at_ms: Option<i64>,
    pub protocol_meta: serde_json::Value,
    /// BACnet network this device was discovered on (e.g. "ip-main", "mstp-field").
    /// Empty string for non-BACnet or legacy devices.
    #[serde(default)]
    pub network_id: String,
}

/// A point discovered on a device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredPoint {
    pub id: String,
    pub device_id: String,
    pub display_name: String,
    pub description: Option<String>,
    pub units: Option<String>,
    pub point_kind: PointKindHint,
    pub writable: bool,
    pub binding: ProtocolBinding,
    pub protocol_meta: serde_json::Value,
    /// State display labels.
    /// Binary points: `{"true": "On", "false": "Off"}`
    /// Multistate points: `{"1": "Off", "2": "Low", "3": "High"}`
    #[serde(default)]
    pub state_labels: Option<std::collections::HashMap<String, String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_roundtrip() {
        for s in &[
            DeviceState::Discovered,
            DeviceState::Accepted,
            DeviceState::Ignored,
        ] {
            let str_val = s.as_str();
            assert_eq!(DeviceState::from_str(str_val), Some(*s));
        }
    }

    #[test]
    fn conn_status_roundtrip() {
        for c in &[ConnStatus::Online, ConnStatus::Offline, ConnStatus::Unknown] {
            let s = c.as_str();
            assert_eq!(ConnStatus::from_str(s), Some(*c));
        }
    }

    #[test]
    fn point_kind_roundtrip() {
        for k in &[
            PointKindHint::Analog,
            PointKindHint::Binary,
            PointKindHint::Multistate,
        ] {
            let s = k.as_str();
            assert_eq!(PointKindHint::from_str(s), Some(*k));
        }
    }
}
