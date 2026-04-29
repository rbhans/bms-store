use dioxus::prelude::*;

use bms_store_storage::auth::can_admin;
use crate::gui::state::{CloseAction, LaunchSelection};
use bms_store_storage::project::{
    create_project, delete_project, export_project, import_project, load_registry,
    migrate_legacy_if_needed, validate_project_path, ProjectPaths,
};
use bms_store_storage::store::user_store::start_user_store_with_path;

#[derive(Debug, Clone, PartialEq)]
enum LauncherTab {
    Recent,
    NewProject,
}

#[component]
pub fn ProjectLauncher(
    on_open: EventHandler<LaunchSelection>,
    initial_action: Option<CloseAction>,
) -> Element {
    let initial_tab = match initial_action {
        Some(CloseAction::ToNewProject) => LauncherTab::NewProject,
        _ => LauncherTab::Recent,
    };

    let mut projects = use_signal(Vec::new);
    let mut selected_id = use_signal(|| Option::<String>::None);
    let mut tab = use_signal(move || initial_tab);
    let mut new_name = use_signal(String::new);
    let mut new_desc = use_signal(String::new);
    let mut error_msg = use_signal(|| Option::<String>::None);

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
                        } else {
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
