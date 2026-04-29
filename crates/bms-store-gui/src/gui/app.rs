use std::collections::HashMap;
use std::sync::Arc;

use dioxus::prelude::*;
use tokio::sync::Mutex;

use bms_store_storage::config::profile::PointValue;

use bms_store_storage::auth::AllRolePermissions;
use bms_core::event::Event;
use bms_store_storage::logic::engine::ExecutionEngine;
use crate::platform::{init_platform, SharedPlatform};
use bms_store_storage::project::{load_project_meta, ProjectMeta, ProjectPaths};
use bms_store_storage::store::audit_store::start_audit_store_with_path;
use bms_store_storage::store::point_store::{PointKey, PointStatusFlags};
use bms_store_storage::store::user_store::{start_user_store_with_path, User, UserStore};

use super::components::alarm_view::AlarmView;
use super::components::building_tree::LocationBreadcrumb;
use super::components::config_view::ConfigView;
use super::components::floor_plan::FloorPlanCanvas;
use super::components::login::{AdminSetup, LoginScreen};
use super::components::point_detail::PointDetail;
use super::components::point_table::PointTable;
use super::components::project_launcher::ProjectLauncher;
use super::components::relationships_section::RelationshipsSection;
use super::components::schedule_view::ScheduleView;
use super::components::sidebar::Sidebar;
use super::components::site_map_view::SiteMapView;
use super::components::supervisor_gate::SupervisorGate;
use super::components::toolbar::Toolbar;
use super::components::trend_chart::TrendView;
use super::components::weather_view::WeatherView;
use super::state::{
    is_site_page, load_mapbox_config, ActiveView, AppState, CloseAction, DashboardTool,
    LaunchSelection, RemoteSiteConfig, SidebarTab, WriteCommand,
};
use super::theme::{apply_theme_css, load_theme_config, save_theme_config};
use bms_store_storage::weather::config::WeatherConfig;
use bms_store_storage::weather::service::WeatherService;

/// Top-level app phase — launcher, single-project, or multi-site supervisor.
#[derive(Clone)]
enum AppPhase {
    /// Show the project launcher.
    Launcher,
    /// One project selected — legacy single-site flow.
    Single(ProjectPaths),
    /// Multiple projects selected — supervisor flow. May be a mix of local
    /// project paths and remote-site connection profiles.
    Supervisor {
        local_sites: Vec<ProjectPaths>,
        remote_sites: Vec<RemoteSiteConfig>,
    },
}

