use dioxus::prelude::*;

use crate::bridge::bacnet::BacnetNetworks;
use crate::gui::state::AppState;
use rustbac_core::types::{ObjectId, ObjectType, PropertyId};

/// Parse a hex string into bytes. Returns None on invalid input.
fn hex_to_bytes(hex: &str) -> Option<Vec<u8>> {
    let hex = hex.trim();
    if hex.is_empty() {
        return Some(vec![]);
    }
    if hex.len() % 2 != 0 {
        return None;
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    for i in (0..hex.len()).step_by(2) {
        let byte = u8::from_str_radix(&hex[i..i + 2], 16).ok()?;
        bytes.push(byte);
    }
    Some(bytes)
}

/// Format bytes as a hex string.
fn bytes_to_hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02X}")).collect()
}

/// Map a dropdown label to an ObjectType.
fn parse_object_type(label: &str) -> ObjectType {
    match label {
        "AnalogInput" => ObjectType::AnalogInput,
        "AnalogOutput" => ObjectType::AnalogOutput,
        "AnalogValue" => ObjectType::AnalogValue,
        "BinaryInput" => ObjectType::BinaryInput,
        "BinaryOutput" => ObjectType::BinaryOutput,
        "BinaryValue" => ObjectType::BinaryValue,
        "MultiStateInput" => ObjectType::MultiStateInput,
        "MultiStateOutput" => ObjectType::MultiStateOutput,
        "MultiStateValue" => ObjectType::MultiStateValue,
        _ => ObjectType::AnalogInput,
    }
}

const OBJECT_TYPE_OPTIONS: &[&str] = &[
    "AnalogInput",
    "AnalogOutput",
    "AnalogValue",
    "BinaryInput",
    "BinaryOutput",
    "BinaryValue",
    "MultiStateInput",
    "MultiStateOutput",
    "MultiStateValue",
];

const PROPERTY_OPTIONS: &[(&str, u32)] = &[
    ("PresentValue", 85),
    ("StatusFlags", 111),
    ("ObjectName", 77),
    ("Description", 28),
    ("OutOfService", 81),
    ("Reliability", 103),
];

