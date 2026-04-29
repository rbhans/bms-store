//! `SiteStatusDashboard` — cards for every loaded site in supervisor mode.
//!
//! For local sites: name, alarm counts by severity, bridge status from
//! `BridgeStartReport`, and a quick "Open this site" action.
//!
//! For Phase 2 remote sites: name, REST-fetched alarm counts, connection
//! status pill (connected/degraded/unreachable), and an "Open in browser"
//! action that pops the remote site's web UI.

use std::sync::Arc;

use dioxus::prelude::*;

use crate::gui::aggregation::alarm_aggregator::SiteAlarmStore;
use crate::gui::aggregation::types::SiteActiveAlarm;
use bms_store_storage::store::alarm_store::{ActiveAlarm, AlarmStore};
use crate::supervisor::health_loop::RemoteSiteStatus;
use crate::supervisor::remote::alarm_store::RemoteAlarmStore;

use super::supervisor_gate::{LoadedSite, LoadedSiteVariant, RemoteLoadedSite, SupervisorHandle};

/// Alarm count summary for one site.
#[derive(Clone, Debug, Default, PartialEq)]
struct AlarmCounts {
    critical: usize,
    warning: usize,
    info: usize,
    life_safety: usize,
}

impl AlarmCounts {
    fn total(&self) -> usize {
        self.critical + self.warning + self.info + self.life_safety
    }

    fn from_local_alarms(alarms: &[ActiveAlarm]) -> Self {
        use bms_store_storage::store::alarm_store::AlarmSeverity;
        let mut out = Self::default();
        for a in alarms {
            match a.severity {
                AlarmSeverity::Critical => out.critical += 1,
                AlarmSeverity::Warning => out.warning += 1,
                AlarmSeverity::Info => out.info += 1,
                AlarmSeverity::LifeSafety => out.life_safety += 1,
            }
        }
        out
    }

    fn from_dto_alarms(alarms: &[SiteActiveAlarm]) -> Self {
        use bms_store_storage::store::alarm_store::AlarmSeverity;
        let mut out = Self::default();
        for a in alarms {
            match a.severity {
                AlarmSeverity::Critical => out.critical += 1,
                AlarmSeverity::Warning => out.warning += 1,
                AlarmSeverity::Info => out.info += 1,
                AlarmSeverity::LifeSafety => out.life_safety += 1,
            }
        }
        out
    }
}

async fn fetch_local_counts(store: AlarmStore) -> AlarmCounts {
    let alarms = store.get_active_alarms().await;
    AlarmCounts::from_local_alarms(&alarms)
}

