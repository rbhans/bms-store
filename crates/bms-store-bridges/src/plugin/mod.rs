pub mod archive;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::config::profile::PointValue;

// Re-exported from the bms-core crate — the canonical definitions live there.
pub use bms_core::{
    AlarmEvaluator, BridgeError, ExportNode, HistoryBackend, HistoryBackendError, HistoryQuery,
    HistoryResult, HistorySample, ImportExportError, ImportExportPlugin, ImportedNode,
    LogicContext, LogicEnginePlugin, ProtocolBridgeHandle, ProtocolPlugin,
};

// Alarm types used by the evaluator — also re-exported from the bms-core crate
pub use bms_core::{AlarmConfig, AlarmConfigId, AlarmParams, AlarmSeverity, AlarmState, AlarmType};

// ----------------------------------------------------------------
// Plugin Registry
// ----------------------------------------------------------------

/// Central registry for all plugins. Plugins are registered at startup.
/// No dynamic loading — all plugins are compiled in.
pub struct PluginRegistry {
    pub protocol_plugins: Vec<Box<dyn ProtocolPlugin>>,
    pub history_backends: Vec<Box<dyn HistoryBackend>>,
    pub alarm_evaluators: Vec<Box<dyn AlarmEvaluator>>,
    pub logic_engines: Vec<Box<dyn LogicEnginePlugin>>,
    pub import_export: Vec<Box<dyn ImportExportPlugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        PluginRegistry {
            protocol_plugins: Vec::new(),
            history_backends: Vec::new(),
            alarm_evaluators: Vec::new(),
            logic_engines: Vec::new(),
            import_export: Vec::new(),
        }
    }

    pub fn register_protocol(&mut self, plugin: Box<dyn ProtocolPlugin>) {
        self.protocol_plugins.push(plugin);
    }

    pub fn register_history_backend(&mut self, backend: Box<dyn HistoryBackend>) {
        self.history_backends.push(backend);
    }

    pub fn register_alarm_evaluator(&mut self, evaluator: Box<dyn AlarmEvaluator>) {
        self.alarm_evaluators.push(evaluator);
    }

    pub fn register_logic_engine(&mut self, engine: Box<dyn LogicEnginePlugin>) {
        self.logic_engines.push(engine);
    }

    pub fn register_import_export(&mut self, plugin: Box<dyn ImportExportPlugin>) {
        self.import_export.push(plugin);
    }

    pub fn find_protocol(&self, protocol_id: &str) -> Option<&dyn ProtocolPlugin> {
        self.protocol_plugins
            .iter()
            .find(|p| p.protocol_id() == protocol_id)
            .map(|p| p.as_ref())
    }

    /// Get all registered protocol IDs.
    pub fn protocol_ids(&self) -> Vec<&str> {
        self.protocol_plugins
            .iter()
            .map(|p| p.protocol_id())
            .collect()
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ----------------------------------------------------------------
// Bridge Registry — protocol-keyed map of live bridge handles
// ----------------------------------------------------------------

/// A protocol-keyed map of live bridge handles for generic write routing.
///
/// Bridges are registered at startup. The registry provides:
/// - `route_write()`: protocol-agnostic write routing (tries each bridge, skips PointNotFound)
/// - `get()`: protocol-specific access via downcast (e.g. `get("bacnet")` → `downcast_ref::<BacnetNetworks>()`)
pub struct BridgeRegistry {
    bridges: HashMap<String, Arc<Mutex<Box<dyn ProtocolBridgeHandle>>>>,
}

impl BridgeRegistry {
    pub fn new() -> Self {
        Self {
            bridges: HashMap::new(),
        }
    }

    /// Register a bridge handle for the given protocol.
    pub fn register(&mut self, protocol_id: &str, handle: Box<dyn ProtocolBridgeHandle>) {
        self.bridges
            .insert(protocol_id.to_string(), Arc::new(Mutex::new(handle)));
    }

    /// Get a bridge handle by protocol ID (for protocol-specific operations via downcast).
    pub fn get(&self, protocol_id: &str) -> Option<Arc<Mutex<Box<dyn ProtocolBridgeHandle>>>> {
        self.bridges.get(protocol_id).cloned()
    }

    /// Route a write to the appropriate bridge.
    ///
    /// Uses the device_id prefix (e.g. "bacnet-", "modbus-") to select the
    /// correct bridge directly. Falls back to probing all bridges if no prefix
    /// matches.
    pub async fn route_write(
        &self,
        device_id: &str,
        point_id: &str,
        value: PointValue,
        priority: Option<u8>,
    ) -> Result<(), BridgeError> {
        // Deterministic routing: match device_id prefix to protocol
        for (proto, handle) in &self.bridges {
            if device_id.starts_with(&format!("{proto}-")) {
                let guard = handle.lock().await;
                return guard
                    .write_point(device_id, point_id, value, priority)
                    .await;
            }
        }

        // Fallback: try each bridge (for devices without a protocol prefix)
        for handle in self.bridges.values() {
            let guard = handle.lock().await;
            match guard
                .write_point(device_id, point_id, value.clone(), priority)
                .await
            {
                Ok(()) => return Ok(()),
                Err(BridgeError::PointNotFound { .. }) => continue,
                Err(e) => return Err(e),
            }
        }
        Err(BridgeError::PointNotFound {
            device_id: device_id.to_string(),
            point_id: point_id.to_string(),
        })
    }

    /// Stop all registered bridges.
    pub async fn stop_all(&self) {
        for handle in self.bridges.values() {
            let mut guard = handle.lock().await;
            let _ = guard.stop().await;
        }
    }

    /// Get all registered protocol IDs.
    pub fn bridge_protocol_ids(&self) -> Vec<String> {
        self.bridges.keys().cloned().collect()
    }
}

impl Default for BridgeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ----------------------------------------------------------------
// Standard alarm evaluator (extracts existing logic into a plugin)
// ----------------------------------------------------------------

/// Default alarm evaluator — implements the standard BAS alarm logic.
pub struct StandardAlarmEvaluator;

impl AlarmEvaluator for StandardAlarmEvaluator {
    fn evaluate(&self, _config: &AlarmConfig, _value: &PointValue, prev: AlarmState) -> AlarmState {
        // Default: delegate to the existing alarm engine logic.
        // This is a placeholder — the actual evaluation still lives in alarm_store.rs
        // for now. The trait boundary is what matters.
        prev
    }
}

// ----------------------------------------------------------------
// Plugin Catalog — metadata for all known optional plugins
// ----------------------------------------------------------------

/// How a plugin is activated: feature-gated at compile time, or data-driven at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginKind {
    /// Requires a Cargo feature flag to compile in.
    FeatureGated,
    /// Always compiled; activated by downloading data or configuring at runtime.
    DataDriven,
}

