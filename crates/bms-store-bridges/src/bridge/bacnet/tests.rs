use super::*;
use crate::config::scenario::{BacnetNetworkConfig, ScenarioSettings};

use config::{bacnet_config_from_scenario, BacnetMode};
use conversion::{apply_bacnet_status_flags, civil_to_days, trend_log_items_to_samples};
use loop_time_sync::{days_to_ymd, now_bacnet_utc};
use rustbac_client::ClientDataValue;
use rustbac_core::types::{ObjectId, ObjectType};

use crate::config::profile::PointValue;
use crate::store::point_store::{PointKey, PointStatusFlags, PointStore};

// -- StatusFlags mapping --------------------------------------------------

#[test]
fn status_flags_all_clear() {
    let store = PointStore::new();
    let key = PointKey {
        device_instance_id: "bacnet-1".into(),
        point_id: "temp".into(),
    };
    store.set(key.clone(), PointValue::Float(72.0));

    // StatusFlags: 4 bits, all clear -> 0x00, unused_bits=4
    let value = ClientDataValue::BitString {
        unused_bits: 4,
        data: vec![0x00],
    };
    apply_bacnet_status_flags(&store, &key, &value);

    let ts = store.get(&key).unwrap();
    assert!(!ts.status.has(PointStatusFlags::ALARM));
    assert!(!ts.status.has(PointStatusFlags::FAULT));
    assert!(!ts.status.has(PointStatusFlags::OVERRIDDEN));
    assert!(!ts.status.has(PointStatusFlags::DISABLED));
}

#[test]
fn status_flags_in_alarm() {
    let store = PointStore::new();
    let key = PointKey {
        device_instance_id: "bacnet-1".into(),
        point_id: "temp".into(),
    };
    store.set(key.clone(), PointValue::Float(72.0));

    // IN_ALARM = bit 0 (MSB) -> byte 0x80, unused_bits=4
    let value = ClientDataValue::BitString {
        unused_bits: 4,
        data: vec![0x80],
    };
    apply_bacnet_status_flags(&store, &key, &value);

    let ts = store.get(&key).unwrap();
    assert!(ts.status.has(PointStatusFlags::ALARM));
    assert!(!ts.status.has(PointStatusFlags::FAULT));
    assert!(!ts.status.has(PointStatusFlags::OVERRIDDEN));
    assert!(!ts.status.has(PointStatusFlags::DISABLED));
}

#[test]
fn status_flags_all_set() {
    let store = PointStore::new();
    let key = PointKey {
        device_instance_id: "bacnet-1".into(),
        point_id: "temp".into(),
    };
    store.set(key.clone(), PointValue::Float(72.0));

    // All 4 bits set: 0xF0 (bits 7,6,5,4 -> IN_ALARM, FAULT, OVERRIDDEN, OUT_OF_SERVICE)
    let value = ClientDataValue::BitString {
        unused_bits: 4,
        data: vec![0xF0],
    };
    apply_bacnet_status_flags(&store, &key, &value);

    let ts = store.get(&key).unwrap();
    assert!(ts.status.has(PointStatusFlags::ALARM));
    assert!(ts.status.has(PointStatusFlags::FAULT));
    assert!(ts.status.has(PointStatusFlags::OVERRIDDEN));
    assert!(ts.status.has(PointStatusFlags::DISABLED));
}

#[test]
fn status_flags_fault_only() {
    let store = PointStore::new();
    let key = PointKey {
        device_instance_id: "bacnet-1".into(),
        point_id: "temp".into(),
    };
    store.set(key.clone(), PointValue::Float(72.0));

    // FAULT = bit 1 -> 0x40, unused_bits=4
    let value = ClientDataValue::BitString {
        unused_bits: 4,
        data: vec![0x40],
    };
    apply_bacnet_status_flags(&store, &key, &value);

    let ts = store.get(&key).unwrap();
    assert!(!ts.status.has(PointStatusFlags::ALARM));
    assert!(ts.status.has(PointStatusFlags::FAULT));
    assert!(!ts.status.has(PointStatusFlags::OVERRIDDEN));
    assert!(!ts.status.has(PointStatusFlags::DISABLED));
}

