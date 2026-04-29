use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::bridge::bacnet::{bacnet_configs_from_scenario, BacnetBridge, BacnetNetworks};
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
    let bacnet_configs = bacnet_configs_from_scenario(&storage.loaded.config.settings);
    let resolved_networks = storage
        .loaded
        .config
        .settings
        .as_ref()
        .map(|settings| settings.resolved_bacnet_networks())
        .unwrap_or_default();

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

    let modbus_config = modbus_config_from_scenario(&storage.loaded.config.settings);
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
