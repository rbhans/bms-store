use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

use crate::bridge::bacnet::{bacnet_configs_from_scenario, BacnetBridge, BacnetNetworks};
use crate::bridge::modbus::{modbus_config_from_scenario, ModbusBridge};
use crate::bridge::traits::PointSource;
use crate::config::loader::{resolve_scenario, LoadedScenario};
use crate::config::template::auto_create_nodes;
use crate::discovery::service::DiscoveryService;
use crate::event::bus::EventBus;
use crate::event::journal::EventJournal;
use crate::export::publisher::ExportPublisher;
use crate::health::HealthRegistry;
use crate::logic::store::{start_program_store_with_path, ProgramStore};
use crate::mqtt::publisher::MqttPublisher;
use crate::notification::router::AlarmRouter;
use crate::plugin::{BridgeRegistry, PluginRegistry};
use crate::project::ProjectPaths;
use crate::store::alarm_store::{start_alarm_engine_with_path, AlarmStore};
#[cfg(feature = "cloud")]
use crate::store::cloud_store::{start_cloud_store_with_path, CloudStore};
use crate::store::commissioning_store::{start_commissioning_store_with_path, CommissioningStore};
use crate::store::discovery_store::{
    start_conn_status_listener, start_discovery_store_with_path, DiscoveryStore,
};
use crate::store::energy_store::{start_energy_store_with_path, EnergyStore};
use crate::store::entity_store::{start_entity_store_with_path, EntityStore};
use crate::store::export_store::{start_export_store_with_path, ExportStore};
use crate::store::fdd_store::{start_fdd_store_with_path, FddStore};
use crate::store::history_store::{start_history_collector_with_path, HistoryStore};
use crate::store::mqtt_store::{start_mqtt_store_with_path, MqttStore};
use crate::store::node_store::{start_node_store_with_path, NodeStore};
use crate::store::notification_store::{start_notification_store_with_path, NotificationStore};
use crate::store::point_store::{PointStore, PointStoreProfileExt};
use crate::store::report_store::start_report_store_with_path;
use crate::store::schedule_store::{start_schedule_engine_with_path, ScheduleStore};
use crate::store::webhook_store::{start_webhook_store_with_path, WebhookStore};
use crate::webhook::dispatcher::WebhookDispatcher;

/// Core model state — the platform data layer.
pub struct ModelState {
    pub point_store: PointStore,
    pub node_store: NodeStore,
    pub event_bus: EventBus,
    pub plugin_registry: PluginRegistry,
    pub loaded: LoadedScenario,
    pub health: HealthRegistry,
    /// Live WASM plugin runtime — holds all loaded plugin instances.
    /// `None` when the `wasm-plugins` feature is disabled or init failed.
    /// Wrapped in Arc so the execution engine can share it.
    #[cfg(feature = "wasm-plugins")]
    pub wasm_runtime: Option<std::sync::Arc<opencrate_plugin_wasm::WasmPluginRuntime>>,
}

/// Automation engines — alarm, schedule, history, logic, notifications.
pub struct AutomationState {
    pub alarm_store: AlarmStore,
    pub schedule_store: ScheduleStore,
    pub history_store: HistoryStore,
    pub entity_store: EntityStore,
    pub discovery_store: DiscoveryStore,
    pub program_store: ProgramStore,
    pub notification_store: NotificationStore,
    pub mqtt_store: MqttStore,
    pub commissioning_store: CommissioningStore,
    pub report_store: crate::store::report_store::ReportStore,
    pub energy_store: EnergyStore,
    pub webhook_store: WebhookStore,
    pub fdd_store: FddStore,
    pub export_store: ExportStore,
    #[cfg(feature = "cloud")]
    pub cloud_store: CloudStore,
}

