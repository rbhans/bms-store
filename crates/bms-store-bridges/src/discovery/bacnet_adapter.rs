use std::time::{SystemTime, UNIX_EPOCH};

use rustbac_core::types::ObjectType;

use crate::bridge::bacnet::{BacnetDevice, BacnetObject};
use crate::node::ProtocolBinding;

use super::bacnet_units::bacnet_unit_to_string;
use super::model::{
    ConnStatus, DeviceState, DiscoveredDevice, DiscoveredPoint, PointKindHint, PROTOCOL_BACNET,
};

/// Convert a BACnet device into a protocol-agnostic DiscoveredDevice.
pub fn adapt_bacnet_device(dev: &BacnetDevice) -> DiscoveredDevice {
    adapt_bacnet_device_with_network(dev, "")
}

/// Convert a BACnet device into a protocol-agnostic DiscoveredDevice,
/// tagging it with the given BACnet network_id.
pub fn adapt_bacnet_device_with_network(dev: &BacnetDevice, network_id: &str) -> DiscoveredDevice {
    let instance = dev.device_id.instance();
    // Include network_id in the device ID to prevent collisions when the same
    // device instance exists on multiple networks (e.g. IP + MS/TP).
    // Legacy/default networks keep the short "bacnet-{instance}" format for compat.
    let id = if network_id.is_empty() || network_id == "default" {
        format!("bacnet-{instance}")
    } else {
        format!("bacnet-{network_id}-{instance}")
    };
    DiscoveredDevice {
        id,
        protocol: PROTOCOL_BACNET.into(),
        state: DeviceState::Discovered,
        conn_status: ConnStatus::Online,
        display_name: format!("BACnet Device {instance}"),
        vendor: dev.vendor.clone(),
        model: dev.model.clone(),
        address: format!("{:?}", dev.address),
        point_count: dev.objects.len(),
        discovered_at_ms: now_ms(),
        accepted_at_ms: None,
        protocol_meta: {
            let mut meta = serde_json::json!({
                "device_instance": instance,
                "object_type": format!("{}", dev.device_id.object_type()),
            });
            let obj = meta.as_object_mut().unwrap();
            if let Some(ref loc) = dev.location {
                obj.insert("location".into(), serde_json::Value::String(loc.clone()));
            }
            if let Some(ref desc) = dev.description {
                obj.insert(
                    "description".into(),
                    serde_json::Value::String(desc.clone()),
                );
            }
            if let Some(max_apdu) = dev.max_apdu {
                obj.insert("max_apdu".into(), serde_json::json!(max_apdu));
            }
            if let Some(seg) = dev.segmentation {
                obj.insert("segmentation".into(), serde_json::json!(seg));
            }
            if let Some(pv) = dev.protocol_version {
                obj.insert("protocol_version".into(), serde_json::json!(pv));
            }
            if let Some(ref asv) = dev.app_software_version {
                obj.insert(
                    "app_software_version".into(),
                    serde_json::Value::String(asv.clone()),
                );
            }
            meta
        },
        network_id: network_id.to_string(),
    }
}

/// Convert a BACnet device's objects into protocol-agnostic DiscoveredPoints,
/// including the network_id in each point's protocol_meta for write routing.
pub fn adapt_bacnet_points_with_network(
    dev: &BacnetDevice,
    network_id: &str,
) -> Vec<DiscoveredPoint> {
    let device_id = if network_id.is_empty() || network_id == "default" {
        format!("bacnet-{}", dev.device_id.instance())
    } else {
        format!("bacnet-{network_id}-{}", dev.device_id.instance())
    };
    let device_instance = dev.device_id.instance();

    dev.objects
        .iter()
        .map(|obj| {
            let point_id = object_point_id(obj);
            let display_name = obj.object_name.clone().unwrap_or_else(|| point_id.clone());
            let units = obj.units.and_then(bacnet_unit_to_string).map(String::from);
            let point_kind = classify_object_type(obj.object_id.object_type());
            let obj_type_str = format!("{}", obj.object_id.object_type());

            let mut meta = serde_json::json!({
                "object_type": obj_type_str,
                "object_instance": obj.object_id.instance(),
                "raw_units": obj.units,
            });
            if !network_id.is_empty() {
                meta.as_object_mut().unwrap().insert(
                    "network_id".into(),
                    serde_json::Value::String(network_id.to_string()),
                );
            }

            DiscoveredPoint {
                id: point_id,
                device_id: device_id.clone(),
                display_name,
                description: obj.description.clone(),
                units,
                point_kind,
                writable: obj.writable,
                binding: ProtocolBinding::bacnet(
                    device_instance,
                    &obj_type_str,
                    obj.object_id.instance(),
                ),
                protocol_meta: meta,
                state_labels: None,
            }
        })
        .collect()
}

