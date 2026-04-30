use std::path::Path;
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::config::loader::{resolve_scenario, LoadedScenario};
use crate::config::template::auto_create_nodes;
use crate::energy::scheduler::start_energy_rollup_scheduler;
use crate::event::bus::EventBus;
use crate::event::journal::{start_event_journal, start_pruning_task, EventJournal};
use crate::export::publisher::ExportPublisher;
use crate::fdd::engine::FddEngine;
use crate::health::HealthRegistry;
use crate::logic::store::{start_program_store_with_path, ProgramStore};
use crate::mqtt::publisher::MqttPublisher;
use crate::notification::router::AlarmRouter;
use crate::project::{load_project_meta, ProjectPaths};
use crate::reporting::engine::ReportEngine;
use crate::reporting::scheduler::ReportScheduler;
use crate::store::alarm_store::{start_alarm_engine_with_path, AlarmStore};
use crate::store::audit_store::{start_audit_store_with_path, AuditStore};
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
use crate::store::override_store::{start_override_store_with_path, OverrideStore};
use crate::store::point_store::PointStore;
use crate::store::report_store::{start_report_store_with_path, ReportStore};
use crate::store::schedule_store::{start_schedule_engine_with_path, ScheduleStore};
use crate::store::user_store::{start_user_store_with_path, UserStore};
use crate::store::webhook_store::{start_webhook_store_with_path, WebhookStore};
use crate::webhook::dispatcher::WebhookDispatcher;

/// Running storage-side state for one bms-store project.
///
/// Phase 2 intentionally excludes protocol bridge startup. The bridge runtime is
/// added in Phase 3 after the bridge/discovery/haystack modules move.
#[derive(Clone)]
pub struct StorageRuntime {
    pub loaded: LoadedScenario,
    pub health: HealthRegistry,
    pub point_store: PointStore,
    pub node_store: NodeStore,
    pub event_bus: EventBus,
    pub history_store: HistoryStore,
    pub alarm_store: AlarmStore,
    pub schedule_store: ScheduleStore,
    pub entity_store: EntityStore,
    pub discovery_store: DiscoveryStore,
    pub program_store: ProgramStore,
    pub notification_store: NotificationStore,
    pub mqtt_store: MqttStore,
    pub commissioning_store: CommissioningStore,
    pub report_store: ReportStore,
    pub energy_store: EnergyStore,
    pub webhook_store: WebhookStore,
    pub fdd_store: FddStore,
    pub export_store: ExportStore,
    pub audit_store: AuditStore,
    pub override_store: OverrideStore,
    pub user_store: UserStore,
    #[cfg(feature = "cloud")]
    pub cloud_store: CloudStore,
    pub event_journal: Option<EventJournal>,
    pub shutdown: CancellationToken,
}

pub async fn boot_project(
    project_root: impl AsRef<Path>,
) -> Result<StorageRuntime, Box<dyn std::error::Error>> {
    boot_project_with_shutdown(project_root, CancellationToken::new()).await
}