async fn fetch_remote_counts(
    client: Arc<crate::supervisor::remote::client::RemoteSiteClient>,
) -> Result<AlarmCounts, String> {
    let store = RemoteAlarmStore::new(client);
    match store.get_active_alarms().await {
        Ok(v) => Ok(AlarmCounts::from_dto_alarms(&v)),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bms_store_storage::store::alarm_store::{ActiveAlarm, AlarmSeverity, AlarmState, AlarmType};

    fn mk_alarm(sev: AlarmSeverity) -> ActiveAlarm {
        ActiveAlarm {
            config_id: 0,
            device_id: "d".into(),
            point_id: "p".into(),
            alarm_type: AlarmType::HighLimit,
            severity: sev,
            state: AlarmState::Offnormal,
            trigger_value: 0.0,
            trigger_time_ms: 0,
            ack_time_ms: None,
            context_snapshot: String::new(),
        }
    }

    #[test]
    fn alarm_counts_empty() {
        let c = AlarmCounts::from_local_alarms(&[]);
        assert_eq!(c.total(), 0);
    }

    #[test]
    fn alarm_counts_mixed() {
        let alarms = vec![
            mk_alarm(AlarmSeverity::Critical),
            mk_alarm(AlarmSeverity::Critical),
            mk_alarm(AlarmSeverity::Warning),
            mk_alarm(AlarmSeverity::Info),
            mk_alarm(AlarmSeverity::LifeSafety),
        ];
        let c = AlarmCounts::from_local_alarms(&alarms);
        assert_eq!(c.critical, 2);
        assert_eq!(c.warning, 1);
        assert_eq!(c.info, 1);
        assert_eq!(c.life_safety, 1);
        assert_eq!(c.total(), 5);
    }
}

#[component]
pub fn SiteStatusDashboard(
    handle: SupervisorHandle,
    on_open_site: EventHandler<String>,
) -> Element {
    let sites = use_hook(|| handle.sites.clone());

    rsx! {
        div { class: "site-status-dashboard",
            div { class: "site-status-header",
                h2 { "Supervisor Dashboard" }
                p { class: "text-muted",
                    "{sites.len()} sites loaded. Click a card to open that site."
                }
            }
            div { class: "site-status-grid",
                for variant in sites.iter() {
                    {
                        match variant {
                            LoadedSiteVariant::Local(site) => rsx! {
                                LocalSiteCard {
                                    key: "{site.paths.root.display()}",
                                    site: site.clone(),
                                    on_open: move |id: String| on_open_site.call(id),
                                }
                            },
                            LoadedSiteVariant::Remote(remote) => rsx! {
                                RemoteSiteCard {
                                    key: "remote::{remote.config_id}",
                                    remote: remote.clone(),
                                    on_open: move |id: String| on_open_site.call(id),
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn LocalSiteCard(site: LoadedSite, on_open: EventHandler<String>) -> Element {
    let alarm_store = use_hook(|| site.platform.alarm_store.clone());
    let display_key = use_hook(|| site.paths.root.display().to_string());
    let counts_res = use_resource(move || {
        let store = alarm_store.clone();
        async move { fetch_local_counts(store).await }
    });

    let counts = counts_res.read().clone().unwrap_or_default();

    let pill_class = if counts.critical > 0 || counts.life_safety > 0 {
        "status-pill status-pill-danger"
    } else if counts.warning > 0 {
        "status-pill status-pill-warning"
    } else {
        "status-pill status-pill-ok"
    };

    let bridge_failures = site.bridge_report.failures();
    let display_key_for_click = display_key.clone();
    let display_key_for_button = display_key.clone();

    rsx! {
        div {
            class: "site-status-card",
            onclick: move |_| on_open.call(display_key_for_click.clone()),

            div { class: "site-status-card-header",
                h3 { "{site.meta.name}" }
                span { class: "{pill_class}",
                    if counts.critical > 0 || counts.life_safety > 0 {
                        "ALARMS"
                    } else if counts.warning > 0 {
                        "WARNINGS"
                    } else {
                        "OK"
                    }
                }
            }

            div { class: "site-status-card-body",
                if counts.total() > 0 {
                    div { class: "site-alarm-counts",
                        if counts.critical > 0 {
                            span { class: "alarm-count alarm-count-critical",
                                "{counts.critical} critical"
                            }
                        }
                        if counts.life_safety > 0 {
                            span { class: "alarm-count alarm-count-life-safety",
                                "{counts.life_safety} life safety"
                            }
                        }
                        if counts.warning > 0 {
                            span { class: "alarm-count alarm-count-warning",
                                "{counts.warning} warning"
                            }
                        }
                        if counts.info > 0 {
                            span { class: "alarm-count alarm-count-info",
                                "{counts.info} info"
                            }
                        }
                    }
                } else {
                    div { class: "text-muted", "No active alarms" }
                }

                if !bridge_failures.is_empty() {
                    div { class: "site-bridge-failures",
                        strong { "Bridge issues:" }
                        ul {
                            for (label, err) in bridge_failures.iter() {
                                li { class: "text-danger", "{label}: {err}" }
                            }
                        }
                    }
                }
            }

            div { class: "site-status-card-footer",
                button {
                    class: "btn btn-sm btn-primary",
                    onclick: move |e| {
                        e.stop_propagation();
                        on_open.call(display_key_for_button.clone());
                    },
                    "Open site"
                }
                span { class: "text-muted text-xs",
                    "{display_key}"
                }
            }
        }
    }
}

#[component]
fn RemoteSiteCard(remote: RemoteLoadedSite, on_open: EventHandler<String>) -> Element {
    let client_for_counts = remote.client.clone();
    let counts_res = use_resource(move || {
        let client = client_for_counts.clone();
        async move { fetch_remote_counts(client).await }
    });

    let counts_outcome = counts_res.read().clone();
    let counts: AlarmCounts = match &counts_outcome {
        Some(Ok(c)) => c.clone(),
        _ => AlarmCounts::default(),
    };
    let counts_error = match &counts_outcome {
        Some(Err(e)) => Some(e.clone()),
        _ => None,
    };

    let status_snapshot = remote.status.read().clone();
    let status_pill_class = match (&status_snapshot, counts_error.is_some()) {
        (RemoteSiteStatus::Unreachable { .. }, _) => "status-pill status-pill-danger",
        (_, true) => "status-pill status-pill-danger",
        (RemoteSiteStatus::Degraded { .. }, _) => "status-pill status-pill-warning",
        _ if counts.critical > 0 || counts.life_safety > 0 => "status-pill status-pill-danger",
        _ if counts.warning > 0 => "status-pill status-pill-warning",
        _ => "status-pill status-pill-ok",
    };
    let status_text = match &status_snapshot {
        RemoteSiteStatus::Connected => {
            if counts.critical > 0 || counts.life_safety > 0 {
                "ALARMS"
            } else if counts.warning > 0 {
                "WARNINGS"
            } else {
                "OK"
            }
        }
        RemoteSiteStatus::Degraded { .. } => "DEGRADED",
        RemoteSiteStatus::Unreachable { .. } => "UNREACHABLE",
    };

    let display_key = format!("remote::{}", remote.config_id);
    let display_key_for_click = display_key.clone();
    let display_key_for_button = display_key.clone();

    rsx! {
        div {
            class: "site-status-card site-status-card-remote",
            onclick: move |_| on_open.call(display_key_for_click.clone()),

            div { class: "site-status-card-header",
                h3 {
                    span { class: "remote-glyph", "🌐 " }
                    "{remote.name}"
                }
                span { class: "{status_pill_class}", "{status_text}" }
            }

            div { class: "site-status-card-body",
                if let Some(err) = counts_error.as_ref() {
                    div { class: "text-danger", "⚠ {err}" }
                } else if counts.total() > 0 {
                    div { class: "site-alarm-counts",
                        if counts.critical > 0 {
                            span { class: "alarm-count alarm-count-critical",
                                "{counts.critical} critical"
                            }
                        }
                        if counts.life_safety > 0 {
                            span { class: "alarm-count alarm-count-life-safety",
                                "{counts.life_safety} life safety"
                            }
                        }
                        if counts.warning > 0 {
                            span { class: "alarm-count alarm-count-warning",
                                "{counts.warning} warning"
                            }
                        }
                        if counts.info > 0 {
                            span { class: "alarm-count alarm-count-info",
                                "{counts.info} info"
                            }
                        }
                    }
                } else {
                    div { class: "text-muted", "No active alarms" }
                }

                if let RemoteSiteStatus::Unreachable { reason, .. } = &status_snapshot {
                    div { class: "site-bridge-failures",
                        strong { "Connection lost:" }
                        p { class: "text-danger", "{reason}" }
                    }
                } else if let RemoteSiteStatus::Degraded { reason } = &status_snapshot {
                    div { class: "text-muted", "{reason}" }
                }
            }

            div { class: "site-status-card-footer",
                button {
                    class: "btn btn-sm btn-primary",
                    onclick: move |e| {
                        e.stop_propagation();
                        on_open.call(display_key_for_button.clone());
                    },
                    "Open site"
                }
                span { class: "text-muted text-xs", "{remote.base_url}" }
            }
        }
    }
}