/// Convert a BACnet device's objects into protocol-agnostic DiscoveredPoints.
pub fn adapt_bacnet_points(dev: &BacnetDevice) -> Vec<DiscoveredPoint> {
    let device_id = format!("bacnet-{}", dev.device_id.instance());
    let device_instance = dev.device_id.instance();

    dev.objects
        .iter()
        .map(|obj| {
            let point_id = object_point_id(obj);
            let display_name = obj.object_name.clone().unwrap_or_else(|| point_id.clone());
            let units = obj.units.and_then(bacnet_unit_to_string).map(String::from);
            let point_kind = classify_object_type(obj.object_id.object_type());
            let obj_type_str = format!("{}", obj.object_id.object_type());

            DiscoveredPoint {
                id: point_id,
                device_id: device_id.clone(),
                display_name,
                description: obj.description.clone(),
                units,
                point_kind,
                writable: obj.writable,
                binding: ProtocolBinding::bacnet(
                    device_instance,
                    &obj_type_str,
                    obj.object_id.instance(),
                ),
                protocol_meta: serde_json::json!({
                    "object_type": obj_type_str,
                    "object_instance": obj.object_id.instance(),
                    "raw_units": obj.units,
                }),
                state_labels: None,
            }
        })
        .collect()
}

/// Build a stable point ID from a BACnet object.
/// Prefers ObjectName if available, otherwise uses "type-instance" format.
fn object_point_id(obj: &BacnetObject) -> String {
    match &obj.object_name {
        Some(name) if !name.is_empty() => name.clone(),
        _ => format!(
            "{}-{}",
            obj.object_id.object_type(),
            obj.object_id.instance()
        ),
    }
}

