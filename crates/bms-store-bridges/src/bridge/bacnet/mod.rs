// ---------------------------------------------------------------------------
// with_client! macro — MUST be defined before all `mod` declarations so that
// child modules can use it without any import (Rust macro scoping rule).
// ---------------------------------------------------------------------------

/// Helper macro to dispatch a method call on TransportClient to the inner Arc.
macro_rules! with_client {
    ($self:expr, |$c:ident| $body:expr) => {
        match $self {
            $crate::bridge::bacnet::transport::TransportClient::Ip($c) => $body,
            $crate::bridge::bacnet::transport::TransportClient::Sc($c) => $body,
            $crate::bridge::bacnet::transport::TransportClient::Mstp($c) => $body,
            $crate::bridge::bacnet::transport::TransportClient::Ip6($c) => $body,
        }
    };
}

// ---------------------------------------------------------------------------
// Submodules
// ---------------------------------------------------------------------------

pub mod config;
pub(crate) mod conversion;
pub(crate) mod transport;

mod data_services;
mod discovery;
mod loop_cov_poll;
mod loop_event_poll;
mod loop_monitor;
pub(crate) mod loop_time_sync;
mod loop_trend_sync;
mod networks;
mod operations;
mod server;
mod source;

#[cfg(test)]
mod tests;

// Re-export backoff from sibling module for use by loop_cov_poll
pub(crate) use super::backoff;

// ---------------------------------------------------------------------------
// Re-exports — preserve all existing public import paths
// ---------------------------------------------------------------------------

pub use config::{
    bacnet_config_from_scenario, bacnet_configs_from_scenario, BacnetConfig, BacnetMode,
};
pub use networks::BacnetNetworks;

// ---------------------------------------------------------------------------
// Imports for this file's struct/type definitions
// ---------------------------------------------------------------------------

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use rustbac_client::{ClientDataValue, ObjectStore};
use rustbac_core::types::ObjectId;
use tokio::task::JoinHandle;

use crate::config::profile::PointValue;
use crate::event::bus::EventBus;
use crate::store::history_store::HistoryStore;
use crate::store::point_store::PointStore;

use config::BacnetConfig as BacnetConfigType;
use transport::TransportClient;

// ---------------------------------------------------------------------------
// Discovered device/object model
// ---------------------------------------------------------------------------

/// A BACnet device discovered on the network.
#[derive(Debug, Clone)]
pub struct BacnetDevice {
    pub device_id: ObjectId,
    pub address: rustbac_datalink::DataLinkAddress,
    pub vendor: Option<String>,
    pub model: Option<String>,
    pub firmware_revision: Option<String>,
    pub location: Option<String>,
    pub description: Option<String>,
    pub max_apdu: Option<u32>,
    pub segmentation: Option<u32>,
    pub protocol_version: Option<u32>,
    pub app_software_version: Option<String>,
    pub objects: Vec<BacnetObject>,
    pub trend_logs: Vec<TrendLogRef>,
}

/// Reference to a TrendLog object on a remote device.
#[derive(Debug, Clone)]
pub struct TrendLogRef {
    pub object_id: ObjectId,
    pub object_name: Option<String>,
}

/// A BACnet object discovered via device walk.
#[derive(Debug, Clone)]
pub struct BacnetObject {
    pub object_id: ObjectId,
    pub object_name: Option<String>,
    pub description: Option<String>,
    pub units: Option<u32>,
    pub present_value: Option<ClientDataValue>,
    pub writable: bool,
}

/// An entry from the BACnet network routing table.
///
/// Represents a router that can reach a given destination network, as
/// reported by I-Am-Router-To-Network messages.
#[derive(Debug, Clone)]
pub struct RouterEntry {
    /// The BACnet network number reachable via this router.
    pub destination_network: u16,
    /// The router's address (IP:port or MAC, as a string).
    pub router_address: String,
}

