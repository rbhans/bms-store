use std::collections::HashMap;

use crate::config::scenario::ScenarioSettings;

// ---------------------------------------------------------------------------
// BACnet network configuration
// ---------------------------------------------------------------------------

/// How the BACnet client connects to the network.
#[derive(Debug, Clone)]
pub enum BacnetMode {
    /// Standard BACnet/IP on the local subnet.
    Normal,
    /// Register as a foreign device with a BBMD for cross-subnet communication.
    Foreign {
        bbmd_addr: std::net::SocketAddr,
        ttl: u16,
    },
    /// BACnet Secure Connect — tunnel over WebSocket to a BACnet/SC hub.
    SecureConnect { hub_endpoint: String },
    /// MS/TP over RS-485 serial port.
    Mstp {
        port: String,
        baud_rate: u32,
        mac_address: u8,
        max_master: u8,
    },
    /// BACnet/IPv6 (Annex U) over UDP multicast.
    Ipv6 {
        multicast_group: std::net::Ipv6Addr,
        interface: String,
    },
}

/// Configuration for the BACnet network transport.
#[derive(Debug, Clone)]
pub struct BacnetConfig {
    pub mode: BacnetMode,
    /// Network identifier (e.g. "ip-main", "mstp-field", or "default" for single-network).
    pub network_id: String,
}

impl Default for BacnetConfig {
    fn default() -> Self {
        Self {
            mode: BacnetMode::Normal,
            network_id: "default".to_string(),
        }
    }
}

/// Parse a single `BacnetNetworkConfig` into a `BacnetMode`.
pub(crate) fn parse_bacnet_mode(
    bacnet_net: &crate::config::scenario::BacnetNetworkConfig,
) -> BacnetMode {
    let mode_str = bacnet_net.mode.as_deref().unwrap_or("normal");
    let mode = match mode_str {
        "foreign" => {
            let addr_str = bacnet_net
                .bbmd_addr
                .as_deref()
                .unwrap_or("255.255.255.255:47808");
            let addr = addr_str
                .parse()
                .unwrap_or_else(|_| "255.255.255.255:47808".parse().unwrap());
            let ttl = bacnet_net.ttl.unwrap_or(60);
            BacnetMode::Foreign {
                bbmd_addr: addr,
                ttl,
            }
        }
        "sc" => {
            let hub = bacnet_net.hub_endpoint.clone().unwrap_or_default();
            BacnetMode::SecureConnect { hub_endpoint: hub }
        }
        "mstp" => {
            let port = bacnet_net.serial_port.clone().unwrap_or_default();
            let baud = bacnet_net.baud_rate.unwrap_or(38400);
            let mac = bacnet_net.mac_address.unwrap_or(0);
            let max_master = bacnet_net.max_master.unwrap_or(127);
            BacnetMode::Mstp {
                port,
                baud_rate: baud,
                mac_address: mac,
                max_master,
            }
        }
        "ipv6" => {
            let group_str = bacnet_net
                .ipv6_multicast_group
                .as_deref()
                .unwrap_or("FF05::BAC0");
            let group: std::net::Ipv6Addr = group_str
                .parse()
                .unwrap_or_else(|_| "FF05::BAC0".parse().unwrap());
            let interface = bacnet_net
                .ipv6_interface
                .clone()
                .unwrap_or_else(|| "0".to_string());
            BacnetMode::Ipv6 {
                multicast_group: group,
                interface,
            }
        }
        _ => BacnetMode::Normal,
    };

    if bacnet_net.network_number.is_some() || bacnet_net.router_ports.is_some() {
        tracing::warn!(
            "BACnet: 'network_number' and 'router_ports' are present in config \
             but routing is not yet wired into the bridge — these settings will be ignored"
        );
    }

    mode
}

/// Convert scenario-level BACnet network settings into a single `BacnetConfig`.
/// For backward compatibility — returns the legacy single config as network_id "default".
pub fn bacnet_config_from_scenario(settings: &Option<ScenarioSettings>) -> BacnetConfig {
    let bacnet_net = settings.as_ref().and_then(|s| s.bacnet.as_ref());

    let bacnet_net = match bacnet_net {
        Some(b) => b,
        None => return BacnetConfig::default(),
    };

    BacnetConfig {
        mode: parse_bacnet_mode(bacnet_net),
        network_id: "default".to_string(),
    }
}

/// Convert scenario-level BACnet settings into a map of `BacnetConfig` keyed by network_id.
/// Uses `resolved_bacnet_networks()` which merges legacy `bacnet` into `bacnet_networks`.
pub fn bacnet_configs_from_scenario(
    settings: &Option<ScenarioSettings>,
) -> HashMap<String, BacnetConfig> {
    let networks = match settings.as_ref() {
        Some(s) => s.resolved_bacnet_networks(),
        None => return HashMap::new(),
    };

    networks
        .iter()
        .map(|(network_id, net_cfg)| {
            let config = BacnetConfig {
                mode: parse_bacnet_mode(net_cfg),
                network_id: network_id.clone(),
            };
            (network_id.clone(), config)
        })
        .collect()
}

/// Resolve an interface string to an OS interface index.
/// Accepts either a numeric index (e.g. "2") or an interface name (e.g. "eth0", "en0").
/// Returns 0 (any interface) if resolution fails.
pub(super) fn resolve_interface_index(interface: &str) -> u32 {
    // Try numeric first.
    if let Ok(idx) = interface.parse::<u32>() {
        return idx;
    }
    // Try resolving as interface name via libc if_nametoindex.
    let c_name = std::ffi::CString::new(interface).unwrap_or_default();
    if c_name.as_bytes().is_empty() {
        return 0;
    }
    // SAFETY: if_nametoindex is safe to call with a valid C string; returns 0 on failure.
    let idx = unsafe { libc::if_nametoindex(c_name.as_ptr()) };
    if idx == 0 {
        tracing::warn!(
            interface,
            "BACnet/IPv6: could not resolve interface to index, using 0 (any)"
        );
    }
    idx
}