/// The full platform — everything except GUI signals.
pub struct Platform {
    pub model: ModelState,
    pub automation: AutomationState,
    pub discovery_service: DiscoveryService,
    /// Shared shutdown token — cancel this to stop all background tasks.
    pub shutdown: tokio_util::sync::CancellationToken,
    /// Shared lock on Atlas matcher — same instance as DiscoveryService holds.
    #[cfg(feature = "atlas")]
    pub atlas_lock: Arc<std::sync::RwLock<Option<Arc<crate::atlas::matcher::AtlasMatcher>>>>,
}

/// Bridge handles for write routing.
pub struct BridgeHandles {
    pub bacnet: BacnetNetworks,
    pub modbus: Option<ModbusBridge>,
}

/// Status of a single bridge start attempt.
#[derive(Debug, Clone, Default)]
pub enum BridgeStartStatus {
    /// Bridge started successfully (or was not configured at all).
    #[default]
    Ok,
    /// Bridge failed to start. The error string is human-readable.
    Failed(String),
}

impl BridgeStartStatus {
    pub fn is_ok(&self) -> bool {
        matches!(self, BridgeStartStatus::Ok)
    }

    pub fn error(&self) -> Option<&str> {
        match self {
            BridgeStartStatus::Failed(e) => Some(e),
            BridgeStartStatus::Ok => None,
        }
    }
}

/// Report of which protocol bridges started cleanly during `init_platform`.
///
/// Used by single-site mode to surface bind failures in the Health view, and by
/// the multi-site supervisor to detect that a port collision (for example BACnet/IP
/// UDP 47808) silently broke a site.
#[derive(Debug, Clone, Default)]
pub struct BridgeStartReport {
    /// Per-BACnet-network status, keyed by `network_id` (e.g. `"default"`, `"site-a"`).
    pub bacnet: std::collections::HashMap<String, BridgeStartStatus>,
    /// Modbus bridge status. `Ok` if Modbus is not configured at all.
    pub modbus: BridgeStartStatus,
}

impl BridgeStartReport {
    /// Returns true if every configured bridge started cleanly.
    pub fn all_ok(&self) -> bool {
        self.modbus.is_ok() && self.bacnet.values().all(|s| s.is_ok())
    }

    /// Iterator over `(label, error)` pairs for any bridge that failed.
    pub fn failures(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for (id, status) in &self.bacnet {
            if let BridgeStartStatus::Failed(e) = status {
                out.push((format!("BACnet/{id}"), e.clone()));
            }
        }
        if let BridgeStartStatus::Failed(e) = &self.modbus {
            out.push(("Modbus".to_string(), e.clone()));
        }
        out
    }
}

/// Bridge handles wrapped in Arc<Mutex<>> for shared concurrent access (GUI/API).
#[derive(Clone)]
pub struct SharedBridgeHandles {
    pub bacnet: Arc<Mutex<BacnetNetworks>>,
    pub modbus: Arc<Mutex<Option<ModbusBridge>>>,
}

impl BridgeHandles {
    /// Wrap bridge handles in Arc<Mutex<>> for shared concurrent access.
    pub fn into_shared(self) -> SharedBridgeHandles {
        SharedBridgeHandles {
            bacnet: Arc::new(Mutex::new(self.bacnet)),
            modbus: Arc::new(Mutex::new(self.modbus)),
        }
    }
}

