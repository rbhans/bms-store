//! Cross-site Alarm view for supervisor mode.
//!
//! Phase 1 scope: read-only active-alarm table aggregated across every loaded
//! site with per-row site label + severity + device/point + trigger time. No
//! bulk config, no history tab — those stay single-site.
//!
//! Phase 2: each `LoadedSiteVariant` produces a trait-object source so a
//! supervisor can mix local and remote sites in one table. Per-site failures
//! show up as "⚠ unreachable" rows above the data table instead of blanking
//! it.

use std::sync::Arc;

use dioxus::prelude::*;

use crate::gui::aggregation::alarm_aggregator::{
    AggregatedAlarmStore, SiteAlarmSource, SiteAlarmStore,
};
use bms_store_storage::store::alarm_store::AlarmSeverity;

use super::supervisor_gate::{LoadedSiteVariant, SupervisorHandle};

#[component]
pub fn CrossSiteAlarmView(handle: SupervisorHandle) -> Element {
    let agg = use_hook(|| {
        let sources: Vec<SiteAlarmSource> = handle
            .sites
            .iter()
            .map(|variant| match variant {
                LoadedSiteVariant::Local(s) => SiteAlarmSource {
                    site_id: s.meta.id.clone(),
                    site_name: s.meta.name.clone(),
                    store: Arc::new(s.platform.alarm_store.clone()) as Arc<dyn SiteAlarmStore>,
                },
                LoadedSiteVariant::Remote(r) => SiteAlarmSource {
                    site_id: r.site_id.clone(),
                    site_name: r.name.clone(),
                    store: Arc::new(
                        crate::supervisor::remote::alarm_store::RemoteAlarmStore::new(
                            r.client.clone(),
                        ),
                    ) as Arc<dyn SiteAlarmStore>,
                },
            })
            .collect();
        AggregatedAlarmStore::new(sources)
    });

    let site_count = agg.site_count();
    let agg_for_resource = agg.clone();
    let active_res = use_resource(move || {
        let agg = agg_for_resource.clone();
        async move { agg.get_active_alarms().await }
    });

    let result = active_res.read().clone().unwrap_or_default();
    let total = result.rows.len();

    rsx! {
        div { class: "cross-site-alarm-view",
            div { class: "cross-site-alarm-header",
                h2 { "Active alarms across all sites" }
                p { class: "text-muted",
                    "{total} active alarm(s) across {site_count} sites"
                }
            }
            if !result.site_errors.is_empty() {
                div { class: "cross-site-alarm-errors",
                    for err in result.site_errors.iter() {
                        div { class: "site-error-row",
                            span { class: "warning-glyph", "⚠" }
                            strong { "{err.site_name}" }
                            span { class: "text-muted", " unreachable: {err.error}" }
                        }
                    }
                }
            }
            if result.rows.is_empty() && result.site_errors.is_empty() {
                div { class: "cross-site-alarm-empty",
                    p { class: "text-muted", "No active alarms." }
                }
            } else if !result.rows.is_empty() {
                table { class: "cross-site-alarm-table",
                    thead {
                        tr {
                            th { "Site" }
                            th { "Severity" }
                            th { "Device" }
                            th { "Point" }
                            th { "Value" }
                            th { "Triggered" }
                            th { "Ack'd" }
                        }
                    }
                    tbody {
                        for row in result.rows.iter() {
                            {
                                let sev_class = severity_class(row.alarm.severity);
                                let value_display = format!("{:.2}", row.alarm.trigger_value);
                                let trigger_display = format_ms(row.alarm.trigger_time_ms);
                                let ack_display = row.alarm
                                    .ack_time_ms
                                    .map(format_ms)
                                    .unwrap_or_else(|| "-".to_string());
                                rsx! {
                                    tr {
                                        td { "{row.site_name}" }
                                        td { class: "{sev_class}",
                                            "{row.alarm.severity.label()}"
                                        }
                                        td { "{row.alarm.device_id}" }
                                        td { "{row.alarm.point_id}" }
                                        td { "{value_display}" }
                                        td { "{trigger_display}" }
                                        td { "{ack_display}" }
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

fn severity_class(sev: AlarmSeverity) -> &'static str {
    match sev {
        AlarmSeverity::LifeSafety => "sev-life-safety",
        AlarmSeverity::Critical => "sev-critical",
        AlarmSeverity::Warning => "sev-warning",
        AlarmSeverity::Info => "sev-info",
    }
}

/// Format an epoch-millisecond timestamp as a compact human string.
fn format_ms(ms: i64) -> String {
    let secs = ms / 1000;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let diff = now - secs;
    if diff < 60 {
        "just now".to_string()
    } else if diff < 3600 {
        format!("{} min ago", diff / 60)
    } else if diff < 86400 {
        format!("{} h ago", diff / 3600)
    } else {
        format!("{} d ago", diff / 86400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_classes() {
        assert_eq!(severity_class(AlarmSeverity::Critical), "sev-critical");
        assert_eq!(severity_class(AlarmSeverity::LifeSafety), "sev-life-safety");
        assert_eq!(severity_class(AlarmSeverity::Warning), "sev-warning");
        assert_eq!(severity_class(AlarmSeverity::Info), "sev-info");
    }
}