/// Static metadata for a known plugin.
#[derive(Debug, Clone, PartialEq)]
pub struct PluginInfo {
    /// Unique identifier (e.g. "atlas", "simulation").
    pub id: &'static str,
    /// Human-readable name.
    pub name: &'static str,
    /// Short description.
    pub description: &'static str,
    /// How the plugin is gated.
    pub kind: PluginKind,
    /// Cargo feature flag name (for FeatureGated plugins).
    pub feature_flag: Option<&'static str>,
    /// Whether the feature was compiled into this binary.
    pub compiled_in: bool,
    /// Config section label to navigate to (if the plugin has a settings page).
    pub config_section: Option<&'static str>,
}

/// Return the catalog of all known plugins with their compile-time availability.
pub fn plugin_catalog() -> Vec<PluginInfo> {
    vec![
        PluginInfo {
            id: "atlas",
            name: "BAS Atlas Taxonomy",
            description:
                "8000+ point/equipment aliases for richer auto-tagging during device acceptance.",
            kind: PluginKind::FeatureGated,
            feature_flag: Some("atlas"),
            compiled_in: cfg!(feature = "atlas"),
            config_section: Some("Atlas"),
        },
        // Future plugins go here. Example:
        // PluginInfo {
        //     id: "simulation",
        //     name: "Building Simulation",
        //     description: "Simulated BAS devices for testing and demo.",
        //     kind: PluginKind::FeatureGated,
        //     feature_flag: Some("simulation"),
        //     compiled_in: cfg!(feature = "simulation"),
        //     config_section: None,
        // },
    ]
}

/// Runtime status of a plugin within a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginStatus {
    /// Feature not compiled in — cannot be used.
    NotCompiled,
    /// Compiled in but no data installed yet.
    Available,
    /// Data installed but explicitly disabled by the user.
    Disabled,
    /// Data installed and active.
    Active,
}

impl PluginStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::NotCompiled => "Not compiled",
            Self::Available => "Available",
            Self::Disabled => "Disabled",
            Self::Active => "Active",
        }
    }

    pub fn css_class(&self) -> &'static str {
        match self {
            Self::NotCompiled => "plugin-status-off",
            Self::Available => "plugin-status-available",
            Self::Disabled => "plugin-status-disabled",
            Self::Active => "plugin-status-active",
        }
    }
}

// ----------------------------------------------------------------
// Plugin Settings — persisted per-project enable/disable state
// ----------------------------------------------------------------

