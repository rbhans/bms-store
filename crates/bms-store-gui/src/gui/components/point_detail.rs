use dioxus::prelude::*;

use crate::auth::Permission;
use crate::config::profile::PointAccess;
use crate::gui::state::AppState;

use super::alarm_view::PointAlarmSection;
use super::trend_chart::navigate_to_trend;
use super::write_dialog::WriteDialog;

#[component]
pub fn PointDetail() -> Element {
    let state = use_context::<AppState>();

    let _version = state.store_version.read();
    let selected_device = state.selected_device.read().clone();
    let selected_point = state.selected_point.read().clone();

    let (Some(device_id), Some(point_id)) = (selected_device, selected_point) else {
        return rsx! {
            div { class: "point-detail-body",
                p { class: "placeholder", "Select a point to view details." }
            }
        };
    };

    let profile_point = state
        .loaded
        .devices
        .iter()
        .find(|d| d.instance_id == device_id)
        .and_then(|d| d.profile.points.iter().find(|p| p.id == point_id));

    let live_value = state.store.get(&crate::store::point_store::PointKey {
        device_instance_id: device_id.clone(),
        point_id: point_id.clone(),
    });

    let is_writable = profile_point
        .map(|p| !matches!(p.access, PointAccess::Input))
        .unwrap_or(false);

    // All points have history unless explicitly excluded
    let is_trended = profile_point.map(|p| !p.history_exclude).unwrap_or(true);

    rsx! {
        div { class: "point-detail-body",
            h4 { class: "detail-point-name", "{point_id}" }

            if let Some(tv) = &live_value {
                if !tv.status.is_normal() {
                    div { class: "status-badges",
                        for flag in tv.status.active_flags() {
                            span {
                                class: "status-badge status-{flag}",
                                "{flag}"
                            }
                        }
                    }
                }
            }

            if let Some(pt) = profile_point {
                dl { class: "detail-grid",
                    dt { "Name" }
                    dd { "{pt.name}" }

                    if let Some(desc) = &pt.description {
                        dt { "Description" }
                        dd { "{desc}" }
                    }

                    dt { "Kind" }
                    dd { "{pt.kind:?}" }

                    dt { "Access" }
                    dd { "{pt.access:?}" }

                    if let Some(units) = &pt.units {
                        dt { "Units" }
                        dd { "{units}" }
                    }

                    if let Some(constraints) = &pt.constraints {
                        if let Some(min) = constraints.min {
                            dt { "Min" }
                            dd { "{min}" }
                        }
                        if let Some(max) = constraints.max {
                            dt { "Max" }
                            dd { "{max}" }
                        }
                        if let Some(states) = &constraints.states {
                            dt { "States" }
                            dd {
                                for (k, v) in states.iter() {
                                    span { class: "state-label", "{k}: {v}" }
                                }
                            }
                        }
                    }

                    if let Some(tv) = &live_value {
                        dt { "Current Value" }
                        dd { class: "live-value", "{tv.value:?}" }
                    }
                }
            } else {
                if let Some(tv) = &live_value {
                    dl { class: "detail-grid",
                        dt { "Current Value" }
                        dd { class: "live-value", "{tv.value:?}" }
                    }
                }
            }

            if is_writable && state.has_permission(Permission::WritePoints) {
                WriteDialog {
                    device_id: device_id.clone(),
                    point_id: point_id.clone(),
                }
            }

            PointAlarmSection {
                device_id: device_id.clone(),
                point_id: point_id.clone(),
            }

            if is_trended {
                {
                    let mut trend_state = use_context::<AppState>();
                    let dev = device_id.clone();
                    let pt = point_id.clone();
                    rsx! {
                        div { class: "trend-action",
                            button {
                                class: "trend-btn",
                                onclick: move |_| {
                                    navigate_to_trend(&mut trend_state, &dev, &pt);
                                },
                                "Trend"
                            }
                        }
                    }
                }
            }
        }
    }
}