/// All platform state in a GUI/API-friendly form (all fields are Clone + 'static).
/// Produced by `Platform::into_shared()`.
#[derive(Clone)]
pub struct SharedPlatform {
    pub point_store: PointStore,
    pub node_store: NodeStore,
    pub event_bus: EventBus,
    pub loaded: LoadedScenario,
    pub health: HealthRegistry,
    pub alarm_store: AlarmStore,
    pub schedule_store: ScheduleStore,
    pub history_store: HistoryStore,
    pub entity_store: EntityStore,
    pub discovery_store: DiscoveryStore,
    pub program_store: ProgramStore,
    pub notification_store: NotificationStore,
    pub mqtt_store: MqttStore,
    pub commissioning_store: CommissioningStore,
    pub report_store: crate::store::report_store::ReportStore,
    pub energy_store: EnergyStore,
    pub webhook_store: WebhookStore,
    pub fdd_store: FddStore,
    pub export_store: ExportStore,
    #[cfg(feature = "cloud")]
    pub cloud_store: CloudStore,
    pub shutdown: tokio_util::sync::CancellationToken,
    pub discovery_service: Arc<DiscoveryService>,
    pub bridge_registry: Arc<BridgeRegistry>,
    #[cfg(feature = "wasm-plugins")]
    pub wasm_runtime: Option<std::sync::Arc<opencrate_plugin_wasm::WasmPluginRuntime>>,
    /// Shared lock on the Atlas matcher — allows the GUI to swap it at runtime.
    /// The same lock is shared with the DiscoveryService for live updates.
    #[cfg(feature = "atlas")]
    pub atlas_lock: Arc<std::sync::RwLock<Option<Arc<crate::atlas::matcher::AtlasMatcher>>>>,
}

impl Platform {
    /// Convert Platform + BridgeHandles into a SharedPlatform with Clone-able handles.
    /// Consumes both Platform and BridgeHandles.
    pub fn into_shared(self, bridges: BridgeHandles) -> SharedPlatform {
        let mut registry = BridgeRegistry::new();
        registry.register("bacnet", Box::new(bridges.bacnet));
        if let Some(modbus) = bridges.modbus {
            registry.register("modbus", Box::new(modbus));
        }
        SharedPlatform {
            point_store: self.model.point_store,
            node_store: self.model.node_store,
            event_bus: self.model.event_bus,
            loaded: self.model.loaded,
            health: self.model.health,
            alarm_store: self.automation.alarm_store,
            schedule_store: self.automation.schedule_store,
            history_store: self.automation.history_store,
            entity_store: self.automation.entity_store,
            discovery_store: self.automation.discovery_store,
            program_store: self.automation.program_store,
            notification_store: self.automation.notification_store,
            mqtt_store: self.automation.mqtt_store,
            commissioning_store: self.automation.commissioning_store,
            report_store: self.automation.report_store,
            energy_store: self.automation.energy_store,
            webhook_store: self.automation.webhook_store,
            fdd_store: self.automation.fdd_store,
            export_store: self.automation.export_store,
            #[cfg(feature = "cloud")]
            cloud_store: self.automation.cloud_store,
            shutdown: self.shutdown,
            discovery_service: Arc::new(self.discovery_service),
            bridge_registry: Arc::new(registry),
            #[cfg(feature = "wasm-plugins")]
            wasm_runtime: self.model.wasm_runtime,
            #[cfg(feature = "atlas")]
            atlas_lock: self.atlas_lock,
        }
    }
}