#[test]
fn status_flags_non_bitstring_ignored() {
    let store = PointStore::new();
    let key = PointKey {
        device_instance_id: "bacnet-1".into(),
        point_id: "temp".into(),
    };
    store.set(key.clone(), PointValue::Float(72.0));

    // Non-BitString value should be silently ignored
    let value = ClientDataValue::Unsigned(42);
    apply_bacnet_status_flags(&store, &key, &value);

    let ts = store.get(&key).unwrap();
    assert!(ts.status.is_normal());
}

// -- Date conversion ------------------------------------------------------

#[test]
fn days_to_ymd_epoch() {
    // 1970-01-01 is day 0, Thursday (weekday=4)
    let (y, m, d, wd) = days_to_ymd(0);
    assert_eq!((y, m, d), (1970, 1, 1));
    assert_eq!(wd, 4); // Thursday
}

#[test]
fn days_to_ymd_known_date() {
    // 2024-01-01 = day 19723 (from epoch), Monday
    let days = 19723;
    let (y, m, d, wd) = days_to_ymd(days);
    assert_eq!((y, m, d), (2024, 1, 1));
    assert_eq!(wd, 1); // Monday
}

#[test]
fn days_to_ymd_leap_day() {
    // 2024-02-29 = 19723 + 59 = 19782
    let days = 19782;
    let (y, m, d, _wd) = days_to_ymd(days);
    assert_eq!((y, m, d), (2024, 2, 29));
}

#[test]
fn now_bacnet_utc_valid_ranges() {
    let (date, time) = now_bacnet_utc();
    // Year should be recent (2020+)
    assert!(date.year_since_1900 >= 120); // 2020
    assert!((1..=12).contains(&date.month));
    assert!((1..=31).contains(&date.day));
    assert!((1..=7).contains(&date.weekday));
    assert!(time.hour < 24);
    assert!(time.minute < 60);
    assert!(time.second < 60);
}

// -- BacnetConfig from scenario -------------------------------------------

#[test]
fn config_from_scenario_none() {
    let config = bacnet_config_from_scenario(&None);
    assert!(matches!(config.mode, BacnetMode::Normal));
}

#[test]
fn config_from_scenario_no_bacnet() {
    let settings = Some(ScenarioSettings {
        tick_rate_ms: Some(100),
        realtime: None,
        bacnet: None,
        modbus: None,
        protocols: Default::default(),
        bacnet_networks: Default::default(),
        ..Default::default()
    });
    let config = bacnet_config_from_scenario(&settings);
    assert!(matches!(config.mode, BacnetMode::Normal));
}

#[test]
fn config_from_scenario_normal() {
    let settings = Some(ScenarioSettings {
        tick_rate_ms: None,
        realtime: None,
        bacnet: Some(BacnetNetworkConfig {
            mode: Some("normal".into()),
            bbmd_addr: None,
            ttl: None,
            hub_endpoint: None,
            server_device_instance: None,
            ..Default::default()
        }),
        modbus: None,
        protocols: Default::default(),
        bacnet_networks: Default::default(),
        ..Default::default()
    });
    let config = bacnet_config_from_scenario(&settings);
    assert!(matches!(config.mode, BacnetMode::Normal));
}

#[test]
fn config_from_scenario_foreign() {
    let settings = Some(ScenarioSettings {
        tick_rate_ms: None,
        realtime: None,
        bacnet: Some(BacnetNetworkConfig {
            mode: Some("foreign".into()),
            bbmd_addr: Some("192.168.1.1:47808".into()),
            ttl: Some(120),
            hub_endpoint: None,
            server_device_instance: None,
            ..Default::default()
        }),
        modbus: None,
        protocols: Default::default(),
        bacnet_networks: Default::default(),
        ..Default::default()
    });
    let config = bacnet_config_from_scenario(&settings);
    match config.mode {
        BacnetMode::Foreign { bbmd_addr, ttl } => {
            assert_eq!(bbmd_addr.to_string(), "192.168.1.1:47808");
            assert_eq!(ttl, 120);
        }
        other => panic!("expected Foreign, got {other:?}"),
    }
}

