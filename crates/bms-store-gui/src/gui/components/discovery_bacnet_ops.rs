use dioxus::prelude::*;

use bms_store_bridges::bridge::bacnet::BacnetNetworks;
use crate::gui::state::AppState;

use super::discovery_utils::{event_state_label, extract_bacnet_instance};

pub(crate) fn render_bacnet_management(
    state: &AppState,
    detail_dev_id: &Option<String>,
    mut event_infos: Signal<Vec<bms_store_bridges::bridge::bacnet::BacnetEventInfo>>,
    mut trend_logs: Signal<Vec<(u32, String)>>,
) -> Element {
    let mgmt_dev_id = detail_dev_id.clone().unwrap_or_default();
    let instance = extract_bacnet_instance(&mgmt_dev_id);
    let bridge_warmstart = state.bacnet_handle();
    let bridge_coldstart = state.bacnet_handle();
    let bridge_sync = state.bacnet_handle();
    let bridge_disable = state.bacnet_handle();
    let bridge_enable = state.bacnet_handle();
    let bridge_events = state.bacnet_handle();
    let bridge_trendlogs = state.bacnet_handle();
    rsx! {
        if let Some(inst) = instance {
            div { class: "discovery-mgmt-section",
                h4 { "Device Control" }
                div { class: "discovery-mgmt-grid",
                    button {
                        class: "discovery-mgmt-btn",
                        onclick: move |_| {
                            let bridge = bridge_sync.clone();
                            spawn(async move {
                                let guard = bridge.lock().await;
                                let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                                if let Some(b) = nets.bridge_for_device(inst) {
                                    if let Err(e) = b.sync_time(inst).await {
                                        eprintln!("Time sync failed: {e}");
                                    }
                                }
                            });
                        },
                        div { class: "discovery-mgmt-btn-icon", "🕐" }
                        span { "Sync Time" }
                    }
                    button {
                        class: "discovery-mgmt-btn",
                        onclick: move |_| {
                            let bridge = bridge_warmstart.clone();
                            spawn(async move {
                                let guard = bridge.lock().await;
                                let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                                if let Some(b) = nets.bridge_for_device(inst) {
                                    if let Err(e) = b.reinitialize_device(inst, true).await {
                                        eprintln!("Warmstart failed: {e}");
                                    }
                                }
                            });
                        },
                        div { class: "discovery-mgmt-btn-icon", "↻" }
                        span { "Warmstart" }
                    }
                    button {
                        class: "discovery-mgmt-btn warn",
                        onclick: move |_| {
                            let bridge = bridge_coldstart.clone();
                            spawn(async move {
                                let guard = bridge.lock().await;
                                let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                                if let Some(b) = nets.bridge_for_device(inst) {
                                    if let Err(e) = b.reinitialize_device(inst, false).await {
                                        eprintln!("Coldstart failed: {e}");
                                    }
                                }
                            });
                        },
                        div { class: "discovery-mgmt-btn-icon", "⚡" }
                        span { "Coldstart" }
                    }
                    button {
                        class: "discovery-mgmt-btn warn",
                        onclick: move |_| {
                            let bridge = bridge_disable.clone();
                            spawn(async move {
                                let guard = bridge.lock().await;
                                let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                                if let Some(b) = nets.bridge_for_device(inst) {
                                    if let Err(e) = b.device_communication_control(inst, false, Some(30)).await {
                                        eprintln!("Disable comm failed: {e}");
                                    }
                                }
                            });
                        },
                        div { class: "discovery-mgmt-btn-icon", "⏸" }
                        span { "Disable Comm" }
                    }
                    button {
                        class: "discovery-mgmt-btn",
                        onclick: move |_| {
                            let bridge = bridge_enable.clone();
                            spawn(async move {
                                let guard = bridge.lock().await;
                                let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                                if let Some(b) = nets.bridge_for_device(inst) {
                                    if let Err(e) = b.device_communication_control(inst, true, None).await {
                                        eprintln!("Enable comm failed: {e}");
                                    }
                                }
                            });
                        },
                        div { class: "discovery-mgmt-btn-icon", "▶" }
                        span { "Enable Comm" }
                    }
                }
            }

            // Events section
            div { class: "discovery-mgmt-section",
                h4 { "Event Information" }
                button {
                    class: "discovery-action-btn",
                    onclick: move |_| {
                        let bridge = bridge_events.clone();
                        spawn(async move {
                            let guard = bridge.lock().await;
                            let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                            if let Some(b) = nets.bridge_for_device(inst) {
                                match b.get_event_info(inst).await {
                                    Ok(events) => event_infos.set(events),
                                    Err(e) => eprintln!("GetEventInfo failed: {e}"),
                                }
                            }
                        });
                    },
                    "Fetch Events"
                }
                if !event_infos.read().is_empty() {
                    table { class: "discovery-point-table",
                        thead {
                            tr {
                                th { "Object" }
                                th { "State" }
                                th { "Ack" }
                            }
                        }
                        tbody {
                            for ev in event_infos.read().iter() {
                                tr {
                                    td { "{ev.object_id.object_type()}-{ev.object_id.instance()}" }
                                    td { "{event_state_label(ev.event_state)}" }
                                    td {
                                        if let Some(ref bits) = ev.acknowledged_transitions {
                                            if bits.is_empty() || bits[0] == 0 { "Unacked" } else { "Acked" }
                                        } else {
                                            "Unknown"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // TrendLog backfill section
            div { class: "discovery-mgmt-section",
                h4 { "Trend Log Backfill" }
                button {
                    class: "discovery-action-btn",
                    onclick: move |_| {
                        let bridge = bridge_trendlogs.clone();
                        spawn(async move {
                            let guard = bridge.lock().await;
                            let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                            if let Some(b) = nets.bridge_for_device(inst) {
                                let tls: Vec<(u32, String)> = b.discovered_devices()
                                    .iter()
                                    .filter(|d| d.device_id.instance() == inst)
                                    .flat_map(|d| d.trend_logs.iter())
                                    .map(|tl| (tl.object_id.instance(), tl.object_name.clone().unwrap_or_else(|| format!("TrendLog-{}", tl.object_id.instance()))))
                                    .collect();
                                trend_logs.set(tls);
                            }
                        });
                    },
                    "Load TrendLogs"
                }
                if !trend_logs.read().is_empty() {
                    table { class: "discovery-point-table",
                        thead {
                            tr {
                                th { "Name" }
                                th { "Instance" }
                                th { "" }
                            }
                        }
                        tbody {
                            for (tl_inst, tl_name) in trend_logs.read().iter() {
                                {
                                    let backfill_bridge = state.bacnet_handle();
                                    let backfill_history = state.history_store.clone();
                                    let dev_key_tl = mgmt_dev_id.clone();
                                    let tl_i = *tl_inst;
                                    let tl_n = tl_name.clone();
                                    rsx! {
                                        tr {
                                            td { "{tl_name}" }
                                            td { "{tl_inst}" }
                                            td {
                                                button {
                                                    class: "discovery-action-btn",
                                                    onclick: move |_| {
                                                        let bridge = backfill_bridge.clone();
                                                        let history: std::sync::Arc<dyn bms_store::plugin::HistoryBackend> =
                                                            std::sync::Arc::new(backfill_history.clone());
                                                        let dk = dev_key_tl.clone();
                                                        let tn = tl_n.clone();
                                                        spawn(async move {
                                                            let guard = bridge.lock().await;
                                                            let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                                                            if let Some(inst) = dk.strip_prefix("bacnet-").and_then(|s| s.parse::<u32>().ok()) {
                                                                if let Some(b) = nets.bridge_for_device(inst) {
                                                                    match b.backfill_trend_log(inst, tl_i, &dk, &tn, &history).await {
                                                                        Ok(n) => println!("Backfilled {n} records from TrendLog-{tl_i}"),
                                                                        Err(e) => eprintln!("Backfill failed: {e}"),
                                                                    }
                                                                }
                                                            }
                                                        });
                                                    },
                                                    "Backfill"
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
        }
    }
}

pub(crate) fn render_bacnet_objects(
    state: &AppState,
    inst: u32,
    mut create_object_type: Signal<String>,
    mut delete_object_input: Signal<String>,
    commission_status: Signal<Option<String>>,
) -> Element {
    let bridge_create = state.bacnet_handle();
    let bridge_delete = state.bacnet_handle();
    let mut cs_create = commission_status;
    let mut cs_delete = commission_status;
    let mut cs_delete_err = commission_status;
    rsx! {
        div { class: "discovery-commission-panel",
            div { class: "discovery-mgmt-section",
                h4 { "Create Object" }
                div { class: "discovery-form-row",
                    select {
                        class: "discovery-input",
                        value: "{create_object_type}",
                        onchange: move |e| create_object_type.set(e.value()),
                        option { value: "AnalogValue", "Analog Value" }
                        option { value: "BinaryValue", "Binary Value" }
                        option { value: "MultiStateValue", "Multi-State Value" }
                        option { value: "AnalogInput", "Analog Input" }
                        option { value: "AnalogOutput", "Analog Output" }
                    }
                    button {
                        class: "discovery-action-btn",
                        onclick: move |_| {
                            let bridge = bridge_create.clone();
                            let obj_type_str = create_object_type.read().clone();
                            spawn(async move {
                                let obj_type = match obj_type_str.as_str() {
                                    "AnalogValue" => rustbac_core::types::ObjectType::AnalogValue,
                                    "BinaryValue" => rustbac_core::types::ObjectType::BinaryValue,
                                    "MultiStateValue" => rustbac_core::types::ObjectType::MultiStateValue,
                                    "AnalogInput" => rustbac_core::types::ObjectType::AnalogInput,
                                    "AnalogOutput" => rustbac_core::types::ObjectType::AnalogOutput,
                                    _ => return,
                                };
                                let guard = bridge.lock().await;
                                let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                                if let Some(b) = nets.bridge_for_device(inst) {
                                    match b.create_object(inst, obj_type).await {
                                        Ok(created_id) => {
                                            cs_create.set(Some(format!("Created: {}-{}", created_id.object_type(), created_id.instance())));
                                        }
                                        Err(e) => cs_create.set(Some(format!("Create failed: {e}"))),
                                    }
                                }
                            });
                        },
                        "Create Object"
                    }
                }
            }
            div { class: "discovery-mgmt-section",
                h4 { "Delete Object" }
                div { class: "discovery-form-row",
                    input {
                        r#type: "text",
                        class: "discovery-input",
                        placeholder: "Object instance to delete",
                        value: "{delete_object_input}",
                        oninput: move |e| delete_object_input.set(e.value()),
                    }
                    button {
                        class: "discovery-action-btn warn",
                        onclick: move |_| {
                            let bridge = bridge_delete.clone();
                            let obj_inst_str = delete_object_input.read().clone();
                            let obj_type_str = create_object_type.read().clone();
                            spawn(async move {
                                let obj_inst: u32 = match obj_inst_str.parse() {
                                    Ok(v) => v,
                                    Err(_) => { cs_delete_err.set(Some("Invalid instance".into())); return; }
                                };
                                let obj_type = match obj_type_str.as_str() {
                                    "AnalogValue" => rustbac_core::types::ObjectType::AnalogValue,
                                    "BinaryValue" => rustbac_core::types::ObjectType::BinaryValue,
                                    "MultiStateValue" => rustbac_core::types::ObjectType::MultiStateValue,
                                    "AnalogInput" => rustbac_core::types::ObjectType::AnalogInput,
                                    "AnalogOutput" => rustbac_core::types::ObjectType::AnalogOutput,
                                    _ => return,
                                };
                                let object_id = rustbac_core::types::ObjectId::new(obj_type, obj_inst);
                                let guard = bridge.lock().await;
                                let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                                if let Some(b) = nets.bridge_for_device(inst) {
                                    match b.delete_object(inst, object_id).await {
                                        Ok(()) => cs_delete.set(Some(format!("Deleted {obj_type_str}-{obj_inst}"))),
                                        Err(e) => cs_delete.set(Some(format!("Delete failed: {e}"))),
                                    }
                                }
                            });
                        },
                        "Delete Object"
                    }
                }
            }
            if let Some(ref status) = *commission_status.read() {
                div { class: "discovery-status-msg", "{status}" }
            }
        }
    }
}
