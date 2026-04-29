use std::time::{SystemTime, UNIX_EPOCH};

use crate::bridge::modbus::ModbusDeviceInfo;
#[cfg(test)]
use crate::bridge::modbus::ModbusPointInfo;
use crate::config::profile::{ModbusDataType, ModbusRegisterType};
use crate::node::ProtocolBinding;

use super::model::{
    ConnStatus, DeviceState, DiscoveredDevice, DiscoveredPoint, PointKindHint, PROTOCOL_MODBUS,
};

/// Convert a Modbus device into a protocol-agnostic DiscoveredDevice.
pub fn adapt_modbus_device(dev: &ModbusDeviceInfo) -> DiscoveredDevice {
    DiscoveredDevice {
        id: format!("modbus-{}", dev.instance_id),
        protocol: PROTOCOL_MODBUS.into(),
        state: DeviceState::Discovered,
        conn_status: ConnStatus::Online,
        display_name: format!("Modbus {} ({}:{})", dev.instance_id, dev.host, dev.port),
        vendor: dev.vendor.clone(),
        model: dev.model.clone(),
        address: format!("{}:{}", dev.host, dev.port),
        point_count: dev.points.len(),
        discovered_at_ms: now_ms(),
        accepted_at_ms: None,
        protocol_meta: serde_json::json!({
            "instance_id": dev.instance_id,
            "host": dev.host,
            "port": dev.port,
            "unit_id": dev.unit_id,
        }),
        network_id: String::new(),
    }
}

/// Convert a Modbus device's points into protocol-agnostic DiscoveredPoints.
pub fn adapt_modbus_points(dev: &ModbusDeviceInfo) -> Vec<DiscoveredPoint> {
    let device_id = format!("modbus-{}", dev.instance_id);

    dev.points
        .iter()
        .map(|pt| {
            let point_kind = classify_register_type(&pt.register_type);
            let data_type_str = pt
                .data_type
                .as_ref()
                .map(data_type_to_string)
                .unwrap_or_else(|| default_data_type_str(&pt.register_type));
            let scale = pt.scale.unwrap_or(1.0);

            DiscoveredPoint {
                id: pt.point_id.clone(),
                device_id: device_id.clone(),
                display_name: pt.point_id.clone(),
                description: Some(format!(
                    "{} @ {}",
                    register_type_str(&pt.register_type),
                    pt.address
                )),
                units: None,
                point_kind,
                writable: pt.writable,
                binding: ProtocolBinding::modbus(
                    &dev.host,
                    dev.port,
                    dev.unit_id,
                    pt.address,
                    &data_type_str,
                    scale,
                ),
                protocol_meta: serde_json::json!({
                    "register_type": register_type_str(&pt.register_type),
                    "address": pt.address,
                    "data_type": pt.data_type.as_ref().map(data_type_to_string),
                    "scale": pt.scale,
                }),
                state_labels: None,
            }
        })
        .collect()
}

fn classify_register_type(rt: &ModbusRegisterType) -> PointKindHint {
    match rt {
        ModbusRegisterType::Coil | ModbusRegisterType::DiscreteInput => PointKindHint::Binary,
        ModbusRegisterType::Holding | ModbusRegisterType::Input => PointKindHint::Analog,
    }
}

fn register_type_str(rt: &ModbusRegisterType) -> &'static str {
    match rt {
        ModbusRegisterType::Coil => "coil",
        ModbusRegisterType::DiscreteInput => "discrete-input",
        ModbusRegisterType::Holding => "holding",
        ModbusRegisterType::Input => "input",
    }
}

fn data_type_to_string(dt: &ModbusDataType) -> String {
    match dt {
        ModbusDataType::Uint16 => "uint16".into(),
        ModbusDataType::Int16 => "int16".into(),
        ModbusDataType::Uint32 => "uint32".into(),
        ModbusDataType::Int32 => "int32".into(),
        ModbusDataType::Float32 => "float32".into(),
        ModbusDataType::Float64 => "float64".into(),
        ModbusDataType::Bool => "bool".into(),
    }
}

fn default_data_type_str(rt: &ModbusRegisterType) -> String {
    match rt {
        ModbusRegisterType::Coil | ModbusRegisterType::DiscreteInput => "bool".into(),
        ModbusRegisterType::Holding | ModbusRegisterType::Input => "uint16".into(),
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

    fn make_test_device() -> ModbusDeviceInfo {
        ModbusDeviceInfo {
            instance_id: "vav-101".into(),
            host: "192.168.1.50".into(),
            port: 502,
            unit_id: 1,
            vendor: None,
            model: None,
            firmware_revision: None,
            points: vec![
                ModbusPointInfo {
                    point_id: "zone-temp".into(),
                    writable: false,
                    register_type: ModbusRegisterType::Input,
                    address: 100,
                    data_type: Some(ModbusDataType::Float32),
                    scale: Some(10.0),
                },
                ModbusPointInfo {
                    point_id: "damper-cmd".into(),
                    writable: true,
                    register_type: ModbusRegisterType::Holding,
                    address: 200,
                    data_type: None,
                    scale: None,
                },
                ModbusPointInfo {
                    point_id: "fan-status".into(),
                    writable: false,
                    register_type: ModbusRegisterType::DiscreteInput,
                    address: 10,
                    data_type: None,
                    scale: None,
                },
            ],
        }
    }

    #[test]
    fn adapt_device_fields() {
        let dev = make_test_device();
        let adapted = adapt_modbus_device(&dev);

        assert_eq!(adapted.id, "modbus-vav-101");
        assert_eq!(adapted.protocol, PROTOCOL_MODBUS);
        assert_eq!(adapted.state, DeviceState::Discovered);
        assert_eq!(adapted.conn_status, ConnStatus::Online);
        assert!(adapted.display_name.contains("vav-101"));
        assert_eq!(adapted.address, "192.168.1.50:502");
        assert_eq!(adapted.point_count, 3);
        assert!(adapted.discovered_at_ms > 0);
        assert!(adapted.accepted_at_ms.is_none());
    }

    #[test]
    fn adapt_points_kind_and_writable() {
        let dev = make_test_device();
        let points = adapt_modbus_points(&dev);

        assert_eq!(points.len(), 3);

        // Input register → Analog, read-only
        assert_eq!(points[0].id, "zone-temp");
        assert_eq!(points[0].point_kind, PointKindHint::Analog);
        assert!(!points[0].writable);

        // Holding register → Analog, writable
        assert_eq!(points[1].id, "damper-cmd");
        assert_eq!(points[1].point_kind, PointKindHint::Analog);
        assert!(points[1].writable);

        // Discrete input → Binary, read-only
        assert_eq!(points[2].id, "fan-status");
        assert_eq!(points[2].point_kind, PointKindHint::Binary);
        assert!(!points[2].writable);
    }

    #[test]
    fn adapt_points_binding() {
        let dev = make_test_device();
        let points = adapt_modbus_points(&dev);

        let binding = &points[0].binding;
        assert!(binding.is_modbus());
        assert_eq!(binding.config["host"], "192.168.1.50");
        assert_eq!(binding.config["port"], 502);
        assert_eq!(binding.config["unit_id"], 1);
        assert_eq!(binding.config["register"], 100);
        assert_eq!(binding.config["data_type"], "float32");
        assert_eq!(binding.config["scale"], 10.0);
    }
}
