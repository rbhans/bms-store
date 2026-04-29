use std::collections::HashSet;

use dioxus::prelude::*;

use bms_store_storage::auth::can_admin;
use crate::gui::state::{CloseAction, LaunchSelection, RemoteSiteConfig};
use bms_store_storage::project::{
    create_project, delete_project, export_project, import_project, load_registry,
    migrate_legacy_if_needed, opencrate_home, validate_project_path, ProjectPaths,
};
// TODO(bms-store-gui): supervisor_user_store, RemoteSiteRow, SupervisorUserStore removed from
// bms_store_storage in the bms-store extraction. The "Supervisor" tab and remote-site
// management functionality is stubbed until Tasks 11/12 implement the replacement.
use bms_store_storage::store::user_store::start_user_store_with_path;
// TODO(bms-store-gui): crate::supervisor::crypto removed in extraction — crypto helpers
// (decrypt_string, encrypt_string, load_or_create_machine_key) are stubbed.

// Stub types replacing supervisor_user_store imports
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
struct RemoteSiteRow {
    pub config_id: String,
    pub site_id: Option<String>,
    pub name: String,
    pub base_url: String,
    pub auth_token_encrypted: Vec<u8>,
    pub last_connected_ms: Option<i64>,
    pub last_status: Option<String>,
    pub created_ms: i64,
}

#[derive(Clone)]
#[allow(dead_code)]
struct SupervisorUserStore;

#[allow(dead_code)]
impl SupervisorUserStore {
    fn open(_path: &std::path::Path) -> Result<Self, String> {
        Err("SupervisorUserStore not available in this build".into())
    }
    async fn list_remote_sites(&self) -> Vec<RemoteSiteRow> { Vec::new() }
    async fn delete_remote_site(&self, _id: &str) -> Result<(), String> { Ok(()) }
    async fn upsert_remote_site(&self, _row: RemoteSiteRow) -> Result<(), String> { Ok(()) }
}

// Stub crypto helpers
#[allow(dead_code)]
fn decrypt_string(_key: &[u8], _ciphertext: &[u8]) -> Result<String, String> {
    Err("crypto not available".into())
}
#[allow(dead_code)]
fn default_machine_key_path() -> std::path::PathBuf { std::path::PathBuf::new() }
#[allow(dead_code)]
fn encrypt_string(_key: &[u8], _plaintext: &str) -> Result<Vec<u8>, String> {
    Err("crypto not available".into())
}
#[allow(dead_code)]
fn load_or_create_machine_key(_path: &std::path::Path) -> Result<Vec<u8>, String> {
    Err("machine key not available".into())
}

use super::remote_site_form::{RemoteSiteForm, RemoteSiteFormData};

#[derive(Debug, Clone, PartialEq)]
enum LauncherTab {
    Recent,
    NewProject,
    Supervisor,
}

