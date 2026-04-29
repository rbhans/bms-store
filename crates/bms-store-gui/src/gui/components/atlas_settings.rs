use dioxus::prelude::*;

use bms_store_storage::atlas::db::AtlasDb;
use bms_store_storage::atlas::matcher::AtlasMatcher;
use bms_store_storage::atlas::model::AtlasStats;
use bms_store_storage::atlas::sync;
use crate::gui::state::AppState;
use std::sync::Arc;

/// Config sub-tab for managing the BAS Atlas taxonomy plugin.
#[component]
pub fn AtlasSettingsView() -> Element {
    let state = use_context::<AppState>();

    // Read the shared lock to get current matcher status
    let atlas_guard = state.atlas_lock.read().unwrap();
    let is_installed = atlas_guard.is_some();
    let in_memory_info = atlas_guard
        .as_ref()
        .map(|m| (m.point_count(), m.equipment_count()));
    drop(atlas_guard);

    let mut local_stats: Signal<Option<AtlasStats>> = use_signal(|| None);
    let mut checking = use_signal(|| false);
    let mut downloading = use_signal(|| false);
    let mut progress = use_signal(|| 0.0f32);
    let mut error_msg: Signal<Option<String>> = use_signal(|| None);
    let mut update_check: Signal<Option<sync::UpdateCheck>> = use_signal(|| None);
    let mut refresh = use_signal(|| 0u32);
    let _v = *refresh.read();

    // Load local stats on mount
    {
        let paths = state.project_paths.clone();
        let _ = use_resource(move || {
            let paths = paths.clone();
            let _v = *refresh.read();
            async move {
                let db_path = paths.db_path("bas-atlas.db");
                if AtlasDb::is_available(&db_path) {
                    if let Ok(db) = AtlasDb::open(&db_path) {
                        if let Ok(stats) = db.stats() {
                            local_stats.set(Some(stats));
                            return;
                        }
                    }
                }
                local_stats.set(None);
            }
        });
    }

    let ls = local_stats.read().clone();
    let uc = update_check.read().clone();
    let err = error_msg.read().clone();
    let prog = *progress.read();
    let is_checking = *checking.read();
    let is_downloading = *downloading.read();

    rsx! {
        div { class: "atlas-settings",
            h3 { "BAS Atlas Taxonomy" }
            p { class: "config-hint",
                "BAS Atlas provides 501 point definitions, 101 equipment types, and 8000+ aliases for dramatically improved auto-tagging accuracy during device acceptance."
            }

            // Status card
            div { class: "atlas-status-card",
                div { class: "atlas-status-row",
                    span { class: "atlas-label", "Status" }
                    if is_installed {
                        span { class: "atlas-badge atlas-badge-ok", "Loaded" }
                    } else if ls.is_some() {
                        span { class: "atlas-badge atlas-badge-warn", "Available (not loaded)" }
                    } else {
                        span { class: "atlas-badge atlas-badge-off", "Not installed" }
                    }
                }

                if let Some(ref stats) = ls {
                    div { class: "atlas-status-row",
                        span { class: "atlas-label", "Version" }
                        span { "{stats.version}" }
                    }
                    div { class: "atlas-status-row",
                        span { class: "atlas-label", "Points" }
                        span { "{stats.total_points}" }
                    }
                    div { class: "atlas-status-row",
                        span { class: "atlas-label", "Equipment" }
                        span { "{stats.total_equipment}" }
                    }
                    div { class: "atlas-status-row",
                        span { class: "atlas-label", "Last Updated" }
                        span { "{format_timestamp(stats.updated_ms)}" }
                    }
                }

                if let Some((pt_count, eq_count)) = in_memory_info {
                    div { class: "atlas-status-row",
                        span { class: "atlas-label", "In-memory" }
                        span { "{pt_count} points, {eq_count} equipment" }
                    }
                }
            }

            // Error display
            if let Some(ref msg) = err {
                div { class: "atlas-error", "{msg}" }
            }

            // Update check results
            if let Some(ref check) = uc {
                div { class: "atlas-update-card",
                    if check.update_available {
                        p { class: "atlas-update-available", "Update available!" }
                        if let Some(ref remote) = check.remote {
                            div { class: "atlas-status-row",
                                span { class: "atlas-label", "Remote version" }
                                span { "{remote.version}" }
                            }
                            div { class: "atlas-status-row",
                                span { class: "atlas-label", "Remote points" }
                                span { "{remote.total_points}" }
                            }
                            div { class: "atlas-status-row",
                                span { class: "atlas-label", "Remote equipment" }
                                span { "{remote.total_equipment}" }
                            }
                        }
                    } else {
                        p { class: "config-hint", "Atlas data is up to date." }
                    }
                }
            }

            // Progress bar during download
            if is_downloading {
                div { class: "atlas-progress",
                    div { class: "atlas-progress-bar",
                        div {
                            class: "atlas-progress-fill",
                            style: "width: {(prog * 100.0) as u32}%",
                        }
                    }
                    span { class: "atlas-progress-text", "{(prog * 100.0) as u32}%" }
                }
            }

            // Action buttons
            div { class: "atlas-actions",
                // Check for updates
                button {
                    class: "config-btn",
                    disabled: is_checking || is_downloading,
                    onclick: {
                        let paths = state.project_paths.clone();
                        move |_| {
                            checking.set(true);
                            error_msg.set(None);
                            let paths = paths.clone();
                            spawn(async move {
                                let db_path = paths.db_path("bas-atlas.db");
                                let local_db = AtlasDb::open(&db_path).ok();
                                match sync::check_for_updates(local_db.as_ref()).await {
                                    Ok(check) => {
                                        update_check.set(Some(check));
                                    }
                                    Err(e) => {
                                        error_msg.set(Some(format!("Update check failed: {e}")));
                                    }
                                }
                                checking.set(false);
                            });
                        }
                    },
                    if is_checking { "Checking..." } else { "Check for Updates" }
                }

                // Download / Update
                button {
                    class: "config-btn config-btn-primary",
                    disabled: is_downloading || is_checking,
                    onclick: {
                        let paths = state.project_paths.clone();
                        let atlas_lock = state.atlas_lock.clone();
                        move |_| {
                            downloading.set(true);
                            error_msg.set(None);
                            progress.set(0.0);
                            let paths = paths.clone();
                            let atlas_lock = atlas_lock.clone();
                            spawn(async move {
                                let db_path = paths.db_path("bas-atlas.db");
                                match sync::download_atlas(&db_path, |p| {
                                    progress.set(p);
                                }).await {
                                    Ok(stats) => {
                                        local_stats.set(Some(stats));
                                        update_check.set(None);
                                        // Reload matcher into the shared lock — takes effect immediately
                                        if let Ok(db) = AtlasDb::open(&db_path) {
                                            if let Ok(matcher) = AtlasMatcher::load(&db) {
                                                *atlas_lock.write().unwrap() = Some(Arc::new(matcher));
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        error_msg.set(Some(format!("Download failed: {e}")));
                                    }
                                }
                                downloading.set(false);
                                let v = *refresh.read();
                                refresh.set(v + 1);
                            });
                        }
                    },
                    if is_downloading { "Downloading..." } else if ls.is_some() { "Update Atlas Data" } else { "Download Atlas Data" }
                }

                // Remove
                if ls.is_some() {
                    button {
                        class: "config-btn config-btn-danger",
                        disabled: is_downloading,
                        onclick: {
                            let paths = state.project_paths.clone();
                            let atlas_lock = state.atlas_lock.clone();
                            move |_| {
                                let db_path = paths.db_path("bas-atlas.db");
                                if let Err(e) = sync::remove_atlas(&db_path) {
                                    error_msg.set(Some(format!("Remove failed: {e}")));
                                } else {
                                    // Clear the shared lock — DiscoveryService stops using Atlas immediately
                                    *atlas_lock.write().unwrap() = None;
                                    local_stats.set(None);
                                    update_check.set(None);
                                    let v = *refresh.read();
                                    refresh.set(v + 1);
                                }
                            }
                        },
                        "Remove Atlas Data"
                    }
                }
            }
        }
    }
}

fn format_timestamp(ms: i64) -> String {
    if ms == 0 {
        return "Unknown".into();
    }
    let secs = ms / 1000;
    let days_ago = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
        - secs)
        / 86400;

    if days_ago == 0 {
        "Today".into()
    } else if days_ago == 1 {
        "Yesterday".into()
    } else {
        format!("{days_ago} days ago")
    }
}