/// Map BACnet object type to a PointKindHint.
fn classify_object_type(ot: ObjectType) -> PointKindHint {
    match ot {
        ObjectType::AnalogInput
        | ObjectType::AnalogOutput
        | ObjectType::AnalogValue
        | ObjectType::Accumulator
        | ObjectType::PulseConverter => PointKindHint::Analog,

        ObjectType::BinaryInput | ObjectType::BinaryOutput | ObjectType::BinaryValue => {
            PointKindHint::Binary
        }

        ObjectType::MultiStateInput
        | ObjectType::MultiStateOutput
        | ObjectType::MultiStateValue => PointKindHint::Multistate,

        // Default to Analog for unknown types
        _ => PointKindHint::Analog,
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustbac_core::types::ObjectId;

    fn make_test_device() -> BacnetDevice {
        BacnetDevice {
            device_id: ObjectId::new(ObjectType::Device, 1000),
            address: rustbac_datalink::DataLinkAddress::Ip(std::net::SocketAddr::new(
                std::net::IpAddr::V4(std::net::Ipv4Addr::new(192, 168, 1, 100)),
                47808,
            )),
            vendor: Some("Acme Controls".into()),
            model: Some("VAV-3000".into()),
            firmware_revision: Some("1.2.3".into()),
            location: None,
            description: None,
            max_apdu: None,
            segmentation: None,
            protocol_version: None,
            app_software_version: None,
            objects: vec![
                BacnetObject {
                    object_id: ObjectId::new(ObjectType::AnalogInput, 1),
                    object_name: Some("discharge-air-temp".into()),
                    description: Some("Discharge Air Temperature".into()),
                    units: Some(64), // °F
                    present_value: None,
                    writable: false,
                },
                BacnetObject {
                    object_id: ObjectId::new(ObjectType::BinaryOutput, 2),
                    object_name: Some("fan-run-cmd".into()),
                    description: None,
                    units: None,
                    present_value: None,
                    writable: true,
                },
                BacnetObject {
                    object_id: ObjectId::new(ObjectType::AnalogValue, 3),
                    object_name: None, // no name — should use fallback
                    description: None,
                    units: Some(98), // %
                    present_value: None,
                    writable: false,
                },
            ],
            trend_logs: vec![],
        }
    }

    #[test]
    fn adapt_device_fields() {
        let dev = make_test_device();
        let adapted = adapt_bacnet_device(&dev);

        assert_eq!(adapted.id, "bacnet-1000");
        assert_eq!(adapted.protocol, PROTOCOL_BACNET);
        assert_eq!(adapted.state, DeviceState::Discovered);
        assert_eq!(adapted.conn_status, ConnStatus::Online);
        assert_eq!(adapted.display_name, "BACnet Device 1000");
        assert_eq!(adapted.vendor.as_deref(), Some("Acme Controls"));
        assert_eq!(adapted.model.as_deref(), Some("VAV-3000"));
        assert_eq!(adapted.point_count, 3);
        assert!(adapted.discovered_at_ms > 0);
        assert!(adapted.accepted_at_ms.is_none());
    }

    #[test]
    fn adapt_points_names_and_units() {
        let dev = make_test_device();
        let points = adapt_bacnet_points(&dev);

        assert_eq!(points.len(), 3);

        // Named point
        assert_eq!(points[0].id, "discharge-air-temp");
        assert_eq!(points[0].display_name, "discharge-air-temp");
        assert_eq!(points[0].units.as_deref(), Some("°F"));
        assert_eq!(points[0].point_kind, PointKindHint::Analog);
        assert!(!points[0].writable);

        // Binary output
        assert_eq!(points[1].id, "fan-run-cmd");
        assert_eq!(points[1].point_kind, PointKindHint::Binary);
        assert!(points[1].writable);

        // Unnamed point — falls back to type-instance format (e.g., "analog-value-3")
        assert!(
            points[2].id.contains("3"),
            "unnamed point should contain instance number, got: {}",
            points[2].id
        );
        assert_eq!(points[2].units.as_deref(), Some("%"));
    }

    #[test]
    fn adapt_points_binding() {
        let dev = make_test_device();
        let points = adapt_bacnet_points(&dev);

        let binding = &points[0].binding;
        assert!(binding.is_bacnet());
        assert_eq!(binding.config["device_instance"], 1000);
        assert_eq!(binding.config["object_instance"], 1);
    }

    #[test]
    fn adapt_device_network_qualified_id_roundtrips() {
        use crate::bridge::bacnet::BacnetNetworks;

        let dev = make_test_device();

        // Default/empty network → simple format
        let adapted = adapt_bacnet_device_with_network(&dev, "");
        assert_eq!(adapted.id, "bacnet-1000");
        let parsed = BacnetNetworks::parse_device_id(&adapted.id);
        assert_eq!(parsed, Some((None, 1000)));

        // "default" network → simple format
        let adapted = adapt_bacnet_device_with_network(&dev, "default");
        assert_eq!(adapted.id, "bacnet-1000");
        let parsed = BacnetNetworks::parse_device_id(&adapted.id);
        assert_eq!(parsed, Some((None, 1000)));

        // Named network → network-qualified format
        let adapted = adapt_bacnet_device_with_network(&dev, "ip-main");
        assert_eq!(adapted.id, "bacnet-ip-main-1000");
        let parsed = BacnetNetworks::parse_device_id(&adapted.id);
        assert_eq!(parsed, Some((Some("ip-main".into()), 1000)));

        // Multi-segment network name
        let adapted = adapt_bacnet_device_with_network(&dev, "mstp-field");
        assert_eq!(adapted.id, "bacnet-mstp-field-1000");
        let parsed = BacnetNetworks::parse_device_id(&adapted.id);
        assert_eq!(parsed, Some((Some("mstp-field".into()), 1000)));
    }
}
