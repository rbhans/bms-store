//! Integration tests for the discovery service flow.
//!
//! These tests exercise end-to-end flows through DiscoveryService, verifying
//! that accept/unaccept/hydrate/rescan operations correctly propagate across
//! all stores (DiscoveryStore, NodeStore, EntityStore, PointStore).

use std::path::PathBuf;

use crate::bridge::bacnet::{BacnetBridge, BacnetDevice, BacnetObject};
use crate::bridge::modbus::{ModbusBridge, ModbusDeviceInfo, ModbusPointInfo};
use crate::config::profile::{ModbusDataType, ModbusRegisterType, PointValue};
use crate::discovery::bacnet_adapter::{adapt_bacnet_device, adapt_bacnet_points};
use crate::discovery::modbus_adapter::{adapt_modbus_device, adapt_modbus_points};
use crate::discovery::model::{
    ConnStatus, DeviceState, DiscoveredDevice, DiscoveredPoint, PointKindHint,
};
use crate::discovery::service::DiscoveryService;
use crate::event::bus::EventBus;
use crate::node::ProtocolBinding;
use crate::store::discovery_store::{start_discovery_store_with_path, DiscoveryStore};
use crate::store::entity_store::{start_entity_store_with_path, EntityStore};
use crate::store::node_store::{start_node_store_with_path, NodeStore};
use crate::store::point_store::{PointKey, PointStore};
use rustbac_core::types::{ObjectId, ObjectType};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a full DiscoveryService with isolated temp databases.
/// Returns (service, point_store, node_store, discovery_store, entity_store).
async fn test_service(
    tag: &str,
) -> (
    DiscoveryService,
    PointStore,
    NodeStore,
    DiscoveryStore,
    EntityStore,
) {
    let pid = std::process::id();
    let base = PathBuf::from(format!("/tmp/opencrate_discovery_tests/{pid}_{tag}"));
    std::fs::create_dir_all(&base).expect("create test dir");

    let discovery_store = start_discovery_store_with_path(&base.join("discovery.db"));
    let node_store = start_node_store_with_path(&base.join("nodes.db"));
    let entity_store = start_entity_store_with_path(&base.join("entities.db"));
    let event_bus = EventBus::new();
    let point_store = PointStore::new().with_event_bus(event_bus.clone());

    let svc = DiscoveryService::new(
        discovery_store.clone(),
        node_store.clone(),
        entity_store.clone(),
        event_bus,
        point_store.clone(),
    );

    (svc, point_store, node_store, discovery_store, entity_store)
}

/// Create a sample discovered device in Discovered state.
fn sample_device(id: &str) -> DiscoveredDevice {
    DiscoveredDevice {
        id: id.to_string(),
        protocol: "bacnet".to_string(),
        state: DeviceState::Discovered,
        conn_status: ConnStatus::Online,
        display_name: format!("AHU-{id}"),
        vendor: Some("TestVendor".into()),
        model: Some("TestModel".into()),
        address: "192.168.1.100".into(),
        point_count: 3,
        discovered_at_ms: 1000,
        accepted_at_ms: None,
        protocol_meta: serde_json::json!({"device_instance": 1000}),
        network_id: String::new(),
    }
}

/// Create 3 sample points: one analog, one binary, one multistate.
fn sample_points(device_id: &str) -> Vec<DiscoveredPoint> {
    vec![
        DiscoveredPoint {
            id: "discharge-air-temp".into(),
            device_id: device_id.into(),
            display_name: "Discharge Air Temp".into(),
            description: Some("Supply air temperature".into()),
            units: Some("degF".into()),
            point_kind: PointKindHint::Analog,
            writable: false,
            binding: ProtocolBinding::bacnet(1000, "analog-input", 1),
            protocol_meta: serde_json::json!({}),
            state_labels: None,
        },
        DiscoveredPoint {
            id: "fan-cmd".into(),
            device_id: device_id.into(),
            display_name: "Fan Command".into(),
            description: None,
            units: None,
            point_kind: PointKindHint::Binary,
            writable: true,
            binding: ProtocolBinding::bacnet(1000, "binary-output", 1),
            protocol_meta: serde_json::json!({}),
            state_labels: Some(
                [("true".into(), "On".into()), ("false".into(), "Off".into())]
                    .into_iter()
                    .collect(),
            ),
        },
        DiscoveredPoint {
            id: "fan-speed".into(),
            device_id: device_id.into(),
            display_name: "Fan Speed".into(),
            description: None,
            units: None,
            point_kind: PointKindHint::Multistate,
            writable: true,
            binding: ProtocolBinding::bacnet(1000, "multistate-output", 1),
            protocol_meta: serde_json::json!({}),
            state_labels: Some(
                [
                    ("1".into(), "Off".into()),
                    ("2".into(), "Low".into()),
                    ("3".into(), "High".into()),
                ]
                .into_iter()
                .collect(),
            ),
        },
    ]
}

