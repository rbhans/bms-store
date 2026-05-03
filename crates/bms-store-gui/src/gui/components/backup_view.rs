//! Backup / restore view.
//!
//! Lets operators create on-demand backups, list existing backups,
//! and restore from a backup with a strongly-worded confirmation modal.

use dioxus::prelude::*;

use crate::gui::state::AppState;
use bms_store_storage::backup::BackupInfo;

#[derive(Debug, Clone, PartialEq)]
enum BackupAction {
    Idle,
    Creating,
    Restoring(String), // filename to restore
}

#[component]
pub fn BackupView() -> Element {
    let state = use_context::<AppState>();
    let scheduler = state.backup_scheduler.clone();

    let mut backups: Signal<Vec<BackupInfo>> = use_signal(Vec::new);
    let mut status_msg: Signal<Option<String>> = use_signal(|| None);
    let mut error_msg: Signal<Option<String>> = use_signal(|| None);
    let mut action = use_signal(|| BackupAction::Idle);
    let mut version = use_signal(|| 0u32);
    let mut show_restore_modal = use_signal(|| Option::<String>::None);

    // Load backup list.
    let sched_load = scheduler.clone();
    let _v = *version.read();
    let _ = use_resource(move || {
        let sched = sched_load.clone();
        let _v = _v;
        async move {
            let list = tokio::task::spawn_blocking(move || {
                sched.lock().unwrap().list_backups()
            })
            .await
            .unwrap_or_default();
            backups.set(list);
        }
    });

    let is_busy = !matches!(*action.read(), BackupAction::Idle);
    let backup_list = backups.read().clone();

    rsx! {
        div { class: "backup-section",
            div { class: "backup-header",
                h2 { "Backup & Restore" }
                button {
                    class: "btn btn-sm btn-primary",
                    disabled: is_busy,
                    onclick: {
                        let sched = scheduler.clone();
                        move |_| {
                            let sched = sched.clone();
                            spawn(async move {
                                action.set(BackupAction::Creating);
                                error_msg.set(None);
                                status_msg.set(None);
                                let result = tokio::task::spawn_blocking(move || {
                                    sched.lock().unwrap().backup_now()
                                })
                                .await;
                                match result {
                                    Ok(Ok(path)) => {
                                        let name = path
                                            .file_name()
                                            .and_then(|n| n.to_str())
                                            .unwrap_or("backup")
                                            .to_string();
                                        status_msg.set(Some(format!("Backup created: {name}")));
                                        let next_v = version.read().wrapping_add(1);
                                        version.set(next_v);
                                    }
                                    Ok(Err(e)) => {
                                        error_msg.set(Some(format!("Backup failed: {e}")));
                                    }
                                    Err(e) => {
                                        error_msg.set(Some(format!("Task error: {e}")));
                                    }
                                }
                                action.set(BackupAction::Idle);
                            });
                        }
                    },
                    if matches!(*action.read(), BackupAction::Creating) {
                        "Creating…"
                    } else {
                        "Create Backup"
                    }
                }
            }

            if let Some(msg) = &*status_msg.read() {
                div { class: "backup-status-ok", "{msg}" }
            }
            if let Some(err) = &*error_msg.read() {
                div { class: "backup-status-err", "{err}" }
            }

            if backup_list.is_empty() {
                div { class: "backup-empty",
                    p { "No backups yet. Click \"Create Backup\" to make one." }
                }
            } else {
                div { class: "backup-table-wrap",
                    table { class: "backup-table",
                        thead {
                            tr {
                                th { "Filename" }
                                th { "Created" }
                                th { "Size" }
                                th { "Action" }
                            }
                        }
                        tbody {
                            for info in &backup_list {
                                {
                                    let fname = info.filename.clone();
                                    let fname_restore = info.filename.clone();
                                    let created = format_ms(info.created_ms);
                                    let size = format_bytes(info.size_bytes);
                                    rsx! {
                                        tr { key: "{fname}",
                                            td { class: "backup-filename", "{fname}" }
                                            td { "{created}" }
                                            td { "{size}" }
                                            td {
                                                button {
                                                    class: "btn btn-xs btn-danger",
                                                    disabled: is_busy,
                                                    onclick: move |_| {
                                                        show_restore_modal.set(Some(fname_restore.clone()));
                                                    },
                                                    "Restore"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Restore confirmation modal — strongly worded.
            if let Some(fname) = show_restore_modal.read().clone() {
                {
                    let fname = fname.clone();
                    let fname_for_title = fname.clone();
                    let sched_restore = scheduler.clone();
                    rsx! {
                        div { class: "modal-overlay",
                            onclick: move |_| show_restore_modal.set(None),
                            div {
                                class: "modal-box restore-confirm-modal",
                                onclick: |e| e.stop_propagation(),
                                div { class: "modal-header",
                                    h3 { "Restore Backup" }
                                }
                                div { class: "modal-body",
                                    div { class: "restore-warning",
                                        p { class: "restore-warning-headline",
                                            "WARNING: This action is destructive and cannot be undone."
                                        }
                                        p {
                                            "Restoring \""
                                            strong { "{fname_for_title}" }
                                            "\" will OVERWRITE ALL current project data — including all points,
                                            devices, history, overrides, users, and settings."
                                        }
                                        p {
                                            "It is strongly recommended to create a fresh backup of the current
                                            state before proceeding."
                                        }
                                        p { class: "restore-warning-final",
                                            "Only proceed if you are certain you want to replace everything with this backup."
                                        }
                                    }
                                }
                                div { class: "modal-footer",
                                    button {
                                        class: "btn btn-secondary",
                                        onclick: move |_| show_restore_modal.set(None),
                                        "Cancel"
                                    }
                                    button {
                                        class: "btn btn-danger",
                                        disabled: is_busy,
                                        onclick: {
                                            let fname = fname.clone();
                                            let sched = sched_restore.clone();
                                            move |_| {
                                                let fname = fname.clone();
                                                let sched = sched.clone();
                                                show_restore_modal.set(None);
                                                spawn(async move {
                                                    action.set(BackupAction::Restoring(fname.clone()));
                                                    error_msg.set(None);
                                                    status_msg.set(None);
                                                    // Restore via project import. BackupScheduler exposes
                                                    // the backup_dir; we call project::import_project.
                                                    let result = tokio::task::spawn_blocking(move || {
                                                        let _sched_guard = sched.lock().unwrap();
                                                        let backup_dir = bms_store_storage::project::opencrate_home().join("backups");
                                                        let path = backup_dir.join(&fname);
                                                        drop(_sched_guard);
                                                        bms_store_storage::project::import_project(&path)
                                                            .map(|_| ())
                                                            .map_err(|e| e.to_string())
                                                    })
                                                    .await;
                                                    match result {
                                                        Ok(Ok(())) => {
                                                            // FIXME: project::import_project creates a NEW project
                                                            // entry — it does not overwrite the running project's
                                                            // stores. Be honest about that in the UI until the
                                                            // restore-replaces-current-project flow is implemented.
                                                            status_msg.set(Some(
                                                                "Backup imported as a NEW project. Open the project launcher to switch to it. The current project is unchanged.".into()
                                                            ));
                                                            let next_v = version.read().wrapping_add(1);
                                                        version.set(next_v);
                                                        }
                                                        Ok(Err(e)) => {
                                                            error_msg.set(Some(format!("Restore failed: {e}")));
                                                        }
                                                        Err(e) => {
                                                            error_msg.set(Some(format!("Task error: {e}")));
                                                        }
                                                    }
                                                    action.set(BackupAction::Idle);
                                                });
                                            }
                                        },
                                        if matches!(&*action.read(), BackupAction::Restoring(_)) {
                                            "Restoring…"
                                        } else {
                                            "Restore — I understand this will overwrite all data"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn format_ms(ms: i64) -> String {
    use std::time::{Duration, UNIX_EPOCH};
    let secs = (UNIX_EPOCH + Duration::from_millis(ms as u64))
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let h = (secs / 3600) % 24;
    let min = (secs / 60) % 60;
    let s = secs % 60;
    let days = secs / 86400;
    let (y, mo, d) = days_to_ymd(days);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{min:02}:{s:02}")
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut year = 1970u64;
    loop {
        let leap = is_leap(year);
        let days_in_year = if leap { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }
    let month_days: &[u64] = if is_leap(year) {
        &[31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        &[31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1u64;
    for &md in month_days {
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }
    (year, month, days + 1)
}

fn is_leap(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}
