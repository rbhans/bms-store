use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use bms_store_storage::config::scenario::{BacnetNetworkConfig, ModbusNetworkConfig};
use bms_store_storage::store::bridge_store::{StoredBacnetNetwork, StoredModbusBus};

use crate::bridge::bacnet::config::parse_bacnet_mode;
use crate::bridge::bacnet::{bacnet_configs_from_scenario, BacnetBridge, BacnetConfig, BacnetNetworks};
use crate::bridge::modbus::{modbus_config_from_scenario, ModbusBridge};
use crate::bridge::traits::PointSource;
use crate::discovery::service::DiscoveryService;
use crate::plugin::{BridgeRegistry, PluginRegistry};

/// Running protocol/discovery/plugin state for one bms-store project.
#[derive(Clone)]
pub struct BridgeRuntime {
    pub discovery_service: Arc<DiscoveryService>,
    pub bridge_registry: Arc<BridgeRegistry>,
    pub plugin_registry: Arc<PluginRegistry>,
}

impl BridgeRuntime {
    pub async fn stop_all(&self) {
        self.bridge_registry.stop_all().await;
    }
}

/// Status of a single protocol bridge start attempt.
#[derive(Debug, Clone, Default)]
pub enum BridgeStartStatus {
    #[default]
    Ok,
    Failed(String),
}

impl BridgeStartStatus {
    pub fn is_ok(&self) -> bool {
        matches!(self, BridgeStartStatus::Ok)
    }

    pub fn error(&self) -> Option<&str> {
        match self {
            BridgeStartStatus::Ok => None,
            BridgeStartStatus::Failed(error) => Some(error),
        }
    }
}

/// Startup report for configured protocol bridges.
#[derive(Debug, Clone, Default)]
pub struct BridgeStartReport {
    pub bacnet: HashMap<String, BridgeStartStatus>,
    pub modbus: BridgeStartStatus,
}

impl BridgeStartReport {
    pub fn all_ok(&self) -> bool {
        self.modbus.is_ok() && self.bacnet.values().all(BridgeStartStatus::is_ok)
    }

    pub fn failures(&self) -> Vec<(String, String)> {
        let mut failures = Vec::new();
        for (network_id, status) in &self.bacnet {
            if let BridgeStartStatus::Failed(error) = status {
                failures.push((format!("BACnet/{network_id}"), error.clone()));
            }
        }
        if let BridgeStartStatus::Failed(error) = &self.modbus {
            failures.push(("Modbus".to_string(), error.clone()));
        }
        failures
    }
}

pub async fn boot_bridges(
    storage: &bms_store_storage::boot::StorageRuntime,
) -> Result<(BridgeRuntime, BridgeStartReport), Box<dyn std::error::Error>> {
    let mut report = BridgeStartReport::default();
    let mut bacnet_configs = bacnet_configs_from_scenario(&storage.loaded.config.settings);
    let mut resolved_networks = storage
        .loaded
        .config
        .settings
        .as_ref()
        .map(|settings| settings.resolved_bacnet_networks())
        .unwrap_or_default();

    let stored_bacnet = storage.bridge_store.list_bacnet_networks().await;
    merge_stored_bacnet(&mut bacnet_configs, &mut resolved_networks, &stored_bacnet);

    let mut bacnet_networks = BacnetNetworks::new();
    let mut sorted_ids: Vec<String> = bacnet_configs.keys().cloned().collect();
    sorted_ids.sort();

    let mut server_assigned = false;
    for network_id in &sorted_ids {
        let config = &bacnet_configs[network_id];
        let network_config = resolved_networks.get(network_id);
        let mut bridge = BacnetBridge::new()
            .with_network_id(network_id.clone())
            .with_bacnet_config(config.clone())
            .with_event_bus(storage.event_bus.clone())
            .with_history_store(storage.history_store.clone());

        if let Some(secs) = network_config.and_then(|network| network.monitor_interval_secs) {
            bridge = bridge.with_monitor_interval(Duration::from_secs(secs));
        }
        if let Some(cycles) = network_config.and_then(|network| network.object_check_cycles) {
            bridge = bridge.with_object_check_cycles(cycles);
        }
        if let Some(secs) = network_config.and_then(|network| network.trend_log_sync_interval_secs)
        {
            bridge = bridge.with_trend_log_sync_interval(Duration::from_secs(secs));
        }

        if !server_assigned {
            if let Some(server_instance) = resolved_networks
                .get(network_id)
                .and_then(|network| network.server_device_instance)
            {
                bridge.init_server_store(server_instance, &storage.point_store);
                server_assigned = true;
            }
        }

        let status = match bridge.start(storage.point_store.clone()).await {
            Ok(()) => BridgeStartStatus::Ok,
            Err(error) => {
                tracing::error!(network_id, "BACnet bridge failed to start: {error}");
                BridgeStartStatus::Failed(error.to_string())
            }
        };
        report.bacnet.insert(network_id.clone(), status);
        bacnet_networks.insert(network_id.clone(), bridge);
    }

    let mut modbus_config = modbus_config_from_scenario(&storage.loaded.config.settings);
    let stored_modbus = storage.bridge_store.list_modbus_buses().await;
    merge_stored_modbus(&mut modbus_config, &stored_modbus);

    let mut modbus = ModbusBridge::new()
        .with_modbus_config(modbus_config)
        .with_event_bus(storage.event_bus.clone())
        .from_loaded_devices(&storage.loaded.devices);
    report.modbus = match modbus.start(storage.point_store.clone()).await {
        Ok(()) => BridgeStartStatus::Ok,
        Err(error) => {
            tracing::error!("Modbus bridge failed to start: {error}");
            BridgeStartStatus::Failed(error.to_string())
        }
    };

    let plugin_registry = PluginRegistry::new();
    let discovery_service = Arc::new(DiscoveryService::new(
        storage.discovery_store.clone(),
        storage.node_store.clone(),
        storage.entity_store.clone(),
        storage.event_bus.clone(),
        storage.point_store.clone(),
    ));

    let _ = discovery_service.regroup_accepted_devices().await;
    discovery_service.hydrate_point_store().await;

    let mut bridge_registry = BridgeRegistry::new();
    bridge_registry.register("bacnet", Box::new(bacnet_networks));
    bridge_registry.register("modbus", Box::new(modbus));

    tracing::info!(
        bacnet_networks = report.bacnet.len(),
        modbus_ok = report.modbus.is_ok(),
        "bms-store bridge runtime booted"
    );

    Ok((
        BridgeRuntime {
            discovery_service,
            bridge_registry: Arc::new(bridge_registry),
            plugin_registry: Arc::new(plugin_registry),
        },
        report,
    ))
}