#[test]
fn config_from_scenario_sc() {
    let settings = Some(ScenarioSettings {
        tick_rate_ms: None,
        realtime: None,
        bacnet: Some(BacnetNetworkConfig {
            mode: Some("sc".into()),
            bbmd_addr: None,
            ttl: None,
            hub_endpoint: Some("wss://hub.example.com:1234/bacnet".into()),
            server_device_instance: None,
            ..Default::default()
        }),
        modbus: None,
        protocols: Default::default(),
        bacnet_networks: Default::default(),
        ..Default::default()
    });
    let config = bacnet_config_from_scenario(&settings);
    match config.mode {
        BacnetMode::SecureConnect { hub_endpoint } => {
            assert_eq!(hub_endpoint, "wss://hub.example.com:1234/bacnet");
        }
        other => panic!("expected SecureConnect, got {other:?}"),
    }
}

#[test]
fn config_from_scenario_mstp() {
    let settings = Some(ScenarioSettings {
        tick_rate_ms: None,
        realtime: None,
        bacnet: Some(BacnetNetworkConfig {
            mode: Some("mstp".into()),
            serial_port: Some("/dev/ttyUSB0".into()),
            baud_rate: Some(9600),
            mac_address: Some(1),
            max_master: Some(64),
            ..Default::default()
        }),
        modbus: None,
        protocols: Default::default(),
        bacnet_networks: Default::default(),
        ..Default::default()
    });
    let config = bacnet_config_from_scenario(&settings);
    match config.mode {
        BacnetMode::Mstp {
            port,
            baud_rate,
            mac_address,
            max_master,
        } => {
            assert_eq!(port, "/dev/ttyUSB0");
            assert_eq!(baud_rate, 9600);
            assert_eq!(mac_address, 1);
            assert_eq!(max_master, 64);
        }
        other => panic!("expected Mstp, got {other:?}"),
    }
}

#[test]
fn config_from_scenario_mstp_defaults() {
    // MS/TP mode without explicit params should use defaults
    let settings = Some(ScenarioSettings {
        tick_rate_ms: None,
        realtime: None,
        bacnet: Some(BacnetNetworkConfig {
            mode: Some("mstp".into()),
            bbmd_addr: None,
            ttl: None,
            hub_endpoint: None,
            server_device_instance: None,
            ..Default::default()
        }),
        modbus: None,
        protocols: Default::default(),
        bacnet_networks: Default::default(),
        ..Default::default()
    });
    let config = bacnet_config_from_scenario(&settings);
    match config.mode {
        BacnetMode::Mstp {
            baud_rate,
            mac_address,
            max_master,
            ..
        } => {
            assert_eq!(baud_rate, 38400);
            assert_eq!(mac_address, 0);
            assert_eq!(max_master, 127);
        }
        other => panic!("expected Mstp, got {other:?}"),
    }
}

#[test]
fn config_from_scenario_foreign_defaults() {
    // Foreign mode without explicit addr/ttl should use defaults
    let settings = Some(ScenarioSettings {
        tick_rate_ms: None,
        realtime: None,
        bacnet: Some(BacnetNetworkConfig {
            mode: Some("foreign".into()),
            bbmd_addr: None,
            ttl: None,
            hub_endpoint: None,
            server_device_instance: None,
            ..Default::default()
        }),
        modbus: None,
        protocols: Default::default(),
        bacnet_networks: Default::default(),
        ..Default::default()
    });
    let config = bacnet_config_from_scenario(&settings);
    match config.mode {
        BacnetMode::Foreign { ttl, .. } => {
            assert_eq!(ttl, 60);
        }
        other => panic!("expected Foreign, got {other:?}"),
    }
}

// -- civil_to_days / days_to_ymd roundtrip ---------------------------------

#[test]
fn civil_to_days_epoch() {
    assert_eq!(civil_to_days(1970, 1, 1), 0);
}

#[test]
fn civil_to_days_known_date() {
    // 2024-01-01 should be day 19723
    assert_eq!(civil_to_days(2024, 1, 1), 19723);
}

#[test]
fn civil_days_roundtrip() {
    for days in [0i64, 1, 365, 10000, 19723, 19782, 20000] {
        let (y, m, d, _wd) = days_to_ymd(days);
        let back = civil_to_days(y, m, d);
        assert_eq!(
            back, days,
            "roundtrip failed for days={days} -> ({y},{m},{d})"
        );
    }
}

// -- TrendLog sample extraction -------------------------------------------

#[test]
fn trend_log_items_empty() {
    let items: Vec<ClientDataValue> = vec![];
    assert!(trend_log_items_to_samples(&items).is_empty());
}

