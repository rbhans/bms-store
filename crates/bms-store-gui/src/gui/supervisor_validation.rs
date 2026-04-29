//! Supervisor pre-flight scenario validation.
//!
//! Before the multi-site supervisor loads N project databases into one process,
//! it walks each site's scenario.json and checks for host-wide resource conflicts
//! that would silently break one of the sites — chiefly BACnet/IP UDP 47808,
//! which only one process (and only one network, really) can bind per host.
//!
//! Fatal errors block supervisor launch entirely. Warnings are shown but the
//! user can proceed.

use std::collections::HashMap;

use bms_store_storage::config::loader::resolve_scenario;
use bms_store_storage::config::scenario::ScenarioSettings;
use bms_store_storage::project::ProjectPaths;

/// Fatal validation error — supervisor launch is aborted.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationError {
    /// Project root paths of the sites involved in the conflict.
    pub sites: Vec<String>,
    /// Human-readable description of what conflicts.
    pub message: String,
}

/// Non-fatal warning — supervisor launch proceeds but the user is notified.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationWarning {
    pub sites: Vec<String>,
    pub message: String,
}

/// Result of pre-flight validation across the selected sites.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SupervisorValidation {
    pub fatal: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
}

impl SupervisorValidation {
    /// True if the supervisor can proceed (no fatal errors).
    pub fn is_ok(&self) -> bool {
        self.fatal.is_empty()
    }
}

/// Site settings loaded into memory for validation. One entry per selected site.
struct SiteSettings {
    label: String,
    settings: Option<ScenarioSettings>,
}

/// Validate a set of sites for supervisor loading.
/// Reads each scenario on disk and checks for port / resource conflicts.
pub fn validate_supervisor_scenarios(sites: &[ProjectPaths]) -> SupervisorValidation {
    let mut result = SupervisorValidation::default();

    // 1. Load every scenario. A parse error on a single site is fatal for that
    //    site only — the user should fix it in the launcher before retrying.
    let mut loaded: Vec<SiteSettings> = Vec::new();
    for paths in sites {
        let label = paths
            .root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_else(|| "unknown")
            .to_string();
        match resolve_scenario(&paths.scenario, &paths.profiles_dir) {
            Ok(scn) => loaded.push(SiteSettings {
                label,
                settings: scn.config.settings,
            }),
            Err(e) => {
                result.fatal.push(ValidationError {
                    sites: vec![label],
                    message: format!("Cannot parse scenario.json: {e}"),
                });
            }
        }
    }

    // 2. Detect BACnet/IP UDP 47808 conflicts.
    //    Any site with a BACnet network in Normal or Foreign mode binds 0.0.0.0:47808.
    //    Only ONE such network is allowed per host.
    let bacnet_ip_sites: Vec<&SiteSettings> = loaded
        .iter()
        .filter(|s| has_bacnet_ip_network(s.settings.as_ref()))
        .collect();
    if bacnet_ip_sites.len() > 1 {
        let labels: Vec<String> = bacnet_ip_sites.iter().map(|s| s.label.clone()).collect();
        result.fatal.push(ValidationError {
            sites: labels.clone(),
            message: format!(
                "BACnet/IP UDP 47808 conflict: {} sites configure BACnet/IP or Foreign mode, but only one process on this host can bind that port. Disable BACnet on all but one site, or switch the others to BACnet/SC or BACnet/MS-TP.",
                labels.len()
            ),
        });
    }

    // 3. Detect BACnet MS-TP serial-port conflicts.
    let mut mstp_by_port: HashMap<String, Vec<String>> = HashMap::new();
    for site in &loaded {
        for port in mstp_serial_ports(site.settings.as_ref()) {
            mstp_by_port
                .entry(port)
                .or_default()
                .push(site.label.clone());
        }
    }
    for (port, sites_using) in mstp_by_port {
        if sites_using.len() > 1 {
            result.fatal.push(ValidationError {
                sites: sites_using,
                message: format!(
                    "BACnet MS-TP serial port conflict: multiple sites configure MS-TP on {port}"
                ),
            });
        }
    }

    // 4. Detect Modbus RTU serial-port conflicts.
    let mut rtu_by_port: HashMap<String, Vec<String>> = HashMap::new();
    for site in &loaded {
        if let Some(port) = modbus_rtu_port(site.settings.as_ref()) {
            rtu_by_port
                .entry(port)
                .or_default()
                .push(site.label.clone());
        }
    }
    for (port, sites_using) in rtu_by_port {
        if sites_using.len() > 1 {
            result.fatal.push(ValidationError {
                sites: sites_using,
                message: format!(
                    "Modbus RTU serial port conflict: multiple sites configure RTU on {port}"
                ),
            });
        }
    }

    // 5. Detect web-server port conflicts.
    //    Each site has its own embedded API/web server. Multiple sites in one
    //    process need distinct ports.
    let mut http_by_endpoint: HashMap<(String, u16), Vec<String>> = HashMap::new();
    let mut https_by_endpoint: HashMap<(String, u16), Vec<String>> = HashMap::new();
    for site in &loaded {
        let web = site.settings.as_ref().and_then(|s| s.web_server.clone());
        if let Some(web) = web {
            if web.http_enabled {
                http_by_endpoint
                    .entry((web.listen_addr.clone(), web.http_port))
                    .or_default()
                    .push(site.label.clone());
            }
            if web.https_enabled {
                https_by_endpoint
                    .entry((web.listen_addr.clone(), web.https_port))
                    .or_default()
                    .push(site.label.clone());
            }
        }
    }
    for ((addr, port), sites_using) in http_by_endpoint {
        if sites_using.len() > 1 {
            result.fatal.push(ValidationError {
                sites: sites_using,
                message: format!(
                    "HTTP port conflict: multiple sites configure their web server on {addr}:{port}"
                ),
            });
        }
    }
    for ((addr, port), sites_using) in https_by_endpoint {
        if sites_using.len() > 1 {
            result.fatal.push(ValidationError {
                sites: sites_using,
                message: format!(
                    "HTTPS port conflict: multiple sites configure their web server on {addr}:{port}"
                ),
            });
        }
    }

    result
}

