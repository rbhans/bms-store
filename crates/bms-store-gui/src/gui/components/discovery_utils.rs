use dioxus::prelude::*;

use bms_store_bridges::discovery::grouping::RelatedGroup;
use bms_store_storage::discovery::model::{ConnStatus, DeviceState, PointKindHint};

/// A group of devices with the same point kind distribution.
#[derive(Clone)]
pub(crate) struct DeviceGroup {
    pub fingerprint: u64,
    pub name: String,
    pub kind_sig: String,
    pub device_ids: Vec<String>,
    pub related: Vec<RelatedGroup>,
}

/// Helper to increment a signal by 1 without borrow conflicts.
pub(crate) fn bump(sig: &mut Signal<u64>) {
    let v = *sig.read();
    sig.set(v + 1);
}

// ── Top-level discovery sub-tabs ──
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum DiscoveryTab {
    AllDevices,
    Bacnet,
    Modbus,
}

// ── Detail sub-tabs for device detail pane (protocol-aware) ──
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum DeviceDetailTab {
    Overview,
    Commission,
    BacnetManagement,
    BacnetAlarms,
    BacnetTrends,
    BacnetFiles,
    BacnetAdvanced,
    BacnetObjects,
    ModbusRegisters,
    ModbusDiagnostics,
}

impl DeviceDetailTab {
    pub fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Commission => "Commission",
            Self::BacnetManagement => "Manage",
            Self::BacnetAlarms => "Alarms",
            Self::BacnetTrends => "Trends",
            Self::BacnetFiles => "Files",
            Self::BacnetAdvanced => "Advanced",
            Self::BacnetObjects => "Objects",
            Self::ModbusRegisters => "Registers",
            Self::ModbusDiagnostics => "Diagnostics",
        }
    }
}

/// Return the detail tabs available for a given device.
pub(crate) fn tabs_for_device(protocol: &str, state: DeviceState) -> Vec<DeviceDetailTab> {
    if state != DeviceState::Accepted {
        return vec![DeviceDetailTab::Overview];
    }
    let mut tabs = vec![DeviceDetailTab::Overview, DeviceDetailTab::Commission];
    match protocol {
        "bacnet" => tabs.extend([
            DeviceDetailTab::BacnetManagement,
            DeviceDetailTab::BacnetAlarms,
            DeviceDetailTab::BacnetTrends,
            DeviceDetailTab::BacnetFiles,
            DeviceDetailTab::BacnetAdvanced,
            DeviceDetailTab::BacnetObjects,
        ]),
        "modbus" => tabs.extend([
            DeviceDetailTab::ModbusRegisters,
            DeviceDetailTab::ModbusDiagnostics,
        ]),
        _ => {}
    }
    tabs
}

#[component]
pub(crate) fn ConnBadge(status: ConnStatus) -> Element {
    let (class, label) = match status {
        ConnStatus::Online => ("discovery-status-badge online", "Online"),
        ConnStatus::Offline => ("discovery-status-badge offline", "Offline"),
        ConnStatus::Unknown => ("discovery-status-badge unknown", "Unknown"),
    };
    rsx! {
        span { class: "{class}", "{label}" }
    }
}

pub(crate) fn protocol_badge(proto: &str) -> &'static str {
    match proto {
        "bacnet" => "B",
        "modbus" => "M",
        _ => "?",
    }
}

pub(crate) fn protocol_badge_class(proto: &str) -> &'static str {
    match proto {
        "bacnet" => "bacnet",
        "modbus" => "modbus",
        _ => "unknown",
    }
}

pub(crate) fn kind_label(kind: PointKindHint) -> &'static str {
    match kind {
        PointKindHint::Analog => "A",
        PointKindHint::Binary => "B",
        PointKindHint::Multistate => "M",
    }
}

/// Extract BACnet device instance number from device ID.
/// Handles both "bacnet-1000" and "bacnet-{network_id}-1000" formats.
/// The instance is always the last numeric segment after the final '-'.
pub(crate) fn extract_bacnet_instance(device_id: &str) -> Option<u32> {
    if !device_id.starts_with("bacnet-") {
        return None;
    }
    device_id.rsplit('-').next().and_then(|s| s.parse().ok())
}

/// Extract the Modbus instance_id from a discovery device_id like "modbus-vav-101".
pub(crate) fn extract_modbus_instance_id(device_id: &str) -> String {
    device_id
        .strip_prefix("modbus-")
        .unwrap_or(device_id)
        .to_string()
}

pub(crate) fn event_state_label(state: u32) -> &'static str {
    match state {
        0 => "Normal",
        1 => "Fault",
        2 => "Offnormal",
        3 => "High Limit",
        4 => "Low Limit",
        5 => "Life Safety",
        _ => "Unknown",
    }
}

/// B4: Hash network_id to a color class from a small palette.
pub(crate) fn network_badge_class(network_id: &str) -> &'static str {
    let hash: u32 = network_id
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    match hash % 5 {
        0 => "net-blue",
        1 => "net-amber",
        2 => "net-green",
        3 => "net-purple",
        _ => "net-teal",
    }
}

/// Decode BACnet segmentation enum value to display string.
pub(crate) fn segmentation_label(val: u32) -> &'static str {
    match val {
        0 => "Both",
        1 => "Transmit",
        2 => "Receive",
        3 => "None",
        _ => "Unknown",
    }
}