/// Upsert a device and its points into the discovery store.
async fn upsert_device_and_points(store: &DiscoveryStore, device_id: &str) {
    let dev = sample_device(device_id);
    store.upsert_device(dev).await.expect("upsert device");
    let pts = sample_points(device_id);
    store
        .upsert_points(device_id, pts)
        .await
        .expect("upsert points");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// 1. Accept a device, verify nodes, points, and state changes.
#[tokio::test]
async fn accept_device_creates_nodes_and_points() {
    let (svc, point_store, node_store, discovery_store, _entity_store) =
        test_service("accept_creates").await;
    let device_id = "bacnet-1000";

    upsert_device_and_points(&discovery_store, device_id).await;

    svc.accept_device(device_id).await.expect("accept");

    // Device state should be Accepted
    let dev = discovery_store.get_device(device_id).await.unwrap();
    assert_eq!(dev.state, DeviceState::Accepted);

    // Equip node should exist
    let equip = node_store.get_node(device_id).await.expect("equip node");
    assert_eq!(equip.node_type, "equip");

    // Equip node should be parented under a group node
    let parent_id = equip.parent_id.as_ref().expect("equip should have parent");
    assert!(
        parent_id.starts_with("group-"),
        "parent should be a group node, got: {parent_id}"
    );

    // Group (Space) node should exist
    let group = node_store.get_node(parent_id).await.expect("group node");
    assert_eq!(group.node_type, "space");

    // Point nodes should exist
    for pt_id in &["discharge-air-temp", "fan-cmd", "fan-speed"] {
        let node_id = format!("{device_id}/{pt_id}");
        let pt_node = node_store
            .get_node(&node_id)
            .await
            .unwrap_or_else(|_| panic!("point node {node_id}"));
        assert_eq!(pt_node.node_type, "point");
        assert_eq!(pt_node.parent_id.as_deref(), Some(device_id));
    }

    // Points should be in PointStore
    let ps_points = point_store.get_all_for_device(device_id);
    assert_eq!(ps_points.len(), 3);
}

/// 2. Verify that accept populates PointStore with correct default values.
#[tokio::test]
async fn accept_device_populates_point_store() {
    let (svc, point_store, _node_store, discovery_store, _entity_store) =
        test_service("accept_populates_ps").await;
    let device_id = "bacnet-2000";

    upsert_device_and_points(&discovery_store, device_id).await;
    svc.accept_device(device_id).await.expect("accept");

    let all = point_store.get_all_for_device(device_id);
    assert_eq!(all.len(), 3, "should have 3 points in PointStore");

    // Check each point's default value matches its kind
    let analog_key = PointKey {
        device_instance_id: device_id.into(),
        point_id: "discharge-air-temp".into(),
    };
    let binary_key = PointKey {
        device_instance_id: device_id.into(),
        point_id: "fan-cmd".into(),
    };
    let multi_key = PointKey {
        device_instance_id: device_id.into(),
        point_id: "fan-speed".into(),
    };

    assert_eq!(
        point_store.get(&analog_key).unwrap().value,
        PointValue::Float(0.0)
    );
    assert_eq!(
        point_store.get(&binary_key).unwrap().value,
        PointValue::Bool(false)
    );
    assert_eq!(
        point_store.get(&multi_key).unwrap().value,
        PointValue::Integer(0)
    );
}

/// 3. Accepting an already-accepted device is a no-op (no error).
#[tokio::test]
async fn accept_already_accepted_is_noop() {
    let (svc, _ps, _ns, discovery_store, _es) = test_service("accept_noop").await;
    let device_id = "bacnet-3000";

    upsert_device_and_points(&discovery_store, device_id).await;

    svc.accept_device(device_id).await.expect("first accept");
    svc.accept_device(device_id)
        .await
        .expect("second accept should not error");

    let dev = discovery_store.get_device(device_id).await.unwrap();
    assert_eq!(dev.state, DeviceState::Accepted);
}

/// 4. hydrate_point_store restores points for accepted devices after restart.
#[tokio::test]
async fn hydrate_point_store_restores_accepted_devices() {
    let pid = std::process::id();
    let base = PathBuf::from(format!(
        "/tmp/opencrate_discovery_tests/{pid}_hydrate_restore"
    ));
    std::fs::create_dir_all(&base).expect("create test dir");

    let discovery_store = start_discovery_store_with_path(&base.join("discovery.db"));
    let node_store = start_node_store_with_path(&base.join("nodes.db"));
    let entity_store = start_entity_store_with_path(&base.join("entities.db"));
    let event_bus = EventBus::new();
    let point_store = PointStore::new().with_event_bus(event_bus.clone());

    let svc = DiscoveryService::new(
        discovery_store.clone(),
        node_store.clone(),
        entity_store.clone(),
        event_bus.clone(),
        point_store.clone(),
    );

    let device_id = "bacnet-4000";
    upsert_device_and_points(&discovery_store, device_id).await;
    svc.accept_device(device_id).await.expect("accept");
    assert_eq!(point_store.get_all_for_device(device_id).len(), 3);

    // Simulate restart: new PointStore (empty), same DiscoveryStore (persistent)
    let new_point_store = PointStore::new().with_event_bus(event_bus.clone());
    assert_eq!(new_point_store.get_all_for_device(device_id).len(), 0);

    let svc2 = DiscoveryService::new(
        discovery_store.clone(),
        node_store.clone(),
        entity_store.clone(),
        event_bus,
        new_point_store.clone(),
    );

    svc2.hydrate_point_store().await;

    let restored = new_point_store.get_all_for_device(device_id);
    assert_eq!(restored.len(), 3, "hydrate should restore 3 points");
}

/// 5. hydrate_point_store does not overwrite existing (real) values.
#[tokio::test]
async fn hydrate_does_not_overwrite_existing_values() {
    let (svc, point_store, _ns, discovery_store, _es) = test_service("hydrate_no_overwrite").await;
    let device_id = "bacnet-5000";

    upsert_device_and_points(&discovery_store, device_id).await;
    svc.accept_device(device_id).await.expect("accept");

    // Simulate a real value arriving from the bridge
    let key = PointKey {
        device_instance_id: device_id.into(),
        point_id: "discharge-air-temp".into(),
    };
    point_store.set(key.clone(), PointValue::Float(72.5));

    // Hydrate should NOT overwrite
    svc.hydrate_point_store().await;

    let val = point_store.get(&key).unwrap();
    assert_eq!(
        val.value,
        PointValue::Float(72.5),
        "hydrate must not overwrite existing value"
    );
}

/// 6. Rescan (upsert) preserves user-edited point properties.
#[tokio::test]
async fn upsert_points_preserves_user_edits_on_rescan() {
    let (_svc, _ps, _ns, discovery_store, _es) = test_service("upsert_preserves").await;
    let device_id = "bacnet-6000";

    upsert_device_and_points(&discovery_store, device_id).await;

    // User renames a point
    discovery_store
        .update_point(
            device_id,
            "discharge-air-temp",
            Some("My Custom Name"),
            None,
            None,
            None,
        )
        .await
        .expect("update point");

    // Verify the rename took
    let pts_before = discovery_store.get_points(device_id).await;
    let dat = pts_before
        .iter()
        .find(|p| p.id == "discharge-air-temp")
        .unwrap();
    assert_eq!(dat.display_name, "My Custom Name");

    // Simulate a rescan: upsert same points again
    let pts = sample_points(device_id);
    discovery_store
        .upsert_points(device_id, pts)
        .await
        .expect("re-upsert");

    // User's edit should be preserved
    let pts_after = discovery_store.get_points(device_id).await;
    let dat_after = pts_after
        .iter()
        .find(|p| p.id == "discharge-air-temp")
        .unwrap();
    assert_eq!(
        dat_after.display_name, "My Custom Name",
        "rescan must preserve user-edited display_name"
    );
}

/// 7. Regroup moves devices with same point kinds to the same group.
#[tokio::test]
async fn regroup_moves_devices_to_correct_groups() {
    let (svc, _ps, node_store, discovery_store, _es) = test_service("regroup").await;

    // Two devices with identical point kind distributions
    for dev_id in &["bacnet-7001", "bacnet-7002"] {
        upsert_device_and_points(&discovery_store, dev_id).await;
        svc.accept_device(dev_id).await.expect("accept");
    }

    svc.regroup_accepted_devices().await.expect("regroup");

    let equip1 = node_store.get_node("bacnet-7001").await.expect("equip 1");
    let equip2 = node_store.get_node("bacnet-7002").await.expect("equip 2");

    assert_eq!(
        equip1.parent_id, equip2.parent_id,
        "both equip nodes should share the same group parent"
    );
    assert!(
        equip1.parent_id.as_ref().unwrap().starts_with("group-"),
        "parent should be a group node"
    );
}

/// 8. Unaccept cleans up nodes, entities, and PointStore entries.
#[tokio::test]
async fn unaccept_device_cleans_up() {
    let (svc, point_store, node_store, discovery_store, entity_store) =
        test_service("unaccept").await;
    let device_id = "bacnet-8000";

    upsert_device_and_points(&discovery_store, device_id).await;
    svc.accept_device(device_id).await.expect("accept");

    // Verify things exist
    assert_eq!(point_store.get_all_for_device(device_id).len(), 3);
    assert!(node_store.get_node(device_id).await.is_ok());

    svc.unaccept_device(device_id).await.expect("unaccept");

    // Device state back to Discovered
    let dev = discovery_store.get_device(device_id).await.unwrap();
    assert_eq!(dev.state, DeviceState::Discovered);

    // Equip node deleted
    assert!(
        node_store.get_node(device_id).await.is_err(),
        "equip node should be deleted"
    );

    // Point nodes deleted
    for pt_id in &["discharge-air-temp", "fan-cmd", "fan-speed"] {
        let node_id = format!("{device_id}/{pt_id}");
        assert!(
            node_store.get_node(&node_id).await.is_err(),
            "point node {node_id} should be deleted"
        );
    }

    // PointStore empty for this device
    assert_eq!(
        point_store.get_all_for_device(device_id).len(),
        0,
        "PointStore should be empty for unaccepted device"
    );

    // Entity deleted
    assert!(
        entity_store.get_entity(device_id).await.is_err(),
        "equip entity should be deleted"
    );
}

/// 9. Upsert device + points separately stores and retrieves correctly.
/// Validates the flow that scan methods use.
#[tokio::test]
async fn scan_stores_device_and_points() {
    let (_svc, _ps, _ns, discovery_store, _es) = test_service("scan_stores").await;
    let device_id = "bacnet-9000";

    // Step 1: upsert device
    let dev = sample_device(device_id);
    discovery_store
        .upsert_device(dev)
        .await
        .expect("upsert device");

    // Step 2: upsert points separately (as scan methods do)
    let pts = sample_points(device_id);
    discovery_store
        .upsert_points(device_id, pts)
        .await
        .expect("upsert points");

    // Verify get_points returns them
    let stored = discovery_store.get_points(device_id).await;
    assert_eq!(stored.len(), 3, "should retrieve 3 points");

    // Verify device is listed
    let devices = discovery_store.list_devices(None).await;
    assert!(
        devices.iter().any(|d| d.id == device_id),
        "device should be in list"
    );
}

/// 10. insert_default does NOT fire ValueChanged events.
#[tokio::test]
async fn insert_default_does_not_fire_events() {
    let event_bus = EventBus::new();
    let mut rx = event_bus.subscribe();
    let point_store = PointStore::new().with_event_bus(event_bus);

    let key = PointKey {
        device_instance_id: "dev-10".into(),
        point_id: "temp".into(),
    };
    point_store.insert_default(key, PointValue::Float(0.0));
    point_store.bump_version();

    // Give a moment for any potential event to arrive
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    match rx.try_recv() {
        Err(tokio::sync::broadcast::error::TryRecvError::Empty) => {
            // Expected: no event
        }
        Ok(event) => {
            panic!("insert_default should NOT fire events, got: {:?}", event);
        }
        Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {
            panic!("unexpected lag");
        }
        Err(tokio::sync::broadcast::error::TryRecvError::Closed) => {
            panic!("channel closed unexpectedly");
        }
    }
}

/// 11. set_if_changed is atomic: skips duplicates, updates on change.
#[tokio::test]
async fn set_if_changed_atomic() {
    let point_store = PointStore::new();
    let key = PointKey {
        device_instance_id: "dev-11".into(),
        point_id: "temp".into(),
    };

    point_store.set(key.clone(), PointValue::Float(72.0));
    let ts1 = point_store.get(&key).unwrap().timestamp;

    // Small delay so timestamps differ if update happens
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // Same value: should NOT update timestamp
    point_store.set_if_changed(key.clone(), PointValue::Float(72.0));
    let ts2 = point_store.get(&key).unwrap().timestamp;
    assert_eq!(ts1, ts2, "timestamp must not change for same value");

    // Different value: SHOULD update
    point_store.set_if_changed(key.clone(), PointValue::Float(73.0));
    let result = point_store.get(&key).unwrap();
    assert_eq!(result.value, PointValue::Float(73.0));
    assert_ne!(ts1, result.timestamp, "timestamp must change for new value");
}

/// 12. Version counter increments atomically on set() calls.
#[tokio::test]
async fn version_counter_increments_atomically() {
    let point_store = PointStore::new();
    let mut rx = point_store.subscribe();

    let initial = *rx.borrow();

    let key = PointKey {
        device_instance_id: "dev-12".into(),
        point_id: "temp".into(),
    };

    point_store.set(key.clone(), PointValue::Float(1.0));
    rx.changed().await.expect("version changed");
    let v1 = *rx.borrow();
    assert_eq!(v1, initial + 1, "version should increment by 1");

    point_store.set(key.clone(), PointValue::Float(2.0));
    rx.changed().await.expect("version changed");
    let v2 = *rx.borrow();
    assert_eq!(v2, initial + 2, "version should increment by 1 again");

    point_store.set(key.clone(), PointValue::Float(3.0));
    rx.changed().await.expect("version changed");
    let v3 = *rx.borrow();
    assert_eq!(v3, initial + 3, "version should increment by 1 each time");
}

// ---------------------------------------------------------------------------
// BACnet protocol integration tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn bacnet_scan_stores_devices_and_points() {
    // Test the full scan_bacnet flow with injected devices
    let (_svc, _ps, _ns, ds, _es) = test_service("bacnet_scan").await;

    let mut bridge = BacnetBridge::new();
    bridge.inject_test_devices(vec![BacnetDevice {
        device_id: ObjectId::new(ObjectType::Device, 1000),
        address: rustbac_datalink::DataLinkAddress::Ip(std::net::SocketAddr::new(
            std::net::IpAddr::V4(std::net::Ipv4Addr::new(192, 168, 1, 100)),
            47808,
        )),
        vendor: Some("Test Vendor".into()),
        model: Some("Test Model".into()),
        firmware_revision: None,
        location: None,
        description: None,
        max_apdu: None,
        segmentation: None,
        protocol_version: None,
        app_software_version: None,
        objects: vec![
            BacnetObject {
                object_id: ObjectId::new(ObjectType::AnalogInput, 1),
                object_name: Some("zone-temp".into()),
                description: Some("Zone Temperature".into()),
                units: Some(64), // degrees-fahrenheit
                present_value: None,
                writable: false,
            },
            BacnetObject {
                object_id: ObjectId::new(ObjectType::BinaryOutput, 1),
                object_name: Some("fan-cmd".into()),
                description: None,
                units: None,
                present_value: None,
                writable: true,
            },
            BacnetObject {
                object_id: ObjectId::new(ObjectType::MultiStateInput, 1),
                object_name: Some("mode".into()),
                description: None,
                units: None,
                present_value: None,
                writable: false,
            },
        ],
        trend_logs: vec![],
    }]);

    // Test the adapter + store flow directly (what scan_bacnet does internally).
    let devices = bridge.discovered_devices();
    assert_eq!(devices.len(), 1);

    let dev = &devices[0];
    let adapted_device = adapt_bacnet_device(dev);
    let adapted_points = adapt_bacnet_points(dev);

    assert_eq!(adapted_device.id, "bacnet-1000");
    assert_eq!(adapted_points.len(), 3);

    // Store device and points (this is what scan_bacnet does)
    ds.upsert_device(adapted_device).await.unwrap();
    ds.upsert_points("bacnet-1000", adapted_points)
        .await
        .unwrap();

    // Verify stored correctly
    let stored_device = ds.get_device("bacnet-1000").await.unwrap();
    assert!(stored_device.display_name.contains("1000"));
    assert_eq!(stored_device.protocol, "bacnet");

    let stored_points = ds.get_points("bacnet-1000").await;
    assert_eq!(stored_points.len(), 3);

    // Verify point kinds
    let zone_temp = stored_points.iter().find(|p| p.id == "zone-temp").unwrap();
    assert_eq!(zone_temp.point_kind, PointKindHint::Analog);
    assert!(!zone_temp.writable);

    let fan_cmd = stored_points.iter().find(|p| p.id == "fan-cmd").unwrap();
    assert_eq!(fan_cmd.point_kind, PointKindHint::Binary);
    assert!(fan_cmd.writable);

    let mode = stored_points.iter().find(|p| p.id == "mode").unwrap();
    assert_eq!(mode.point_kind, PointKindHint::Multistate);
}