/// Merge BridgeStore-managed BACnet networks into the scenario-derived maps.
/// Stored rows win on key conflict; disabled rows skipped; bad JSON warned + skipped.
fn merge_stored_bacnet(
    bacnet_configs: &mut HashMap<String, BacnetConfig>,
    resolved_networks: &mut HashMap<String, BacnetNetworkConfig>,
    stored: &[StoredBacnetNetwork],
) {
    for row in stored {
        if !row.enabled {
            continue;
        }
        match serde_json::from_str::<BacnetNetworkConfig>(&row.config_json) {
            Ok(net_cfg) => {
                let cfg = BacnetConfig {
                    mode: parse_bacnet_mode(&net_cfg),
                    network_id: row.name.clone(),
                };
                if resolved_networks.contains_key(&row.name) {
                    tracing::info!(
                        network_id = %row.name,
                        "BridgeStore overrides scenario.json BACnet network"
                    );
                }
                resolved_networks.insert(row.name.clone(), net_cfg);
                bacnet_configs.insert(row.name.clone(), cfg);
            }
            Err(e) => {
                tracing::warn!(
                    network_id = %row.name,
                    "BridgeStore BACnet network config_json failed to parse, skipping: {e}"
                );
            }
        }
    }
}

/// Merge BridgeStore-managed Modbus buses. ModbusBridge currently supports a
/// single network config — if multiple stored buses are enabled we log and use
/// the first by name order. Stored rows override scenario.json.
fn merge_stored_modbus(
    modbus_config: &mut Option<ModbusNetworkConfig>,
    stored: &[StoredModbusBus],
) {
    let enabled: Vec<&StoredModbusBus> = stored.iter().filter(|b| b.enabled).collect();
    if enabled.len() > 1 {
        tracing::warn!(
            count = enabled.len(),
            "Multiple enabled Modbus buses in BridgeStore; ModbusBridge supports one — using first by name order"
        );
    }
    let Some(first) = enabled.into_iter().next() else {
        return;
    };
    match serde_json::from_str::<ModbusNetworkConfig>(&first.config_json) {
        Ok(cfg) => {
            if modbus_config.is_some() {
                tracing::info!(
                    name = %first.name,
                    "BridgeStore overrides scenario.json Modbus config"
                );
            }
            *modbus_config = Some(cfg);
        }
        Err(e) => {
            tracing::warn!(
                name = %first.name,
                "BridgeStore Modbus bus config_json failed to parse, skipping: {e}"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stored_bacnet(name: &str, json: &str, enabled: bool) -> StoredBacnetNetwork {
        StoredBacnetNetwork {
            id: 0,
            name: name.into(),
            config_json: json.into(),
            enabled,
            created_ms: 0,
            updated_ms: 0,
        }
    }

    fn stored_modbus(name: &str, json: &str, enabled: bool) -> StoredModbusBus {
        StoredModbusBus {
            id: 0,
            name: name.into(),
            config_json: json.into(),
            enabled,
            created_ms: 0,
            updated_ms: 0,
        }
    }

    #[test]
    fn merge_bacnet_adds_new_enabled_row() {
        let mut configs: HashMap<String, BacnetConfig> = HashMap::new();
        let mut resolved: HashMap<String, BacnetNetworkConfig> = HashMap::new();
        let stored = vec![stored_bacnet(
            "ip-main",
            r#"{"mode":"normal","monitor_interval_secs":120}"#,
            true,
        )];
        merge_stored_bacnet(&mut configs, &mut resolved, &stored);
        assert!(configs.contains_key("ip-main"));
        assert!(resolved.contains_key("ip-main"));
        assert_eq!(resolved["ip-main"].monitor_interval_secs, Some(120));
    }

    #[test]
    fn merge_bacnet_skips_disabled() {
        let mut configs: HashMap<String, BacnetConfig> = HashMap::new();
        let mut resolved: HashMap<String, BacnetNetworkConfig> = HashMap::new();
        let stored = vec![stored_bacnet("off", r#"{"mode":"normal"}"#, false)];
        merge_stored_bacnet(&mut configs, &mut resolved, &stored);
        assert!(configs.is_empty());
        assert!(resolved.is_empty());
    }

    #[test]
    fn merge_bacnet_stored_overrides_scenario() {
        let mut configs = bacnet_configs_from_scenario(&None);
        let mut resolved: HashMap<String, BacnetNetworkConfig> = HashMap::new();
        resolved.insert(
            "default".into(),
            BacnetNetworkConfig {
                monitor_interval_secs: Some(300),
                ..Default::default()
            },
        );
        configs.insert("default".into(), BacnetConfig::default());
        let stored = vec![stored_bacnet(
            "default",
            r#"{"mode":"normal","monitor_interval_secs":42}"#,
            true,
        )];
        merge_stored_bacnet(&mut configs, &mut resolved, &stored);
        assert_eq!(resolved["default"].monitor_interval_secs, Some(42));
    }

    #[test]
    fn merge_bacnet_bad_json_skipped() {
        let mut configs: HashMap<String, BacnetConfig> = HashMap::new();
        let mut resolved: HashMap<String, BacnetNetworkConfig> = HashMap::new();
        let stored = vec![stored_bacnet("broken", "not-json", true)];
        merge_stored_bacnet(&mut configs, &mut resolved, &stored);
        assert!(configs.is_empty());
        assert!(resolved.is_empty());
    }

    #[test]
    fn merge_modbus_no_scenario_uses_stored() {
        let mut cfg: Option<ModbusNetworkConfig> = None;
        let stored = vec![stored_modbus(
            "rtu",
            r#"{"mode":"rtu","serial_port":"/dev/ttyUSB0","baud_rate":9600}"#,
            true,
        )];
        merge_stored_modbus(&mut cfg, &stored);
        let cfg = cfg.expect("modbus config");
        assert_eq!(cfg.mode.as_deref(), Some("rtu"));
        assert_eq!(cfg.serial_port.as_deref(), Some("/dev/ttyUSB0"));
    }

    #[test]
    fn merge_modbus_stored_overrides_scenario() {
        let mut cfg = Some(ModbusNetworkConfig {
            mode: Some("tcp".into()),
            serial_port: None,
            baud_rate: None,
            default_timeout_ms: None,
            default_retry_count: None,
        });
        let stored = vec![stored_modbus(
            "rtu-bus",
            r#"{"mode":"rtu","baud_rate":19200}"#,
            true,
        )];
        merge_stored_modbus(&mut cfg, &stored);
        let cfg = cfg.unwrap();
        assert_eq!(cfg.mode.as_deref(), Some("rtu"));
        assert_eq!(cfg.baud_rate, Some(19200));
    }

    #[test]
    fn merge_modbus_skips_disabled() {
        let mut cfg: Option<ModbusNetworkConfig> = None;
        let stored = vec![stored_modbus("off", r#"{"mode":"rtu"}"#, false)];
        merge_stored_modbus(&mut cfg, &stored);
        assert!(cfg.is_none());
    }

    #[test]
    fn merge_modbus_uses_first_enabled_when_multiple() {
        let mut cfg: Option<ModbusNetworkConfig> = None;
        let stored = vec![
            stored_modbus("a", r#"{"mode":"tcp","baud_rate":1}"#, true),
            stored_modbus("b", r#"{"mode":"rtu","baud_rate":2}"#, true),
        ];
        merge_stored_modbus(&mut cfg, &stored);
        assert_eq!(cfg.unwrap().baud_rate, Some(1));
    }

    #[test]
    fn merge_modbus_bad_json_skipped() {
        let mut cfg: Option<ModbusNetworkConfig> = None;
        let stored = vec![stored_modbus("broken", "{invalid", true)];
        merge_stored_modbus(&mut cfg, &stored);
        assert!(cfg.is_none());
    }
}
