use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioConfig {
    pub scenario: ScenarioMeta,
    pub settings: Option<ScenarioSettings>,
    pub devices: Vec<DeviceInstance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioMeta {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScenarioSettings {
    pub tick_rate_ms: Option<u64>,
    pub realtime: Option<bool>,
    /// BACnet network config (kept for backward compat with existing scenarios)
    pub bacnet: Option<BacnetNetworkConfig>,
    /// Multiple BACnet networks keyed by network_id (e.g. "ip-main", "mstp-field").
    /// When non-empty, takes precedence over the legacy `bacnet` field.
    #[serde(default)]
    pub bacnet_networks: std::collections::HashMap<String, BacnetNetworkConfig>,
    /// Modbus network config (kept for backward compat with existing scenarios)
    pub modbus: Option<ModbusNetworkConfig>,
    /// Extensible protocol configs for plugin-provided protocols.
    /// Key = protocol identifier (e.g. "knx"), value = protocol-specific JSON config.
    #[serde(default)]
    pub protocols: std::collections::HashMap<String, serde_json::Value>,
    /// Web server configuration (HTTP/HTTPS ports, TLS).
    #[serde(default)]
    pub web_server: Option<WebServerConfig>,
    /// Optional durable event journal configuration.
    /// When absent or disabled, the event bus remains pure in-memory broadcast.
    #[serde(default)]
    pub event_journal: Option<EventJournalConfig>,
}

impl ScenarioSettings {
    /// Returns all BACnet networks. If `bacnet_networks` is empty but `bacnet` is set,
    /// wraps the legacy config as `{"default": config}`.
    pub fn resolved_bacnet_networks(
        &self,
    ) -> std::collections::HashMap<String, BacnetNetworkConfig> {
        if !self.bacnet_networks.is_empty() {
            return self.bacnet_networks.clone();
        }
        if let Some(ref cfg) = self.bacnet {
            let mut m = std::collections::HashMap::new();
            m.insert("default".to_string(), cfg.clone());
            return m;
        }
        std::collections::HashMap::new()
    }
}

/// BACnet network transport configuration in the scenario file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BacnetNetworkConfig {
    /// "normal", "foreign", or "sc"
    pub mode: Option<String>,
    /// BBMD address for foreign device mode (e.g., "192.168.1.1:47808")
    pub bbmd_addr: Option<String>,
    /// TTL in seconds for foreign device registration (default: 60)
    pub ttl: Option<u16>,
    /// WebSocket endpoint for BACnet/SC mode (e.g., "wss://hub.example.com:1234/bacnet")
    pub hub_endpoint: Option<String>,
    /// If set, start a BACnet server exposing local points as this device instance.
    pub server_device_instance: Option<u32>,
    /// Serial port path for MS/TP mode (e.g., "/dev/ttyUSB0" or "COM3").
    pub serial_port: Option<String>,
    /// Baud rate for MS/TP mode (default: 38400).
    pub baud_rate: Option<u32>,
    /// This node's MAC address for MS/TP mode (0-127, default: 0).
    pub mac_address: Option<u8>,
    /// Highest MAC address to poll for new masters in MS/TP mode (default: 127).
    pub max_master: Option<u8>,
    /// IPv6 multicast group for BACnet/IPv6 mode (default: "FF05::BAC0").
    pub ipv6_multicast_group: Option<String>,
    /// Interface name or index for IPv6 multicast (e.g., "eth0" or "0").
    pub ipv6_interface: Option<String>,
    /// Interval in seconds between background device monitor cycles (default: 300).
    /// Set to 0 to disable the background monitor.
    pub monitor_interval_secs: Option<u64>,
    /// Number of monitor cycles between object-list-length checks (default: 6).
    pub object_check_cycles: Option<u32>,
    /// Interval in seconds between trend log sync cycles (default: 600).
    pub trend_log_sync_interval_secs: Option<u64>,
    /// BACnet network number for this node (reserved for future router support — currently ignored).
    pub network_number: Option<u16>,
    /// Additional router ports (reserved for future router support — currently ignored).
    pub router_ports: Option<Vec<RouterPortConfig>>,
}

/// Configuration for an additional router port.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterPortConfig {
    /// The BACnet network number for this port.
    pub network: u16,
    /// Transport mode for this port ("normal", "mstp", "ipv6").
    pub mode: String,
    /// Bind address for IP-based transports.
    pub bind_addr: Option<String>,
    /// Serial port for MS/TP transports.
    pub serial_port: Option<String>,
    /// IPv6 multicast group (for ipv6 mode).
    pub multicast_group: Option<String>,
}

/// Modbus network transport configuration in the scenario file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModbusNetworkConfig {
    /// "tcp" (default) or "rtu"
    pub mode: Option<String>,
    /// RTU: serial port path (e.g. "/dev/ttyUSB0" or "COM3")
    pub serial_port: Option<String>,
    /// RTU: baud rate (default: 9600)
    pub baud_rate: Option<u32>,
    /// Response timeout in milliseconds (default: 5000)
    pub default_timeout_ms: Option<u64>,
    /// Number of retries on read failure (default: 3)
    pub default_retry_count: Option<u8>,
}

/// Web server configuration for the API/UI server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebServerConfig {
    /// Enable HTTP listener (default: true).
    #[serde(default = "default_true")]
    pub http_enabled: bool,
    /// HTTP listen port (default: 8080).
    #[serde(default = "default_http_port")]
    pub http_port: u16,
    /// Enable HTTPS listener (default: false).
    #[serde(default)]
    pub https_enabled: bool,
    /// HTTPS listen port (default: 8443).
    #[serde(default = "default_https_port")]
    pub https_port: u16,
    /// Path to TLS certificate file (PEM).
    pub cert_file: Option<String>,
    /// Path to TLS private key file (PEM).
    pub key_file: Option<String>,
    /// Redirect HTTP requests to HTTPS (default: false).
    #[serde(default)]
    pub redirect_to_https: bool,
    /// Listen address / network interface (default: "0.0.0.0").
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,
}

impl Default for WebServerConfig {
    fn default() -> Self {
        Self {
            http_enabled: true,
            http_port: 8080,
            https_enabled: false,
            https_port: 8443,
            cert_file: None,
            key_file: None,
            redirect_to_https: false,
            listen_addr: "0.0.0.0".into(),
        }
    }
}

fn default_true() -> bool {
    true
}
fn default_http_port() -> u16 {
    8080
}
fn default_https_port() -> u16 {
    8443
}
fn default_listen_addr() -> String {
    "0.0.0.0".into()
}

/// Configuration for the durable event journal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventJournalConfig {
    /// Enable the durable journal (default: false).
    #[serde(default)]
    pub enabled: bool,
    /// Maximum age of journal entries in seconds (default: 86400 = 24h).
    #[serde(default = "default_journal_max_age")]
    pub max_age_secs: u64,
    /// Maximum number of journal entries (default: 500_000).
    #[serde(default = "default_journal_max_events")]
    pub max_events: u64,
    /// Pruning interval in seconds (default: 300 = 5min).
    #[serde(default = "default_journal_prune_interval")]
    pub prune_interval_secs: u64,
}

impl Default for EventJournalConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_age_secs: 86400,
            max_events: 500_000,
            prune_interval_secs: 300,
        }
    }
}

fn default_journal_max_age() -> u64 {
    86400
}
fn default_journal_max_events() -> u64 {
    500_000
}
fn default_journal_prune_interval() -> u64 {
    300
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInstance {
    pub profile: String,
    pub instance_id: String,
    pub overrides: Option<serde_json::Value>,
}
