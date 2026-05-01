use std::collections::HashMap;
use std::sync::Arc;

use dioxus::prelude::*;
use tokio::sync::Mutex;

use bms_store_storage::config::profile::PointValue;

use bms_store_storage::auth::AllRolePermissions;
use bms_store_storage::backup::BackupScheduler;
use bms_store_storage::api_key_store::ApiKeyStore;
use bms_store_storage::logic::engine::ExecutionEngine;
use crate::platform::{init_platform, SharedPlatform};
use bms_store_storage::project::{load_project_meta, ProjectMeta, ProjectPaths};
use bms_store_storage::store::audit_store::start_audit_store_with_path;
use bms_store_storage::store::point_store::{PointKey, PointStatusFlags};
use bms_store_storage::store::user_store::{start_user_store_with_path, User, UserStore};

use super::components::building_tree::LocationBreadcrumb;
use super::components::config_view::ConfigView;
use super::components::login::{AdminSetup, LoginScreen};
use super::components::point_detail::PointDetail;
use super::components::point_table::PointTable;
use super::components::project_launcher::ProjectLauncher;
use super::components::relationships_section::RelationshipsSection;
use super::components::sidebar::Sidebar;
use super::components::toolbar::Toolbar;
use super::state::{
    ActiveView, AppState, CloseAction,
    LaunchSelection, SidebarTab, WriteCommand,
};
use super::theme::{apply_theme_css, load_theme_config, save_theme_config};

/// Top-level app phase. Currently single-project only; the enum shape is
/// kept so a future Multi-site variant can be added without re-architecting.
#[derive(Clone)]
enum AppPhase {
    Launcher,
    Single(ProjectPaths),
}