#[test]
fn trend_log_items_non_constructed_skipped() {
    let items = vec![ClientDataValue::Real(42.0), ClientDataValue::Unsigned(7)];
    assert!(trend_log_items_to_samples(&items).is_empty());
}

#[test]
fn trend_log_items_constructed_with_value() {
    // Constructed with an OctetString date, OctetString time, and a Real value
    let date_bytes = vec![
        124, // 1900+124 = 2024
        1,   // January
        1,   // day 1
        1,   // Monday
    ];
    let time_bytes = vec![
        12, // hour
        30, // minute
        0,  // second
        0,  // hundredths
    ];
    let items = vec![ClientDataValue::Constructed {
        tag_num: 0,
        values: vec![
            ClientDataValue::OctetString(date_bytes),
            ClientDataValue::OctetString(time_bytes),
            ClientDataValue::Real(72.5),
        ],
    }];
    let samples = trend_log_items_to_samples(&items);
    assert_eq!(samples.len(), 1);
    assert!((samples[0].1 - 72.5).abs() < f64::EPSILON);
    // Timestamp should be 2024-01-01 12:30:00 UTC in ms
    let expected_ts = 19723 * 86400 * 1000 + 12 * 3600000 + 30 * 60000;
    assert_eq!(samples[0].0, expected_ts);
}

// -- BacnetEventInfo enriched fields ------------------------------------

#[test]
fn bacnet_event_info_has_extended_fields() {
    let info = BacnetEventInfo {
        object_id: ObjectId::new(ObjectType::AnalogInput, 1),
        event_state: 2,
        acknowledged_transitions: Some(vec![0xE0]),
        notify_type: Some(1),
        event_enable: Some(vec![0xE0]),
        event_priorities: Some([3, 3, 3]),
    };
    assert_eq!(info.event_state, 2);
    assert!(info.acknowledged_transitions.is_some());
    assert_eq!(info.event_priorities.unwrap(), [3, 3, 3]);
}

// -- parse_device_id / multi-network routing ------------------------------

#[test]
fn parse_device_id_simple_format() {
    // "bacnet-1000" -> no network, instance 1000
    let result = BacnetNetworks::parse_device_id("bacnet-1000");
    assert_eq!(result, Some((None, 1000)));
}

#[test]
fn parse_device_id_network_qualified() {
    // "bacnet-ip-main-1000" -> network "ip-main", instance 1000
    let result = BacnetNetworks::parse_device_id("bacnet-ip-main-1000");
    assert_eq!(result, Some((Some("ip-main".into()), 1000)));
}

#[test]
fn parse_device_id_single_segment_network() {
    // "bacnet-mstp-500" -> network "mstp", instance 500
    let result = BacnetNetworks::parse_device_id("bacnet-mstp-500");
    assert_eq!(result, Some((Some("mstp".into()), 500)));
}

#[test]
fn parse_device_id_multi_segment_network() {
    // "bacnet-building-a-floor-2-42" -> network "building-a-floor-2", instance 42
    let result = BacnetNetworks::parse_device_id("bacnet-building-a-floor-2-42");
    assert_eq!(result, Some((Some("building-a-floor-2".into()), 42)));
}

#[test]
fn parse_device_id_rejects_non_bacnet() {
    assert_eq!(BacnetNetworks::parse_device_id("modbus-1000"), None);
    assert_eq!(BacnetNetworks::parse_device_id("1000"), None);
    assert_eq!(BacnetNetworks::parse_device_id(""), None);
}

#[test]
fn parse_device_id_rejects_no_instance() {
    // "bacnet-abc" where "abc" is not a valid u32
    assert_eq!(BacnetNetworks::parse_device_id("bacnet-abc"), None);
}

#[test]
fn find_network_for_device_id_uses_embedded_network() {
    let parsed = BacnetNetworks::parse_device_id("bacnet-ip-main-1000");
    assert_eq!(parsed, Some((Some("ip-main".into()), 1000)));

    let parsed2 = BacnetNetworks::parse_device_id("bacnet-mstp-field-1000");
    assert_eq!(parsed2, Some((Some("mstp-field".into()), 1000)));

    // Same instance (1000), different networks -- parse_device_id disambiguates
    assert_ne!(parsed.unwrap().0, parsed2.unwrap().0);
}
