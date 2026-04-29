//! Drill-in view for a single remote site in supervisor mode.
//!
//! Phase 2 MVP scope: read-only summary. Active alarms (top 20), 7-day energy
//! KPIs, and an "Open in browser" button that pops the remote site's own web
//! UI in the system browser. Deep edits (Discovery, Programming, Schedules,
//! Config) happen on the remote site itself, not through the supervisor.

use std::sync::Arc;

use dioxus::prelude::*;

use crate::gui::aggregation::alarm_aggregator::SiteAlarmStore;
use crate::gui::aggregation::energy_aggregator::SiteEnergyStore;
use crate::gui::aggregation::types::{AggregatorError, SiteActiveAlarm};
use bms_store_storage::store::alarm_store::AlarmSeverity;
use crate::supervisor::health_loop::RemoteSiteStatus;
use crate::supervisor::remote::alarm_store::RemoteAlarmStore;
use crate::supervisor::remote::energy_store::RemoteEnergyStore;

use super::supervisor_gate::RemoteLoadedSite;

const SEVEN_DAYS_MS: i64 = 7 * 24 * 60 * 60 * 1000;

#[component]
pub fn RemoteSiteView(remote: RemoteLoadedSite) -> Element {
    let client = remote.client.clone();
    let base_url = remote.base_url.clone();
    let status = remote.status;

    let alarm_client = client.clone();
    let alarms_res = use_resource(move || {
        let store = RemoteAlarmStore::new(alarm_client.clone());
        async move { store.get_active_alarms().await }
    });

    let energy_client = client.clone();
    let energy_res = use_resource(move || {
        let store = RemoteEnergyStore::new(energy_client.clone());
        async move { compute_kpis(&store).await }
    });

    let status_snapshot = status.read().clone();
    let status_label = status_snapshot.label();
    let status_pill_class = match &status_snapshot {
        RemoteSiteStatus::Connected => "status-pill status-pill-ok",
        RemoteSiteStatus::Degraded { .. } => "status-pill status-pill-warning",
        RemoteSiteStatus::Unreachable { .. } => "status-pill status-pill-danger",
    };

    let alarms_outcome: Result<Vec<SiteActiveAlarm>, AggregatorError> =
        alarms_res.read().clone().unwrap_or_else(|| Ok(Vec::new()));
    let alarm_count_str = match &alarms_outcome {
        Ok(v) => format!("{} active", v.len()),
        Err(e) => format!("error: {e}"),
    };

    let energy_outcome: Result<EnergyKpiSummary, AggregatorError> = energy_res
        .read()
        .clone()
        .unwrap_or_else(|| Ok(EnergyKpiSummary::default()));

    let open_url = base_url.clone();

    rsx! {
        div { class: "remote-site-view",
            div { class: "remote-site-header",
                div { class: "remote-site-title",
                    span { class: "remote-glyph", "🌐 " }
                    h2 { "{remote.name}" }
                    span { class: "{status_pill_class}", "{status_label}" }
                }
                div { class: "remote-site-meta",
                    span { class: "text-muted", "{base_url}" }
                }
                div { class: "remote-site-actions",
                    button {
                        class: "btn btn-primary",
                        onclick: move |_| {
                            if let Err(e) = webbrowser::open(&open_url) {
                                tracing::warn!(url = %open_url, "failed to open browser: {e}");
                            }
                        },
                        "Open in browser"
                    }
                }
            }

            div { class: "remote-site-section",
                h3 { "Active alarms" }
                p { class: "text-muted", "{alarm_count_str}" }
                match &alarms_outcome {
                    Ok(alarms) if alarms.is_empty() => rsx! {
                        p { class: "text-muted", "No active alarms." }
                    },
                    Ok(alarms) => rsx! {
                        table { class: "remote-alarm-table",
                            thead {
                                tr {
                                    th { "Severity" }
                                    th { "Device" }
                                    th { "Point" }
                                    th { "Value" }
                                }
                            }
                            tbody {
                                for alarm in alarms.iter().take(20) {
                                    {
                                        let sev_class = severity_class(alarm.severity);
                                        let value = format!("{:.2}", alarm.trigger_value);
                                        rsx! {
                                            tr {
                                                td { class: "{sev_class}", "{alarm.severity.label()}" }
                                                td { "{alarm.device_id}" }
                                                td { "{alarm.point_id}" }
                                                td { "{value}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },
                    Err(e) => rsx! {
                        div { class: "site-error-row",
                            span { class: "warning-glyph", "⚠ " }
                            "Failed to fetch alarms: {e}"
                        }
                    },
                }
            }

            div { class: "remote-site-section",
                h3 { "Energy — last 7 days" }
                match &energy_outcome {
                    Ok(kpi) if kpi.meter_count == 0 => rsx! {
                        p { class: "text-muted", "No energy meters configured on this site." }
                    },
                    Ok(kpi) => {
                        let kwh = format!("{:.0}", kpi.total_kwh);
                        let cost = format!("{:.2}", kpi.total_cost);
                        let peak = format!("{:.1}", kpi.peak_kw);
                        let lf = format!("{:.0}%", kpi.load_factor * 100.0);
                        let meters = kpi.meter_count;
                        rsx! {
                            div { class: "remote-energy-grid",
                                div { class: "remote-energy-cell",
                                    span { class: "label", "Total kWh" }
                                    span { class: "value", "{kwh}" }
                                }
                                div { class: "remote-energy-cell",
                                    span { class: "label", "Total cost" }
                                    span { class: "value", "{cost}" }
                                }
                                div { class: "remote-energy-cell",
                                    span { class: "label", "Peak kW" }
                                    span { class: "value", "{peak}" }
                                }
                                div { class: "remote-energy-cell",
                                    span { class: "label", "Load factor" }
                                    span { class: "value", "{lf}" }
                                }
                                div { class: "remote-energy-cell",
                                    span { class: "label", "Meters" }
                                    span { class: "value", "{meters}" }
                                }
                            }
                        }
                    }
                    Err(e) => rsx! {
                        div { class: "site-error-row",
                            span { class: "warning-glyph", "⚠ " }
                            "Failed to fetch energy data: {e}"
                        }
                    },
                }
            }

            if let RemoteSiteStatus::Unreachable { reason, .. } = &status_snapshot {
                div { class: "remote-site-warning",
                    strong { "Connection lost: " }
                    "{reason}"
                }
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
struct EnergyKpiSummary {
    total_kwh: f64,
    total_cost: f64,
    peak_kw: f64,
    meter_count: usize,
    load_factor: f64,
}

async fn compute_kpis(store: &RemoteEnergyStore) -> Result<EnergyKpiSummary, AggregatorError> {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let start = now_ms - SEVEN_DAYS_MS;
    let store_dyn: Arc<dyn SiteEnergyStore> = Arc::new(store.clone());
    let meters = store_dyn.list_meters().await?;
    let meter_count = meters.len();
    let mut total_kwh = 0.0f64;
    let mut total_cost = 0.0f64;
    let mut peak_kw = 0.0f64;
    let mut weighted_avg = 0.0f64;
    for m in &meters {
        let rollups = store_dyn.query_daily_rollups(m.id, start, now_ms).await?;
        for r in &rollups {
            total_kwh += r.consumption_kwh;
            total_cost += r.cost;
            if r.peak_demand_kw > peak_kw {
                peak_kw = r.peak_demand_kw;
            }
            weighted_avg += r.avg_kw;
        }
    }
    let load_factor = if peak_kw > 0.0 && !meters.is_empty() {
        (weighted_avg / (meters.len() as f64)) / peak_kw
    } else {
        0.0
    };
    Ok(EnergyKpiSummary {
        total_kwh,
        total_cost,
        peak_kw,
        meter_count,
        load_factor,
    })
}

fn severity_class(sev: AlarmSeverity) -> &'static str {
    match sev {
        AlarmSeverity::LifeSafety => "sev-life-safety",
        AlarmSeverity::Critical => "sev-critical",
        AlarmSeverity::Warning => "sev-warning",
        AlarmSeverity::Info => "sev-info",
    }
}