#[tokio::test]
async fn bacnet_accept_after_scan_full_flow() {
    // Full flow: scan → store → accept → verify everything
    let (svc, ps, ns, ds, _es) = test_service("bacnet_full").await;

    let mut bridge = BacnetBridge::new();
    bridge.inject_test_devices(vec![BacnetDevice {
        device_id: ObjectId::new(ObjectType::Device, 2000),
        address: rustbac_datalink::DataLinkAddress::Ip(std::net::SocketAddr::new(
            std::net::IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 50)),
            47808,
        )),
        vendor: Some("ACME".into()),
        model: None,
        firmware_revision: None,
        location: None,
        description: None,
        max_apdu: None,
        segmentation: None,
        protocol_version: None,
        app_software_version: None,
        objects: vec![
            BacnetObject {
                object_id: ObjectId::new(ObjectType::AnalogInput, 10),
                object_name: Some("supply-air-temp".into()),
                description: Some("Supply Air Temperature".into()),
                units: Some(64),
                present_value: None,
                writable: false,
            },
            BacnetObject {
                object_id: ObjectId::new(ObjectType::AnalogOutput, 1),
                object_name: Some("damper-pos".into()),
                description: None,
                units: Some(98), // percent
                present_value: None,
                writable: true,
            },
        ],
        trend_logs: vec![],
    }]);

    // Simulate what scan_bacnet does
    let dev = &bridge.discovered_devices()[0];
    let adapted_device = adapt_bacnet_device(dev);
    let adapted_points = adapt_bacnet_points(dev);
    let device_id = adapted_device.id.clone();

    ds.upsert_device(adapted_device).await.unwrap();
    ds.upsert_points(&device_id, adapted_points).await.unwrap();

    // Accept the device
    svc.accept_device(&device_id).await.unwrap();

    // Verify device state
    let dev = ds.get_device(&device_id).await.unwrap();
    assert_eq!(dev.state, DeviceState::Accepted);

    // Verify nodes created
    let equip = ns.get_node(&device_id).await.unwrap();
    assert_eq!(equip.node_type, "equip");
    assert!(equip.parent_id.is_some()); // Should be in a group

    // Verify group node exists
    let group_id = equip.parent_id.unwrap();
    let group = ns.get_node(&group_id).await.unwrap();
    assert_eq!(group.node_type, "space");

    // Verify point nodes
    let point1 = ns
        .get_node(&format!("{}/supply-air-temp", device_id))
        .await
        .unwrap();
    assert_eq!(point1.node_type, "point");
    assert_eq!(point1.parent_id.as_deref(), Some(device_id.as_str()));

    // Verify PointStore populated
    let live_points = ps.get_all_for_device(&device_id);
    assert_eq!(live_points.len(), 2);
}

