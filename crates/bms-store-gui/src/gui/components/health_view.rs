//! System health dashboard.
//!
//! Shows per-subsystem status from the `HealthRegistry`.
//! Auto-refreshes every 5 seconds using a polling `use_resource` keyed on a tick counter.

use dioxus::prelude::*;

use crate::gui::state::AppState;
use bms_store_storage::health::HealthStatus;

#[component]
pub fn HealthView() -> Element {
    let state = use_context::<AppState>();
    let health = state.health.clone();

    // Tick counter incremented every 5s to drive re-polling.
    let tick = use_signal(|| 0u64);
    let tick_val = *tick.read();

    // Background task: increment tick every 5 seconds.
    use_future(move || {
        let mut t = tick;
        async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                let next = t.read().wrapping_add(1);
                t.set(next);
            }
        }
    });

    // Snapshot health on each tick.
    let snapshot = use_resource(move || {
        let reg = health.clone();
        let _t = tick_val;
        async move { reg.snapshot() }
    });

    let entries = snapshot.read();
    let entries = entries.as_deref().unwrap_or(&[]);

    let overall_ok = entries.iter().all(|(_, s)| matches!(s, HealthStatus::Healthy));
    let overall_class = if entries.is_empty() {
        "health-overall health-ok"
    } else if overall_ok {
        "health-overall health-ok"
    } else {
        "health-overall health-degraded"
    };

    rsx! {
        div { class: "health-section",
            div { class: "health-header",
                h2 { "System Health" }
                span { class: overall_class,
                    if entries.is_empty() {
                        "No subsystems registered"
                    } else if overall_ok {
                        "All systems healthy"
                    } else {
                        "One or more subsystems degraded"
                    }
                }
                span { class: "health-refresh-hint", "Auto-refreshes every 5s" }
            }

            if entries.is_empty() {
                div { class: "health-empty",
                    p { "No subsystems have reported health status yet." }
                    p { class: "health-empty-hint", "Subsystems register automatically when the platform boots." }
                }
            } else {
                div { class: "health-card-grid",
                    for (name, status) in entries {
                        {
                            let badge_class = match status {
                                HealthStatus::Healthy => "health-badge health-badge-ok",
                                HealthStatus::Degraded(_) => "health-badge health-badge-warn",
                                HealthStatus::Down(_) => "health-badge health-badge-down",
                                HealthStatus::Unknown => "health-badge health-badge-unknown",
                            };
                            let badge_label = match status {
                                HealthStatus::Healthy => "Healthy",
                                HealthStatus::Degraded(_) => "Degraded",
                                HealthStatus::Down(_) => "Down",
                                HealthStatus::Unknown => "Unknown",
                            };
                            let detail = match status {
                                HealthStatus::Healthy | HealthStatus::Unknown => None,
                                HealthStatus::Degraded(msg) => Some(msg.clone()),
                                HealthStatus::Down(msg) => Some(msg.clone()),
                            };
                            let name = name.clone();
                            rsx! {
                                div { class: "health-card",
                                    div { class: "health-card-header",
                                        span { class: "health-card-name", "{name}" }
                                        span { class: badge_class, "{badge_label}" }
                                    }
                                    if let Some(msg) = detail {
                                        div { class: "health-card-detail", "{msg}" }
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