/// Initialize the platform from project paths.
/// Used by both CLI and GUI.
/// The `shutdown` token is shared with all background tasks — cancel it to stop everything.
///
/// Returns the [`Platform`], the bridge handles, and a [`BridgeStartReport`] describing
/// which protocol bridges started cleanly. Bridge bind failures (e.g. BACnet/IP UDP 47808
/// already in use) are recorded in the report rather than failing the init — the platform
/// is still usable for non-bridge operations and the GUI can surface the failure to the user.
pub async fn init_platform(
    paths: &ProjectPaths,
    shutdown: tokio_util::sync::CancellationToken,
) -> Result<(Platform, BridgeHandles, BridgeStartReport), Box<dyn std::error::Error>> {
    // Ensure data directory exists
    if !paths.data_dir.exists() {
        std::fs::create_dir_all(&paths.data_dir)?;
    }

    let loaded = resolve_scenario(&paths.scenario, &paths.profiles_dir)?;

    let health = HealthRegistry::new();

    // Resolve the site id used by the event journal — taken from the
    // project.json UUID when available (empty string in legacy mode), so
    // multi-site supervisor can distinguish per-site events in a shared
    // journal later (Phase 2) without schema changes.
    let site_id: String = crate::project::load_project_meta(&paths.root)
        .map(|m| m.id)
        .unwrap_or_default();

    // Optional durable event journal — enabled via scenario settings.
    let journal: Option<EventJournal> = loaded
        .config
        .settings
        .as_ref()
        .and_then(|s| s.event_journal.as_ref())
        .filter(|jcfg| jcfg.enabled)
        .map(|jcfg| {
            let j = crate::event::journal::start_event_journal(&paths.db_path("event_journal.db"));
            let j = j.with_site_id(site_id.clone());
            crate::event::journal::start_pruning_task(
                j.clone(),
                jcfg.max_age_secs,
                jcfg.max_events,
                jcfg.prune_interval_secs,
                shutdown.clone(),
            );
            tracing::info!(
                max_age_secs = jcfg.max_age_secs,
                max_events = jcfg.max_events,
                site_id = %site_id,
                "Event journal enabled"
            );
            j
        });

    let event_bus = match journal {
        Some(ref j) => EventBus::with_journal(Arc::new(j.clone())),
        None => EventBus::new(),
    };
    let point_store = PointStore::new().with_event_bus(event_bus.clone());
    let node_store =
        start_node_store_with_path(&paths.db_path("nodes.db")).with_event_bus(event_bus.clone());

    // Initialize point store from loaded profiles
    for dev in &loaded.devices {
        point_store.initialize_from_profile(&dev.instance_id, &dev.profile);
    }

    // Auto-create nodes from scenario (equip + point nodes with auto-tagging)
    auto_create_nodes(&node_store, &loaded).await;

    // Start automation engines (all receive shared shutdown token)
    let history_store = start_history_collector_with_path(
        &point_store,
        &loaded.devices,
        &paths.db_path("history.db"),
        Some(shutdown.clone()),
    );
    let alarm_store = start_alarm_engine_with_path(
        &point_store,
        &paths.db_path("alarms.db"),
        Some(shutdown.clone()),
    )
    .with_event_bus(event_bus.clone());
    let schedule_store = start_schedule_engine_with_path(
        &point_store,
        &paths.db_path("schedules.db"),
        Some(shutdown.clone()),
    )
    .with_event_bus(event_bus.clone());
    let entity_store = start_entity_store_with_path(&paths.db_path("entities.db"))
        .with_event_bus(event_bus.clone());
    let discovery_store = start_discovery_store_with_path(&paths.db_path("discovery.db"))
        .with_event_bus(event_bus.clone());
    start_conn_status_listener(discovery_store.clone(), event_bus.clone(), shutdown.clone());

    // Create program store (logic engine is started by the caller so it can
    // provide a write_callback — the GUI needs one, the CLI does not).
    let program_store = start_program_store_with_path(&paths.db_path("programs.db"));

    // Create notification store for alarm routing
    let notification_store = start_notification_store_with_path(&paths.db_path("notifications.db"));

    // Create MQTT config store
    let mqtt_store = start_mqtt_store_with_path(&paths.db_path("mqtt.db"));
    let commissioning_store =
        start_commissioning_store_with_path(&paths.db_path("commissioning.db"));
    let report_store = start_report_store_with_path(&paths.db_path("reports.db"));
    let energy_store = start_energy_store_with_path(&paths.db_path("energy.db"));
    let webhook_store = start_webhook_store_with_path(&paths.db_path("webhooks.db"));
    let fdd_store = start_fdd_store_with_path(&paths.db_path("fdd.db"));
    let export_store = start_export_store_with_path(&paths.db_path("export.db"));
    #[cfg(feature = "cloud")]
    let cloud_store = start_cloud_store_with_path(&paths.db_path("cloud.db"));

    // Seed built-in FDD rules
    {
        let builtin_rules = crate::fdd::rules::builtin_fdd_rules();
        let _ = fdd_store.seed_builtin_rules(builtin_rules).await;
    }

    // Start protocol bridges — multiple BACnet networks
    let bacnet_configs = bacnet_configs_from_scenario(&loaded.config.settings);
    let mut bacnet_networks = BacnetNetworks::new();

    // Resolve per-network server_device_instance from each network's config.
    // Also look up the resolved network configs for per-network server settings.
    let resolved_networks = loaded
        .config
        .settings
        .as_ref()
        .map(|s| s.resolved_bacnet_networks())
        .unwrap_or_default();

    // Use sorted order for deterministic "first network" selection
    let mut sorted_ids: Vec<String> = bacnet_configs.keys().cloned().collect();
    sorted_ids.sort();

    // Track whether any network has already claimed the server role
    let mut server_assigned = false;

    // Per-bridge start report — populated as we attempt to start each bridge.
    let mut bridge_report = BridgeStartReport::default();

    for network_id in &sorted_ids {
        let config = &bacnet_configs[network_id];
        let net_cfg = resolved_networks.get(network_id);
        let mut bridge = BacnetBridge::new()
            .with_network_id(network_id.clone())
            .with_bacnet_config(config.clone())
            .with_event_bus(event_bus.clone())
            .with_history_backend(Arc::new(history_store.clone()));

        // Apply per-network monitor interval
        if let Some(secs) = net_cfg.and_then(|n| n.monitor_interval_secs) {
            bridge = bridge.with_monitor_interval(Duration::from_secs(secs));
        }
        // Apply per-network object check cycles
        if let Some(cycles) = net_cfg.and_then(|n| n.object_check_cycles) {
            bridge = bridge.with_object_check_cycles(cycles);
        }
        // Apply per-network trend log sync interval
        if let Some(secs) = net_cfg.and_then(|n| n.trend_log_sync_interval_secs) {
            bridge = bridge.with_trend_log_sync_interval(Duration::from_secs(secs));
        }

        // Check per-network server_device_instance from the scenario config
        let net_server_instance = resolved_networks
            .get(network_id)
            .and_then(|n| n.server_device_instance);

        if !server_assigned {
            if let Some(si) = net_server_instance {
                bridge.init_server_store(si, &point_store);
                server_assigned = true;
            }
        }

        let status = match bridge.start(point_store.clone()).await {
            Ok(()) => BridgeStartStatus::Ok,
            Err(e) => {
                tracing::error!(network_id, "BACnet bridge failed to start: {e}");
                BridgeStartStatus::Failed(format!("{e}"))
            }
        };
        bridge_report.bacnet.insert(network_id.clone(), status);
        bacnet_networks.insert(network_id.clone(), bridge);
    }

    let modbus_config = modbus_config_from_scenario(&loaded.config.settings);
    let modbus_base = ModbusBridge::new()
        .with_modbus_config(modbus_config)
        .with_event_bus(event_bus.clone());
    let mut modbus = crate::bridge::modbus::with_loaded_devices(modbus_base, &loaded.devices);
    bridge_report.modbus = match modbus.start(point_store.clone()).await {
        Ok(()) => BridgeStartStatus::Ok,
        Err(e) => {
            tracing::error!("Modbus bridge failed to start: {e}");
            BridgeStartStatus::Failed(format!("{e}"))
        }
    };

    #[allow(unused_mut)]
    let mut plugin_registry = PluginRegistry::new();

    // Load WASM plugins from data/plugins/*/plugin.toml.
    //
    // Each plugin receives a clone of the platform handles so host functions
    // (points.get/set, nodes.*, events.subscribe, history.query) delegate
    // directly to the live stores. Missing handles surface to plugins as
    // safe defaults (None / empty) rather than panics.
    #[cfg(feature = "wasm-plugins")]
    let wasm_runtime: Option<std::sync::Arc<opencrate_plugin_wasm::WasmPluginRuntime>> = {
        let plugin_settings = crate::plugin::load_plugin_settings(&paths.data_dir);
        // Plugin KV store: single SQLite file shared by every plugin, each
        // plugin namespaced by id. `open` creates the file if missing; if
        // open fails we log and fall through with storage = None (plugins
        // get an error back from storage.* calls, no crash).
        let storage: Option<std::sync::Arc<dyn opencrate_plugin_wasm::PluginStorage>> =
            match crate::plugin::storage::SqlitePluginStorage::shared(
                &crate::plugin::storage::SqlitePluginStorage::default_path(&paths.data_dir),
            ) {
                Ok(s) => Some(s),
                Err(e) => {
                    tracing::warn!("Plugin KV storage unavailable: {e}");
                    None
                }
            };

        let handles = opencrate_plugin_wasm::host::HostHandles {
            point_store: Some(point_store.clone()),
            node_store: Some(node_store.clone()),
            event_bus: Some(event_bus.clone()),
            history_backend: Some(std::sync::Arc::new(history_store.clone())),
            // None → runtime allocates a default 256-entry ring buffer.
            log_buffer: None,
            storage,
        };

        match opencrate_plugin_wasm::WasmPluginRuntime::with_handles(handles) {
            Ok(wasm_runtime) => {
                // Wrap early so adapter registrations can share the handle
                // with the main plugin_registry.alarm_evaluators entries.
                let wasm_runtime = std::sync::Arc::new(wasm_runtime);
                let plugins_dir = paths.data_dir.join("plugins");
                if plugins_dir.exists() {
                    if let Ok(entries) = std::fs::read_dir(&plugins_dir) {
                        for entry in entries.flatten() {
                            let plugin_dir = entry.path();
                            if !plugin_dir.join("plugin.toml").exists() {
                                continue;
                            }
                            let plugin_id = entry.file_name().to_string_lossy().to_string();

                            // Validate id (attacker-controlled — the same
                            // check used by install/uninstall). Any rejects
                            // a directory whose name contains path traversal
                            // or shell metacharacters before we pass it to
                            // the loader as a plugin identifier.
                            if let Err(e) = crate::plugin::archive::validate_plugin_id(&plugin_id) {
                                tracing::warn!(
                                    plugin = %plugin_id,
                                    "Refusing to load plugin with invalid id: {e}"
                                );
                                continue;
                            }

                            // Skip plugins the user has disabled (but keep
                            // tracking the id for the GUI list).
                            if plugin_settings.disabled.contains(&plugin_id) {
                                tracing::debug!(
                                    plugin = %plugin_id,
                                    "WASM plugin disabled — skipping"
                                );
                                plugin_registry.wasm_plugin_ids.push(plugin_id);
                                continue;
                            }

                            match opencrate_plugin_wasm::loader::load_plugin_dir(
                                &wasm_runtime,
                                &plugin_id,
                                &plugin_dir,
                            )
                            .await
                            {
                                Ok(Some(instance)) => {
                                    plugin_registry.wasm_plugin_ids.push(plugin_id.clone());
                                    wasm_runtime.register(instance);
                                    // Adapter registration is *query-based*,
                                    // not push-based: engine-side consumers
                                    // call `WasmPluginRuntime::alarm_evaluators()`
                                    // / `history_backends()` / `import_exports()`
                                    // to get a fresh snapshot wrapping every
                                    // plugin that currently exports the matching
                                    // interface. Rebuilding the list every time
                                    // keeps reloads / enable / disable from
                                    // leaving stale adapters behind and lets
                                    // import/export adapters populate their
                                    // `supported_formats` from the live guest.
                                }
                                Ok(None) => {
                                    // No [wasm] section — not a WASM plugin.
                                    // Silently ignored so data-plugin dirs
                                    // coexist with WASM plugin dirs.
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        plugin = %plugin_id,
                                        "Failed to load WASM plugin: {e}"
                                    );
                                }
                            }
                        }
                    }
                }
                tracing::info!(
                    count = plugin_registry.wasm_plugin_ids.len(),
                    "WASM plugin runtime initialized"
                );
                Some(wasm_runtime)
            }
            Err(e) => {
                tracing::warn!("Failed to initialize WASM plugin runtime: {e}");
                None
            }
        }
    };

    #[allow(unused_mut)]
    let mut discovery_service = DiscoveryService::new(
        discovery_store.clone(),
        node_store.clone(),
        entity_store.clone(),
        event_bus.clone(),
        point_store.clone(),
    );

    // Conditionally load the BAS Atlas matcher for richer auto-tagging
    #[cfg(feature = "atlas")]
    {
        let plugin_settings = crate::plugin::load_plugin_settings(&paths.data_dir);
        let atlas_disabled = plugin_settings.disabled.contains(&"atlas".to_string());
        let atlas_path = paths.db_path("bas-atlas.db");
        if atlas_disabled {
            tracing::debug!("Atlas plugin disabled by user");
        } else if crate::atlas::db::AtlasDb::is_available(&atlas_path) {
            match crate::atlas::db::AtlasDb::open(&atlas_path) {
                Ok(db) => match crate::atlas::matcher::AtlasMatcher::load(&db) {
                    Ok(matcher) => {
                        let m = Arc::new(matcher);
                        tracing::info!(
                            points = m.point_count(),
                            equipment = m.equipment_count(),
                            "Atlas taxonomy loaded"
                        );
                        discovery_service.set_atlas(Some(m));
                    }
                    Err(e) => {
                        tracing::warn!("Failed to load Atlas matcher: {e}");
                    }
                },
                Err(e) => {
                    tracing::warn!("Failed to open Atlas database: {e}");
                }
            }
        } else {
            tracing::debug!("Atlas database not found at {}", atlas_path.display());
        }
    }

    // Auto-regroup accepted devices (fixes legacy point-set-fingerprint grouping)
    let _ = discovery_service.regroup_accepted_devices().await;

    // Hydrate PointStore with points from previously-accepted devices
    discovery_service.hydrate_point_store().await;

    // Start alarm notification router
    let project_name = paths
        .root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("OpenCrate")
        .to_string();
    let router = AlarmRouter::new(
        notification_store.clone(),
        alarm_store.clone(),
        schedule_store.clone(),
        project_name.clone(),
    );
    router.start(&event_bus, Some(shutdown.clone()), journal.as_ref());

    // Start MQTT publisher (EventBus → MQTT brokers)
    let mqtt_publisher = MqttPublisher::new(mqtt_store.clone());
    mqtt_publisher.start(&event_bus, Some(shutdown.clone()), journal.as_ref());

    // Start webhook dispatcher (EventBus → HTTP endpoints)
    let webhook_dispatcher = WebhookDispatcher::new(
        webhook_store.clone(),
        alarm_store.clone(),
        node_store.clone(),
        project_name.clone(),
    );
    webhook_dispatcher.start(&event_bus, journal.as_ref());

    // Start database export publisher (EventBus → external databases)
    let export_publisher = ExportPublisher::new(export_store.clone());
    export_publisher.start(&event_bus, Some(shutdown.clone()), journal.as_ref());

    // Start cloud platform publisher (EventBus → AWS IoT / Azure IoT Hub / Google Pub/Sub)
    #[cfg(feature = "cloud")]
    {
        let cloud_publisher = crate::cloud::publisher::CloudPublisher::new(cloud_store.clone());
        cloud_publisher.start(&event_bus, Some(shutdown.clone()), journal.as_ref());
    }

    // Start report scheduler (reads SMTP config from ReportStore at send time)
    {
        let report_engine = std::sync::Arc::new(crate::reporting::engine::ReportEngine::new(
            history_store.clone(),
            alarm_store.clone(),
            point_store.clone(),
            node_store.clone(),
        ));
        let report_scheduler = crate::reporting::scheduler::ReportScheduler::new(
            report_store.clone(),
            report_engine,
            project_name.clone(),
            shutdown.clone(),
        );
        report_scheduler.start();
    }

    // Start energy rollup scheduler
    crate::energy::scheduler::start_energy_rollup_scheduler(
        energy_store.clone(),
        history_store.clone(),
        shutdown.clone(),
    );

    // Start FDD evaluation engine
    crate::fdd::engine::FddEngine::new(
        fdd_store.clone(),
        node_store.clone(),
        point_store.clone(),
        event_bus.clone(),
    )
    .start(shutdown.clone());

    let platform = Platform {
        model: ModelState {
            point_store,
            node_store,
            event_bus,
            plugin_registry,
            loaded,
            health,
            #[cfg(feature = "wasm-plugins")]
            wasm_runtime,
        },
        automation: AutomationState {
            alarm_store,
            schedule_store,
            history_store,
            entity_store,
            discovery_store,
            program_store,
            notification_store,
            mqtt_store,
            commissioning_store,
            report_store,
            energy_store,
            webhook_store,
            fdd_store,
            export_store,
            #[cfg(feature = "cloud")]
            cloud_store,
        },
        shutdown,
        #[cfg(feature = "atlas")]
        atlas_lock: discovery_service.atlas_lock().clone(),
        discovery_service,
    };

    let bridges = BridgeHandles {
        bacnet: bacnet_networks,
        modbus: Some(modbus),
    };

    if !bridge_report.all_ok() {
        for (label, err) in bridge_report.failures() {
            tracing::warn!(bridge = %label, "Bridge failed to start: {err}");
        }
    }

    Ok((platform, bridges, bridge_report))
}

