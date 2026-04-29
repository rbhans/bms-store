// Re-exported from the bms-core crate — the canonical definitions live there.
pub use bms_core::{
    Node, NodeCapabilities, NodeId, NodeSnapshot, NodeType, PointStatusFlags, ProtocolBinding,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_builder() {
        let node = Node::new("ahu-1/dat", NodeType::Point, "Discharge Air Temp")
            .with_parent("ahu-1")
            .with_capabilities(NodeCapabilities::new(true, false, true, true, false));

        assert_eq!(node.id, "ahu-1/dat");
        assert_eq!(node.parent_id.as_deref(), Some("ahu-1"));
        assert!(node.is_point());
        assert!(node.capabilities.readable);
        assert!(!node.capabilities.writable);
    }

    #[test]
    fn equip_node() {
        let node = Node::new("ahu-1", NodeType::Equip, "AHU-1");
        assert!(!node.is_point());
        assert_eq!(node.node_type, NodeType::Equip);
    }

    #[test]
    fn node_type_roundtrip() {
        for nt in &[
            NodeType::Site,
            NodeType::Space,
            NodeType::Equip,
            NodeType::Point,
            NodeType::VirtualPoint,
        ] {
            let s = nt.as_str();
            let parsed = NodeType::from_str(s).unwrap();
            assert_eq!(&parsed, nt);
        }
    }

    #[test]
    fn protocol_binding_new_format() {
        let b = ProtocolBinding::bacnet(1000, "analog-input", 1);
        let json = serde_json::to_string(&b).unwrap();
        let deser: ProtocolBinding = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.protocol, "bacnet");
        assert_eq!(deser.config["device_instance"], 1000);
        assert_eq!(deser.config["object_type"], "analog-input");
    }

    #[test]
    fn protocol_binding_legacy_bacnet() {
        // Old tagged-enum format: {"protocol":"bacnet","device_instance":1000,...}
        let legacy = r#"{"protocol":"bacnet","device_instance":1000,"object_type":"analog-input","object_instance":1}"#;
        let b: ProtocolBinding = serde_json::from_str(legacy).unwrap();
        assert_eq!(b.protocol, "bacnet");
        assert_eq!(b.config["device_instance"], 1000);
        assert_eq!(b.config["object_type"], "analog-input");
        assert_eq!(b.config["object_instance"], 1);
    }

    #[test]
    fn protocol_binding_legacy_modbus() {
        let legacy = r#"{"protocol":"modbus","host":"192.168.1.1","port":502,"unit_id":1,"register":100,"data_type":"uint16","scale":1.0}"#;
        let b: ProtocolBinding = serde_json::from_str(legacy).unwrap();
        assert_eq!(b.protocol, "modbus");
        assert_eq!(b.config["host"], "192.168.1.1");
        assert_eq!(b.config["port"], 502);
    }

    #[test]
    fn protocol_binding_legacy_virtual() {
        let legacy = r#"{"protocol":"virtual"}"#;
        let b: ProtocolBinding = serde_json::from_str(legacy).unwrap();
        assert!(b.is_virtual());
        assert_eq!(b.config, serde_json::Value::Null);
    }

    #[test]
    fn protocol_binding_custom_protocol() {
        let b = ProtocolBinding::new("knx", serde_json::json!({"group_address": "1/2/3"}));
        let json = serde_json::to_string(&b).unwrap();
        let deser: ProtocolBinding = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.protocol, "knx");
        assert_eq!(deser.config["group_address"], "1/2/3");
    }
}