/// Summary of an event/alarm on a remote BACnet device.
#[derive(Debug, Clone)]
pub struct BacnetEventInfo {
    pub object_id: ObjectId,
    pub event_state: u32,
    pub acknowledged_transitions: Option<Vec<u8>>,
    pub notify_type: Option<u32>,
    pub event_enable: Option<Vec<u8>>,
    pub event_priorities: Option<[u32; 3]>,
}

/// Information about a BACnet point's priority array.
#[derive(Debug, Clone)]
pub struct PriorityArrayInfo {
    /// 16 priority levels, index 0 = priority 1. None means relinquished (Null).
    pub levels: [Option<PointValue>; 16],
    /// The value used when all 16 levels are relinquished.
    pub relinquish_default: Option<PointValue>,
}

/// Health snapshot for COV subscriptions on a bridge.
#[derive(Debug, Clone)]
pub struct CovHealthSnapshot {
    pub total: usize,
    pub active: usize,
    pub stale: usize,
    pub mode: String,
    pub subscriptions: Vec<CovSubscriptionStatus>,
}

/// Status of a single COV subscription.
#[derive(Debug, Clone)]
pub struct CovSubscriptionStatus {
    pub device_instance: u32,
    pub object_id: ObjectId,
    pub active: bool,
    pub last_update_ms: i64,
    pub lifetime_secs: u32,
    pub subscribed_at_ms: i64,
}

// ---------------------------------------------------------------------------
// BacnetBridge — client-side BACnet integration (IP + SC)
// ---------------------------------------------------------------------------

pub struct BacnetBridge {
    /// Network identifier for this bridge instance (e.g. "ip-main", "mstp-field").
    network_id: String,
    discovery_timeout: Duration,
    poll_interval: Duration,
    cov_lifetime: u32,
    bacnet_config: BacnetConfigType,
    pub(crate) transport: Option<TransportClient>,
    pub(crate) devices: Vec<BacnetDevice>,
    /// Maps (device_instance, object_instance) -> PointKey for fast lookup
    pub(crate) point_map: HashMap<(u32, u32), ObjectId>,
    pub(crate) store: Option<PointStore>,
    pub(crate) history_store: Option<HistoryStore>,
    pub(crate) event_bus: Option<EventBus>,
    pub(crate) cov_handle: Option<JoinHandle<()>>,
    pub(crate) poll_handle: Option<JoinHandle<()>>,
    pub(crate) time_sync_handle: Option<JoinHandle<()>>,
    pub(crate) event_poll_handle: Option<JoinHandle<()>>,
    pub(crate) trend_log_handle: Option<JoinHandle<()>>,
    pub(crate) monitor_handle: Option<JoinHandle<()>>,
    /// Interval between background device monitor cycles (default 300s).
    /// Set to zero to disable.
    monitor_interval: Duration,
    /// Number of monitor cycles between object-list-length checks (default 6).
    object_check_cycles: u32,
    /// Interval between trend log sync cycles (default 600s).
    trend_log_sync_interval: Duration,
    /// Object store for the optional BACnet server (exposes local points to the network).
    pub(crate) server_object_store: Option<Arc<ObjectStore>>,
    /// Device instance number for the BACnet server (if configured).
    pub(crate) server_device_instance: Option<u32>,
    /// Device instances seen in the most recent rescan (Who-Is responses).
    pub(crate) last_scan_instances: HashSet<u32>,
}

impl Default for BacnetBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl BacnetBridge {
    pub fn new() -> Self {
        BacnetBridge {
            network_id: "default".to_string(),
            discovery_timeout: Duration::from_secs(5),
            poll_interval: Duration::from_secs(30),
            cov_lifetime: 300, // 5 minutes
            bacnet_config: BacnetConfigType::default(),
            transport: None,
            devices: Vec::new(),
            point_map: HashMap::new(),
            store: None,
            history_store: None,
            event_bus: None,
            cov_handle: None,
            poll_handle: None,
            time_sync_handle: None,
            event_poll_handle: None,
            trend_log_handle: None,
            monitor_handle: None,
            monitor_interval: Duration::from_secs(300),
            object_check_cycles: 6,
            trend_log_sync_interval: Duration::from_secs(600),
            server_object_store: None,
            server_device_instance: None,
            last_scan_instances: HashSet::new(),
        }
    }