/// Legacy convenience: initialize from raw scenario + profiles paths.
/// Constructs a temporary ProjectPaths treating CWD as project root.
pub async fn init_platform_legacy(
    scenario_path: &Path,
    profiles_dir: &Path,
) -> Result<(Platform, BridgeHandles, BridgeStartReport), Box<dyn std::error::Error>> {
    // Build a ProjectPaths that points to CWD-relative locations
    let cwd = std::env::current_dir()?;
    let paths = ProjectPaths {
        root: cwd.clone(),
        scenario: if scenario_path.is_absolute() {
            scenario_path.to_path_buf()
        } else {
            cwd.join(scenario_path)
        },
        profiles_dir: if profiles_dir.is_absolute() {
            profiles_dir.to_path_buf()
        } else {
            cwd.join(profiles_dir)
        },
        data_dir: cwd.join("data"),
    };
    init_platform(&paths, tokio_util::sync::CancellationToken::new()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_start_report_default_is_all_ok() {
        let report = BridgeStartReport::default();
        assert!(report.all_ok());
        assert!(report.failures().is_empty());
    }

    #[test]
    fn bridge_start_report_records_modbus_failure() {
        let mut report = BridgeStartReport::default();
        report.modbus = BridgeStartStatus::Failed("port in use".into());
        assert!(!report.all_ok());
        let failures = report.failures();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].0, "Modbus");
        assert_eq!(failures[0].1, "port in use");
    }

    #[test]
    fn bridge_start_report_records_bacnet_per_network_failure() {
        let mut report = BridgeStartReport::default();
        report
            .bacnet
            .insert("default".to_string(), BridgeStartStatus::Ok);
        report.bacnet.insert(
            "site-b".to_string(),
            BridgeStartStatus::Failed("47808 already in use".into()),
        );
        assert!(!report.all_ok());
        let failures = report.failures();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].0, "BACnet/site-b");
        assert!(failures[0].1.contains("47808"));
    }

    #[test]
    fn bridge_start_status_helpers() {
        let ok = BridgeStartStatus::Ok;
        assert!(ok.is_ok());
        assert!(ok.error().is_none());

        let failed = BridgeStartStatus::Failed("boom".into());
        assert!(!failed.is_ok());
        assert_eq!(failed.error(), Some("boom"));
    }
}