#[component]
pub fn BacnetDeviceAdvanced(device_instance: u32) -> Element {
    let state = use_context::<AppState>();

    // COV Property Subscribe state
    let mut cov_obj_type: Signal<String> = use_signal(|| "AnalogInput".to_string());
    let mut cov_obj_instance: Signal<String> = use_signal(|| "0".to_string());
    let mut cov_property: Signal<String> = use_signal(|| "85".to_string());
    let mut cov_increment: Signal<String> = use_signal(String::new);
    let mut cov_lifetime: Signal<String> = use_signal(|| "300".to_string());
    let mut cov_status: Signal<Option<String>> = use_signal(|| None);
    let mut cov_loading = use_signal(|| false);

    // Private Transfer state
    let mut pt_vendor_id: Signal<String> = use_signal(|| "0".to_string());
    let mut pt_service_number: Signal<String> = use_signal(|| "0".to_string());
    let mut pt_params_hex: Signal<String> = use_signal(String::new);
    let mut pt_result: Signal<Option<String>> = use_signal(|| None);
    let mut pt_error: Signal<Option<String>> = use_signal(|| None);
    let mut pt_loading = use_signal(|| false);

    // C2: COV Health Snapshot
    let mut cov_health: Signal<Option<crate::bridge::bacnet::CovHealthSnapshot>> =
        use_signal(|| None);
    {
        let bridge_handle = state.bacnet_handle();
        use_effect(move || {
            let bridge_handle = bridge_handle.clone();
            spawn(async move {
                let guard = bridge_handle.lock().await;
                let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                if let Some(b) = nets.bridge_for_device(device_instance) {
                    cov_health.set(Some(b.cov_health()));
                }
            });
        });
    }

    rsx! {
        div { class: "bacnet-device-cov",
            // C2: Subscription Health — only render when there's actual data
            if let Some(ref health) = *cov_health.read() {
              if health.total > 0 || !health.subscriptions.is_empty() {
                div { class: "bacnet-tool-section",
                    h4 { class: "bacnet-tool-title",
                        "Subscription Health"
                        span { class: "bacnet-tool-badge", "{health.mode}" }
                    }
                    div { class: "bacnet-cov-summary",
                        span { class: "bacnet-cov-stat",
                            strong { "{health.active}" }
                            " active"
                        }
                        span { class: "bacnet-cov-stat",
                            strong { "{health.stale}" }
                            " stale"
                        }
                        span { class: "bacnet-cov-stat-total",
                            "{health.total} total"
                        }
                    }
                    if !health.subscriptions.is_empty() {
                        table { class: "discovery-point-table",
                            thead {
                                tr {
                                    th { "Object" }
                                    th { "Status" }
                                    th { "Last Update" }
                                    th { "Lifetime" }
                                }
                            }
                            tbody {
                                for sub in health.subscriptions.iter() {
                                    {
                                        let status_class = if sub.active { "bacnet-cov-active" } else { "bacnet-cov-stale" };
                                        let status_label = if sub.active { "Active" } else { "Stale" };
                                        let last_update = if sub.last_update_ms > 0 {
                                            let ago_secs = (std::time::SystemTime::now()
                                                .duration_since(std::time::UNIX_EPOCH)
                                                .unwrap_or_default()
                                                .as_millis() as i64 - sub.last_update_ms) / 1000;
                                            if ago_secs < 60 { format!("{ago_secs}s ago") }
                                            else if ago_secs < 3600 { format!("{}m ago", ago_secs / 60) }
                                            else { format!("{}h ago", ago_secs / 3600) }
                                        } else {
                                            "Never".to_string()
                                        };
                                        rsx! {
                                            tr {
                                                td { "{sub.object_id:?}" }
                                                td { span { class: status_class, "{status_label}" } }
                                                td { "{last_update}" }
                                                td { "{sub.lifetime_secs}s" }
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

            div { class: "bacnet-tool-section",
                h4 { class: "bacnet-tool-title", "COV Property Subscription" }

                div { class: "bacnet-tool-form",
                    select {
                        class: "discovery-input",
                        value: "{cov_obj_type}",
                        onchange: move |e| cov_obj_type.set(e.value()),
                        for opt in OBJECT_TYPE_OPTIONS.iter() {
                            option { value: "{opt}", "{opt}" }
                        }
                    }
                    input {
                        r#type: "text",
                        class: "discovery-input",
                        placeholder: "Object instance",
                        value: "{cov_obj_instance}",
                        oninput: move |e| cov_obj_instance.set(e.value()),
                    }
                }
                div { class: "bacnet-tool-form",
                    select {
                        class: "discovery-input",
                        value: "{cov_property}",
                        onchange: move |e| cov_property.set(e.value()),
                        for (label, val) in PROPERTY_OPTIONS.iter() {
                            option { value: "{val}", "{label} ({val})" }
                        }
                    }
                    input {
                        r#type: "text",
                        class: "discovery-input",
                        placeholder: "COV increment (optional)",
                        value: "{cov_increment}",
                        oninput: move |e| cov_increment.set(e.value()),
                    }
                    input {
                        r#type: "text",
                        class: "discovery-input",
                        placeholder: "Lifetime (seconds)",
                        value: "{cov_lifetime}",
                        oninput: move |e| cov_lifetime.set(e.value()),
                    }
                }
                div { class: "bacnet-tool-form",
                    button {
                        class: "discovery-action-btn",
                        disabled: *cov_loading.read(),
                        onclick: {
                            let bridge_handle = state.bacnet_handle();
                            move |_| {
                                let bridge_handle = bridge_handle.clone();
                                let obj_type_str = cov_obj_type.read().clone();
                                let obj_inst_str = cov_obj_instance.read().clone();
                                let prop_str = cov_property.read().clone();
                                let inc_str = cov_increment.read().clone();
                                let life_str = cov_lifetime.read().clone();
                                cov_loading.set(true);
                                cov_status.set(None);
                                spawn(async move {
                                    let obj_inst: u32 = match obj_inst_str.parse() {
                                        Ok(v) => v,
                                        Err(_) => {
                                            cov_status.set(Some("Invalid object instance".into()));
                                            cov_loading.set(false);
                                            return;
                                        }
                                    };
                                    let prop_val: u32 = match prop_str.parse() {
                                        Ok(v) => v,
                                        Err(_) => {
                                            cov_status.set(Some("Invalid property ID".into()));
                                            cov_loading.set(false);
                                            return;
                                        }
                                    };
                                    let lifetime: u32 = match life_str.parse() {
                                        Ok(v) => v,
                                        Err(_) => {
                                            cov_status.set(Some("Invalid lifetime".into()));
                                            cov_loading.set(false);
                                            return;
                                        }
                                    };
                                    let increment: Option<f32> = if inc_str.trim().is_empty() {
                                        None
                                    } else {
                                        match inc_str.parse() {
                                            Ok(v) => Some(v),
                                            Err(_) => {
                                                cov_status.set(Some("Invalid COV increment".into()));
                                                cov_loading.set(false);
                                                return;
                                            }
                                        }
                                    };
                                    let object_id = ObjectId::new(parse_object_type(&obj_type_str), obj_inst);
                                    let property_id = PropertyId::from_u32(prop_val);
                                    let guard = bridge_handle.lock().await;
                                    let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                                    if let Some(b) = nets.bridge_for_device(device_instance) {
                                        match b.subscribe_cov_property(device_instance, object_id, property_id, increment, lifetime).await {
                                            Ok(()) => cov_status.set(Some("Subscribed successfully".into())),
                                            Err(e) => cov_status.set(Some(format!("Subscribe failed: {e}"))),
                                        }
                                    }
                                    cov_loading.set(false);
                                });
                            }
                        },
                        if *cov_loading.read() { "Subscribing..." } else { "Subscribe" }
                    }
                    button {
                        class: "discovery-action-btn",
                        disabled: *cov_loading.read(),
                        onclick: {
                            let bridge_handle = state.bacnet_handle();
                            move |_| {
                                let bridge_handle = bridge_handle.clone();
                                let obj_type_str = cov_obj_type.read().clone();
                                let obj_inst_str = cov_obj_instance.read().clone();
                                let prop_str = cov_property.read().clone();
                                cov_loading.set(true);
                                cov_status.set(None);
                                spawn(async move {
                                    let obj_inst: u32 = match obj_inst_str.parse() {
                                        Ok(v) => v,
                                        Err(_) => {
                                            cov_status.set(Some("Invalid object instance".into()));
                                            cov_loading.set(false);
                                            return;
                                        }
                                    };
                                    let prop_val: u32 = match prop_str.parse() {
                                        Ok(v) => v,
                                        Err(_) => {
                                            cov_status.set(Some("Invalid property ID".into()));
                                            cov_loading.set(false);
                                            return;
                                        }
                                    };
                                    let object_id = ObjectId::new(parse_object_type(&obj_type_str), obj_inst);
                                    let property_id = PropertyId::from_u32(prop_val);
                                    let guard = bridge_handle.lock().await;
                                    let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                                    if let Some(b) = nets.bridge_for_device(device_instance) {
                                        match b.cancel_cov_property_subscription(device_instance, object_id, property_id).await {
                                            Ok(()) => cov_status.set(Some("Subscription cancelled".into())),
                                            Err(e) => cov_status.set(Some(format!("Cancel failed: {e}"))),
                                        }
                                    }
                                    cov_loading.set(false);
                                });
                            }
                        },
                        "Cancel"
                    }
                }

                if let Some(ref status) = *cov_status.read() {
                    div { class: "bacnet-tool-status", "{status}" }
                }
            }

            div { class: "bacnet-tool-section",
                h4 { class: "bacnet-tool-title", "Private Transfer" }

                div { class: "bacnet-tool-form",
                    input {
                        r#type: "text",
                        class: "discovery-input",
                        placeholder: "Vendor ID",
                        value: "{pt_vendor_id}",
                        oninput: move |e| pt_vendor_id.set(e.value()),
                    }
                    input {
                        r#type: "text",
                        class: "discovery-input",
                        placeholder: "Service number",
                        value: "{pt_service_number}",
                        oninput: move |e| pt_service_number.set(e.value()),
                    }
                }
                div { class: "bacnet-tool-form",
                    textarea {
                        class: "discovery-input",
                        placeholder: "Parameters hex (optional, e.g., 0102FF)",
                        rows: "2",
                        value: "{pt_params_hex}",
                        oninput: move |e| pt_params_hex.set(e.value()),
                    }
                }
                div { class: "bacnet-tool-form",
                    button {
                        class: "discovery-action-btn",
                        disabled: *pt_loading.read(),
                        onclick: {
                            let bridge_handle = state.bacnet_handle();
                            move |_| {
                                let bridge_handle = bridge_handle.clone();
                                let vid_str = pt_vendor_id.read().clone();
                                let sn_str = pt_service_number.read().clone();
                                let params_str = pt_params_hex.read().clone();
                                pt_loading.set(true);
                                pt_error.set(None);
                                pt_result.set(None);
                                spawn(async move {
                                    let vendor_id: u32 = match vid_str.parse() {
                                        Ok(v) => v,
                                        Err(_) => {
                                            pt_error.set(Some("Invalid vendor ID".into()));
                                            pt_loading.set(false);
                                            return;
                                        }
                                    };
                                    let service_number: u32 = match sn_str.parse() {
                                        Ok(v) => v,
                                        Err(_) => {
                                            pt_error.set(Some("Invalid service number".into()));
                                            pt_loading.set(false);
                                            return;
                                        }
                                    };
                                    let params_bytes = if params_str.trim().is_empty() {
                                        None
                                    } else {
                                        match hex_to_bytes(&params_str) {
                                            Some(b) => Some(b),
                                            None => {
                                                pt_error.set(Some("Invalid hex parameters".into()));
                                                pt_loading.set(false);
                                                return;
                                            }
                                        }
                                    };
                                    let guard = bridge_handle.lock().await;
                                    let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                                    if let Some(b) = nets.bridge_for_device(device_instance) {
                                        let params_ref = params_bytes.as_deref();
                                        match b.private_transfer(device_instance, vendor_id, service_number, params_ref).await {
                                            Ok((ack_vid, ack_sn, result_block)) => {
                                                let result_hex = result_block
                                                    .as_ref()
                                                    .map(|rb| bytes_to_hex(rb))
                                                    .unwrap_or_else(|| "(none)".to_string());
                                                pt_result.set(Some(format!(
                                                    "Vendor: {ack_vid}, Service: {ack_sn}, Result: {result_hex}"
                                                )));
                                            }
                                            Err(e) => pt_error.set(Some(format!("Private transfer failed: {e}"))),
                                        }
                                    }
                                    pt_loading.set(false);
                                });
                            }
                        },
                        if *pt_loading.read() { "Sending..." } else { "Send" }
                    }
                }

                if let Some(ref err) = *pt_error.read() {
                    div { class: "bacnet-tool-status", "Error: {err}" }
                }
                if let Some(ref result) = *pt_result.read() {
                    div { class: "bacnet-tool-status", "{result}" }
                }
            }
        }
    }
}
