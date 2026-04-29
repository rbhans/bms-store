//! `SupervisorGate` — entry point for multi-site supervisor mode.
//!
//! Takes N selected project paths, runs pre-flight scenario validation,
//! sequentially initializes each site's platform, and hands off to
//! `SupervisorApp` once all sites have loaded (or failed).
//!
//! Phase 1 scope: no supervisor user auth yet — each site uses its own
//! per-project `users.db`. Auth federation arrives in Step 6.

use std::sync::Arc;

use dioxus::prelude::*;

use bms_store_storage::auth::AllRolePermissions;
use crate::gui::state::{CloseAction, RemoteSiteConfig};
use crate::gui::supervisor_validation::{validate_supervisor_scenarios, SupervisorValidation};
use crate::platform::{init_platform, BridgeStartReport, SharedPlatform};
use bms_store_storage::project::{load_project_meta, opencrate_home, ProjectMeta, ProjectPaths};
use bms_store_storage::store::audit_store::start_audit_store_with_path;
use bms_store_storage::store::supervisor_user_store::{SupervisorRole, SupervisorUser, SupervisorUserStore};
use bms_store_storage::store::user_store::{start_user_store_with_path, User, UserStore};
use crate::supervisor::health_loop::{spawn_health_loop, RemoteSiteStatus, TrackedSite};
use crate::supervisor::remote::client::RemoteSiteClient;
use crate::supervisor::remote::types::{RemoteCredentials, RemoteSiteError};
use bms_store_storage::weather::config::WeatherConfig;
use bms_store_storage::weather::service::WeatherService;

use super::supervisor_app::SupervisorApp;

/// In-memory description of one fully-loaded local site — the output of the
/// `SupervisorGate` init loop. `SupervisorApp` consumes a Vec of these to
/// populate `SupervisorState`.
#[derive(Clone)]
pub struct LoadedSite {
    pub paths: ProjectPaths,
    pub meta: ProjectMeta,
    pub platform: SharedPlatform,
    pub bridge_report: BridgeStartReport,
    pub user_store: UserStore,
    pub audit_store: bms_store_storage::store::audit_store::AuditStore,
    pub weather_service: Arc<WeatherService>,
    /// Per-site shutdown — child of the supervisor token.
    pub shutdown: tokio_util::sync::CancellationToken,
}

impl PartialEq for LoadedSite {
    fn eq(&self, other: &Self) -> bool {
        // Dioxus needs PartialEq for component props. Compare by site id only.
        self.paths.root == other.paths.root
    }
}

/// Phase 2: in-memory description of one remote site reachable over HTTP.
/// The supervisor's health loop drives `status` via the apply callback.
///
/// `status` is a `SyncSignal` (Dioxus signal backed by `SyncStorage`) so the
/// health-loop background tokio task can update it across thread boundaries.
#[derive(Clone)]
pub struct RemoteLoadedSite {
    /// Supervisor-side stable id (UUID, primary key in `remote_site_endpoint`).
    pub config_id: String,
    /// Remote project UUID, populated from the first successful `system_info` call.
    pub site_id: String,
    pub name: String,
    pub base_url: String,
    pub client: Arc<RemoteSiteClient>,
    pub status: SyncSignal<RemoteSiteStatus>,
}

impl PartialEq for RemoteLoadedSite {
    fn eq(&self, other: &Self) -> bool {
        self.config_id == other.config_id
    }
}

/// One slot in the supervisor's site list. Either an in-process local site or
/// an HTTP-backed remote site. Cross-site views, the dashboard, and the site
/// picker pattern-match on this enum.
///
/// `Local` is large (carries SharedPlatform, BridgeStartReport, several store
/// handles); `Remote` is small. We tolerate the size mismatch because the
/// vector typically holds <10 sites — the wasted bytes per slot are
/// negligible compared to the boxing churn.
#[derive(Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum LoadedSiteVariant {
    Local(LoadedSite),
    Remote(RemoteLoadedSite),
}

impl LoadedSiteVariant {
    /// Project UUID for local sites; supervisor-side config id for remote sites
    /// (until the first successful connect populates the real site_id).
    pub fn id(&self) -> &str {
        match self {
            LoadedSiteVariant::Local(s) => &s.meta.id,
            LoadedSiteVariant::Remote(r) => &r.site_id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            LoadedSiteVariant::Local(s) => &s.meta.name,
            LoadedSiteVariant::Remote(r) => &r.name,
        }
    }

