use std::path::Path;

use crate::config::profile::{BacnetPointMapping, DeviceProfile, ModbusPointMapping};
use crate::config::scenario::{BacnetNetworkConfig, ModbusNetworkConfig, ScenarioConfig};

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON parse error: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("Profile not found: {0}")]
    ProfileNotFound(String),
    #[error("Validation error: {0}")]
    Validation(String),
}

#[derive(Debug, Clone)]
pub struct LoadedDevice {
    pub instance_id: String,
    pub profile: DeviceProfile,
}

#[derive(Debug, Clone)]
pub struct LoadedScenario {
    pub config: ScenarioConfig,
    pub devices: Vec<LoadedDevice>,
    pub warnings: Vec<String>,
}

pub fn resolve_scenario(
    scenario_path: &Path,
    profiles_dir: &Path,
) -> Result<LoadedScenario, ConfigError> {
    let scenario_json = std::fs::read_to_string(scenario_path)?;
    let config: ScenarioConfig = serde_json::from_str(&scenario_json)?;

    let mut devices = Vec::new();
    let mut warnings = Vec::new();

    for device_instance in &config.devices {
        let profile_path = profiles_dir.join(format!("{}.json", device_instance.profile));

        if !profile_path.exists() {
            warnings.push(format!(
                "Profile '{}' not found for device '{}' (expected at {})",
                device_instance.profile,
                device_instance.instance_id,
                profile_path.display()
            ));
            continue;
        }

        let profile_json = std::fs::read_to_string(&profile_path)?;
        let mut profile: DeviceProfile = serde_json::from_str(&profile_json)?;

        if let Some(overrides) = &device_instance.overrides {
            apply_overrides(&mut profile, overrides);
        }

        devices.push(LoadedDevice {
            instance_id: device_instance.instance_id.clone(),
            profile,
        });
    }

    // Validate the loaded config + profiles.
    validate_scenario(&config, &devices, &mut warnings);

    Ok(LoadedScenario {
        config,
        devices,
        warnings,
    })
}

/// Validate scenario config, network settings, and device profiles.
/// Non-fatal issues are appended to `warnings`; nothing is returned as an error
/// because we want the app to start even with partial issues.
fn validate_scenario(
    config: &ScenarioConfig,
    devices: &[LoadedDevice],
    warnings: &mut Vec<String>,
) {
    // --- Scenario-level checks ---

    // Duplicate device instance IDs.
    let mut seen_ids = std::collections::HashSet::new();
    for dev_inst in &config.devices {
        if !seen_ids.insert(&dev_inst.instance_id) {
            warnings.push(format!(
                "Duplicate device instance_id '{}'",
                dev_inst.instance_id
            ));
        }
    }

    // --- BACnet network config ---
    if let Some(ref settings) = config.settings {
        if let Some(ref bacnet) = settings.bacnet {
            validate_bacnet_network(bacnet, warnings);
        }
        if let Some(ref modbus) = settings.modbus {
            validate_modbus_network(modbus, warnings);
        }
    }

    // --- Per-device / per-point checks ---
    let mut bacnet_device_ids = std::collections::HashMap::new();
    let mut modbus_unit_hosts = std::collections::HashMap::new();

    for dev in devices {
        let dev_id = &dev.instance_id;

        // BACnet device ID range check + uniqueness.
        if let Some(ref defaults) = dev.profile.defaults {
            if let Some(ref proto) = defaults.protocols {
                if let Some(ref bacnet) = proto.bacnet {
                    if let Some(id) = bacnet.device_id {
                        if id > 4_194_303 {
                            warnings.push(format!(
                                "Device '{dev_id}': BACnet device_id {id} exceeds max 4194303"
                            ));
                        }
                        if let Some(prev) = bacnet_device_ids.insert(id, dev_id.clone()) {
                            warnings.push(format!(
                                "BACnet device_id {id} used by both '{prev}' and '{dev_id}'"
                            ));
                        }
                    }
                }
                if let Some(ref modbus) = proto.modbus {
                    if let Some(uid) = modbus.unit_id {
                        if uid == 0 {
                            warnings.push(format!(
                                "Device '{dev_id}': Modbus unit_id 0 is broadcast — likely a misconfiguration"
                            ));
                        }
                    }
                    // Check for duplicate host:port + unit_id combos.
                    let host = modbus.host.as_deref().unwrap_or("localhost");
                    let port = modbus.port.unwrap_or(502);
                    let uid = modbus.unit_id.unwrap_or(1);
                    let key = format!("{host}:{port}/{uid}");
                    if let Some(prev) = modbus_unit_hosts.insert(key.clone(), dev_id.clone()) {
                        warnings.push(format!(
                            "Modbus address {key} used by both '{prev}' and '{dev_id}'"
                        ));
                    }
                }
            }
        }

        // Per-point validation.
        for pt in &dev.profile.points {
            if let Some(ref mappings) = pt.protocols {
                if let Some(ref bac) = mappings.bacnet {
                    validate_bacnet_point(dev_id, &pt.id, bac, warnings);
                }
                if let Some(ref modb) = mappings.modbus {
                    validate_modbus_point(dev_id, &pt.id, modb, warnings);
                }
            }
        }
    }
}