#[tokio::test]
async fn bacnet_multiple_devices_group_by_kind() {
    // Two BACnet devices with same object types should group together
    let (svc, _ps, ns, ds, _es) = test_service("bacnet_group").await;

    // Create two devices with identical object type layouts
    let make_objects = || {
        vec![
            BacnetObject {
                object_id: ObjectId::new(ObjectType::AnalogInput, 1),
                object_name: Some("temp".into()),
                description: None,
                units: None,
                present_value: None,
                writable: false,
            },
            BacnetObject {
                object_id: ObjectId::new(ObjectType::BinaryOutput, 1),
                object_name: Some("cmd".into()),
                description: None,
                units: None,
                present_value: None,
                writable: true,
            },
        ]
    };

    let mut bridge = BacnetBridge::new();
    bridge.inject_test_devices(vec![
        BacnetDevice {
            device_id: ObjectId::new(ObjectType::Device, 3000),
            address: rustbac_datalink::DataLinkAddress::Ip("192.168.1.1:47808".parse().unwrap()),
            vendor: None,
            model: None,
            firmware_revision: None,
            location: None,
            description: None,
            max_apdu: None,
            segmentation: None,
            protocol_version: None,
            app_software_version: None,
            objects: make_objects(),
            trend_logs: vec![],
        },
        BacnetDevice {
            device_id: ObjectId::new(ObjectType::Device, 3001),
            address: rustbac_datalink::DataLinkAddress::Ip("192.168.1.2:47808".parse().unwrap()),
            vendor: None,
            model: None,
            firmware_revision: None,
            location: None,
            description: None,
            max_apdu: None,
            segmentation: None,
            protocol_version: None,
            app_software_version: None,
            objects: make_objects(),
            trend_logs: vec![],
        },
    ]);

    // Simulate scan + accept both
    for dev in bridge.discovered_devices() {
        let ad = adapt_bacnet_device(dev);
        let ap = adapt_bacnet_points(dev);
        let did = ad.id.clone();
        ds.upsert_device(ad).await.unwrap();
        ds.upsert_points(&did, ap).await.unwrap();
        svc.accept_device(&did).await.unwrap();
    }

    // Verify both devices share the same group
    let equip1 = ns.get_node("bacnet-3000").await.unwrap();
    let equip2 = ns.get_node("bacnet-3001").await.unwrap();
    assert!(equip1.parent_id.is_some());
    assert_eq!(
        equip1.parent_id, equip2.parent_id,
        "Devices with same point kinds should be in same group"
    );
}