#[component]
pub fn App() -> Element {
    // If --project <path> was supplied on the command line, main.rs stashes the path
    // in BMS_STORE_GUI_PROJECT before Dioxus starts.  Pick it up once at init time
    // and skip straight to the single-project view.
    let mut phase = use_signal(|| {
        if let Ok(p) = std::env::var("BMS_STORE_GUI_PROJECT") {
            let root = std::path::PathBuf::from(p);
            AppPhase::Single(bms_store_storage::project::ProjectPaths::from_root(root))
        } else {
            AppPhase::Launcher
        }
    });
    let mut initial_tab = use_signal(|| Option::<CloseAction>::None);

    let current_phase = phase.read().clone();

    rsx! {
        document::Link { rel: "stylesheet", href: asset!("/assets/style.css") }
        document::Link { rel: "manifest", href: asset!("/assets/manifest.json") }
        document::Meta { name: "viewport", content: "width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no" }
        document::Meta { name: "theme-color", content: "#232120" }
        document::Meta { name: "apple-mobile-web-app-capable", content: "yes" }
        document::Meta { name: "apple-mobile-web-app-status-bar-style", content: "black-translucent" }
        document::Script { r#"if('serviceWorker' in navigator)navigator.serviceWorker.register('/sw.js')"# }

        match current_phase {
            AppPhase::Single(paths) => rsx! {
                ProjectGate {
                    key: "{paths.root.display()}",
                    paths: paths.clone(),
                    on_close: move |action: CloseAction| {
                        initial_tab.set(Some(action));
                        phase.set(AppPhase::Launcher);
                    },
                }
            },
            AppPhase::Launcher => rsx! {
                ProjectLauncher {
                    on_open: move |selection: LaunchSelection| {
                        match selection {
                            LaunchSelection::Single(paths) => phase.set(AppPhase::Single(paths)),
                        }
                    },
                    initial_action: *initial_tab.read(),
                }
            },
        }
    }
}

/// Gate component: initializes platform, creates UserStore, shows login/setup or ProjectApp.
#[component]
fn ProjectGate(paths: ProjectPaths, on_close: EventHandler<CloseAction>) -> Element {
    let project_paths = use_hook(|| paths.clone());

    // Ensure data directory exists
    use_hook(|| {
        let _ = std::fs::create_dir_all(&project_paths.data_dir);
    });

    let user_store = use_hook(|| start_user_store_with_path(&project_paths.db_path("users.db")));

    let mut current_user = use_signal(|| Option::<User>::None);
    let mut needs_setup = use_signal(|| Option::<bool>::None);
    let role_permissions = use_signal(AllRolePermissions::default);

    // Check if any users exist on mount + load role permissions
    {
        let store = user_store.clone();
        let mut rp = role_permissions;
        let _ = use_resource(move || {
            let store = store.clone();
            async move {
                let has_users = store.has_any_users().await;
                needs_setup.set(Some(!has_users));
                let perms = store.get_all_role_permissions().await;
                rp.set(perms);
            }
        });
    }

    // Initialize platform asynchronously (runs in parallel with login flow)
    let mut platform_data = use_signal(|| Option::<SharedPlatform>::None);
    let mut init_error = use_signal(|| Option::<String>::None);
    {
        let pp = project_paths.clone();
        use_hook(move || {
            spawn(async move {
                let shutdown_token = tokio_util::sync::CancellationToken::new();
                match init_platform(&pp, shutdown_token).await {
                    Ok((platform, report)) => {
                        if !report.all_ok() {
                            for (label, err) in report.failures() {
                                tracing::warn!(bridge = %label, "Bridge failed to start: {err}");
                            }
                        }
                        platform_data.set(Some(platform));
                    }
                    Err(e) => {
                        init_error.set(Some(format!("{e}")));
                    }
                }
            });
        });
    }

    let setup_check = needs_setup.read().clone();
    let logged_in = current_user.read().is_some();
    let has_platform = platform_data.read().is_some();
    let has_error = init_error.read().is_some();


    rsx! {
        document::Link { rel: "stylesheet", href: asset!("/assets/style.css") }

        if has_error {
            div { class: "login-backdrop",
                div { class: "login-card",
                    h3 { "Failed to load project" }
                    p { "{init_error.read().as_deref().unwrap_or_default()}" }
                    button {
                        class: "btn btn-primary",
                        onclick: move |_| on_close.call(CloseAction::ToRecent),
                        "Back to Projects"
                    }
                }
            }
        } else if setup_check.is_none() {
            // Loading user check...
            div { class: "login-backdrop",
                div { class: "login-card",
                    p { "Loading..." }
                }
            }
        } else if setup_check == Some(true) && !logged_in {
            // No users — show admin setup (platform init runs in background)
            AdminSetup {
                user_store: user_store.clone(),
                on_login: move |user: User| {
                    current_user.set(Some(user));
                    needs_setup.set(Some(false));
                },
            }
        } else if !logged_in {
            // Users exist — show login (platform init runs in background)
            LoginScreen {
                user_store: user_store.clone(),
                on_login: move |user: User| {
                    current_user.set(Some(user));
                },
            }
        } else if !has_platform {
            // Logged in but platform still initializing
            div { class: "login-backdrop",
                div { class: "login-card",
                    p { "Initializing project..." }
                }
            }
        } else {
            // Logged in + platform ready — show main app
            ProjectApp {
                paths: paths.clone(),
                on_close: move |action: CloseAction| {
                    on_close.call(action);
                },
                user_store: user_store.clone(),
                current_user: current_user,
                role_permissions: role_permissions,
                platform_data: platform_data,
            }
        }
    }
}

#[component]
pub(crate) fn ProjectApp(
    paths: ProjectPaths,
    on_close: EventHandler<CloseAction>,
    user_store: UserStore,
    current_user: Signal<Option<User>>,
    role_permissions: Signal<AllRolePermissions>,
    platform_data: Signal<Option<SharedPlatform>>,
) -> Element {
    let project_paths = use_hook(|| paths.clone());
    let project_meta = use_hook(|| {
        load_project_meta(&paths.root).unwrap_or_else(|_| ProjectMeta {
            id: "unknown".to_string(),
            name: "Unknown Project".to_string(),
            description: String::new(),
            created_ms: 0,
            version: "0.1.0".to_string(),
        })
    });

    // Extract stores from pre-initialized platform (created by ProjectGate).
    // ProjectGate gates this branch on `has_platform`, but we use `expect` with
    // a clear diagnostic so any race/order bug surfaces with context (the
    // panic hook in main.rs writes a backtrace to ~/.bms-store-gui/last-panic.log).
    let plat = use_hook(|| {
        platform_data.read().clone().expect(
            "ProjectApp mounted with platform_data=None — \
             ProjectGate's has_platform check failed to gate properly. \
             See ~/.bms-store-gui/last-panic.log for backtrace."
        )
    });

    let store = plat.point_store.clone();
    let node_store = plat.node_store.clone();
    let event_bus = plat.event_bus.clone();
    let loaded = plat.loaded.clone();
    let history_store = plat.history_store.clone();
    let entity_store = plat.entity_store.clone();
    let discovery_store = plat.discovery_store.clone();
    let discovery_service = plat.discovery_service.clone();
    let bridge_registry = plat.bridge_registry.clone();
    let program_store = plat.program_store.clone();
    let mqtt_store = plat.mqtt_store.clone();
    let webhook_store = plat.webhook_store.clone();
    let export_store = plat.export_store.clone();
    let naming_rule_store = plat.naming_rule_store.clone();
    let override_store = plat.override_store.clone();

    // Backup scheduler — constructed from project paths; not part of StorageRuntime.
    let backup_scheduler = use_hook({
        let paths = project_paths.clone();
        let project_id = project_meta.id.clone();
        move || {
            let sched = BackupScheduler::new(&project_id, &paths.data_dir);
            sched.start();
            std::sync::Arc::new(std::sync::Mutex::new(sched))
        }
    });

    // API key store — file-backed, stored next to project data.
    let api_key_store = use_hook({
        let paths = project_paths.clone();
        move || {
            std::sync::Arc::new(ApiKeyStore::new(paths.data_dir.join("api_keys.json")))
        }
    });

    // Per-site shutdown token — lifecycle matches the *site*, not this component.
    // Tasks bound to this token are things that should live as long as the site
    // is loaded (bridges, logic engine, alarm status sync).
    // See `view_shutdown` below for view-local tasks.
    let shutdown_token = use_hook(tokio_util::sync::CancellationToken::new);

    // View-local shutdown token — a fresh token per ProjectApp mount.
    let view_shutdown = use_hook(tokio_util::sync::CancellationToken::new);

    // Write channel — GUI-specific (logic engine + write dialog)
    let (write_tx, write_rx) = use_hook(|| {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<WriteCommand>();
        (tx, Arc::new(Mutex::new(Some(rx))))
    });

    // Start logic engine with write callback (routes writes through the GUI write channel)
    use_hook({
        let ps = program_store.clone();
        let pt_store = store.clone();
        let eb = event_bus.clone();
        let wtx = write_tx.clone();
        let token = shutdown_token.clone();
        move || {
            let write_cb: bms_store_storage::logic::engine::WriteCallback = std::sync::Arc::new(
                move |node_id: &str,
                      value: bms_store_storage::config::profile::PointValue,
                      priority: Option<u8>| {
                    if let Some((dev, pt)) = node_id.split_once('/') {
                        let _ = wtx.send(WriteCommand {
                            device_id: dev.to_string(),
                            point_id: pt.to_string(),
                            value,
                            priority,
                        });
                    }
                },
            );
            let engine = ExecutionEngine {
                program_store: ps,
                point_store: pt_store,
                event_bus: eb,
                write_callback: Some(write_cb),
            };
            let handle = engine.start();
            tokio::spawn(async move {
                token.cancelled().await;
                handle.abort();
            });
        }
    });

    let mut store_version = use_signal(|| 0u64);
    let mut node_version = use_signal(|| 0u64);
    let selected_device = use_signal(|| Option::<String>::None);
    let selected_point = use_signal(|| Option::<String>::None);
    let mut write_error = use_signal(|| Option::<String>::None);
    let active_view = use_signal(|| ActiveView::Home);
    let sidebar_tab = use_signal(|| SidebarTab::Devices);
    let detail_open = use_signal(|| false);
    let saved_layout = use_hook(|| super::state::load_layout(&project_paths));
    let nav_tree = use_signal(|| {
        saved_layout
            .as_ref()
            .map(|s| s.nav_tree.clone())
            .unwrap_or_default()
    });
    let next_node_id = use_signal(|| {
        saved_layout
            .as_ref()
            .map(|s| s.next_node_id)
            .unwrap_or(1u32)
    });
    let audit_store = use_hook(|| start_audit_store_with_path(&project_paths.db_path("audit.db")));

    let theme_config = use_signal(|| load_theme_config(&project_paths));
    let pending_config_section = use_signal(|| Option::<String>::None);
    let sidebar_visible = use_signal(|| true);

    // Build a SiteContext bundling all per-site handles.
    let site_ctx = use_hook(|| crate::gui::site_context::SiteContext {
        site_id: project_meta.id.clone(),
        project_meta: project_meta.clone(),
        project_paths: project_paths.clone(),
        platform: plat.clone(),
        bridge_report: crate::platform::BridgeStartReport::default(),
        audit_store: audit_store.clone(),
        user_store: user_store.clone(),
        current_user,
        role_permissions,
        theme_config,
        store_version,
        node_version,
        shutdown: shutdown_token.clone(),
    });

    let app_state = use_hook(|| AppState {
        site: site_ctx.clone(),
        store: store.clone(),
        node_store: node_store.clone(),
        event_bus: event_bus.clone(),
        loaded: loaded.clone(),
        project_meta: project_meta.clone(),
        project_paths: project_paths.clone(),
        active_view,
        sidebar_tab,
        selected_device,
        selected_point,
        detail_open,
        store_version,
        node_version,
        nav_tree,
        write_tx: write_tx.clone(),
        write_error,
        next_node_id,
        history_store: history_store.clone(),
        entity_store: entity_store.clone(),
        discovery_store: discovery_store.clone(),
        discovery_service: discovery_service.clone(),
        bridge_registry: bridge_registry.clone(),
        program_store: program_store.clone(),
        mqtt_store: mqtt_store.clone(),
        webhook_store: webhook_store.clone(),
        export_store: export_store.clone(),
        override_store: override_store.clone(),
        backup_scheduler: backup_scheduler.clone(),
        api_key_store: api_key_store.clone(),
        health: plat.health.clone(),
        current_user,
        user_store: user_store.clone(),
        role_permissions,
        audit_store: audit_store.clone(),
        naming_rule_store: naming_rule_store.clone(),
        theme_config,
        pending_config_section,
        sidebar_visible,
        atlas_lock: plat.atlas_lock.clone(),
    });
    use_context_provider(|| app_state.clone());

    // Apply theme CSS whenever theme_config changes
    {
        let pp = project_paths.clone();
        use_effect(move || {
            let cfg = theme_config.read().clone();
            save_theme_config(&pp, &cfg);
            // Defer eval to after the webview is ready
            spawn(async move {
                apply_theme_css(&cfg);
            });
        });
    }

    // Cancel view-local tasks on unmount (store version watchers, anything
    // tied to view Signals). Also cancel the per-site shutdown token since
    // the component only unmounts when the project is being torn down.
    let drop_view_token = view_shutdown.clone();
    let drop_site_token = shutdown_token.clone();
    use_drop(move || {
        drop_view_token.cancel();
        drop_site_token.cancel();
    });

    // Clear pending (un-acted-on) discovered devices from previous session
    {
        let startup_ds = discovery_store.clone();
        use_hook(move || {
            spawn(async move {
                let cleared = startup_ds.clear_pending().await;
                if cleared > 0 {
                    tracing::info!(
                        cleared,
                        "Cleared pending discovered devices from previous session"
                    );
                }
            });
        });
    }

    // Store version watchers for Dioxus reactivity — tied to the view scope.
    {
        let watcher_store = store.clone();
        let watcher_shutdown1 = view_shutdown.clone();
        let watcher_shutdown2 = view_shutdown.clone();
        use_hook(move || {
            spawn(async move {
                let mut rx = watcher_store.subscribe();
                loop {
                    tokio::select! {
                        _ = watcher_shutdown1.cancelled() => break,
                        result = rx.changed() => {
                            if result.is_err() { break; }
                            store_version.set(*rx.borrow());
                        }
                    }
                }
            });
            let watcher_nodes = node_store.clone();
            spawn(async move {
                let mut rx = watcher_nodes.subscribe();
                loop {
                    tokio::select! {
                        _ = watcher_shutdown2.cancelled() => break,
                        result = rx.changed() => {
                            if result.is_err() { break; }
                            node_version.set(*rx.borrow());
                        }
                    }
                }
            });
        });
    }

    // Gracefully stop bridges when the site's shutdown token is cancelled.
    //
    // In single-site mode this hook registers a task that waits on our own
    // freshly-created token — cancelled by `use_drop` above when the whole
    {
        let registry = bridge_registry.clone();
        let bridge_shutdown = shutdown_token.clone();
        use_hook(move || {
            spawn(async move {
                bridge_shutdown.cancelled().await;
                registry.stop_all().await;
            });
        });
    }

    // Status sync — periodic stale check.
    // Lifetime is the view scope (restarts on remount).
    let sync_store = store.clone();
    let stale_shutdown = view_shutdown.clone();
    use_hook(move || {

        // Stale check remains periodic (every 30 seconds — staleness is time-based)
        let stale_store = sync_store.clone();
        spawn(async move {
            loop {
                tokio::select! {
                    _ = stale_shutdown.cancelled() => break,
                    _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {
                        let keys = stale_store.all_keys();
                        for key in &keys {
                            if let Some(tv) = stale_store.get(key) {
                                let age = tv.timestamp.elapsed();
                                if age > std::time::Duration::from_secs(300) {
                                    stale_store.set_status(key, PointStatusFlags::STALE);
                                } else {
                                    stale_store.clear_status(key, PointStatusFlags::STALE);
                                }
                            }
                        }
                    }
                }
            }
        });
    });

    // Write command handler — routes writes via BridgeRegistry
    let write_store = store.clone();
    let write_rx_slot = write_rx.clone();
    let write_registry = bridge_registry.clone();
    let write_audit = audit_store.clone();
    let write_user = current_user;
    use_hook(move || {
        spawn(async move {
            let mut rx = write_rx_slot.lock().await.take().unwrap();
            while let Some(cmd) = rx.recv().await {
                let mut write_failed: Option<String> = None;
                let resource_id = format!("{}/{}", cmd.device_id, cmd.point_id);

                // Route write through bridge registry (tries all registered bridges)
                match write_registry
                    .route_write(
                        &cmd.device_id,
                        &cmd.point_id,
                        cmd.value.clone(),
                        cmd.priority,
                    )
                    .await
                {
                    Ok(()) => {
                        write_error.set(None);
                    }
                    Err(e) => {
                        eprintln!("Write error: {e}");
                        let msg = format!("Write failed: {e}");
                        write_error.set(Some(msg.clone()));
                        write_failed = Some(msg);
                    }
                }

                // Audit log the write attempt
                {
                    use bms_store_storage::store::audit_store::{AuditAction, AuditEntryBuilder};
                    let user = write_user.read();
                    let (uid, uname) = match user.as_ref() {
                        Some(u) => (u.id.as_str().to_string(), u.username.clone()),
                        None => ("system".into(), "system".into()),
                    };
                    let details = format!("value={:?} priority={:?}", cmd.value, cmd.priority);
                    let builder = if let Some(ref err) = write_failed {
                        AuditEntryBuilder::new(AuditAction::WritePoint, "point")
                            .resource_id(&resource_id)
                            .details(&details)
                            .failure(err)
                    } else {
                        AuditEntryBuilder::new(AuditAction::WritePoint, "point")
                            .resource_id(&resource_id)
                            .details(&details)
                    };
                    let _ = write_audit.log_action(&uid, &uname, builder).await;
                }

                if write_failed.is_some() {
                    continue;
                }

                // Also update local store so UI reflects immediately
                let write_key = PointKey {
                    device_instance_id: cmd.device_id.clone(),
                    point_id: cmd.point_id.clone(),
                };
                write_store.set(write_key.clone(), cmd.value);
                write_store.set_status(&write_key, PointStatusFlags::OVERRIDDEN);
            }
        });
    });

    let current_view = active_view.read().clone();
    let show_detail = *detail_open.read();
    let is_config = matches!(current_view, ActiveView::Config);

    rsx! {
        div { class: "app-shell",
            Toolbar {
                on_close_project: move |action: CloseAction| {
                    on_close.call(action);
                },
            }

            div { class: "app-body",
                if is_config {
                    // Config view has its own 3-pane layout
                    ConfigView {}
                } else {
                    if *app_state.sidebar_visible.read() {
                        Sidebar {}
                    }

                    div { class: "main-content",
                        match &current_view {
                            ActiveView::Home => rsx! { HomeView {} },
                            ActiveView::Page(_) => rsx! { HomeView {} },
                            ActiveView::Device { .. } => rsx! { HomeView {} },
                            ActiveView::Config => rsx! { },
                        }
                    }

                    if show_detail {
                        DetailsPane {}
                    }
                }
            }
        }
    }
}

#[component]
fn HomeView() -> Element {
    let state = use_context::<AppState>();
    let selected = state.selected_device.read().clone();

    let Some(device_id) = selected else {
        return rsx! {
            div { class: "view-placeholder",
                h2 { "Welcome" }
                p { "Select a device from the sidebar to view its points." }
            }
        };
    };

    rsx! {
        div { class: "home-device-view",
            LocationBreadcrumb {}
            PointTable { key: "{device_id}" }
            RelationshipsSection {}
        }
    }
}

#[component]
fn DetailsPane() -> Element {
    let mut state = use_context::<AppState>();
    let selected_device = state.selected_device.read().clone();
    let selected_point = state.selected_point.read().clone();

    rsx! {
        div { class: "details-pane",
            div { class: "details-header",
                span { "Details" }
                button {
                    class: "close-btn",
                    onclick: move |_| state.detail_open.set(false),
                    "x"
                }
            }
            if selected_point.is_some() {
                PointDetail {}
            } else if let Some(dev_id) = selected_device {
                DeviceSummary { key: "{dev_id}", device_id: dev_id.clone() }
            } else {
                div { class: "point-detail-body",
                    p { class: "placeholder", "Select a zone or point to view details." }
                }
            }
        }
    }
}

/// Compact device summary shown in the detail pane when a zone is clicked.
#[component]
fn DeviceSummary(device_id: String) -> Element {
    let state = use_context::<AppState>();
    let _version = state.store_version.read();

    // Fetch device + point node info from NodeStore (display names, units)
    let dev_id_clone = device_id.clone();
    let ns = state.node_store.clone();
    let node_ver = state.node_version.cloned();
    let node_data: Signal<(String, HashMap<String, (String, Option<String>)>)> =
        use_signal(|| (String::new(), HashMap::new()));
    {
        let ns = ns.clone();
        let did = dev_id_clone.clone();
        let mut node_data = node_data.clone();
        let _ = use_resource(move || {
            let ns = ns.clone();
            let did = did.clone();
            let _nv = node_ver;
            async move {
                // Device display name
                let dev_name = match ns.get_node(&did).await {
                    Ok(n) => {
                        if n.dis.is_empty() {
                            did.clone()
                        } else {
                            n.dis.clone()
                        }
                    }
                    Err(_) => did.clone(),
                };
                // Point nodes for this device — get display names and units
                let point_nodes = ns.list_nodes(Some("point"), Some(&did)).await;
                let mut info: HashMap<String, (String, Option<String>)> = HashMap::new();
                for pn in &point_nodes {
                    let point_id = pn
                        .id
                        .strip_prefix(&format!("{}/", did))
                        .unwrap_or(&pn.id)
                        .to_string();
                    let display = if pn.dis.is_empty() {
                        point_id.clone()
                    } else {
                        pn.dis.clone()
                    };
                    let units = pn
                        .properties
                        .get("units")
                        .cloned()
                        .or_else(|| pn.tags.get("unit").cloned().flatten());
                    info.insert(point_id, (display, units));
                }
                node_data.set((dev_name, info));
            }
        });
    }

    let data = node_data.read();
    let dev_name = if data.0.is_empty() {
        &device_id
    } else {
        &data.0
    };
    let point_info = &data.1;

    // Get live points from PointStore (synchronous, reliable)
    let mut points: Vec<(String, String, String)> = state
        .store
        .all_keys()
        .into_iter()
        .filter(|k| k.device_instance_id == device_id)
        .map(|k| {
            let val = state.store.get(&k);
            let (display, units) = point_info
                .get(&k.point_id)
                .map(|(d, u)| (d.clone(), u.clone()))
                .unwrap_or_else(|| (k.point_id.clone(), None));
            let val_str = match &val {
                Some(tv) => {
                    let v = match &tv.value {
                        PointValue::Bool(b) => {
                            if *b {
                                "ON".into()
                            } else {
                                "OFF".into()
                            }
                        }
                        PointValue::Integer(i) => i.to_string(),
                        PointValue::Float(f) => format!("{f:.1}"),
                    };
                    match &units {
                        Some(u) => format!("{v} {u}"),
                        None => v,
                    }
                }
                None => "—".into(),
            };
            (k.point_id.clone(), display, val_str)
        })
        .collect();
    points.sort_by(|a, b| a.1.cmp(&b.1));

    if points.is_empty() {
        return rsx! {
            div { class: "point-detail-body",
                h4 { class: "detail-point-name", "{dev_name}" }
                p { class: "placeholder", "No points found for this device." }
            }
        };
    }

    rsx! {
        div { class: "point-detail-body",
            h4 { class: "detail-point-name", "{dev_name}" }

            table { class: "detail-point-table",
                thead {
                    tr {
                        th { "Point" }
                        th { "Value" }
                    }
                }
                tbody {
                    for (pt_id, pt_name, val_str) in points.iter() {
                        tr {
                            key: "{pt_id}",
                            td { "{pt_name}" }
                            td { "{val_str}" }
                        }
                    }
                }
            }
        }
    }
}