    pub fn with_network_id(mut self, network_id: String) -> Self {
        self.network_id = network_id;
        self
    }

    /// Returns the network_id for this bridge.
    pub fn network_id(&self) -> &str {
        &self.network_id
    }

    pub fn with_discovery_timeout(mut self, timeout: Duration) -> Self {
        self.discovery_timeout = timeout;
        self
    }

    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    pub fn with_event_bus(mut self, bus: EventBus) -> Self {
        self.event_bus = Some(bus);
        self
    }

    pub fn with_bacnet_config(mut self, config: BacnetConfig) -> Self {
        self.bacnet_config = config;
        self
    }

    pub fn with_history_store(mut self, store: HistoryStore) -> Self {
        self.history_store = Some(store);
        self
    }

    pub fn with_monitor_interval(mut self, interval: Duration) -> Self {
        self.monitor_interval = interval;
        self
    }

    pub fn with_object_check_cycles(mut self, cycles: u32) -> Self {
        self.object_check_cycles = cycles;
        self
    }

    pub fn with_trend_log_sync_interval(mut self, interval: Duration) -> Self {
        self.trend_log_sync_interval = interval;
        self
    }

    /// Returns device instances that the bridge is actively polling/monitoring.
    /// Used by the background monitor to know which instances to check.
    pub fn accepted_device_instances(&self) -> Vec<u32> {
        self.devices
            .iter()
            .map(|d| d.device_id.instance())
            .collect()
    }

    /// Returns the configured trend log sync interval.
    pub fn trend_log_sync_interval(&self) -> Duration {
        self.trend_log_sync_interval
    }

    /// Return a snapshot of COV subscription health.
    pub fn cov_health(&self) -> CovHealthSnapshot {
        // Runtime COV health tracking is not yet wired into run_cov_inner,
        // so return a placeholder based on whether COV is active.
        let has_cov = self.cov_handle.is_some();
        CovHealthSnapshot {
            total: 0,
            active: 0,
            stale: 0,
            mode: if has_cov {
                "cov".into()
            } else {
                "polling".into()
            },
            subscriptions: Vec::new(),
        }
    }

    /// Reset trend log backfill for a specific device by restarting the sync loop.
    /// The next cycle will re-read all records from scratch since last_counts is cleared.
    pub fn reset_trend_log_backfill(&mut self) {
        if let Some(h) = self.trend_log_handle.take() {
            h.abort();
        }
        // Re-spawn with fresh state (last_counts will be empty -> full re-read)
        if let Some(ref tc) = self.transport {
            let has_trend_logs = self.devices.iter().any(|d| !d.trend_logs.is_empty());
            if has_trend_logs {
                if let Some(ref history_store) = self.history_store {
                    let tl_tc = tc.clone();
                    let tl_devices = self.devices.clone();
                    let tl_history = history_store.clone();
                    let tl_interval = self.trend_log_sync_interval;
                    let tl_handle = tokio::spawn(async move {
                        // No startup delay on manual reset
                        loop_trend_sync::run_trend_log_sync_loop(
                            tl_tc,
                            &tl_devices,
                            tl_history,
                            tl_interval,
                        )
                        .await;
                    });
                    self.trend_log_handle = Some(tl_handle);
                }
            }
        }
    }

    pub fn discovered_devices(&self) -> &[BacnetDevice] {
        &self.devices
    }

    /// Returns the set of device instances that responded to Who-Is in the most recent rescan.
    pub fn last_scan_instances(&self) -> &HashSet<u32> {
        &self.last_scan_instances
    }
}