pub async fn boot_project_with_shutdown(
    project_root: impl AsRef<Path>,
    shutdown: CancellationToken,
) -> Result<StorageRuntime, Box<dyn std::error::Error>> {
    let paths = ProjectPaths::from_root(project_root.as_ref().to_path_buf());
    std::fs::create_dir_all(&paths.data_dir)?;

    let loaded = resolve_scenario(&paths.scenario, &paths.profiles_dir)?;
    let project_meta = load_project_meta(&paths.root).ok();
    let site_id = project_meta
        .as_ref()
        .map(|meta| meta.id.clone())
        .unwrap_or_default();
    let project_name = project_meta
        .map(|meta| meta.name)
        .unwrap_or_else(|| loaded.config.scenario.name.clone());

    let event_journal = loaded
        .config
        .settings
        .as_ref()
        .and_then(|settings| settings.event_journal.as_ref())
        .filter(|journal_config| journal_config.enabled)
        .map(|journal_config| {
            let journal = start_event_journal(&paths.db_path("event_journal.db"))
                .with_site_id(site_id.clone());
            start_pruning_task(
                journal.clone(),
                journal_config.max_age_secs,
                journal_config.max_events,
                journal_config.prune_interval_secs,
                shutdown.clone(),
            );
            journal
        });

    let event_bus = match event_journal {
        Some(ref journal) => EventBus::with_journal(Arc::new(journal.clone())),
        None => EventBus::new(),
    };

    let point_store = PointStore::new().with_event_bus(event_bus.clone());
    let node_store =
        start_node_store_with_path(&paths.db_path("nodes.db")).with_event_bus(event_bus.clone());

    for device in &loaded.devices {
        point_store.initialize_from_profile(&device.instance_id, &device.profile);
    }
    auto_create_nodes(&node_store, &loaded).await;

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

    let program_store = start_program_store_with_path(&paths.db_path("programs.db"));
    let notification_store = start_notification_store_with_path(&paths.db_path("notifications.db"));
    let mqtt_store = start_mqtt_store_with_path(&paths.db_path("mqtt.db"));
    let commissioning_store =
        start_commissioning_store_with_path(&paths.db_path("commissioning.db"));
    let report_store = start_report_store_with_path(&paths.db_path("reports.db"));
    let energy_store = start_energy_store_with_path(&paths.db_path("energy.db"));
    let webhook_store = start_webhook_store_with_path(&paths.db_path("webhooks.db"));
    let fdd_store = start_fdd_store_with_path(&paths.db_path("fdd.db"));
    let export_store = start_export_store_with_path(&paths.db_path("export.db"));
    let audit_store = start_audit_store_with_path(&paths.db_path("audit.db"));
    let override_store = start_override_store_with_path(&paths.db_path("overrides.db"));
    let user_store = start_user_store_with_path(&paths.db_path("users.db"));
    #[cfg(feature = "cloud")]
    let cloud_store = start_cloud_store_with_path(&paths.db_path("cloud.db"));

    let _ = fdd_store
        .seed_builtin_rules(crate::fdd::rules::builtin_fdd_rules())
        .await;

    AlarmRouter::new(
        notification_store.clone(),
        alarm_store.clone(),
        schedule_store.clone(),
        project_name.clone(),
    )
    .start(&event_bus, Some(shutdown.clone()), event_journal.as_ref());

    MqttPublisher::new(mqtt_store.clone()).start(
        &event_bus,
        Some(shutdown.clone()),
        event_journal.as_ref(),
    );

    WebhookDispatcher::new(
        webhook_store.clone(),
        node_store.clone(),
        project_name.clone(),
    )
    .start(&event_bus, event_journal.as_ref());

    ExportPublisher::new(export_store.clone()).start(
        &event_bus,
        Some(shutdown.clone()),
        event_journal.as_ref(),
    );

    let report_engine = Arc::new(ReportEngine::new(
        history_store.clone(),
        alarm_store.clone(),
        point_store.clone(),
        node_store.clone(),
    ));
    ReportScheduler::new(
        report_store.clone(),
        report_engine,
        project_name,
        shutdown.clone(),
    )
    .start();

    start_energy_rollup_scheduler(
        energy_store.clone(),
        history_store.clone(),
        shutdown.clone(),
    );

    FddEngine::new(
        fdd_store.clone(),
        node_store.clone(),
        point_store.clone(),
        event_bus.clone(),
    )
    .start(shutdown.clone());

    tracing::info!(
        project = %paths.root.display(),
        devices = loaded.devices.len(),
        points = point_store.point_count(),
        "bms-store storage runtime booted"
    );

    Ok(StorageRuntime {
        loaded,
        health: HealthRegistry::new(),
        point_store,
        node_store,
        event_bus,
        history_store,
        alarm_store,
        schedule_store,
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
        audit_store,
        override_store,
        user_store,
        #[cfg(feature = "cloud")]
        cloud_store,
        event_journal,
        shutdown,
    })
}
