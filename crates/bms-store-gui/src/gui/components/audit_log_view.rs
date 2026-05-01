use dioxus::prelude::*;

use crate::gui::state::AppState;
use bms_store_storage::store::audit_store::{AuditAction, AuditEntry, AuditQuery};

#[component]
pub fn AuditLogView() -> Element {
    let state = use_context::<AppState>();
    let audit_store = state.audit_store.clone();

    let mut entries = use_signal(Vec::<AuditEntry>::new);
    let mut total_count = use_signal(|| 0i64);
    let mut current_page = use_signal(|| 0i64);
    let mut filter_action = use_signal(|| Option::<AuditAction>::None);
    let mut filter_user = use_signal(String::new);
    let mut selected_entry = use_signal(|| Option::<i64>::None);
    let mut loading = use_signal(|| false);

    let page_size: i64 = 50;

    // Load entries when filters or page change
    let store_for_load = audit_store.clone();
    let _ = use_resource(move || {
        let store = store_for_load.clone();
        let action = filter_action.read().clone();
        let user_filter = filter_user.read().clone();
        let page = *current_page.read();
        async move {
            loading.set(true);
            let query = AuditQuery {
                action,
                user_id: if user_filter.is_empty() {
                    None
                } else {
                    Some(user_filter.clone())
                },
                limit: Some(page_size),
                offset: Some(page * page_size),
                ..Default::default()
            };
            let count_query = AuditQuery {
                action: query.action,
                user_id: query.user_id.clone(),
                ..Default::default()
            };
            if let Ok(e) = store.query(query).await {
                entries.set(e);
            }
            if let Ok(c) = store.count(count_query).await {
                total_count.set(c);
            }
            loading.set(false);
        }
    });

    let total = *total_count.read();
    let page = *current_page.read();
    let total_pages = (total + page_size - 1) / page_size;
    let has_prev = page > 0;
    let has_next = page < total_pages - 1;

    let selected_id = *selected_entry.read();
    let detail = entries
        .read()
        .iter()
        .find(|e| Some(e.id) == selected_id)
        .cloned();

    rsx! {
        div { class: "audit-log-view",
            // Filter bar
            div { class: "audit-filters",
                div { class: "audit-filter-group",
                    label { "Action:" }
                    select {
                        value: filter_action.read().as_ref().map(|a| a.as_str().to_string()).unwrap_or_default(),
                        onchange: move |e| {
                            let val = e.value();
                            if val.is_empty() {
                                filter_action.set(None);
                            } else {
                                filter_action.set(AuditAction::from_str(&val));
                            }
                            current_page.set(0);
                        },
                        option { value: "", "All Actions" }
                        {AuditAction::all().iter().map(|a| {
                            let key = a.as_str().to_string();
                            let label = a.label().to_string();
                            rsx! {
                                option { value: "{key}", "{label}" }
                            }
                        })}
                    }
                }
                div { class: "audit-filter-group",
                    label { "User:" }
                    input {
                        r#type: "text",
                        placeholder: "Filter by username...",
                        value: "{filter_user}",
                        oninput: move |e| {
                            filter_user.set(e.value());
                            current_page.set(0);
                        },
                    }
                }
                span { class: "audit-count", "{total} entries" }
            }

            // Two-pane layout
            div { class: "audit-panes",
                // Entry list
                div { class: "audit-list",
                    if *loading.read() && entries.read().is_empty() {
                        div { class: "audit-loading", "Loading..." }
                    }
                    table { class: "audit-table",
                        thead {
                            tr {
                                th { "Time" }
                                th { "User" }
                                th { "Action" }
                                th { "Resource" }
                                th { "Result" }
                            }
                        }
                        tbody {
                            {entries.read().iter().map(|entry| {
                                let eid = entry.id;
                                let is_selected = selected_id == Some(eid);
                                let cls = if is_selected { "audit-row selected" } else { "audit-row" };
                                let time = format_time_ms(entry.timestamp_ms);
                                let username = entry.username.clone();
                                let action_label = entry.action.label().to_string();
                                let resource = entry.resource_id.clone().unwrap_or_default();
                                let result_cls = if entry.result == bms_store_storage::store::audit_store::AuditResult::Success {
                                    "audit-result success"
                                } else {
                                    "audit-result failure"
                                };
                                let result_str = entry.result.as_str().to_string();
                                rsx! {
                                    tr {
                                        class: "{cls}",
                                        onclick: move |_| selected_entry.set(Some(eid)),
                                        td { class: "audit-col-time", "{time}" }
                                        td { "{username}" }
                                        td { "{action_label}" }
                                        td { class: "audit-col-resource", "{resource}" }
                                        td { span { class: "{result_cls}", "{result_str}" } }
                                    }
                                }
                            })}
                        }
                    }

                    // Pagination
                    if total_pages > 1 {
                        div { class: "audit-pagination",
                            button {
                                class: "btn btn-sm",
                                disabled: !has_prev,
                                onclick: move |_| {
                                    let p = *current_page.read();
                                    if p > 0 { current_page.set(p - 1); }
                                },
                                "← Prev"
                            }
                            span { "Page {page + 1} of {total_pages}" }
                            button {
                                class: "btn btn-sm",
                                disabled: !has_next,
                                onclick: move |_| {
                                    let p = *current_page.read();
                                    current_page.set(p + 1);
                                },
                                "Next →"
                            }
                        }
                    }
                }

                // Detail pane
                div { class: "audit-detail",
                    if let Some(ref entry) = detail {
                        div { class: "audit-detail-content",
                            h3 { "{entry.action.label()}" }
                            div { class: "audit-detail-meta",
                                div { class: "audit-detail-field",
                                    span { class: "audit-detail-label", "Time" }
                                    span { "{format_time_ms(entry.timestamp_ms)}" }
                                }
                                div { class: "audit-detail-field",
                                    span { class: "audit-detail-label", "User" }
                                    span { "{entry.username} ({entry.user_id})" }
                                }
                                div { class: "audit-detail-field",
                                    span { class: "audit-detail-label", "Resource Type" }
                                    span { "{entry.resource_type}" }
                                }
                                if let Some(ref rid) = entry.resource_id {
                                    div { class: "audit-detail-field",
                                        span { class: "audit-detail-label", "Resource ID" }
                                        span { "{rid}" }
                                    }
                                }
                                div { class: "audit-detail-field",
                                    span { class: "audit-detail-label", "Result" }
                                    {
                                        let rcls = if entry.result == bms_store_storage::store::audit_store::AuditResult::Success {
                                            "audit-result success"
                                        } else {
                                            "audit-result failure"
                                        };
                                        let rstr = entry.result.as_str();
                                        rsx! { span { class: "{rcls}", "{rstr}" } }
                                    }
                                }
                                if let Some(ref details) = entry.details {
                                    div { class: "audit-detail-field",
                                        span { class: "audit-detail-label", "Details" }
                                        span { class: "audit-detail-details", "{details}" }
                                    }
                                }
                                if let Some(ref err) = entry.error_message {
                                    div { class: "audit-detail-field",
                                        span { class: "audit-detail-label", "Error" }
                                        span { class: "audit-detail-error", "{err}" }
                                    }
                                }
                            }
                        }
                    } else {
                        div { class: "audit-placeholder",
                            "Select an entry to view details"
                        }
                    }
                }
            }
        }
    }
}

fn format_time_ms(ms: i64) -> String {
    let secs = ms / 1000;
    let dt = chrono_lite(secs);
    dt
}

fn chrono_lite(epoch_secs: i64) -> String {
    // Simple UTC timestamp formatter
    let s = epoch_secs;
    let days = s / 86400;
    let time_of_day = s % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Days since epoch to date (simplified)
    let mut y = 1970i64;
    let mut remaining = days;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }
    let months = [
        31,
        if is_leap(y) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut m = 1;
    for &md in &months {
        if remaining < md {
            break;
        }
        remaining -= md;
        m += 1;
    }
    let d = remaining + 1;
    format!("{y:04}-{m:02}-{d:02} {hours:02}:{minutes:02}:{seconds:02}")
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}