    pub fn is_remote(&self) -> bool {
        matches!(self, LoadedSiteVariant::Remote(_))
    }

    /// Stable display key for Dioxus `key` props (must be unique within the
    /// supervisor session). Local sites use the project root path; remote
    /// sites use the supervisor-side config id.
    pub fn display_key(&self) -> String {
        match self {
            LoadedSiteVariant::Local(s) => s.paths.root.display().to_string(),
            LoadedSiteVariant::Remote(r) => format!("remote::{}", r.config_id),
        }
    }
}

/// Opaque bundle wrapping the supervisor-wide shutdown token + loaded sites for
/// passing as a single `Dioxus` prop. `sites` may hold a heterogeneous mix of
/// local and remote variants (Phase 2).
#[derive(Clone)]
pub struct SupervisorHandle {
    pub sites: Vec<LoadedSiteVariant>,
    pub shutdown: tokio_util::sync::CancellationToken,
    /// Authenticated supervisor user driving this session. `None` only in
    /// single-site legacy flow (unused there) — multi-site always populates.
    pub supervisor_user: Option<SupervisorUser>,
    /// Per-site grants for the authenticated supervisor user. Empty vec means
    /// either the user is a SuperAdmin (implicit Admin on every site) or they
    /// have no explicit grants. Keyed by project UUID from `supervisor_site_grants`.
    pub grants: Vec<bms_store_storage::store::supervisor_user_store::SiteGrant>,
}

impl PartialEq for SupervisorHandle {
    fn eq(&self, other: &Self) -> bool {
        // Stable identity = ordered display-key list across all variants.
        self.sites.len() == other.sites.len()
            && self
                .sites
                .iter()
                .zip(other.sites.iter())
                .all(|(a, b)| a.display_key() == b.display_key())
    }
}

/// Synthesize a per-site `User` from the authenticated supervisor user + grants
/// for the given site. This becomes the `current_user` passed down to
/// `ProjectApp`, so `AppState::has_permission` resolves against a real user
/// instead of the broken `None` default from Step 2.
///
/// Mapping rules:
/// - `SuperAdmin` → implicit `UserRole::Admin` on every site.
/// - `Operator` / `Viewer` → look up the grant for this site; default `Viewer`
///   if the user has no grant (which should not happen post Step 6 filtering,
///   but we degrade safely rather than panicking).
pub fn synthesize_site_user(
    sup_user: &SupervisorUser,
    grants: &[bms_store_storage::store::supervisor_user_store::SiteGrant],
    site_id: &str,
) -> bms_store_storage::store::user_store::User {
    use bms_store_storage::store::user_store::{User, UserRole};
    let role = match sup_user.role {
        SupervisorRole::SuperAdmin => UserRole::Admin,
        _ => grants
            .iter()
            .find(|g| g.site_id == site_id)
            .and_then(|g| match g.site_role.as_str() {
                "admin" => Some(UserRole::Admin),
                "operator" => Some(UserRole::Operator),
                "viewer" => Some(UserRole::Viewer),
                _ => None,
            })
            .unwrap_or(UserRole::Viewer),
    };
    User {
        // Synthetic id so audit entries can be linked back to the supervisor user.
        id: format!("supervisor:{}", sup_user.id),
        username: sup_user.username.clone(),
        display_name: if sup_user.display_name.is_empty() {
            sup_user.username.clone()
        } else {
            sup_user.display_name.clone()
        },
        role,
        // Not used for auth — supervisor already authenticated this identity.
        password_hash: String::new(),
        created_ms: sup_user.created_ms,
        last_login_ms: sup_user.last_login_ms,
        disabled: sup_user.disabled,
    }
}

/// Per-site overrides passed into `ProjectApp` by `SupervisorApp` so per-site
/// SQLite threads aren't double-started. All fields have trivial `PartialEq`
/// via their wrapped types.
#[derive(Clone)]
pub struct ProjectAppOverrides {
    pub audit_store: bms_store_storage::store::audit_store::AuditStore,
    pub weather_service: Arc<WeatherService>,
    pub shutdown: tokio_util::sync::CancellationToken,
}

impl PartialEq for ProjectAppOverrides {
    fn eq(&self, _other: &Self) -> bool {
        // Opaque bundle — trivial equality (Dioxus re-renders drive from other props).
        true
    }
}