// ---------------------------------------------------------------------------
// Modbus protocol integration tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn modbus_scan_stores_devices_and_points() {
    let (_svc, _ps, _ns, ds, _es) = test_service("modbus_scan").await;

    // Create a ModbusBridge with injected devices
    let mut bridge = ModbusBridge::new();
    bridge.inject_test_devices(vec![ModbusDeviceInfo {
        instance_id: "vav-1".into(),
        host: "192.168.1.50".into(),
        port: 502,
        unit_id: 1,
        vendor: Some("Test Corp".into()),
        model: Some("VAV-100".into()),
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
    }]);

    // Simulate what scan_modbus does
    let devices = bridge.discovered_devices();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].points.len(), 3);

    let dev = &devices[0];
    let adapted_device = adapt_modbus_device(dev);
    let adapted_points = adapt_modbus_points(dev);

    assert_eq!(adapted_device.id, "modbus-vav-1");
    assert_eq!(adapted_device.protocol, "modbus");
    assert_eq!(adapted_points.len(), 3);

    // Store
    ds.upsert_device(adapted_device).await.unwrap();
    ds.upsert_points("modbus-vav-1", adapted_points)
        .await
        .unwrap();

    // Verify
    let stored_points = ds.get_points("modbus-vav-1").await;
    assert_eq!(stored_points.len(), 3);

    let zone = stored_points.iter().find(|p| p.id == "zone-temp").unwrap();
    assert_eq!(zone.point_kind, PointKindHint::Analog);
    assert!(!zone.writable);

    let fan = stored_points.iter().find(|p| p.id == "fan-status").unwrap();
    assert_eq!(fan.point_kind, PointKindHint::Binary);
}

