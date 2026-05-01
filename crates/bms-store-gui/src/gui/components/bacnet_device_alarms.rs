use dioxus::prelude::*;

use bms_store_bridges::bridge::bacnet::BacnetNetworks;
use crate::gui::state::AppState;
use rustbac_client::{AlarmSummaryItem, EnrollmentSummaryItem};

fn event_state_label(raw: u32) -> &'static str {
    match raw {
        0 => "Normal",
        1 => "Fault",
        2 => "Offnormal",
        3 => "High Limit",
        4 => "Low Limit",
        5 => "Life Safety",
        _ => "Unknown",
    }
}

fn ack_transitions_label(bits: &rustbac_client::ClientBitString) -> &'static str {
    if bits.data.is_empty() || bits.data[0] == 0 {
        "Unacked"
    } else {
        "Acked"
    }
}

#[component]
pub fn BacnetDeviceAlarms(device_instance: u32) -> Element {
    let state = use_context::<AppState>();
    let mut alarm_items: Signal<Vec<AlarmSummaryItem>> = use_signal(Vec::new);
    let mut alarm_error: Signal<Option<String>> = use_signal(|| None);
    let mut alarm_loading = use_signal(|| false);

    let mut enrollment_items: Signal<Vec<EnrollmentSummaryItem>> = use_signal(Vec::new);
    let mut enrollment_error: Signal<Option<String>> = use_signal(|| None);
    let mut enrollment_loading = use_signal(|| false);

    rsx! {
        div { class: "bacnet-device-alarms",
            // Alarm Summary
            div { class: "bacnet-tool-section",
                h4 { class: "bacnet-tool-title", "Alarm Summary" }
                div { class: "bacnet-tool-actions",
                    button {
                        class: "discovery-action-btn",
                        disabled: *alarm_loading.read(),
                        onclick: {
                            let bridge_handle = state.bacnet_handle();
                            move |_| {
                                let bridge_handle = bridge_handle.clone();
                                alarm_loading.set(true);
                                alarm_error.set(None);
                                spawn(async move {
                                    let guard = bridge_handle.lock().await;
                                    let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                                    if let Some(b) = nets.bridge_for_device(device_instance) {
                                        match b.get_alarm_summary(device_instance).await {
                                            Ok(items) => alarm_items.set(items),
                                            Err(e) => alarm_error.set(Some(format!("{e}"))),
                                        }
                                    }
                                    alarm_loading.set(false);
                                });
                            }
                        },
                        if *alarm_loading.read() { "Loading..." } else { "Get Alarm Summary" }
                    }
                }

                if let Some(ref err) = *alarm_error.read() {
                    div { class: "bacnet-tool-status error", "Error: {err}" }
                }

                if !alarm_items.read().is_empty() {
                    div { class: "bacnet-tool-result",
                        table { class: "discovery-point-table",
                            thead {
                                tr {
                                    th { "Object" }
                                    th { "State" }
                                    th { "Ack Transitions" }
                                }
                            }
                            tbody {
                                for item in alarm_items.read().iter() {
                                    tr {
                                        td { "{item.object_id.object_type()}-{item.object_id.instance()}" }
                                        td { "{event_state_label(item.alarm_state_raw)}" }
                                        td { "{ack_transitions_label(&item.acknowledged_transitions)}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Enrollment Summary
            div { class: "bacnet-tool-section",
                h4 { class: "bacnet-tool-title", "Enrollment Summary" }
                div { class: "bacnet-tool-actions",
                    button {
                        class: "discovery-action-btn",
                        disabled: *enrollment_loading.read(),
                        onclick: {
                            let bridge_handle = state.bacnet_handle();
                            move |_| {
                                let bridge_handle = bridge_handle.clone();
                                enrollment_loading.set(true);
                                enrollment_error.set(None);
                                spawn(async move {
                                    let guard = bridge_handle.lock().await;
                                    let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                                    if let Some(b) = nets.bridge_for_device(device_instance) {
                                        match b.get_enrollment_summary(device_instance).await {
                                            Ok(items) => enrollment_items.set(items),
                                            Err(e) => enrollment_error.set(Some(format!("{e}"))),
                                        }
                                    }
                                    enrollment_loading.set(false);
                                });
                            }
                        },
                        if *enrollment_loading.read() { "Loading..." } else { "Get Enrollment Summary" }
                    }
                }

                if let Some(ref err) = *enrollment_error.read() {
                    div { class: "bacnet-tool-status error", "Error: {err}" }
                }

                if !enrollment_items.read().is_empty() {
                    div { class: "bacnet-tool-result",
                        table { class: "discovery-point-table",
                            thead {
                                tr {
                                    th { "Object" }
                                    th { "Event Type" }
                                    th { "State" }
                                    th { "Priority" }
                                    th { "Notification Class" }
                                }
                            }
                            tbody {
                                for item in enrollment_items.read().iter() {
                                    tr {
                                        td { "{item.object_id.object_type()}-{item.object_id.instance()}" }
                                        td { "{item.event_type}" }
                                        td { "{event_state_label(item.event_state_raw)}" }
                                        td { "{item.priority}" }
                                        td { "{item.notification_class}" }
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