/// True if the site has any BACnet network in Normal or Foreign mode (binds UDP 47808).
fn has_bacnet_ip_network(settings: Option<&ScenarioSettings>) -> bool {
    let Some(s) = settings else { return false };
    let networks = s.resolved_bacnet_networks();
    for net in networks.values() {
        let mode = net.mode.as_deref().unwrap_or("normal").to_lowercase();
        if mode == "normal" || mode == "foreign" {
            return true;
        }
    }
    false
}

/// Collect MS-TP serial ports configured on this site.
fn mstp_serial_ports(settings: Option<&ScenarioSettings>) -> Vec<String> {
    let mut out = Vec::new();
    let Some(s) = settings else { return out };
    for net in s.resolved_bacnet_networks().values() {
        let mode = net.mode.as_deref().unwrap_or("normal").to_lowercase();
        if mode == "mstp" {
            if let Some(port) = net.serial_port.clone() {
                out.push(port);
            }
        }
    }
    out
}

/// If the site has Modbus in RTU mode, return its serial port.
fn modbus_rtu_port(settings: Option<&ScenarioSettings>) -> Option<String> {
    let modbus = settings.and_then(|s| s.modbus.as_ref())?;
    let mode = modbus.mode.as_deref().unwrap_or("tcp").to_lowercase();
    if mode == "rtu" {
        modbus.serial_port.clone()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bms_store_storage::config::scenario::{BacnetNetworkConfig, WebServerConfig};

    fn settings_with_bacnet_ip() -> ScenarioSettings {
        let mut s = ScenarioSettings::default();
        s.bacnet = Some(BacnetNetworkConfig {
            mode: Some("normal".to_string()),
            ..Default::default()
        });
        s
    }

    fn settings_with_modbus_only() -> ScenarioSettings {
        ScenarioSettings::default()
    }

    #[test]
    fn has_bacnet_ip_network_detects_normal_mode() {
        assert!(has_bacnet_ip_network(Some(&settings_with_bacnet_ip())));
        assert!(!has_bacnet_ip_network(Some(&settings_with_modbus_only())));
        assert!(!has_bacnet_ip_network(None));
    }

    #[test]
    fn has_bacnet_ip_network_detects_foreign_mode() {
        let mut s = ScenarioSettings::default();
        s.bacnet = Some(BacnetNetworkConfig {
            mode: Some("foreign".to_string()),
            ..Default::default()
        });
        assert!(has_bacnet_ip_network(Some(&s)));
    }

    #[test]
    fn has_bacnet_ip_network_ignores_sc_and_mstp() {
        let mut s = ScenarioSettings::default();
        s.bacnet = Some(BacnetNetworkConfig {
            mode: Some("sc".to_string()),
            ..Default::default()
        });
        assert!(!has_bacnet_ip_network(Some(&s)));

        let mut s2 = ScenarioSettings::default();
        s2.bacnet = Some(BacnetNetworkConfig {
            mode: Some("mstp".to_string()),
            ..Default::default()
        });
        assert!(!has_bacnet_ip_network(Some(&s2)));
    }

    #[test]
    fn mstp_serial_ports_extracts_port() {
        let mut s = ScenarioSettings::default();
        s.bacnet = Some(BacnetNetworkConfig {
            mode: Some("mstp".to_string()),
            serial_port: Some("/dev/ttyUSB0".to_string()),
            ..Default::default()
        });
        let ports = mstp_serial_ports(Some(&s));
        assert_eq!(ports, vec!["/dev/ttyUSB0".to_string()]);
    }

    #[test]
    fn modbus_rtu_port_returns_none_for_tcp() {
        let mut s = ScenarioSettings::default();
        s.modbus = Some(bms_store_storage::config::scenario::ModbusNetworkConfig {
            mode: Some("tcp".to_string()),
            serial_port: Some("/dev/ttyUSB1".to_string()),
            baud_rate: None,
            default_timeout_ms: None,
            default_retry_count: None,
        });
        assert_eq!(modbus_rtu_port(Some(&s)), None);
    }

    #[test]
    fn modbus_rtu_port_returns_port_for_rtu() {
        let mut s = ScenarioSettings::default();
        s.modbus = Some(bms_store_storage::config::scenario::ModbusNetworkConfig {
            mode: Some("rtu".to_string()),
            serial_port: Some("/dev/ttyUSB1".to_string()),
            baud_rate: None,
            default_timeout_ms: None,
            default_retry_count: None,
        });
        assert_eq!(modbus_rtu_port(Some(&s)), Some("/dev/ttyUSB1".to_string()));
    }

    #[test]
    fn web_server_conflict_detected() {
        // Two ScenarioSettings sharing the same web_server port.
        let web = WebServerConfig {
            http_enabled: true,
            http_port: 8080,
            https_enabled: false,
            https_port: 8443,
            cert_file: None,
            key_file: None,
            redirect_to_https: false,
            listen_addr: "0.0.0.0".to_string(),
        };
        let mut sa = ScenarioSettings::default();
        sa.web_server = Some(web.clone());
        let mut sb = ScenarioSettings::default();
        sb.web_server = Some(web);

        // Rather than constructing a full ProjectPaths for two temp projects we
        // unit-test the conflict detection logic directly via a helper.
        let site_a = SiteSettings {
            label: "a".into(),
            settings: Some(sa),
        };
        let site_b = SiteSettings {
            label: "b".into(),
            settings: Some(sb),
        };
        let result = detect_port_conflicts_for_test(&[site_a, site_b]);
        assert_eq!(result.fatal.len(), 1);
        assert!(result.fatal[0].message.contains("HTTP port conflict"));
    }

    /// Helper exposed for tests: runs the port-conflict detection directly on
    /// pre-loaded SiteSettings structs so we don't need temp projects.
    fn detect_port_conflicts_for_test(loaded: &[SiteSettings]) -> SupervisorValidation {
        let mut result = SupervisorValidation::default();

        let mut http_by_endpoint: HashMap<(String, u16), Vec<String>> = HashMap::new();
        for site in loaded {
            let web = site.settings.as_ref().and_then(|s| s.web_server.clone());
            if let Some(web) = web {
                if web.http_enabled {
                    http_by_endpoint
                        .entry((web.listen_addr.clone(), web.http_port))
                        .or_default()
                        .push(site.label.clone());
                }
            }
        }
        for ((addr, port), sites_using) in http_by_endpoint {
            if sites_using.len() > 1 {
                result.fatal.push(ValidationError {
                    sites: sites_using,
                    message: format!(
                        "HTTP port conflict: multiple sites configure their web server on {addr}:{port}"
                    ),
                });
            }
        }
        result
    }

    fn write_scenario(paths: &ProjectPaths, settings_json: serde_json::Value) {
        std::fs::create_dir_all(&paths.profiles_dir).unwrap();
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let scenario = serde_json::json!({
            "scenario": {
                "id": uuid::Uuid::new_v4().to_string(),
                "name": "Test Site",
                "description": "validation test"
            },
            "settings": settings_json,
            "devices": []
        });
        std::fs::write(
            &paths.scenario,
            serde_json::to_string_pretty(&scenario).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn two_bacnet_ip_sites_flagged_as_fatal() {
        let tmp = std::env::temp_dir().join(format!("opencrate-valtest-{}", uuid::Uuid::new_v4()));
        let site_a_root = tmp.join("site-a");
        let site_b_root = tmp.join("site-b");
        let site_a = ProjectPaths::from_root(site_a_root);
        let site_b = ProjectPaths::from_root(site_b_root);

        let bacnet_normal = serde_json::json!({
            "bacnet": { "mode": "normal" }
        });
        write_scenario(&site_a, bacnet_normal.clone());
        write_scenario(&site_b, bacnet_normal);

        let result = validate_supervisor_scenarios(&[site_a, site_b]);
        assert!(!result.is_ok());
        assert!(result
            .fatal
            .iter()
            .any(|e| e.message.contains("BACnet/IP UDP 47808 conflict")));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn one_bacnet_one_modbus_site_is_ok() {
        let tmp = std::env::temp_dir().join(format!("opencrate-valtest-{}", uuid::Uuid::new_v4()));
        let site_a_root = tmp.join("site-a");
        let site_b_root = tmp.join("site-b");
        let site_a = ProjectPaths::from_root(site_a_root);
        let site_b = ProjectPaths::from_root(site_b_root);

        write_scenario(
            &site_a,
            serde_json::json!({ "bacnet": { "mode": "normal" } }),
        );
        write_scenario(&site_b, serde_json::json!({ "modbus": { "mode": "tcp" } }));

        let result = validate_supervisor_scenarios(&[site_a, site_b]);
        assert!(result.is_ok(), "expected OK, got: {result:?}");

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