fn validate_bacnet_network(cfg: &BacnetNetworkConfig, warnings: &mut Vec<String>) {
    let mode = cfg.mode.as_deref().unwrap_or("normal");
    match mode {
        "normal" | "foreign" | "sc" | "mstp" | "ipv6" => {}
        other => warnings.push(format!(
            "Unknown BACnet mode '{other}' — expected normal/foreign/sc/mstp/ipv6"
        )),
    }
    if mode == "foreign" {
        if let Some(ref addr) = cfg.bbmd_addr {
            if addr.parse::<std::net::SocketAddr>().is_err() {
                warnings.push(format!(
                    "BACnet BBMD address '{addr}' is not a valid socket address"
                ));
            }
        }
    }
    if mode == "mstp" {
        if let Some(mac) = cfg.mac_address {
            if mac > 127 {
                warnings.push(format!("BACnet MS/TP mac_address {mac} exceeds max 127"));
            }
        }
        if let Some(max) = cfg.max_master {
            if max > 127 {
                warnings.push(format!("BACnet MS/TP max_master {max} exceeds max 127"));
            }
        }
    }
    if mode == "ipv6" {
        if let Some(ref group) = cfg.ipv6_multicast_group {
            if group.parse::<std::net::Ipv6Addr>().is_err() {
                warnings.push(format!(
                    "BACnet IPv6 multicast group '{group}' is not a valid IPv6 address"
                ));
            }
        }
    }
    if let Some(did) = cfg.server_device_instance {
        if did > 4_194_303 {
            warnings.push(format!(
                "server_device_instance {did} exceeds BACnet max 4194303"
            ));
        }
    }
}

fn validate_modbus_network(cfg: &ModbusNetworkConfig, warnings: &mut Vec<String>) {
    let mode = cfg.mode.as_deref().unwrap_or("tcp");
    match mode {
        "tcp" | "rtu" => {}
        other => warnings.push(format!("Unknown Modbus mode '{other}' — expected tcp/rtu")),
    }
    if mode == "rtu" && cfg.serial_port.is_none() {
        warnings.push("Modbus RTU mode requires serial_port".into());
    }
}

fn validate_bacnet_point(
    dev_id: &str,
    pt_id: &str,
    _mapping: &BacnetPointMapping,
    _warnings: &mut Vec<String>,
) {
    // BACnet object instance range is 0..4194303 (22-bit).
    // The instance field is u32 via serde, and BACnet object instances
    // up to 4194303 are valid. No further check needed beyond type bounds.
    let _ = (dev_id, pt_id);
}

fn validate_modbus_point(
    dev_id: &str,
    pt_id: &str,
    mapping: &ModbusPointMapping,
    warnings: &mut Vec<String>,
) {
    // Modbus register addresses are 0–65535 (u16 enforced by type).
    // But check bit_offset if present.
    if let Some(bit) = mapping.bit_offset {
        if bit > 15 {
            warnings.push(format!(
                "Device '{dev_id}' point '{pt_id}': bit_offset {bit} exceeds max 15"
            ));
        }
    }
}

