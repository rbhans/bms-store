use std::sync::Arc;

use bms_store_bridges::discovery::service::DiscoveryService;
use bms_store_bridges::plugin::{BridgeRegistry, PluginRegistry};
use bms_store_storage::config::loader::LoadedScenario;
use bms_store_storage::event::bus::EventBus;
use bms_store_storage::health::HealthRegistry;
use bms_store_storage::logic::store::ProgramStore;
use bms_store_storage::project::ProjectPaths;
use bms_store_storage::store::audit_store::AuditStore;
use bms_store_storage::store::commissioning_store::CommissioningStore;
use bms_store_storage::store::discovery_store::DiscoveryStore;
use bms_store_storage::store::entity_store::EntityStore;
use bms_store_storage::store::export_store::ExportStore;
use bms_store_storage::store::history_store::HistoryStore;
use bms_store_storage::store::mqtt_store::MqttStore;
use bms_store_storage::store::node_store::NodeStore;
use bms_store_storage::store::override_store::OverrideStore;
use bms_store_storage::store::point_store::PointStore;
use bms_store_storage::store::user_store::UserStore;
use bms_store_storage::store::webhook_store::WebhookStore;
use tokio_util::sync::CancellationToken;

// Re-export bridge boot types so existing callers (app.rs, site_context.rs) keep working.
pub use bms_store_bridges::boot::{BridgeStartReport, BridgeStartStatus};

/// All platform state in a GUI/API-friendly, Clone + 'static form.
/// Produced directly by [`init_platform`].
#[derive(Clone)]
pub struct SharedPlatform {
    pub point_store: PointStore,
    pub node_store: NodeStore,
    pub event_bus: EventBus,
    pub loaded: LoadedScenario,
    pub health: HealthRegistry,
    pub history_store: HistoryStore,
    pub entity_store: EntityStore,
    pub discovery_store: DiscoveryStore,
    pub program_store: ProgramStore,
    pub mqtt_store: MqttStore,
    pub commissioning_store: CommissioningStore,
    pub webhook_store: WebhookStore,
    pub export_store: ExportStore,
    pub override_store: OverrideStore,
    pub user_store: UserStore,
    pub audit_store: AuditStore,
    pub shutdown: CancellationToken,
    pub discovery_service: Arc<DiscoveryService>,
    pub bridge_registry: Arc<BridgeRegistry>,
    pub plugin_registry: Arc<PluginRegistry>,
    /// Shared lock on the Atlas matcher — allows the GUI to swap it at runtime.
    pub atlas_lock: Arc<std::sync::RwLock<Option<Arc<bms_store_storage::atlas::matcher::AtlasMatcher>>>>,
}

/// Initialize the platform from project paths.
///
/// Delegates to:
/// - [`bms_store_storage::boot::boot_project_with_shutdown`] for all store start-up.
/// - [`bms_store_bridges::boot::boot_bridges`] for BACnet, Modbus, discovery, and plugins.
///
/// The `shutdown` token is shared with all background tasks — cancel it to stop everything.
///
/// Returns the [`SharedPlatform`] and a [`BridgeStartReport`] describing which protocol
/// bridges started cleanly.  Bridge bind failures (e.g. BACnet/IP UDP 47808 already in
/// use) are recorded in the report rather than failing the init — the platform is still
/// usable for non-bridge operations and the GUI can surface the failure to the user.
pub async fn init_platform(
    paths: &ProjectPaths,
    shutdown: CancellationToken,
) -> Result<(SharedPlatform, BridgeStartReport), Box<dyn std::error::Error>> {
    // Stage 1: boot the storage layer (all stores, event bus, schedulers, pub/sub).
    tracing::info!(project = %paths.root.display(), "Booting storage layer…");
    let storage = bms_store_storage::boot::boot_project_with_shutdown(
        &paths.root,
        shutdown.clone(),
    )
    .await?;

    // Stage 2: boot the bridge layer (BACnet, Modbus, discovery, plugins).
    tracing::info!("Booting bridge layer…");
    let (bridge_runtime, bridge_report) =
        bms_store_bridges::boot::boot_bridges(&storage).await?;

    if !bridge_report.all_ok() {
        for (label, err) in bridge_report.failures() {
            tracing::warn!(bridge = %label, "Bridge failed to start: {err}");
        }
    }

    // Stage 3: grab the atlas_lock from the discovery service before moving it.
    let atlas_lock = bridge_runtime.discovery_service.atlas_lock().clone();

    // Stage 4: compose SharedPlatform from the two runtimes.
    let platform = SharedPlatform {
        // Storage layer fields.
        point_store: storage.point_store,
        node_store: storage.node_store,
        event_bus: storage.event_bus,
        loaded: storage.loaded,
        health: storage.health,
        history_store: storage.history_store,
        entity_store: storage.entity_store,
        discovery_store: storage.discovery_store,
        program_store: storage.program_store,
        mqtt_store: storage.mqtt_store,
        commissioning_store: storage.commissioning_store,
        webhook_store: storage.webhook_store,
        export_store: storage.export_store,
        override_store: storage.override_store,
        user_store: storage.user_store,
        audit_store: storage.audit_store,
        shutdown: storage.shutdown,

        // Bridge layer fields.
        discovery_service: bridge_runtime.discovery_service,
        bridge_registry: bridge_runtime.bridge_registry,
        plugin_registry: bridge_runtime.plugin_registry,

        // Atlas — derived from discovery service before it was moved.
        atlas_lock,
    };

    Ok((platform, bridge_report))
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