#[tokio::test]
async fn modbus_accept_after_scan_full_flow() {
    let (svc, ps, ns, ds, _es) = test_service("modbus_full").await;

    let mut bridge = ModbusBridge::new();
    bridge.inject_test_devices(vec![ModbusDeviceInfo {
        instance_id: "ahu-1".into(),
        host: "10.0.0.100".into(),
        port: 502,
        unit_id: 5,
        vendor: None,
        model: None,
        firmware_revision: None,
        points: vec![
            ModbusPointInfo {
                point_id: "supply-temp".into(),
                writable: false,
                register_type: ModbusRegisterType::Input,
                address: 0,
                data_type: Some(ModbusDataType::Int16),
                scale: Some(10.0),
            },
            ModbusPointInfo {
                point_id: "fan-speed".into(),
                writable: true,
                register_type: ModbusRegisterType::Holding,
                address: 100,
                data_type: None,
                scale: None,
            },
        ],
    }]);

    let dev = &bridge.discovered_devices()[0];
    let ad = adapt_modbus_device(dev);
    let ap = adapt_modbus_points(dev);
    let device_id = ad.id.clone();

    ds.upsert_device(ad).await.unwrap();
    ds.upsert_points(&device_id, ap).await.unwrap();

    // Accept
    svc.accept_device(&device_id).await.unwrap();

    // Verify full stack
    let dev_state = ds.get_device(&device_id).await.unwrap();
    assert_eq!(dev_state.state, DeviceState::Accepted);

    let equip = ns.get_node(&device_id).await.unwrap();
    assert_eq!(equip.node_type, "equip");

    let pt_node = ns
        .get_node(&format!("{}/supply-temp", device_id))
        .await
        .unwrap();
    assert_eq!(pt_node.node_type, "point");

    let live = ps.get_all_for_device(&device_id);
    assert_eq!(live.len(), 2);

    // Verify correct default values
    let supply = live
        .iter()
        .find(|(k, _)| k.point_id == "supply-temp")
        .unwrap();
    assert!(matches!(supply.1.value, PointValue::Float(f) if f == 0.0));
}

