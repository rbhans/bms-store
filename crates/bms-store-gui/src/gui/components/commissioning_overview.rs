use dioxus::prelude::*;

use bms_store_storage::auth::Permission;
use crate::gui::state::AppState;
use bms_store_storage::store::commissioning_store::{CommissionSummary, SessionStatus};

#[component]
pub fn CommissioningOverview() -> Element {
    let state = use_context::<AppState>();
    let can_manage = state.has_permission(Permission::ManageCommissioning);
    let cs = state.commissioning_store.clone();
    let ds = state.discovery_store.clone();
    let mut summaries: Signal<Vec<(CommissionSummary, String)>> = use_signal(Vec::new);
    let mut refresh = use_signal(|| 0u64);

    // Load summaries with device names
    {
        let cs = cs.clone();
        let ds = ds.clone();
        let _ = use_resource(move || {
            let cs = cs.clone();
            let ds = ds.clone();
            let _r = *refresh.read();
            async move {
                let sums = cs.get_summaries().await;
                let mut enriched = Vec::new();
                for s in sums {
                    let dev = ds.get_device(&s.device_id).await;
                    let name = dev.map(|d| d.display_name).unwrap_or(s.device_id.clone());
                    enriched.push((s, name));
                }
                summaries.set(enriched);
            }
        });
    }

    let total_devices = summaries.read().len();
    let signed_off = summaries
        .read()
        .iter()
        .filter(|(s, _)| s.status == SessionStatus::SignedOff)
        .count();
    let in_progress = summaries
        .read()
        .iter()
        .filter(|(s, _)| {
            s.status == SessionStatus::InProgress || s.status == SessionStatus::Completed
        })
        .count();
    let not_started = summaries
        .read()
        .iter()
        .filter(|(s, _)| s.status == SessionStatus::NotStarted)
        .count();

    rsx! {
        div { class: "section-header",
            h3 { "Commissioning Overview" }
        }

        div { class: "commission-summary-cards",
            div { class: "summary-card",
                div { class: "summary-value", "{total_devices}" }
                div { class: "summary-label", "Total Devices" }
            }
            div { class: "summary-card signed-off",
                div { class: "summary-value", "{signed_off}" }
                div { class: "summary-label", "Signed Off" }
            }
            div { class: "summary-card in-progress",
                div { class: "summary-value", "{in_progress}" }
                div { class: "summary-label", "In Progress" }
            }
            div { class: "summary-card not-started",
                div { class: "summary-value", "{not_started}" }
                div { class: "summary-label", "Not Started" }
            }
        }

        if can_manage && !summaries.read().is_empty() {
            div { class: "commission-export-bar",
                button {
                    class: "btn btn-sm",
                    onclick: {
                        let cs = cs.clone();
                        let summaries_ref = summaries.clone();
                        move |_| {
                            let cs = cs.clone();
                            let data = summaries_ref.read().clone();
                            spawn(async move {
                                export_all_csv(&cs, &data).await;
                            });
                        }
                    },
                    "Export All CSV"
                }
            }
        }

        table { class: "data-table",
            thead {
                tr {
                    th { "Device" }
                    th { "Status" }
                    th { "Progress" }
                    th { "Verified" }
                    th { "Failed" }
                    th { "Deferred" }
                    th { "Remaining" }
                }
            }
            tbody {
                for (s, name) in summaries.read().iter() {
                    {
                        let total = s.total;
                        let verified = s.verified;
                        let pct = if total > 0 { (verified * 100) / total } else { 0 };
                        let status_class = match s.status {
                            SessionStatus::NotStarted => "badge badge-inactive",
                            SessionStatus::InProgress => "badge badge-info",
                            SessionStatus::Completed => "badge badge-warning",
                            SessionStatus::SignedOff => "badge badge-ok",
                        };
                        rsx! {
                            tr {
                                td { "{name}" }
                                td {
                                    span { class: "{status_class}", "{s.status.label()}" }
                                }
                                td {
                                    div { class: "commission-progress-bar",
                                        if total > 0 {
                                            div {
                                                class: "progress-segment verified",
                                                style: "width: {s.verified * 100 / total}%",
                                            }
                                            div {
                                                class: "progress-segment failed",
                                                style: "width: {s.failed * 100 / total}%",
                                            }
                                            div {
                                                class: "progress-segment deferred",
                                                style: "width: {s.deferred * 100 / total}%",
                                            }
                                        }
                                    }
                                    span { class: "progress-text", "{pct}%" }
                                }
                                td { "{verified}" }
                                td { "{s.failed}" }
                                td { "{s.deferred}" }
                                td { "{s.not_started}" }
                            }
                        }
                    }
                }
            }
        }

        if summaries.read().is_empty() {
            div { class: "empty-state",
                "No commissioning sessions started yet. Open a device in Discovery and click the Commission tab to begin."
            }
        }
    }
}

async fn export_all_csv(
    cs: &bms_store_storage::store::commissioning_store::CommissioningStore,
    data: &[(CommissionSummary, String)],
) {
    let dialog = rfd::AsyncFileDialog::new()
        .set_file_name("commissioning_report.csv")
        .add_filter("CSV", &["csv"])
        .save_file()
        .await;

    if let Some(handle) = dialog {
        let mut csv = String::from(
            "device_id,device_name,status,total,verified,failed,deferred,not_started\n",
        );
        for (s, name) in data {
            csv.push_str(&format!(
                "{},{},{},{},{},{},{},{}\n",
                s.device_id,
                name.replace(',', ";"),
                s.status.as_str(),
                s.total,
                s.verified,
                s.failed,
                s.deferred,
                s.not_started,
            ));
        }
        let _ = handle.write(csv.as_bytes()).await;
    }
}