#[component]
pub fn App() -> Element {
    let mut phase = use_signal(|| AppPhase::Launcher);
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
            AppPhase::Supervisor { local_sites, remote_sites } => {
                let key_str = {
                    let local_part = local_sites
                        .iter()
                        .map(|p| p.root.display().to_string())
                        .collect::<Vec<_>>()
                        .join("|");
                    let remote_part = remote_sites
                        .iter()
                        .map(|r| r.config_id.clone())
                        .collect::<Vec<_>>()
                        .join("|");
                    format!("{local_part}::{remote_part}")
                };
                rsx! {
                    SupervisorGate {
                        key: "{key_str}",
                        local_sites: local_sites.clone(),
                        remote_sites: remote_sites.clone(),
                        on_close: move |action: CloseAction| {
                            initial_tab.set(Some(action));
                            phase.set(AppPhase::Launcher);
                        },
                    }
                }
            }
            AppPhase::Launcher => rsx! {
                ProjectLauncher {
                    on_open: move |selection: LaunchSelection| {
                        match selection {
                            LaunchSelection::Single(paths) => phase.set(AppPhase::Single(paths)),
                            LaunchSelection::Supervisor { local_sites, remote_sites } => {
                                phase.set(AppPhase::Supervisor { local_sites, remote_sites })
                            }
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
                    Ok((platform, bridges, report)) => {
                        if !report.all_ok() {
                            for (label, err) in report.failures() {
                                tracing::warn!(bridge = %label, "Bridge failed to start: {err}");
                            }
                        }
                        platform_data.set(Some(platform.into_shared(bridges)));
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
    /// Optional bundle of pre-built per-site handles (used by SupervisorApp
    /// to avoid double-starting SQLite threads for the same audit/weather DB
    /// and to tie each site's shutdown to the supervisor-wide token).
    /// When `None` (single-site ProjectGate flow), these are built fresh.
    #[props(default)]
    supervisor_overrides: Option<crate::gui::components::supervisor_gate::ProjectAppOverrides>,
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

    // Extract stores from pre-initialized platform (created by ProjectGate)
    let plat = use_hook(|| platform_data.read().clone().unwrap());

    let store = plat.point_store.clone();
    let node_store = plat.node_store.clone();
    let event_bus = plat.event_bus.clone();
    let loaded = plat.loaded.clone();
    let history_store = plat.history_store.clone();
    let alarm_store = plat.alarm_store.clone();
    let schedule_store = plat.schedule_store.clone();
    let entity_store = plat.entity_store.clone();
    let discovery_store = plat.discovery_store.clone();
    let discovery_service = plat.discovery_service.clone();
    let bridge_registry = plat.bridge_registry.clone();
    let program_store = plat.program_store.clone();
    let notification_store = plat.notification_store.clone();
    let mqtt_store = plat.mqtt_store.clone();
    let commissioning_store = plat.commissioning_store.clone();
    let report_store = plat.report_store.clone();
    let energy_store = plat.energy_store.clone();
    let webhook_store = plat.webhook_store.clone();
    let fdd_store = plat.fdd_store.clone();
    let export_store = plat.export_store.clone();

    // Per-site shutdown token — lifecycle matches the *site*, not this component.
    //
    // In supervisor mode this is the supervisor-owned per-site child token. In
    // single-site mode we own it ourselves. Tasks bound to this token are things
    // that should live as long as the site is loaded (bridges, logic engine,
    // alarm status sync) — **not** things tied to a view mount. See
    // `view_shutdown` below for view-local tasks.
    //
    // Crucially, `use_drop` below cancels this token **only** in single-site
    // mode. In supervisor mode the supervisor owns the token and is the only
    // thing allowed to cancel it; otherwise switching views (Dashboard → Alarms
    // → Site) would re-mount this component and the old unmount would kill the
    // live site's bridges.
    let shutdown_token = use_hook(|| {
        supervisor_overrides
            .as_ref()
            .map(|o| o.shutdown.clone())
            .unwrap_or_else(tokio_util::sync::CancellationToken::new)
    });

    // View-local shutdown token — a fresh token per ProjectApp mount.
    // Cancelled on unmount, drives tasks that are view-lifetime: store version
    // watchers, anything that reads view-scoped Signals. This token is owned
    // by this component instance and is safe to cancel on drop regardless of
    // supervisor vs single-site mode.
    let view_shutdown = use_hook(tokio_util::sync::CancellationToken::new);
    let is_supervisor_mode = supervisor_overrides.is_some();

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
        #[cfg(feature = "wasm-plugins")]
        let wasm_rt = plat.wasm_runtime.clone();
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
                #[cfg(feature = "wasm-plugins")]
                wasm_runtime: wasm_rt.clone(),
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
    let pages = use_signal(|| {
        saved_layout
            .as_ref()
            .map(|s| s.pages.clone())
            .unwrap_or_default()
    });
    let site_maps = use_signal(|| {
        saved_layout
            .as_ref()
            .map(|s| s.site_maps.clone())
            .unwrap_or_default()
    });
    let mapbox_config = use_signal(|| load_mapbox_config(&project_paths));
    let dashboards = use_signal(Vec::new);
    let active_dashboard_id = use_signal(|| Option::<String>::None);
    let selected_widget = use_signal(|| Option::<String>::None);
    let dashboard_tool = use_signal(|| DashboardTool::Select);
    let next_widget_id = use_signal(|| 1u32);
    let drag_op = use_signal(|| Option::<crate::gui::state::DragOp>::None);
    let quick_trend_device = use_signal(|| Option::<String>::None);
    let quick_trend_point = use_signal(|| Option::<String>::None);
    let quick_trend_range = use_signal(|| crate::gui::state::TrendRange::Hour1);

    let audit_store = use_hook(|| {
        supervisor_overrides
            .as_ref()
            .map(|o| o.audit_store.clone())
            .unwrap_or_else(|| start_audit_store_with_path(&project_paths.db_path("audit.db")))
    });

    let weather_config = use_hook(|| WeatherConfig::load(&project_paths.data_dir));
    let weather_service = use_hook(|| {
        supervisor_overrides
            .as_ref()
            .map(|o| o.weather_service.clone())
            .unwrap_or_else(|| Arc::new(WeatherService::new(weather_config)))
    });
    let weather_data = use_signal(|| Option::<bms_store_storage::weather::model::WeatherData>::None);
    let theme_config = use_signal(|| load_theme_config(&project_paths));
    let pending_config_section = use_signal(|| Option::<String>::None);

    // Start weather refresh loop
    {
        let svc = weather_service.clone();
        let token = shutdown_token.clone();
        let mut wd = weather_data;
        use_hook(move || {
            let svc2 = svc.clone();
            svc.start_refresh_loop(token);
            // Watch for updates
            spawn(async move {
                let mut rx = svc2.subscribe();
                loop {
                    if rx.changed().await.is_err() {
                        break;
                    }
                    if let Some(data) = svc2.latest().await {
                        wd.set(Some(data));
                    }
                }
            });
        });
    }

    // Build a SiteContext + 1-site SupervisorState that the new state types can
    // observe. The legacy field facade below is still populated from `plat`
    // clones so existing view code (~30 files) keeps working unchanged.
    let site_ctx = use_hook(|| crate::gui::site_context::SiteContext {
        site_id: project_meta.id.clone(),
        project_meta: project_meta.clone(),
        project_paths: project_paths.clone(),
        platform: plat.clone(),
        // ProjectGate's init_platform call already logs failures via tracing.
        // Step 2 will thread the real report through here for the multi-site
        // SupervisorGate / Site Status Dashboard.
        bridge_report: crate::platform::BridgeStartReport::default(),
        audit_store: audit_store.clone(),
        user_store: user_store.clone(),
        current_user,
        role_permissions,
        weather_service: weather_service.clone(),
        weather_data,
        theme_config,
        store_version,
        node_version,
        shutdown: shutdown_token.clone(),
    });
    let supervisor_state = use_hook(|| {
        crate::gui::supervisor_state::SupervisorState::single_site(
            site_ctx.clone(),
            active_view,
            sidebar_tab,
            shutdown_token.clone(),
        )
    });
    use_context_provider(|| supervisor_state.clone());

    let app_state = use_hook(|| AppState {
        site: site_ctx.clone(),
        supervisor: supervisor_state.clone(),
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
        pages,
        site_maps,
        mapbox_config,
        history_store: history_store.clone(),
        dashboards,
        active_dashboard_id,
        selected_widget,
        dashboard_tool,
        next_widget_id,
        drag_op,
        quick_trend_device,
        quick_trend_point,
        quick_trend_range,
        alarm_store: alarm_store.clone(),
        schedule_store: schedule_store.clone(),
        entity_store: entity_store.clone(),
        discovery_store: discovery_store.clone(),
        discovery_service: discovery_service.clone(),
        bridge_registry: bridge_registry.clone(),
        program_store: program_store.clone(),
        notification_store: notification_store.clone(),
        mqtt_store: mqtt_store.clone(),
        commissioning_store: commissioning_store.clone(),
        report_store: report_store.clone(),
        energy_store: energy_store.clone(),
        webhook_store: webhook_store.clone(),
        fdd_store: fdd_store.clone(),
        export_store: export_store.clone(),
        #[cfg(feature = "cloud")]
        cloud_store: plat.cloud_store.clone(),
        health: plat.health.clone(),
        #[cfg(feature = "wasm-plugins")]
        wasm_runtime: plat.wasm_runtime.clone(),
        current_user,
        user_store: user_store.clone(),
        role_permissions,
        audit_store: audit_store.clone(),
        weather_service: weather_service.clone(),
        weather_data,
        theme_config,
        pending_config_section,
        sidebar_visible: use_signal(|| true),
        #[cfg(feature = "atlas")]
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

    // Cancel *view-local* tasks on unmount (store version watchers, anything
    // tied to view Signals). In single-site mode also cancel the per-site
    // shutdown token — that's the legacy behavior, and in single-site mode the
    // component only unmounts when the whole project is being torn down.
    //
    // In supervisor mode we MUST NOT cancel the per-site token here: the
    // component remounts on every view switch (Dashboard/Alarms/Energy → Site)
    // and cancelling it would kill the still-active site's bridges, logic
    // engine, alarm sync, etc. The supervisor owns the per-site token and
    // only cancels it when actually closing the site.
    let drop_view_token = view_shutdown.clone();
    let drop_site_token = shutdown_token.clone();
    use_drop(move || {
        drop_view_token.cancel();
        if !is_supervisor_mode {
            drop_site_token.cancel();
        }
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

    // Store version watchers for Dioxus reactivity — tied to the view scope
    // so they're dropped cleanly on remount (e.g. supervisor view switch).
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
    // project tears down. In supervisor mode the supervisor owns the stop
    // lifecycle directly (it calls `registry.stop_all()` when closing a site)
    // and we must NOT register this hook, because ProjectApp remounts on
    // every view switch and each remount would pile up another waiter.
    if !is_supervisor_mode {
        let registry = bridge_registry.clone();
        let bridge_shutdown = shutdown_token.clone();
        use_hook(move || {
            spawn(async move {
                bridge_shutdown.cancelled().await;
                registry.stop_all().await;
            });
        });
    }

    // Auto-start embedded API server when both desktop and api features are enabled.
    // Uses app_state (which already has clones of all stores) to avoid move conflicts.
    #[cfg(feature = "api")]
    {
        let api_app = app_state.clone();
        let api_paths = project_paths.clone();
        let api_loaded = loaded.clone();
        use_hook(move || {
            spawn(async move {
                use crate::api::{self, ApiState};
                use bms_store_storage::backup::BackupScheduler;
                use crate::gui::components::web_server_settings::load_web_server_config;
                use bms_store_storage::store::override_store::start_override_store_with_path;

                let override_store =
                    start_override_store_with_path(&api_paths.db_path("overrides.db"));
                let jwt_secret =
                    api::load_or_create_jwt_secret(&api_paths.data_dir.join("api_secret.key"));
                let backup = BackupScheduler::new(&api_app.project_meta.id, &api_paths.data_dir);
                backup.start();

                let web_cfg = api_loaded
                    .config
                    .settings
                    .as_ref()
                    .and_then(|s| s.web_server.clone());
                let file_web_cfg = load_web_server_config(&api_paths, web_cfg.as_ref());

                let api_addr: std::net::SocketAddr =
                    format!("{}:{}", file_web_cfg.listen_addr, file_web_cfg.http_port)
                        .parse()
                        .unwrap_or_else(|_| "127.0.0.1:8080".parse().unwrap());

                let tls_config = if file_web_cfg.https_enabled {
                    match (
                        file_web_cfg.cert_file.as_ref(),
                        file_web_cfg.key_file.as_ref(),
                    ) {
                        (Some(cert), Some(key)) => {
                            let https_addr: std::net::SocketAddr =
                                format!("{}:{}", file_web_cfg.listen_addr, file_web_cfg.https_port)
                                    .parse()
                                    .unwrap_or_else(|_| "127.0.0.1:8443".parse().unwrap());
                            Some(api::TlsConfig {
                                addr: https_addr,
                                cert_file: cert.clone(),
                                key_file: key.clone(),
                            })
                        }
                        _ => None,
                    }
                } else {
                    None
                };

                let api_state = ApiState::from_stores(
                    api_app.store.clone(),
                    api_app.node_store.clone(),
                    api_app.alarm_store.clone(),
                    api_app.schedule_store.clone(),
                    api_app.history_store.clone(),
                    api_app.entity_store.clone(),
                    api_app.discovery_store.clone(),
                    api_app.discovery_service.clone(),
                    api_app.program_store.clone(),
                    api_app.event_bus.clone(),
                    api_app.bridge_registry.clone(),
                    api_app.user_store.clone(),
                    api_app.audit_store.clone(),
                    override_store,
                    api_app.report_store.clone(),
                    api_app.energy_store.clone(),
                    api_app.webhook_store.clone(),
                    api_app.fdd_store.clone(),
                    api_app.export_store.clone(),
                    #[cfg(feature = "cloud")]
                    api_app.cloud_store.clone(),
                    api_app.health.clone(),
                    backup,
                    jwt_secret,
                    api_app.project_meta.name.clone(),
                    api_app.project_paths.data_dir.clone(),
                    #[cfg(feature = "wasm-plugins")]
                    api_app.wasm_runtime.clone(),
                );

                tracing::info!(%api_addr, "Starting embedded API server");
                if let Err(e) = api::start_api_server(
                    api_state,
                    api_addr,
                    file_web_cfg.http_enabled,
                    tls_config,
                )
                .await
                {
                    tracing::error!("Embedded API server error: {e}");
                }
            });
        });
    }

    // Status sync — EventBus-driven alarm flag projection + periodic stale
    // check. Lifetime is the view (restarts on remount) rather than the site,
    // so we don't stack up N overlapping status-sync tasks in supervisor mode
    // when the user flips between Dashboard / Alarms / Site.
    let sync_store = store.clone();
    let sync_alarm = alarm_store.clone();
    let sync_bus = event_bus.clone();
    let alarm_shutdown = view_shutdown.clone();
    let stale_shutdown = view_shutdown.clone();
    use_hook(move || {
        // Alarm flag sync via EventBus (immediate, replaces 3-second poll for alarms)
        let alarm_store_clone = sync_store.clone();
        let alarm_alarm_clone = sync_alarm.clone();
        let mut alarm_rx = sync_bus.subscribe();
        spawn(async move {
            // Do an initial full sync on startup
            {
                let keys = alarm_store_clone.all_keys();
                let active = alarm_alarm_clone.get_active_alarms().await;
                let alarmed_points: std::collections::HashSet<(String, String)> = active
                    .iter()
                    .map(|a| (a.device_id.clone(), a.point_id.clone()))
                    .collect();
                for key in &keys {
                    let is_alarmed = alarmed_points
                        .contains(&(key.device_instance_id.clone(), key.point_id.clone()));
                    if is_alarmed {
                        alarm_store_clone.set_status(key, PointStatusFlags::ALARM);
                    } else {
                        alarm_store_clone.clear_status(key, PointStatusFlags::ALARM);
                    }
                }
            }

            // Then react to alarm events
            loop {
                tokio::select! {
                    _ = alarm_shutdown.cancelled() => break,
                    result = alarm_rx.recv() => {
                        match result {
                            Ok(event) => match event.as_ref() {
                                Event::AlarmRaised { node_id, .. } => {
                                    if let Some((dev, pt)) = node_id.split_once('/') {
                                        let key = PointKey {
                                            device_instance_id: dev.to_string(),
                                            point_id: pt.to_string(),
                                        };
                                        alarm_store_clone.set_status(&key, PointStatusFlags::ALARM);
                                    }
                                }
                                Event::AlarmCleared { node_id, .. } => {
                                    if let Some((dev, pt)) = node_id.split_once('/') {
                                        let key = PointKey {
                                            device_instance_id: dev.to_string(),
                                            point_id: pt.to_string(),
                                        };
                                        alarm_store_clone.clear_status(&key, PointStatusFlags::ALARM);
                                    }
                                }
                                _ => {}
                            },
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(_) => break,
                        }
                    }
                }
            }
        });

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
    let is_history = matches!(current_view, ActiveView::History);
    let is_alarms = matches!(current_view, ActiveView::Alarms);
    let is_schedules = matches!(current_view, ActiveView::Schedules);
    let is_config = matches!(current_view, ActiveView::Config);
    let is_weather = matches!(current_view, ActiveView::Weather);

    rsx! {
        div { class: "app-shell",
            Toolbar {
                on_close_project: move |action: CloseAction| {
                    on_close.call(action);
                },
            }

            div { class: "app-body",
                if is_history {
                    // History view has its own 3-pane layout
                    TrendView {}
                } else if is_alarms {
                    // Alarm view has its own 3-pane layout
                    AlarmView {}
                } else if is_schedules {
                    // Schedule view has its own 3-pane layout
                    ScheduleView {}
                } else if is_config {
                    // Config view has its own 3-pane layout
                    ConfigView {}
                } else if is_weather {
                    // Weather view is full-pane
                    WeatherView {}
                } else {
                    if *app_state.sidebar_visible.read() {
                        Sidebar {}
                    }

                    div { class: "main-content",
                        match &current_view {
                            ActiveView::Home => rsx! { HomeView {} },
                            ActiveView::Alarms => rsx! { },
                            ActiveView::Schedules => rsx! { },
                            ActiveView::History => rsx! { },
                            ActiveView::Page(id) => {
                                let is_site = is_site_page(&app_state.nav_tree.read(), id);
                                if is_site {
                                    rsx! { SiteMapView { page_id: id.clone() } }
                                } else {
                                    rsx! { FloorPlanCanvas { page_id: id.clone() } }
                                }
                            },
                            ActiveView::Device { .. } => rsx! { HomeView {} },
                            ActiveView::Config => rsx! { },
                            ActiveView::Weather => rsx! { },
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
                DeviceSummary { key: "{dev_id}", device_id: dev_id }
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
