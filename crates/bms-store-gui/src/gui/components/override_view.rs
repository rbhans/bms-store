//! Override management view.
//!
//! Lists active overrides and allows operators to release individual overrides
//! or release all at once (with confirmation).

use dioxus::prelude::*;

use crate::gui::state::AppState;
use bms_store_storage::store::override_store::Override;
use super::preview_modal::{ChangeKind, PreviewModal, PreviewRow};

#[component]
pub fn OverrideView() -> Element {
    let state = use_context::<AppState>();
    let override_store = state.override_store.clone();

    let mut overrides: Signal<Vec<Override>> = use_signal(Vec::new);
    let mut version = use_signal(|| 0u64);
    let mut action_error: Signal<Option<String>> = use_signal(|| None);
    let mut show_release_all_modal = use_signal(|| false);
    let mut releasing = use_signal(|| false);

    // Load active overrides when version changes.
    let store_load = override_store.clone();
    let _v = *version.read();
    let _ = use_resource(move || {
        let store = store_load.clone();
        let _v = _v;
        async move {
            let list = store.list_active().await;
            overrides.set(list);
        }
    });

    // Subscribe to override store version channel for live reactivity.
    let store_sub = override_store.clone();
    use_future(move || {
        let store = store_sub.clone();
        async move {
            let mut rx = store.subscribe();
            loop {
                if rx.changed().await.is_err() {
                    break;
                }
                version.set(*rx.borrow());
            }
        }
    });

    let active = overrides.read().clone();

    // Build release-all preview rows.
    let release_all_rows: Vec<PreviewRow> = active
        .iter()
        .map(|ov| PreviewRow {
            id: ov.id.to_string(),
            label: format!("{} / {}", ov.device_id, ov.point_id),
            before: ov.override_value.to_string(),
            after: ov.original_value
                .as_ref()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "(no original value)".into()),
            change_kind: ChangeKind::Remove,
        })
        .collect();

    rsx! {
        div { class: "override-section",
            div { class: "override-header",
                h2 { "Active Overrides" }
                div { class: "override-toolbar",
                    if !active.is_empty() {
                        button {
                            class: "btn btn-sm btn-danger",
                            disabled: *releasing.read(),
                            onclick: move |_| show_release_all_modal.set(true),
                            "Release All ({active.len()})"
                        }
                    }
                }
            }

            if let Some(err) = &*action_error.read() {
                div { class: "override-error", "{err}" }
            }

            if active.is_empty() {
                div { class: "override-empty",
                    p { "No active overrides." }
                    p { class: "override-empty-hint",
                        "Overrides are created when an operator manually writes a value to a point.
                        They appear here until released."
                    }
                }
            } else {
                div { class: "override-table-wrap",
                    table { class: "override-table",
                        thead {
                            tr {
                                th { "Device" }
                                th { "Point" }
                                th { "Override Value" }
                                th { "Original Value" }
                                th { "Priority" }
                                th { "Set By" }
                                th { "Created" }
                                th { "Expires" }
                                th { "Action" }
                            }
                        }
                        tbody {
                            for ov in &active {
                                {
                                    let ov_id = ov.id;
                                    let store_rel = override_store.clone();
                                    let device = ov.device_id.clone();
                                    let point = ov.point_id.clone();
                                    let val = ov.override_value.to_string();
                                    let orig = ov.original_value
                                        .as_ref()
                                        .map(|v| v.to_string())
                                        .unwrap_or_else(|| "—".into());
                                    let prio = ov.priority
                                        .map(|p| p.to_string())
                                        .unwrap_or_else(|| "—".into());
                                    let by = ov.created_by.clone();
                                    let created = format_ms(ov.created_ms);
                                    let expires = ov.expires_ms
                                        .map(format_ms)
                                        .unwrap_or_else(|| "Never".into());
                                    rsx! {
                                        tr { key: "{ov_id}",
                                            td { "{device}" }
                                            td { "{point}" }
                                            td { class: "override-val", "{val}" }
                                            td { class: "override-orig", "{orig}" }
                                            td { "{prio}" }
                                            td { "{by}" }
                                            td { "{created}" }
                                            td { "{expires}" }
                                            td {
                                                button {
                                                    class: "btn btn-xs btn-warning",
                                                    disabled: *releasing.read(),
                                                    onclick: move |_| {
                                                        let store = store_rel.clone();
                                                        spawn(async move {
                                                            releasing.set(true);
                                                            match store.relinquish(ov_id).await {
                                                                Ok(_) => {
                                                                    action_error.set(None);
                                                                }
                                                                Err(e) => {
                                                                    action_error.set(Some(format!("Release failed: {e}")));
                                                                }
                                                            }
                                                            releasing.set(false);
                                                        });
                                                    },
                                                    "Release"
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

            // Release All confirmation modal.
            if *show_release_all_modal.read() {
                {
                    let store_all = override_store.clone();
                    let rows = release_all_rows.clone();
                    rsx! {
                        PreviewModal {
                            title: "Release All Overrides".to_string(),
                            rows: rows,
                            on_confirm: move |_| {
                                let store = store_all.clone();
                                let all = overrides.read().clone();
                                show_release_all_modal.set(false);
                                spawn(async move {
                                    releasing.set(true);
                                    let mut errors = Vec::new();
                                    for ov in &all {
                                        if let Err(e) = store.relinquish(ov.id).await {
                                            errors.push(format!("#{}: {e}", ov.id));
                                        }
                                    }
                                    if errors.is_empty() {
                                        action_error.set(None);
                                    } else {
                                        action_error.set(Some(format!("Some releases failed: {}", errors.join("; "))));
                                    }
                                    releasing.set(false);
                                });
                            },
                            on_cancel: move |_| show_release_all_modal.set(false),
                        }
                    }
                }
            }
        }
    }
}

fn format_ms(ms: i64) -> String {
    use std::time::{Duration, UNIX_EPOCH};
    let d = UNIX_EPOCH + Duration::from_millis(ms as u64);
    let secs = d.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let h = (secs / 3600) % 24;
    let min = (secs / 60) % 60;
    let s = secs % 60;
    // Days since epoch — approximate date as YYYY-MM-DD is complex without chrono.
    // Show ISO-8601-like without external deps.
    let days = secs / 86400;
    let (y, m, day) = days_to_ymd(days);
    format!("{y:04}-{m:02}-{day:02} {h:02}:{min:02}:{s:02}")
}

/// Naive Gregorian date from days since Unix epoch (1970-01-01).
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
    let leap = is_leap(year);
    let month_days: &[u64] = if leap {
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