#[tokio::test]
async fn modbus_network_scan_simulation() {
    // Simulates what scan_modbus_network does: upsert device + upsert points
    let (_svc, _ps, _ns, ds, _es) = test_service("modbus_net_scan").await;

    // Simulate scanned device with probed registers
    let device = DiscoveredDevice {
        id: "modbus-scan-192.168.1.50-502-1".into(),
        protocol: "modbus".into(),
        state: DeviceState::Discovered,
        conn_status: ConnStatus::Online,
        display_name: "Modbus scan-192.168.1.50-502-1 (192.168.1.50:502)".into(),
        vendor: None,
        model: None,
        address: "192.168.1.50:502".into(),
        point_count: 3,
        discovered_at_ms: 1000,
        accepted_at_ms: None,
        protocol_meta: serde_json::json!({"host": "192.168.1.50", "port": 502, "unit_id": 1}),
        network_id: String::new(),
    };

    let points = vec![
        DiscoveredPoint {
            id: "holding-0".into(),
            device_id: device.id.clone(),
            display_name: "holding-0".into(),
            description: Some("holding @ 0".into()),
            units: None,
            point_kind: PointKindHint::Analog,
            writable: true,
            binding: ProtocolBinding::modbus("192.168.1.50", 502, 1, 0, "uint16", 1.0),
            protocol_meta: serde_json::json!({}),
            state_labels: None,
        },
        DiscoveredPoint {
            id: "input-100".into(),
            device_id: device.id.clone(),
            display_name: "input-100".into(),
            description: Some("input @ 100".into()),
            units: None,
            point_kind: PointKindHint::Analog,
            writable: false,
            binding: ProtocolBinding::modbus("192.168.1.50", 502, 1, 100, "uint16", 1.0),
            protocol_meta: serde_json::json!({}),
            state_labels: None,
        },
        DiscoveredPoint {
            id: "coil-0".into(),
            device_id: device.id.clone(),
            display_name: "coil-0".into(),
            description: Some("coil @ 0".into()),
            units: None,
            point_kind: PointKindHint::Binary,
            writable: true,
            binding: ProtocolBinding::modbus("192.168.1.50", 502, 1, 0, "bool", 1.0),
            protocol_meta: serde_json::json!({}),
            state_labels: None,
        },
    ];

    let device_id = device.id.clone();

    // Step 1: Upsert device
    ds.upsert_device(device).await.unwrap();

    // Step 2: Upsert points
    ds.upsert_points(&device_id, points).await.unwrap();

    // Verify points are stored
    let stored = ds.get_points(&device_id).await;
    assert_eq!(stored.len(), 3, "All 3 probed points should be stored");

    // Verify the binary point
    let coil = stored.iter().find(|p| p.id == "coil-0").unwrap();
    assert_eq!(coil.point_kind, PointKindHint::Binary);
    assert!(coil.writable);
}

#[tokio::test]
async fn modbus_rescan_preserves_renamed_points() {
    // Verify that rescanning doesn't destroy user edits on Modbus points
    let (_svc, _ps, _ns, ds, _es) = test_service("modbus_rescan").await;

    let mut bridge = ModbusBridge::new();
    bridge.inject_test_devices(vec![ModbusDeviceInfo {
        instance_id: "meter-1".into(),
        host: "10.0.0.1".into(),
        port: 502,
        unit_id: 1,
        vendor: None,
        model: None,
        firmware_revision: None,
        points: vec![ModbusPointInfo {
            point_id: "holding-0".into(),
            writable: true,
            register_type: ModbusRegisterType::Holding,
            address: 0,
            data_type: None,
            scale: None,
        }],
    }]);

    // First scan
    let dev = &bridge.discovered_devices()[0];
    let ad = adapt_modbus_device(dev);
    let ap = adapt_modbus_points(dev);
    ds.upsert_device(ad).await.unwrap();
    ds.upsert_points("modbus-meter-1", ap).await.unwrap();

    // User renames the point
    ds.update_point(
        "modbus-meter-1",
        "holding-0",
        Some("Power Meter kW"),
        None,
        None,
        None,
    )
    .await
    .unwrap();

    // Verify rename took
    let pts = ds.get_points("modbus-meter-1").await;
    assert_eq!(pts[0].display_name, "Power Meter kW");

    // Second scan (rescan) — same device, same points
    let dev2 = &bridge.discovered_devices()[0];
    let ap2 = adapt_modbus_points(dev2);
    ds.upsert_points("modbus-meter-1", ap2).await.unwrap();

    // User edit should be preserved
    let pts2 = ds.get_points("modbus-meter-1").await;
    assert_eq!(
        pts2[0].display_name, "Power Meter kW",
        "User rename should survive rescan"
    );
}