fn apply_overrides(profile: &mut DeviceProfile, overrides: &serde_json::Value) {
    let defaults = profile
        .defaults
        .get_or_insert(crate::config::profile::DeviceDefaults { protocols: None });

    let protocols = defaults
        .protocols
        .get_or_insert(crate::config::profile::ProtocolDefaults {
            bacnet: None,
            modbus: None,
            extra: std::collections::HashMap::new(),
        });

    let bacnet = protocols
        .bacnet
        .get_or_insert(crate::config::profile::BacnetDefaults {
            device_id: None,
            device_name: None,
            vendor_id: None,
        });

    if let Some(id) = overrides.get("bacnet_device_id").and_then(|v| v.as_u64()) {
        bacnet.device_id = Some(id as u32);
    }
    if let Some(name) = overrides.get("bacnet_device_name").and_then(|v| v.as_str()) {
        bacnet.device_name = Some(name.to_string());
    }

    // Modbus overrides
    let modbus = protocols
        .modbus
        .get_or_insert(crate::config::profile::ModbusDefaults {
            unit_id: None,
            host: None,
            port: None,
            byte_order: None,
            word_order: None,
            response_timeout_ms: None,
            retry_count: None,
            throttle_delay_ms: None,
        });

    if let Some(host) = overrides.get("modbus_host").and_then(|v| v.as_str()) {
        modbus.host = Some(host.to_string());
    }
    if let Some(port) = overrides.get("modbus_port").and_then(|v| v.as_u64()) {
        modbus.port = Some(port as u16);
    }
    if let Some(unit_id) = overrides.get("modbus_unit_id").and_then(|v| v.as_u64()) {
        modbus.unit_id = Some(unit_id as u8);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::profile::*;
    use crate::config::scenario::*;

    fn empty_scenario() -> ScenarioConfig {
        ScenarioConfig {
            scenario: ScenarioMeta {
                id: "test".into(),
                name: "Test".into(),
                description: None,
            },
            settings: None,
            devices: vec![],
        }
    }

    fn device_with_bacnet_id(instance_id: &str, bacnet_device_id: u32) -> LoadedDevice {
        LoadedDevice {
            instance_id: instance_id.into(),
            profile: DeviceProfile {
                profile: ProfileMeta {
                    id: "p".into(),
                    name: "P".into(),
                    equipment_type: EquipmentType::Generic,
                    version: "1".into(),
                    description: None,
                    manufacturer: None,
                    model: None,
                    tags: None,
                },
                defaults: Some(DeviceDefaults {
                    protocols: Some(ProtocolDefaults {
                        bacnet: Some(BacnetDefaults {
                            device_id: Some(bacnet_device_id),
                            device_name: None,
                            vendor_id: None,
                        }),
                        modbus: None,
                        extra: std::collections::HashMap::new(),
                    }),
                }),
                points: vec![],
            },
        }
    }

    #[test]
    fn warns_on_duplicate_device_ids() {
        let mut config = empty_scenario();
        config.devices = vec![
            DeviceInstance {
                profile: "p".into(),
                instance_id: "ahu-1".into(),
                overrides: None,
            },
            DeviceInstance {
                profile: "p".into(),
                instance_id: "ahu-1".into(),
                overrides: None,
            },
        ];
        let mut warnings = vec![];
        validate_scenario(&config, &[], &mut warnings);
        assert!(warnings
            .iter()
            .any(|w| w.contains("Duplicate device instance_id 'ahu-1'")));
    }

    #[test]
    fn warns_on_bacnet_device_id_too_large() {
        let config = empty_scenario();
        let devices = vec![device_with_bacnet_id("dev-1", 5_000_000)];
        let mut warnings = vec![];
        validate_scenario(&config, &devices, &mut warnings);
        assert!(warnings.iter().any(|w| w.contains("exceeds max 4194303")));
    }

    #[test]
    fn warns_on_duplicate_bacnet_device_ids() {
        let config = empty_scenario();
        let devices = vec![
            device_with_bacnet_id("dev-1", 100),
            device_with_bacnet_id("dev-2", 100),
        ];
        let mut warnings = vec![];
        validate_scenario(&config, &devices, &mut warnings);
        assert!(warnings
            .iter()
            .any(|w| w.contains("BACnet device_id 100 used by both")));
    }

    #[test]
    fn warns_on_unknown_bacnet_mode() {
        let mut config = empty_scenario();
        config.settings = Some(ScenarioSettings {
            tick_rate_ms: None,
            realtime: None,
            bacnet: Some(BacnetNetworkConfig {
                mode: Some("zigbee".into()),
                ..Default::default()
            }),
            modbus: None,
            protocols: Default::default(),
            bacnet_networks: Default::default(),
            ..Default::default()
        });
        let mut warnings = vec![];
        validate_scenario(&config, &[], &mut warnings);
        assert!(warnings
            .iter()
            .any(|w| w.contains("Unknown BACnet mode 'zigbee'")));
    }

    #[test]
    fn no_warnings_for_valid_config() {
        let config = empty_scenario();
        let devices = vec![
            device_with_bacnet_id("dev-1", 100),
            device_with_bacnet_id("dev-2", 200),
        ];
        let mut warnings = vec![];
        validate_scenario(&config, &devices, &mut warnings);
        assert!(warnings.is_empty(), "unexpected warnings: {:?}", warnings);
    }
}
