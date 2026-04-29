use dioxus::prelude::*;

use crate::bridge::bacnet::BacnetNetworks;
use crate::config::profile::{PointKind, PointValue};
use crate::gui::state::{AppState, WriteCommand};

/// BACnet priority level labels.
const PRIORITY_LABELS: &[&str] = &[
    "Manual Life Safety", // 1
    "Auto Life Safety",   // 2
    "(Available)",        // 3
    "(Available)",        // 4
    "Critical Equipment", // 5
    "Min On/Off",         // 6
    "(Available)",        // 7
    "Manual Operator",    // 8
    "(Available)",        // 9
    "(Available)",        // 10
    "(Available)",        // 11
    "(Available)",        // 12
    "(Available)",        // 13
    "(Available)",        // 14
    "(Available)",        // 15
    "(Available)",        // 16
];

#[component]
pub fn WriteDialog(device_id: String, point_id: String) -> Element {
    let state = use_context::<AppState>();
    let mut input_value = use_signal(|| String::new());
    let mut local_error = use_signal(|| Option::<String>::None);
    let mut selected_priority = use_signal(|| 16u8);

    // Find the point definition to know kind + constraints
    let profile_point = state
        .loaded
        .devices
        .iter()
        .find(|d| d.instance_id == *device_id)
        .and_then(|d| d.profile.points.iter().find(|p| p.id == *point_id));

    let kind = profile_point
        .map(|p| p.kind.clone())
        .unwrap_or(PointKind::Analog);
    let constraints = profile_point.and_then(|p| p.constraints.clone());
    let profile_priority =
        profile_point.and_then(|p| p.protocols.as_ref()?.bacnet.as_ref()?.priority);
    // Initialize selected_priority from profile on first render only
    let mut priority_initialized = use_signal(|| false);
    use_effect(move || {
        if !*priority_initialized.read() {
            if let Some(p) = profile_priority {
                selected_priority.set(p);
            }
            priority_initialized.set(true);
        }
    });

    // Clone what we need for the closure
    let constraints_for_display = constraints.clone();
    let kind_for_placeholder = kind.clone();

    let dev_id = device_id.clone();
    let pt_id = point_id.clone();
    let write_tx = state.write_tx.clone();

    let mut on_submit = {
        let kind = kind.clone();
        let constraints = constraints.clone();
        let dev_id = dev_id.clone();
        let pt_id = pt_id.clone();
        let write_tx = write_tx.clone();
        move || {
            let raw = input_value.read().trim().to_string();
            if raw.is_empty() {
                local_error.set(Some("Enter a value.".into()));
                return;
            }

            let parsed = match kind {
                PointKind::Binary => match raw.to_lowercase().as_str() {
                    "true" | "on" | "1" => Ok(PointValue::Bool(true)),
                    "false" | "off" | "0" => Ok(PointValue::Bool(false)),
                    _ => Err("Expected: true/false, on/off, or 1/0".into()),
                },
                PointKind::Multistate => raw
                    .parse::<i64>()
                    .map(PointValue::Integer)
                    .map_err(|_| "Expected an integer.".to_string()),
                PointKind::Analog => raw
                    .parse::<f64>()
                    .map(PointValue::Float)
                    .map_err(|_| "Expected a number.".to_string()),
            };

            match parsed {
                Err(e) => {
                    local_error.set(Some(e));
                }
                Ok(value) => {
                    if let Some(ref c) = constraints {
                        if let Some(min) = c.min {
                            if value.as_f64() < min {
                                local_error.set(Some(format!("Value must be >= {min}")));
                                return;
                            }
                        }
                        if let Some(max) = c.max {
                            if value.as_f64() > max {
                                local_error.set(Some(format!("Value must be <= {max}")));
                                return;
                            }
                        }
                    }

                    let cmd = WriteCommand {
                        device_id: dev_id.clone(),
                        point_id: pt_id.clone(),
                        value,
                        priority: Some(*selected_priority.read()),
                    };
                    if write_tx.send(cmd).is_err() {
                        local_error.set(Some("Write channel closed.".into()));
                    } else {
                        local_error.set(None);
                        input_value.set(String::new());
                    }
                }
            }
        }
    };

    let placeholder = match kind_for_placeholder {
        PointKind::Binary => "true / false",
        PointKind::Multistate => "state number",
        PointKind::Analog => "number",
    };

    let error_text = local_error
        .read()
        .clone()
        .or_else(|| state.write_error.read().clone());

    let mut on_submit_key = on_submit.clone();

    rsx! {
        div { class: "write-dialog",
            h4 { "Write Value" }
            div { class: "write-form",
                input {
                    r#type: "text",
                    placeholder: "{placeholder}",
                    value: "{input_value}",
                    oninput: move |e| input_value.set(e.value()),
                    onkeypress: move |e| {
                        if e.key() == Key::Enter {
                            on_submit_key();
                        }
                    },
                }
                select {
                    class: "write-priority-select",
                    value: "{selected_priority}",
                    onchange: move |evt: Event<FormData>| {
                        if let Ok(p) = evt.value().parse::<u8>() {
                            selected_priority.set(p);
                        }
                    },
                    option { value: "16", "16 — Default" }
                    option { value: "15", "15" }
                    option { value: "14", "14" }
                    option { value: "13", "13" }
                    option { value: "12", "12" }
                    option { value: "11", "11" }
                    option { value: "10", "10" }
                    option { value: "9", "9" }
                    option { value: "8", "8 — Manual Operator" }
                    option { value: "7", "7" }
                    option { value: "6", "6" }
                    option { value: "5", "5 — Critical Equipment" }
                    option { value: "4", "4" }
                    option { value: "3", "3" }
                    option { value: "2", "2 — Auto Life Safety" }
                    option { value: "1", "1 — Manual Life Safety" }
                }
                button {
                    onclick: move |_| on_submit(),
                    "Write"
                }
            }
            if let Some(ref c) = constraints_for_display {
                div { class: "write-constraints",
                    if let Some(min) = c.min {
                        span { "Min: {min}" }
                    }
                    if let Some(max) = c.max {
                        span { "Max: {max}" }
                    }
                }
            }
            // C3: Priority Array (BACnet writable points only)
            {
                let bacnet_info = profile_point.and_then(|p| {
                    let b = p.protocols.as_ref()?.bacnet.as_ref()?;
                    let obj_type = match b.object_type {
                        crate::config::profile::BacnetObjectType::AnalogOutput => Some(rustbac_core::types::ObjectType::AnalogOutput),
                        crate::config::profile::BacnetObjectType::AnalogValue => Some(rustbac_core::types::ObjectType::AnalogValue),
                        crate::config::profile::BacnetObjectType::BinaryOutput => Some(rustbac_core::types::ObjectType::BinaryOutput),
                        crate::config::profile::BacnetObjectType::BinaryValue => Some(rustbac_core::types::ObjectType::BinaryValue),
                        crate::config::profile::BacnetObjectType::MultistateOutput => Some(rustbac_core::types::ObjectType::MultiStateOutput),
                        crate::config::profile::BacnetObjectType::MultistateValue => Some(rustbac_core::types::ObjectType::MultiStateValue),
                        _ => None,
                    };
                    obj_type.map(|ot| (ot, b.instance))
                });
                let dev_instance: Option<u32> = device_id.parse().ok().or_else(|| {
                    // Handle both "bacnet-1000" and "bacnet-{network}-1000"
                    if !device_id.starts_with("bacnet-") { return None; }
                    device_id.rsplit('-').next().and_then(|s| s.parse().ok())
                });
                if let (Some((obj_type, obj_inst)), Some(dev_inst)) = (bacnet_info, dev_instance) {
                    let bridge_handle = state.bacnet_handle();
                    let object_id = rustbac_core::types::ObjectId::new(obj_type, obj_inst);
                    let mut pa_expanded = use_signal(|| false);
                    let mut pa_loading = use_signal(|| false);
                    let mut pa_data: Signal<Option<crate::bridge::bacnet::PriorityArrayInfo>> = use_signal(|| None);
                    let mut pa_error: Signal<Option<String>> = use_signal(|| None);
                    let mut relinquish_default: Signal<Option<String>> = use_signal(|| None);

                    rsx! {
                        div { class: "write-priority-array",
                            button {
                                class: "write-pa-toggle",
                                onclick: {
                                    let bridge_handle = bridge_handle.clone();
                                    move |_| {
                                        let is_expanded = *pa_expanded.read();
                                        pa_expanded.set(!is_expanded);
                                        if !is_expanded && pa_data.read().is_none() {
                                            // Load on first expand
                                            let bridge_handle = bridge_handle.clone();
                                            pa_loading.set(true);
                                            pa_error.set(None);
                                            spawn(async move {
                                                let guard = bridge_handle.lock().await;
                                                let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                                                if let Some(b) = nets.bridge_for_device(dev_inst) {
                                                    match b.read_priority_array(dev_inst, object_id).await {
                                                        Ok(info) => {
                                                            let rd = info.relinquish_default.as_ref().map(|v| format!("{v:?}"));
                                                            relinquish_default.set(rd);
                                                            pa_data.set(Some(info));
                                                        }
                                                        Err(e) => pa_error.set(Some(format!("{e}"))),
                                                    }
                                                }
                                                pa_loading.set(false);
                                            });
                                        }
                                    }
                                },
                                if *pa_expanded.read() { "▾ Priority Array" } else { "▸ Priority Array" }
                            }
                            if *pa_expanded.read() {
                                if *pa_loading.read() {
                                    div { class: "write-pa-loading", "Loading..." }
                                }
                                if let Some(ref err) = *pa_error.read() {
                                    div { class: "write-error", "{err}" }
                                }
                                if let Some(ref info) = *pa_data.read() {
                                    table { class: "write-pa-table",
                                        thead {
                                            tr {
                                                th { "Level" }
                                                th { "Label" }
                                                th { "Value" }
                                            }
                                        }
                                        tbody {
                                            for (i, val) in info.levels.iter().enumerate() {
                                                {
                                                    let level = i + 1;
                                                    let label = PRIORITY_LABELS.get(i).unwrap_or(&"");
                                                    let value_str = match val {
                                                        Some(v) => format!("{v:?}"),
                                                        None => "-- (Relinquished)".to_string(),
                                                    };
                                                    let row_class = if val.is_some() { "write-pa-row occupied" } else { "write-pa-row" };
                                                    rsx! {
                                                        tr { class: row_class,
                                                            td { "{level}" }
                                                            td { class: "text-muted", "{label}" }
                                                            td { "{value_str}" }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    if let Some(ref rd) = *relinquish_default.read() {
                                        div { class: "write-pa-relinquish",
                                            "Relinquish Default: {rd}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    rsx! {}
                }
            }

            if let Some(err) = error_text {
                div { class: "write-error", "{err}" }
            }
        }
    }
}