/// Sequential init progress for the loading UI.
#[derive(Clone, Debug)]
pub struct SiteLoadProgress {
    pub label: String,
    pub status: SiteLoadStatus,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SiteLoadStatus {
    Pending,
    Loading,
    Loaded,
    Failed(String),
}

#[component]
pub fn SupervisorGate(
    local_sites: Vec<ProjectPaths>,
    remote_sites: Vec<RemoteSiteConfig>,
    on_close: EventHandler<CloseAction>,
) -> Element {
    let local_sites_hook = use_hook(|| local_sites.clone());
    let remote_sites_hook = use_hook(|| remote_sites.clone());
    let total_sites = local_sites_hook.len() + remote_sites_hook.len();

    // Supervisor-wide shutdown token. Each site gets a child_token() so closing
    // one site cancels only that site's background tasks.
    let supervisor_shutdown = use_hook(tokio_util::sync::CancellationToken::new);

    // Supervisor user store at ~/.opencrate/supervisor.db.
    let supervisor_user_store = use_hook(|| {
        let db_path = opencrate_home().join("supervisor.db");
        SupervisorUserStore::open(&db_path).ok()
    });

    // Authentication phase state.
    let mut needs_setup = use_signal(|| Option::<bool>::None);
    let mut supervisor_user = use_signal(|| Option::<SupervisorUser>::None);
    let mut grants = use_signal(Vec::<bms_store_storage::store::supervisor_user_store::SiteGrant>::new);
    let mut auth_error = use_signal(|| Option::<String>::None);
    let mut setup_username = use_signal(String::new);
    let mut setup_display = use_signal(String::new);
    let mut setup_password = use_signal(String::new);
    let mut login_username = use_signal(String::new);
    let mut login_password = use_signal(String::new);

    // Check if the supervisor user store has any users on mount.
    {
        let store_opt = supervisor_user_store.clone();
        use_hook(move || {
            spawn(async move {
                match store_opt {
                    Some(s) => needs_setup.set(Some(!s.has_any_users().await)),
                    None => {
                        // Could not open store — treat as needing setup so the
                        // user at least sees an actionable error. Step 8 can
                        // surface a clearer diagnostic.
                        needs_setup.set(Some(true));
                    }
                }
            });
        });
    }

    let mut validation = use_signal(|| Option::<SupervisorValidation>::None);
    let mut progress = use_signal(|| {
        let mut entries: Vec<SiteLoadProgress> = local_sites_hook
            .iter()
            .map(|p| SiteLoadProgress {
                label: p
                    .root
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("site")
                    .to_string(),
                status: SiteLoadStatus::Pending,
            })
            .collect();
        for r in &remote_sites_hook {
            entries.push(SiteLoadProgress {
                label: format!("{} (remote)", r.name),
                status: SiteLoadStatus::Pending,
            });
        }
        entries
    });
    let mut loaded_sites = use_signal(Vec::<LoadedSiteVariant>::new);
    let mut all_done = use_signal(|| false);

    // A signal that flips true once the supervisor user is authenticated —
    // the init hook below blocks on this, so validation + platform init only
    // run after login succeeds.
    let mut start_init = use_signal(|| false);

    // Run validation + sequential init on mount (waits for `start_init`).
    {
        let local_for_load = local_sites_hook.clone();
        let remote_for_load = remote_sites_hook.clone();
        let supervisor_shutdown = supervisor_shutdown.clone();
        let user_store_for_status = supervisor_user_store.clone();
        use_effect(move || {
            if !*start_init.read() {
                return;
            }
            let local_for_load = local_for_load.clone();
            let remote_for_load = remote_for_load.clone();
            let supervisor_shutdown = supervisor_shutdown.clone();
            let user_store_for_status = user_store_for_status.clone();
            spawn(async move {
                // 1. Pre-flight validation — local sites only. Remote sites
                //    have no UDP/serial/HTTP-port collisions on this host.
                let v = validate_supervisor_scenarios(&local_for_load);
                let is_ok = v.is_ok();
                validation.set(Some(v));
                if !is_ok {
                    return;
                }

                // 2. Sequential local init — important so a BACnet/IP bind
                //    race lands at the second site rather than silently
                //    failing one of two parallel binds.
                for (idx, paths) in local_for_load.iter().enumerate() {
                    let mut p = progress.write();
                    if let Some(item) = p.get_mut(idx) {
                        item.status = SiteLoadStatus::Loading;
                    }
                    drop(p);

                    let site_shutdown = supervisor_shutdown.child_token();
                    match init_platform(paths, site_shutdown.clone()).await {
                        Ok((platform, bridges, report)) => {
                            if !report.all_ok() {
                                for (label, err) in report.failures() {
                                    tracing::warn!(bridge = %label, site = %paths.root.display(), "Bridge failed to start: {err}");
                                }
                            }
                            let shared = platform.into_shared(bridges);
                            let meta =
                                load_project_meta(&paths.root).unwrap_or_else(|_| ProjectMeta {
                                    id: paths
                                        .root
                                        .file_name()
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("site")
                                        .to_string(),
                                    name: paths
                                        .root
                                        .file_name()
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("Site")
                                        .to_string(),
                                    description: String::new(),
                                    created_ms: 0,
                                    version: "0.1.0".to_string(),
                                });
                            let user_store = start_user_store_with_path(&paths.db_path("users.db"));
                            let audit_store =
                                start_audit_store_with_path(&paths.db_path("audit.db"));
                            let weather_cfg = WeatherConfig::load(&paths.data_dir);
                            let weather_service = Arc::new(WeatherService::new(weather_cfg));
                            weather_service.start_refresh_loop(site_shutdown.clone());

                            // Per-site bridge stop: when this site's child
                            // token fires, tear down its protocol bridges. In
                            // supervisor mode this is the ONLY registration
                            // of this task — ProjectApp skips it to avoid
                            // stacking up waiters on every view remount.
                            {
                                let bridge_registry = shared.bridge_registry.clone();
                                let stop_token = site_shutdown.clone();
                                tokio::spawn(async move {
                                    stop_token.cancelled().await;
                                    bridge_registry.stop_all().await;
                                });
                            }

                            let site = LoadedSite {
                                paths: paths.clone(),
                                meta,
                                platform: shared,
                                bridge_report: report,
                                user_store,
                                audit_store,
                                weather_service,
                                shutdown: site_shutdown,
                            };
                            loaded_sites.write().push(LoadedSiteVariant::Local(site));
                            let mut p = progress.write();
                            if let Some(item) = p.get_mut(idx) {
                                item.status = SiteLoadStatus::Loaded;
                            }
                        }
                        Err(e) => {
                            let mut p = progress.write();
                            if let Some(item) = p.get_mut(idx) {
                                item.status = SiteLoadStatus::Failed(format!("{e}"));
                            }
                            tracing::error!(site = %paths.root.display(), "Failed to initialize site: {e}");
                        }
                    }
                }

                // 3. Parallel remote init — no host-resource constraints.
                let local_count = local_for_load.len();
                let remote_futs: Vec<_> = remote_for_load
                    .iter()
                    .enumerate()
                    .map(|(rel_idx, cfg)| {
                        let cfg = cfg.clone();
                        let abs_idx = local_count + rel_idx;
                        let user_store = user_store_for_status.clone();
                        async move {
                            let creds = RemoteCredentials {
                                username: cfg.username.clone(),
                                password: cfg.password.clone(),
                            };
                            let client = match RemoteSiteClient::new(&cfg.base_url, creds) {
                                Ok(c) => Arc::new(c),
                                Err(e) => return (abs_idx, Err(e)),
                            };

                            // Reachability check.
                            if let Err(e) = client.health().await {
                                return (abs_idx, Err(e));
                            }
                            // Project metadata fetch.
                            let info = match client.system_info().await {
                                Ok(v) => v,
                                Err(e) => return (abs_idx, Err(e)),
                            };
                            // The remote `system_info` does not include a
                            // project UUID today — fall back to the supervisor
                            // config_id so display key remains stable. A
                            // future remote API addition can populate the
                            // real UUID here.
                            let site_id = cfg.config_id.clone();

                            // Persist last_status / last_connected.
                            if let Some(s) = user_store {
                                let _ = s
                                    .update_remote_site_status(
                                        &cfg.config_id,
                                        Some(&site_id),
                                        "connected",
                                        Some(now_ms()),
                                    )
                                    .await;
                            }

                            let remote = RemoteLoadedSite {
                                config_id: cfg.config_id.clone(),
                                site_id,
                                name: cfg.name.clone(),
                                base_url: cfg.base_url.clone(),
                                client,
                                status: Signal::new_maybe_sync(RemoteSiteStatus::Connected),
                            };
                            tracing::info!(
                                site = %cfg.name,
                                version = %info.version,
                                "Remote site connected"
                            );
                            (abs_idx, Ok(remote))
                        }
                    })
                    .collect();

                // Mark all remotes as Loading first.
                {
                    let mut p = progress.write();
                    for i in local_count..(local_count + remote_for_load.len()) {
                        if let Some(item) = p.get_mut(i) {
                            item.status = SiteLoadStatus::Loading;
                        }
                    }
                }

                let remote_results = futures::future::join_all(remote_futs).await;
                for (abs_idx, result) in remote_results {
                    match result {
                        Ok(remote) => {
                            loaded_sites.write().push(LoadedSiteVariant::Remote(remote));
                            let mut p = progress.write();
                            if let Some(item) = p.get_mut(abs_idx) {
                                item.status = SiteLoadStatus::Loaded;
                            }
                        }
                        Err(e) => {
                            let msg = describe_remote_err(&e);
                            tracing::error!(site_idx = abs_idx, "Remote site init failed: {msg}");
                            let mut p = progress.write();
                            if let Some(item) = p.get_mut(abs_idx) {
                                item.status = SiteLoadStatus::Failed(msg);
                            }
                        }
                    }
                }

                all_done.set(true);
            });
        });
    }

    // Cancel the supervisor-wide shutdown token when this component unmounts.
    {
        let token = supervisor_shutdown.clone();
        use_drop(move || {
            token.cancel();
        });
    }

    // -------- Auth phase --------
    let setup_check = *needs_setup.read();
    let logged_in = supervisor_user.read().is_some();

    if setup_check.is_none() {
        // Still checking whether the store has users.
        return rsx! {
            div { class: "login-backdrop",
                div { class: "login-card",
                    p { "Loading supervisor user store…" }
                }
            }
        };
    }

    if !logged_in {
        if setup_check == Some(true) {
            // First-run: admin setup form.
            let store_clone = supervisor_user_store.clone();
            return rsx! {
                div { class: "login-backdrop",
                    div { class: "login-card",
                        h3 { "Create Supervisor Admin" }
                        p { class: "text-muted",
                            "No supervisor users exist yet. Create a super-admin to continue."
                        }
                        label { "Username" }
                        input {
                            r#type: "text",
                            value: "{setup_username.read()}",
                            oninput: move |e| setup_username.set(e.value()),
                        }
                        label { "Display name" }
                        input {
                            r#type: "text",
                            value: "{setup_display.read()}",
                            oninput: move |e| setup_display.set(e.value()),
                        }
                        label { "Password" }
                        input {
                            r#type: "password",
                            value: "{setup_password.read()}",
                            oninput: move |e| setup_password.set(e.value()),
                        }
                        if let Some(err) = auth_error.read().clone() {
                            div { class: "project-error", "{err}" }
                        }
                        div { class: "login-card-actions",
                            button {
                                class: "btn btn-primary",
                                onclick: move |_| {
                                    let store = store_clone.clone();
                                    let username = setup_username.read().trim().to_string();
                                    let display = setup_display.read().trim().to_string();
                                    let password = setup_password.read().clone();
                                    auth_error.set(None);
                                    spawn(async move {
                                        let Some(store) = store else {
                                            auth_error.set(Some("supervisor.db could not be opened".into()));
                                            return;
                                        };
                                        if username.is_empty() || password.is_empty() {
                                            auth_error.set(Some("Username and password required".into()));
                                            return;
                                        }
                                        match store.create_user(&username, if display.is_empty() { &username } else { &display }, SupervisorRole::SuperAdmin, &password).await {
                                            Ok(user) => {
                                                supervisor_user.set(Some(user));
                                                needs_setup.set(Some(false));
                                                start_init.set(true);
                                            }
                                            Err(e) => auth_error.set(Some(format!("{e}"))),
                                        }
                                    });
                                },
                                "Create Admin"
                            }
                            button {
                                class: "btn",
                                onclick: move |_| on_close.call(CloseAction::ToSupervisor),
                                "Cancel"
                            }
                        }
                    }
                }
            };
        } else {
            // Users exist — show login form.
            let store_clone = supervisor_user_store.clone();
            return rsx! {
                div { class: "login-backdrop",
                    div { class: "login-card",
                        h3 { "Supervisor Login" }
                        label { "Username" }
                        input {
                            r#type: "text",
                            value: "{login_username.read()}",
                            oninput: move |e| login_username.set(e.value()),
                        }
                        label { "Password" }
                        input {
                            r#type: "password",
                            value: "{login_password.read()}",
                            oninput: move |e| login_password.set(e.value()),
                        }
                        if let Some(err) = auth_error.read().clone() {
                            div { class: "project-error", "{err}" }
                        }
                        div { class: "login-card-actions",
                            button {
                                class: "btn btn-primary",
                                onclick: move |_| {
                                    let store = store_clone.clone();
                                    let username = login_username.read().clone();
                                    let password = login_password.read().clone();
                                    auth_error.set(None);
                                    spawn(async move {
                                        let Some(store) = store else {
                                            auth_error.set(Some("supervisor.db could not be opened".into()));
                                            return;
                                        };
                                        match store.authenticate(&username, &password).await {
                                            Ok(user) => {
                                                // Load this user's site grants so the synthesized
                                                // per-site User carries the right role.
                                                let user_grants = store.list_grants(&user.id).await;
                                                grants.set(user_grants);
                                                supervisor_user.set(Some(user));
                                                start_init.set(true);
                                            }
                                            Err(e) => auth_error.set(Some(format!("{e}"))),
                                        }
                                    });
                                },
                                "Log In"
                            }
                            button {
                                class: "btn",
                                onclick: move |_| on_close.call(CloseAction::ToSupervisor),
                                "Cancel"
                            }
                        }
                    }
                }
            };
        }
    }

    // -------- Loading phase --------
    let validation_snapshot = validation.read().clone();
    let progress_snapshot = progress.read().clone();
    let loaded_count = loaded_sites.read().len();
    let total = total_sites;

    if let Some(v) = &validation_snapshot {
        if !v.is_ok() {
            // Fatal validation errors — block launch.
            let errors = v.fatal.clone();
            return rsx! {
                div { class: "login-backdrop",
                    div { class: "login-card",
                        h3 { "Cannot launch supervisor" }
                        p { class: "text-muted",
                            "The selected projects have conflicts that prevent them from running in one process:"
                        }
                        ul { class: "supervisor-error-list",
                            for err in errors.iter() {
                                li {
                                    strong {
                                        "{err.sites.join(\", \")}"
                                    }
                                    ": {err.message}"
                                }
                            }
                        }
                        button {
                            class: "btn btn-primary",
                            onclick: move |_| on_close.call(CloseAction::ToSupervisor),
                            "Back to Launcher"
                        }
                    }
                }
            };
        }
    }

    if !*all_done.read() {
        return rsx! {
            div { class: "login-backdrop",
                div { class: "login-card",
                    h3 { "Loading supervisor…" }
                    p { class: "text-muted",
                        "{loaded_count} / {total} sites loaded"
                    }
                    ul { class: "supervisor-progress-list",
                        for item in progress_snapshot.iter() {
                            li {
                                strong { "{item.label}" }
                                match &item.status {
                                    SiteLoadStatus::Pending => rsx! { span { class: "text-muted", " — waiting" } },
                                    SiteLoadStatus::Loading => rsx! { span { class: "text-muted", " — loading…" } },
                                    SiteLoadStatus::Loaded => rsx! { span { class: "text-success", " — loaded" } },
                                    SiteLoadStatus::Failed(e) => rsx! { span { class: "text-danger", " — failed: {e}" } },
                                }
                            }
                        }
                    }
                }
            }
        };
    }

    // All sites have either loaded or failed. If zero loaded, bail back.
    let sites_snapshot = loaded_sites.read().clone();
    if sites_snapshot.is_empty() {
        return rsx! {
            div { class: "login-backdrop",
                div { class: "login-card",
                    h3 { "No sites loaded" }
                    p { class: "text-muted", "All selected sites failed to initialize." }
                    button {
                        class: "btn btn-primary",
                        onclick: move |_| on_close.call(CloseAction::ToSupervisor),
                        "Back to Launcher"
                    }
                }
            }
        };
    }

    let sup_user_snapshot = supervisor_user.read().clone();
    let grants_snapshot = grants.read().clone();

    // Spawn the remote-site health loop once after init completes. The loop
    // is keyed by the supervisor shutdown token so it dies with the supervisor.
    {
        let sites_for_loop = sites_snapshot.clone();
        let token = supervisor_shutdown.clone();
        let store_for_loop = supervisor_user_store.clone();
        use_hook(move || {
            let tracked: Vec<TrackedSite> = sites_for_loop
                .iter()
                .filter_map(|variant| match variant {
                    LoadedSiteVariant::Remote(remote) => {
                        let mut status_signal = remote.status;
                        Some(TrackedSite {
                            config_id: remote.config_id.clone(),
                            site_id: remote.site_id.clone(),
                            client: remote.client.clone(),
                            apply: Box::new(move |new_status| {
                                status_signal.set(new_status);
                            }),
                        })
                    }
                    LoadedSiteVariant::Local(_) => None,
                })
                .collect();
            spawn_health_loop(tracked, store_for_loop, token);
        });
    }

    rsx! {
        SupervisorApp {
            handle: SupervisorHandle {
                sites: sites_snapshot,
                shutdown: supervisor_shutdown.clone(),
                supervisor_user: sup_user_snapshot,
                grants: grants_snapshot,
            },
            on_close: move |action: CloseAction| on_close.call(action),
        }
    }
}

/// Tiny helper: build a placeholder RolePermissions signal for a site. In Step 6
/// this is replaced by loading real role permissions from the site's user store.
pub fn placeholder_role_permissions() -> AllRolePermissions {
    AllRolePermissions::default()
}

/// Tiny helper: a blank `current_user` signal for an un-logged-in site. Not used
/// yet — single-site auth still runs per-site in Step 2; Step 6 introduces the
/// supervisor auth flow.
pub fn placeholder_current_user() -> Option<User> {
    None
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn describe_remote_err(e: &RemoteSiteError) -> String {
    match e {
        RemoteSiteError::Unreachable(s) => format!("unreachable: {s}"),
        RemoteSiteError::AuthFailed => "auth failed (check username/password)".into(),
        RemoteSiteError::BadStatus(c) => format!("HTTP {c}"),
        RemoteSiteError::Decode(s) => format!("decode error: {s}"),
        RemoteSiteError::Timeout => "timeout".into(),
        RemoteSiteError::Setup(s) => format!("client setup: {s}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bms_store_storage::store::supervisor_user_store::SiteGrant;
    use bms_store_storage::store::user_store::UserRole;

    fn mk_sup_user(role: SupervisorRole) -> SupervisorUser {
        SupervisorUser {
            id: "sup-1".into(),
            username: "alice".into(),
            display_name: "Alice".into(),
            role,
            password_hash: String::new(),
            created_ms: 0,
            last_login_ms: None,
            disabled: false,
        }
    }

    #[test]
    fn synthesize_user_super_admin_is_admin_on_every_site() {
        let user = mk_sup_user(SupervisorRole::SuperAdmin);
        let u = synthesize_site_user(&user, &[], "any-site");
        assert_eq!(u.role, UserRole::Admin);
        assert_eq!(u.username, "alice");
        assert!(u.id.starts_with("supervisor:"));
    }

    #[test]
    fn synthesize_user_operator_uses_grant() {
        let user = mk_sup_user(SupervisorRole::Operator);
        let grants = vec![
            SiteGrant {
                user_id: "sup-1".into(),
                site_id: "site-a".into(),
                site_role: "admin".into(),
            },
            SiteGrant {
                user_id: "sup-1".into(),
                site_id: "site-b".into(),
                site_role: "viewer".into(),
            },
        ];
        let ua = synthesize_site_user(&user, &grants, "site-a");
        assert_eq!(ua.role, UserRole::Admin);
        let ub = synthesize_site_user(&user, &grants, "site-b");
        assert_eq!(ub.role, UserRole::Viewer);
    }

    #[test]
    fn synthesize_user_missing_grant_defaults_to_viewer() {
        let user = mk_sup_user(SupervisorRole::Operator);
        let u = synthesize_site_user(&user, &[], "orphan-site");
        assert_eq!(u.role, UserRole::Viewer);
    }

    #[test]
    fn synthesize_user_falls_back_display_to_username() {
        let mut user = mk_sup_user(SupervisorRole::SuperAdmin);
        user.display_name = String::new();
        let u = synthesize_site_user(&user, &[], "site");
        assert_eq!(u.display_name, "alice");
    }
}