// ---------------------------------------------------------------------------
// skip_auto_tag — Haystack opt-out
// ---------------------------------------------------------------------------

/// Default accept_device populates equip + point tags via heuristics
/// (Atlas off in this build). Sanity check before the skip variant.
#[tokio::test]
async fn accept_device_default_populates_tags() {
    let (svc, _ps, _ns, ds, es) = test_service("accept_default_tags").await;
    let device_id = "bacnet-3000";
    upsert_device_and_points(&ds, device_id).await;

    svc.accept_device(device_id).await.expect("accept");

    let equip = es.get_entity(device_id).await.expect("equip entity");
    assert!(
        !equip.tags.is_empty(),
        "default accept should populate equip tags via heuristic, got empty"
    );

    let point = es
        .get_entity(&format!("{device_id}/discharge-air-temp"))
        .await
        .expect("point entity");
    assert!(
        !point.tags.is_empty(),
        "default accept should populate point tags, got empty"
    );
}

/// Default accept records tag provenance — every auto-generated tag gets
/// a row in entity_tag_provenance with source `"heuristic"` (Atlas off
/// in this build) and an evidence string referring to the input name.
#[tokio::test]
async fn accept_device_records_tag_provenance() {
    let (svc, _ps, _ns, ds, es) = test_service("accept_records_provenance").await;
    let device_id = "bacnet-3500";
    upsert_device_and_points(&ds, device_id).await;

    svc.accept_device(device_id).await.expect("accept");

    let equip_provenance = es.list_tag_provenance(device_id).await;
    assert!(
        !equip_provenance.is_empty(),
        "default accept should record provenance for every equip tag"
    );
    for (_tag, prov) in &equip_provenance {
        assert_eq!(prov.source, "heuristic", "atlas off, expect heuristic");
        assert!(prov.evidence.is_some());
        assert_eq!(prov.taxonomy.as_deref(), Some("haystack-5"));
    }

    let point_provenance = es
        .list_tag_provenance(&format!("{device_id}/discharge-air-temp"))
        .await;
    assert!(
        !point_provenance.is_empty(),
        "default accept should record provenance for point tags"
    );
}

/// skip_auto_tag => no provenance rows (no auto-tags written).
#[tokio::test]
async fn accept_device_skip_auto_tag_writes_no_provenance() {
    use crate::discovery::service::AcceptOptions;

    let (svc, _ps, _ns, ds, es) = test_service("accept_skip_no_provenance").await;
    let device_id = "bacnet-3501";
    upsert_device_and_points(&ds, device_id).await;
    svc.accept_device_with_options(
        device_id,
        AcceptOptions {
            skip_auto_tag: true,
            target_space_id: None,
        },
    )
    .await
    .expect("accept");

    assert!(es.list_tag_provenance(device_id).await.is_empty());
    assert!(es
        .list_tag_provenance(&format!("{device_id}/discharge-air-temp"))
        .await
        .is_empty());
}

/// `AcceptOptions::skip_auto_tag` opts out of Haystack auto-tagging entirely.
/// Equip and point entities are created with empty tag sets so consumers can
/// apply their own taxonomy via the entity API.
#[tokio::test]
async fn accept_device_skip_auto_tag_creates_empty_tags() {
    use crate::discovery::service::AcceptOptions;

    let (svc, _ps, _ns, ds, es) = test_service("accept_skip_tag").await;
    let device_id = "bacnet-4000";
    upsert_device_and_points(&ds, device_id).await;

    let opts = AcceptOptions {
        skip_auto_tag: true,
        target_space_id: None,
    };
    svc.accept_device_with_options(device_id, opts)
        .await
        .expect("accept");

    let equip = es.get_entity(device_id).await.expect("equip entity");
    assert!(
        equip.tags.is_empty(),
        "skip_auto_tag should leave equip with no tags, got {:?}",
        equip.tags
    );

    for pt_id in &["discharge-air-temp", "fan-cmd", "fan-speed"] {
        let entity_id = format!("{device_id}/{pt_id}");
        let point = es.get_entity(&entity_id).await.expect("point entity");
        assert!(
            point.tags.is_empty(),
            "skip_auto_tag should leave point {pt_id} with no tags, got {:?}",
            point.tags
        );
    }
}