/// Per-project plugin settings, persisted to `data/plugin-settings.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginSettings {
    /// Plugin ID → enabled. Missing = default (enabled if installed).
    pub disabled: Vec<String>,
}

const PLUGIN_SETTINGS_FILE: &str = "plugin-settings.json";

/// Save plugin settings to the project data directory.
pub fn save_plugin_settings(data_dir: &Path, settings: &PluginSettings) {
    let path = data_dir.join(PLUGIN_SETTINGS_FILE);
    if let Ok(json) = serde_json::to_string_pretty(settings) {
        let _ = std::fs::write(path, json);
    }
}

/// Load plugin settings from the project data directory.
pub fn load_plugin_settings(data_dir: &Path) -> PluginSettings {
    let path = data_dir.join(PLUGIN_SETTINGS_FILE);
    std::fs::read_to_string(path)
        .ok()
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}

/// Determine the runtime status of a plugin given its info and project state.
pub fn resolve_plugin_status(
    info: &PluginInfo,
    data_installed: bool,
    settings: &PluginSettings,
) -> PluginStatus {
    if !info.compiled_in {
        return PluginStatus::NotCompiled;
    }
    if !data_installed {
        return PluginStatus::Available;
    }
    if settings.disabled.contains(&info.id.to_string()) {
        return PluginStatus::Disabled;
    }
    PluginStatus::Active
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_creation() {
        let reg = PluginRegistry::new();
        assert!(reg.protocol_plugins.is_empty());
        assert!(reg.history_backends.is_empty());
        assert!(reg.protocol_ids().is_empty());
    }

    #[test]
    fn bridge_registry_creation() {
        let reg = BridgeRegistry::new();
        assert!(reg.bridge_protocol_ids().is_empty());
        assert!(reg.get("bacnet").is_none());
    }

    #[test]
    fn standard_evaluator() {
        let eval = StandardAlarmEvaluator;
        let state = eval.evaluate(
            &AlarmConfig::new(
                1,
                "d".into(),
                "p".into(),
                AlarmType::HighLimit,
                AlarmSeverity::Warning,
                true,
                AlarmParams::HighLimit {
                    limit: 100.0,
                    deadband: 1.0,
                    delay_secs: 0,
                },
            ),
            &PointValue::Float(105.0),
            AlarmState::Normal,
        );
        // Standard evaluator is a placeholder — just returns prev state
        assert_eq!(state, AlarmState::Normal);
    }

    #[test]
    fn plugin_catalog_contains_atlas() {
        let catalog = plugin_catalog();
        assert!(!catalog.is_empty());
        let atlas = catalog.iter().find(|p| p.id == "atlas");
        assert!(atlas.is_some());
        let atlas = atlas.unwrap();
        assert_eq!(atlas.name, "BAS Atlas Taxonomy");
        assert_eq!(atlas.feature_flag, Some("atlas"));
    }

    #[test]
    fn resolve_status_not_compiled() {
        let info = PluginInfo {
            id: "test",
            name: "Test",
            description: "",
            kind: PluginKind::FeatureGated,
            feature_flag: Some("test"),
            compiled_in: false,
            config_section: None,
        };
        let settings = PluginSettings::default();
        assert_eq!(
            resolve_plugin_status(&info, false, &settings),
            PluginStatus::NotCompiled
        );
        assert_eq!(
            resolve_plugin_status(&info, true, &settings),
            PluginStatus::NotCompiled
        );
    }

    #[test]
    fn resolve_status_available_active_disabled() {
        let info = PluginInfo {
            id: "test",
            name: "Test",
            description: "",
            kind: PluginKind::FeatureGated,
            feature_flag: Some("test"),
            compiled_in: true,
            config_section: None,
        };
        let settings = PluginSettings::default();
        // Not installed → Available
        assert_eq!(
            resolve_plugin_status(&info, false, &settings),
            PluginStatus::Available
        );
        // Installed → Active
        assert_eq!(
            resolve_plugin_status(&info, true, &settings),
            PluginStatus::Active
        );
        // Installed but disabled → Disabled
        let disabled_settings = PluginSettings {
            disabled: vec!["test".into()],
        };
        assert_eq!(
            resolve_plugin_status(&info, true, &disabled_settings),
            PluginStatus::Disabled
        );
    }

    #[test]
    fn plugin_settings_persistence() {
        let dir = std::env::temp_dir().join(format!("plugin-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        let settings = PluginSettings {
            disabled: vec!["atlas".into()],
        };
        save_plugin_settings(&dir, &settings);
        let loaded = load_plugin_settings(&dir);
        assert_eq!(loaded.disabled, vec!["atlas".to_string()]);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