#[component]
pub fn ProjectLauncher(
    on_open: EventHandler<LaunchSelection>,
    initial_action: Option<CloseAction>,
) -> Element {
    let initial_tab = match initial_action {
        Some(CloseAction::ToNewProject) => LauncherTab::NewProject,
        Some(CloseAction::ToSupervisor) => LauncherTab::Supervisor,
        _ => LauncherTab::Recent,
    };

    let mut projects = use_signal(Vec::new);
    let mut selected_id = use_signal(|| Option::<String>::None);
    let mut supervisor_selection = use_signal(HashSet::<String>::new);
    let mut remote_selection = use_signal(HashSet::<String>::new);
    let mut remote_sites = use_signal(Vec::<RemoteSiteRow>::new);
    let mut remote_form_open = use_signal(|| Option::<RemoteSiteFormData>::None);
    let mut tab = use_signal(move || initial_tab);
    let mut new_name = use_signal(String::new);
    let mut new_desc = use_signal(String::new);
    let mut error_msg = use_signal(|| Option::<String>::None);

    // Supervisor user store handle (Phase 2: also holds remote_site_endpoint).
    let supervisor_user_store = use_hook(|| {
        let db_path = opencrate_home().join("supervisor.db");
        SupervisorUserStore::open(&db_path).ok()
    });

    // Load projects once on mount
    use_hook(|| {
        // Try legacy migration on first load
        if let Some(_migrated) = migrate_legacy_if_needed() {
            // Registry now has the migrated project
        }
        match load_registry() {
            Ok(mut reg) => {
                reg.projects
                    .sort_by(|a, b| b.last_opened_ms.cmp(&a.last_opened_ms));
                projects.set(reg.projects);
            }
            Err(e) => {
                error_msg.set(Some(format!("Registry error: {e}")));
            }
        }
    });

    // Load saved remote sites once on mount.
    {
        let store = supervisor_user_store.clone();
        use_hook(move || {
            if let Some(s) = store {
                spawn(async move {
                    let rows = s.list_remote_sites().await;
                    remote_sites.set(rows);
                });
            }
        });
    }
    let mut confirm_delete = use_signal(|| Option::<String>::None);
    let mut delete_username = use_signal(String::new);
    let mut delete_password = use_signal(String::new);
    let mut delete_error = use_signal(|| Option::<String>::None);
    let mut delete_busy = use_signal(|| false);

    let selected = selected_id.read().clone();
    let current_tab = tab.read().clone();

    rsx! {
        div { class: "project-launcher-backdrop",
            div { class: "project-launcher",
                div { class: "project-launcher-header",
                    img {
                        src: asset!("/assets/opencrate_icon.svg"),
                        width: "32",
                        height: "32",
                    }
                    h1 { "OpenCrate BMS" }
                }

                div { class: "project-launcher-body",
                    // Left: project list
                    div { class: "project-list-pane",
                        div { class: "project-list-tabs",
                            button {
                                class: if current_tab == LauncherTab::Recent { "tab-btn active" } else { "tab-btn" },
                                onclick: move |_| tab.set(LauncherTab::Recent),
                                "Recent Projects"
                            }
                            button {
                                class: if current_tab == LauncherTab::NewProject { "tab-btn active" } else { "tab-btn" },
                                onclick: move |_| tab.set(LauncherTab::NewProject),
                                "New Project"
                            }
                            button {
                                class: if current_tab == LauncherTab::Supervisor { "tab-btn active" } else { "tab-btn" },
                                onclick: move |_| tab.set(LauncherTab::Supervisor),
                                "Supervisor"
                            }
                        }

                        if current_tab == LauncherTab::Recent {
                            div { class: "project-list",
                                if projects.read().is_empty() {
                                    div { class: "project-list-empty",
                                        p { "No projects yet." }
                                        p { class: "text-muted", "Create a new project to get started." }
                                    }
                                } else {
                                    for proj in projects.read().iter() {
                                        {
                                            let proj_id = proj.id.clone();
                                            let is_selected = selected.as_deref() == Some(&proj.id);
                                            let last_opened = format_timestamp(proj.last_opened_ms);
                                            rsx! {
                                                div {
                                                    class: if is_selected { "project-list-item selected" } else { "project-list-item" },
                                                    onclick: move |_| selected_id.set(Some(proj_id.clone())),
                                                    ondoubleclick: {
                                                        let proj_id = proj.id.clone();
                                                        let proj_path = proj.path.clone();
                                                        move |_| {
                                                            let paths = ProjectPaths::from_root(proj_path.clone());
                                                            if let Err(e) = validate_project_path(&paths) {
                                                                error_msg.set(Some(e));
                                                                return;
                                                            }
                                                            bms_store_storage::project::touch_project(&proj_id);
                                                            on_open.call(LaunchSelection::Single(paths));
                                                        }
                                                    },
                                                    div { class: "project-item-name", "{proj.name}" }
                                                    div { class: "project-item-desc", "{last_opened}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        } else if current_tab == LauncherTab::NewProject {
                            // New project form
                            div { class: "new-project-form",
                                label { "Project Name" }
                                input {
                                    r#type: "text",
                                    placeholder: "My Building",
                                    value: "{new_name.read()}",
                                    oninput: move |e| new_name.set(e.value()),
                                }

                                label { "Description" }
                                input {
                                    r#type: "text",
                                    placeholder: "Optional description",
                                    value: "{new_desc.read()}",
                                    oninput: move |e| new_desc.set(e.value()),
                                }

                                button {
                                    class: "btn btn-primary",
                                    disabled: new_name.read().trim().is_empty(),
                                    onclick: move |_| {
                                        let name = new_name.read().trim().to_string();
                                        let desc = new_desc.read().trim().to_string();
                                        match create_project(&name, &desc, None, None) {
                                            Ok(proj_ref) => {
                                                let paths = ProjectPaths::from_root(proj_ref.path.clone());
                                                // Refresh list
                                                let mut reg = load_registry().unwrap_or_default();
                                                reg.projects.sort_by(|a, b| b.last_opened_ms.cmp(&a.last_opened_ms));
                                                projects.set(reg.projects);
                                                error_msg.set(None);
                                                on_open.call(LaunchSelection::Single(paths));
                                            }
                                            Err(e) => {
                                                error_msg.set(Some(format!("Failed to create project: {e}")));
                                            }
                                        }
                                    },
                                    "Create Project"
                                }
                            }
                        } else {
                            // Supervisor: pick local projects and/or remote sites to load.
                            div { class: "project-list",
                                div { class: "supervisor-intro",
                                    p { class: "text-muted",
                                        "Select projects to open in supervisor mode. "
                                        "The supervisor aggregates alarms, energy, and site status across sites. "
                                        "You can mix local projects with remote sites that run on other machines."
                                    }
                                    p { class: "text-muted",
                                        "Note: only one local site per host can run BACnet/IP (UDP 47808). "
                                        "Remote sites run in independent processes and have no such limit."
                                    }
                                }

                                // ---- Remote sites section ----
                                div { class: "supervisor-section",
                                    div { class: "supervisor-section-header",
                                        h4 { "Remote sites" }
                                        button {
                                            class: "btn btn-sm",
                                            onclick: move |_| {
                                                remote_form_open.set(Some(RemoteSiteFormData::default()));
                                            },
                                            "+ Add Remote Site"
                                        }
                                    }
                                    if remote_sites.read().is_empty() {
                                        p { class: "text-muted text-xs",
                                            "No remote sites configured. Click \"Add Remote Site\" to add one."
                                        }
                                    } else {
                                        for row in remote_sites.read().iter() {
                                            {
                                                let cfg_id = row.config_id.clone();
                                                let cfg_id_for_toggle = row.config_id.clone();
                                                let cfg_id_for_delete = row.config_id.clone();
                                                let is_checked = remote_selection.read().contains(&row.config_id);
                                                let display_url = row.base_url.clone();
                                                let row_clone = row.clone();
                                                let store_for_delete = supervisor_user_store.clone();
                                                rsx! {
                                                    div {
                                                        class: "project-list-item",
                                                        onclick: move |_| {
                                                            let mut sel = remote_selection.write();
                                                            if sel.contains(&cfg_id_for_toggle) {
                                                                sel.remove(&cfg_id_for_toggle);
                                                            } else {
                                                                sel.insert(cfg_id_for_toggle.clone());
                                                            }
                                                        },
                                                        input {
                                                            r#type: "checkbox",
                                                            checked: is_checked,
                                                            onclick: move |evt: MouseEvent| evt.stop_propagation(),
                                                            onchange: move |_| {
                                                                let mut sel = remote_selection.write();
                                                                if sel.contains(&cfg_id) {
                                                                    sel.remove(&cfg_id);
                                                                } else {
                                                                    sel.insert(cfg_id.clone());
                                                                }
                                                            },
                                                        }
                                                        div { class: "project-item-name",
                                                            span { class: "remote-glyph", "🌐 " }
                                                            "{row_clone.name}"
                                                        }
                                                        div { class: "project-item-desc", "{display_url}" }
                                                        button {
                                                            class: "btn btn-sm",
                                                            onclick: move |evt: MouseEvent| {
                                                                evt.stop_propagation();
                                                                let store = store_for_delete.clone();
                                                                let id = cfg_id_for_delete.clone();
                                                                spawn(async move {
                                                                    if let Some(s) = store {
                                                                        let _ = s.delete_remote_site(&id).await;
                                                                        let rows = s.list_remote_sites().await;
                                                                        remote_sites.set(rows);
                                                                    }
                                                                });
                                                            },
                                                            "✕"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // ---- Local sites section ----
                                div { class: "supervisor-section",
                                    h4 { "Local projects" }
                                    if projects.read().is_empty() {
                                        div { class: "project-list-empty",
                                            p { "No local projects yet." }
                                            p { class: "text-muted", "Create projects first, then return here." }
                                        }
                                    } else {
                                        for proj in projects.read().iter() {
                                            {
                                                let proj_id = proj.id.clone();
                                                let proj_id_for_toggle = proj.id.clone();
                                                let is_checked = supervisor_selection.read().contains(&proj.id);
                                                let last_opened = format_timestamp(proj.last_opened_ms);
                                                rsx! {
                                                    div {
                                                        class: "project-list-item",
                                                        onclick: move |_| {
                                                            let mut sel = supervisor_selection.write();
                                                            if sel.contains(&proj_id_for_toggle) {
                                                                sel.remove(&proj_id_for_toggle);
                                                            } else {
                                                                sel.insert(proj_id_for_toggle.clone());
                                                            }
                                                        },
                                                        input {
                                                            r#type: "checkbox",
                                                            checked: is_checked,
                                                            onclick: move |evt: MouseEvent| evt.stop_propagation(),
                                                            onchange: move |_| {
                                                                let mut sel = supervisor_selection.write();
                                                                if sel.contains(&proj_id) {
                                                                    sel.remove(&proj_id);
                                                                } else {
                                                                    sel.insert(proj_id.clone());
                                                                }
                                                            },
                                                        }
                                                        div { class: "project-item-name", "{proj.name}" }
                                                        div { class: "project-item-desc", "{last_opened}" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // ---- Launch button ----
                                {
                                    let local_count = supervisor_selection.read().len();
                                    let remote_count = remote_selection.read().len();
                                    let total_selected = local_count + remote_count;
                                    let store_for_launch = supervisor_user_store.clone();
                                    rsx! {
                                        div { class: "supervisor-launch",
                                            button {
                                                class: "btn btn-primary",
                                                disabled: total_selected < 1,
                                                onclick: move |_| {
                                                    let selected_local: Vec<String> = supervisor_selection
                                                        .read()
                                                        .iter()
                                                        .cloned()
                                                        .collect();
                                                    let selected_remote: Vec<String> = remote_selection
                                                        .read()
                                                        .iter()
                                                        .cloned()
                                                        .collect();

                                                    // Resolve local project paths.
                                                    let mut local_resolved: Vec<ProjectPaths> = Vec::new();
                                                    for id in &selected_local {
                                                        if let Some(p) = projects.read().iter().find(|p| p.id == *id) {
                                                            let paths = ProjectPaths::from_root(p.path.clone());
                                                            if let Err(e) = validate_project_path(&paths) {
                                                                error_msg.set(Some(format!("{}: {}", p.name, e)));
                                                                return;
                                                            }
                                                            local_resolved.push(paths);
                                                            bms_store_storage::project::touch_project(id);
                                                        }
                                                    }

                                                    // Resolve and decrypt remote sites.
                                                    let store = store_for_launch.clone();
                                                    let saved_rows = remote_sites.read().clone();
                                                    spawn(async move {
                                                        let key_path = default_machine_key_path();
                                                        let key = match load_or_create_machine_key(&key_path) {
                                                            Ok(k) => k,
                                                            Err(e) => {
                                                                error_msg.set(Some(format!("machine key: {e}")));
                                                                return;
                                                            }
                                                        };
                                                        let mut remote_resolved: Vec<RemoteSiteConfig> = Vec::new();
                                                        for cfg_id in &selected_remote {
                                                            let Some(row) = saved_rows.iter().find(|r| r.config_id == *cfg_id) else {
                                                                continue;
                                                            };
                                                            let plaintext = match decrypt_string(&key, &row.auth_token_encrypted) {
                                                                Ok(p) => p,
                                                                Err(e) => {
                                                                    error_msg.set(Some(format!(
                                                                        "{}: failed to decrypt credentials ({e})",
                                                                        row.name
                                                                    )));
                                                                    return;
                                                                }
                                                            };
                                                            let parsed: serde_json::Value = match serde_json::from_str(&plaintext) {
                                                                Ok(v) => v,
                                                                Err(e) => {
                                                                    error_msg.set(Some(format!(
                                                                        "{}: malformed credentials ({e})",
                                                                        row.name
                                                                    )));
                                                                    return;
                                                                }
                                                            };
                                                            let username = parsed
                                                                .get("username")
                                                                .and_then(|v| v.as_str())
                                                                .unwrap_or("")
                                                                .to_string();
                                                            let password = parsed
                                                                .get("password")
                                                                .and_then(|v| v.as_str())
                                                                .unwrap_or("")
                                                                .to_string();
                                                            remote_resolved.push(RemoteSiteConfig {
                                                                config_id: row.config_id.clone(),
                                                                name: row.name.clone(),
                                                                base_url: row.base_url.clone(),
                                                                username,
                                                                password,
                                                            });
                                                        }
                                                        let _ = store; // already used above for decrypt; placate unused
                                                        if local_resolved.is_empty() && remote_resolved.is_empty() {
                                                            error_msg.set(Some("Select at least one site.".into()));
                                                            return;
                                                        }
                                                        error_msg.set(None);
                                                        on_open.call(LaunchSelection::Supervisor {
                                                            local_sites: local_resolved,
                                                            remote_sites: remote_resolved,
                                                        });
                                                    });
                                                },
                                                {
                                                    if total_selected < 1 {
                                                        "Select at least one site to launch".to_string()
                                                    } else {
                                                        format!("Launch Supervisor ({total_selected} sites)")
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Right: actions
                    div { class: "project-actions-pane",
                        if let Some(ref sel_id) = selected {
                            {
                                let open_id = sel_id.clone();
                                let export_id = sel_id.clone();
                                let delete_id = sel_id.clone();
                                let sel_proj = projects.read().iter().find(|p| p.id == *sel_id).cloned();
                                rsx! {
                                    if let Some(proj) = sel_proj {
                                        div { class: "project-detail-card",
                                            h3 { "{proj.name}" }
                                            p { class: "text-muted", "{format_timestamp(proj.last_opened_ms)}" }

                                            div { class: "project-actions",
                                                button {
                                                    class: "btn btn-primary",
                                                    onclick: {
                                                        let path = proj.path.clone();
                                                        move |_| {
                                                            let paths = ProjectPaths::from_root(path.clone());
                                                            if let Err(e) = validate_project_path(&paths) {
                                                                error_msg.set(Some(e));
                                                                return;
                                                            }
                                                            bms_store_storage::project::touch_project(&open_id);
                                                            on_open.call(LaunchSelection::Single(paths));
                                                        }
                                                    },
                                                    "Open"
                                                }

                                                button {
                                                    class: "btn",
                                                    onclick: move |_| {
                                                        // Export to Desktop
                                                        let home = std::env::var("HOME").unwrap_or_default();
                                                        let dest = std::path::PathBuf::from(&home)
                                                            .join("Desktop")
                                                            .join(format!("{}.ocrate", export_id));
                                                        match export_project(&export_id, &dest) {
                                                            Ok(()) => error_msg.set(Some(format!("Exported to {}", dest.display()))),
                                                            Err(e) => error_msg.set(Some(format!("Export failed: {e}"))),
                                                        }
                                                    },
                                                    "Export"
                                                }

                                                if confirm_delete.read().as_deref() == Some(sel_id.as_str()) {
                                                    {
                                                        let del_path = proj.path.clone();
                                                        let del_id_auth = delete_id.clone();
                                                        rsx! {
                                                            div { class: "delete-confirm",
                                                                span { class: "delete-confirm-title", "Admin credentials required to delete" }
                                                                input {
                                                                    r#type: "text",
                                                                    placeholder: "Username",
                                                                    value: "{delete_username.read()}",
                                                                    oninput: move |evt: FormEvent| delete_username.set(evt.value().to_string()),
                                                                }
                                                                input {
                                                                    r#type: "password",
                                                                    placeholder: "Password",
                                                                    value: "{delete_password.read()}",
                                                                    oninput: move |evt: FormEvent| delete_password.set(evt.value().to_string()),
                                                                }
                                                                if let Some(ref err) = *delete_error.read() {
                                                                    span { class: "delete-error", "{err}" }
                                                                }
                                                                div { class: "delete-confirm-actions",
                                                                    button {
                                                                        class: "btn btn-danger",
                                                                        disabled: *delete_busy.read(),
                                                                        onclick: {
                                                                            let del_path = del_path.clone();
                                                                            let del_id = del_id_auth.clone();
                                                                            move |_| {
                                                                                let del_path = del_path.clone();
                                                                                let del_id = del_id.clone();
                                                                                let uname = delete_username.read().clone();
                                                                                let pwd = delete_password.read().clone();
                                                                                delete_busy.set(true);
                                                                                delete_error.set(None);
                                                                                spawn(async move {
                                                                                    let paths = ProjectPaths::from_root(del_path);
                                                                                    let db_path = paths.db_path("users.db");
                                                                                    let user_store = start_user_store_with_path(&db_path);
                                                                                    let has_users = user_store.has_any_users().await;

                                                                                    if has_users {
                                                                                        match user_store.authenticate(&uname, &pwd).await {
                                                                                            Ok(user) => {
                                                                                                if !can_admin(&user) {
                                                                                                    delete_error.set(Some("Admin role required.".into()));
                                                                                                    delete_busy.set(false);
                                                                                                    return;
                                                                                                }
                                                                                            }
                                                                                            Err(_) => {
                                                                                                delete_error.set(Some("Invalid credentials.".into()));
                                                                                                delete_busy.set(false);
                                                                                                return;
                                                                                            }
                                                                                        }
                                                                                    }

                                                                                    if let Err(e) = delete_project(&del_id) {
                                                                                        delete_error.set(Some(format!("Delete failed: {e}")));
                                                                                        delete_busy.set(false);
                                                                                        return;
                                                                                    }
                                                                                    confirm_delete.set(None);
                                                                                    selected_id.set(None);
                                                                                    delete_username.set(String::new());
                                                                                    delete_password.set(String::new());
                                                                                    delete_error.set(None);
                                                                                    delete_busy.set(false);
                                                                                    let mut reg = load_registry().unwrap_or_default();
                                                                                    reg.projects.sort_by(|a, b| b.last_opened_ms.cmp(&a.last_opened_ms));
                                                                                    projects.set(reg.projects);
                                                                                });
                                                                            }
                                                                        },
                                                                        "Confirm Delete"
                                                                    }
                                                                    button {
                                                                        class: "btn",
                                                                        onclick: move |_| {
                                                                            confirm_delete.set(None);
                                                                            delete_username.set(String::new());
                                                                            delete_password.set(String::new());
                                                                            delete_error.set(None);
                                                                        },
                                                                        "Cancel"
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                } else {
                                                    button {
                                                        class: "btn btn-danger",
                                                        onclick: {
                                                            let del_id = sel_id.clone();
                                                            move |_| {
                                                                confirm_delete.set(Some(del_id.clone()));
                                                                delete_username.set(String::new());
                                                                delete_password.set(String::new());
                                                                delete_error.set(None);
                                                            }
                                                        },
                                                        "Delete"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            div { class: "project-actions-empty",
                                p { class: "text-muted", "Select a project or create a new one." }

                                button {
                                    class: "btn",
                                    onclick: move |_| {
                                        // Import .ocrate file
                                        spawn(async move {
                                            let file = rfd::AsyncFileDialog::new()
                                                .add_filter("OpenCrate Project", &["ocrate"])
                                                .pick_file()
                                                .await;
                                            if let Some(f) = file {
                                                let path = std::path::PathBuf::from(f.path());
                                                match import_project(&path) {
                                                    Ok(_proj_ref) => {
                                                        let mut reg = load_registry().unwrap_or_default();
                                                        reg.projects.sort_by(|a, b| b.last_opened_ms.cmp(&a.last_opened_ms));
                                                        projects.set(reg.projects);
                                                        error_msg.set(None);
                                                    }
                                                    Err(e) => {
                                                        error_msg.set(Some(format!("Import failed: {e}")));
                                                    }
                                                }
                                            }
                                        });
                                    },
                                    "Import .ocrate"
                                }
                            }
                        }

                        if let Some(ref msg) = *error_msg.read() {
                            div { class: "project-error", "{msg}" }
                        }
                    }
                }
            }

            // ---- Add/Edit Remote Site modal ----
            if let Some(initial_form) = remote_form_open.read().clone() {
                {
                    let store = supervisor_user_store.clone();
                    rsx! {
                        RemoteSiteForm {
                            initial: initial_form,
                            on_save: move |data: RemoteSiteFormData| {
                                let store = store.clone();
                                spawn(async move {
                                    let key_path = default_machine_key_path();
                                    let key = match load_or_create_machine_key(&key_path) {
                                        Ok(k) => k,
                                        Err(e) => {
                                            error_msg.set(Some(format!("machine key: {e}")));
                                            return;
                                        }
                                    };
                                    let payload = serde_json::json!({
                                        "username": data.username,
                                        "password": data.password,
                                    });
                                    let plaintext = payload.to_string();
                                    let encrypted = match encrypt_string(&key, &plaintext) {
                                        Ok(c) => c,
                                        Err(e) => {
                                            error_msg.set(Some(format!("encrypt: {e}")));
                                            return;
                                        }
                                    };
                                    let row = RemoteSiteRow {
                                        config_id: if data.config_id.is_empty() {
                                            uuid::Uuid::new_v4().to_string()
                                        } else {
                                            data.config_id.clone()
                                        },
                                        site_id: None,
                                        name: data.name,
                                        base_url: data.base_url,
                                        auth_token_encrypted: encrypted,
                                        last_connected_ms: None,
                                        last_status: None,
                                        created_ms: std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .unwrap_or_default()
                                            .as_millis() as i64,
                                    };
                                    if let Some(s) = store {
                                        if let Err(e) = s.upsert_remote_site(row).await {
                                            error_msg.set(Some(format!("save: {e}")));
                                            return;
                                        }
                                        let rows = s.list_remote_sites().await;
                                        remote_sites.set(rows);
                                    }
                                    remote_form_open.set(None);
                                });
                            },
                            on_cancel: move |_| remote_form_open.set(None),
                        }
                    }
                }
            }
        }
    }
}

fn format_timestamp(ms: i64) -> String {
    if ms == 0 {
        return "Never".to_string();
    }
    let secs = ms / 1000;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let diff = now - secs;

    if diff < 60 {
        "Just now".to_string()
    } else if diff < 3600 {
        format!("{} min ago", diff / 60)
    } else if diff < 86400 {
        format!("{} hours ago", diff / 3600)
    } else if diff < 604800 {
        format!("{} days ago", diff / 86400)
    } else {
        format!("{} weeks ago", diff / 604800)
    }
}
