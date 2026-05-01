use dioxus::prelude::*;
use std::net::SocketAddrV4;
use std::time::Duration;

use rustbac_client::{BroadcastDistributionEntry, DiscoveredObject, ForeignDeviceTableEntry};
use rustbac_core::types::{ObjectId, ObjectType};

use bms_store_bridges::bridge::bacnet::BacnetNetworks;
use crate::gui::state::AppState;

#[component]
pub fn BacnetNetworkTools() -> Element {
    // ── BBMD state ──
    let mut bdt_entries: Signal<Vec<BroadcastDistributionEntry>> = use_signal(Vec::new);
    let mut fdt_entries: Signal<Vec<ForeignDeviceTableEntry>> = use_signal(Vec::new);
    let mut delete_fdt_input: Signal<String> = use_signal(String::new);
    let mut bbmd_status: Signal<Option<String>> = use_signal(|| None);
    let mut bbmd_loading = use_signal(|| false);

    // ── Who-Has state ──
    let mut search_by_name = use_signal(|| true);
    let mut who_has_name_input: Signal<String> = use_signal(String::new);
    let mut who_has_type_input: Signal<String> = use_signal(|| "AnalogValue".to_string());
    let mut who_has_instance_input: Signal<String> = use_signal(String::new);
    let mut who_has_results: Signal<Vec<DiscoveredObject>> = use_signal(Vec::new);
    let mut who_has_status: Signal<Option<String>> = use_signal(|| None);
    let mut who_has_loading = use_signal(|| false);

    let state = use_context::<AppState>();

    // ── BBMD handlers ──
    let read_bdt = {
        let bridge_handle = state.bacnet_handle();
        move |_| {
            let bridge_handle = bridge_handle.clone();
            bbmd_loading.set(true);
            bbmd_status.set(None);
            spawn(async move {
                let guard = bridge_handle.lock().await;
                let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                if let Some(b) = nets.first() {
                    match b.read_bdt().await {
                        Ok(entries) => bdt_entries.set(entries),
                        Err(e) => bbmd_status.set(Some(format!("Read BDT error: {e}"))),
                    }
                } else {
                    bbmd_status.set(Some("BACnet bridge not available".into()));
                }
                bbmd_loading.set(false);
            });
        }
    };

    let read_fdt = {
        let bridge_handle = state.bacnet_handle();
        move |_| {
            let bridge_handle = bridge_handle.clone();
            bbmd_loading.set(true);
            bbmd_status.set(None);
            spawn(async move {
                let guard = bridge_handle.lock().await;
                let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                if let Some(b) = nets.first() {
                    match b.read_fdt().await {
                        Ok(entries) => fdt_entries.set(entries),
                        Err(e) => bbmd_status.set(Some(format!("Read FDT error: {e}"))),
                    }
                } else {
                    bbmd_status.set(Some("BACnet bridge not available".into()));
                }
                bbmd_loading.set(false);
            });
        }
    };

    let do_delete_fdt = {
        let bridge_handle = state.bacnet_handle();
        move |_| {
            let addr_str = delete_fdt_input.read().clone();
            let bridge_handle = bridge_handle.clone();
            bbmd_status.set(None);
            let addr: SocketAddrV4 = match addr_str.parse() {
                Ok(a) => a,
                Err(e) => {
                    bbmd_status.set(Some(format!("Invalid address: {e}")));
                    return;
                }
            };
            bbmd_loading.set(true);
            spawn(async move {
                let guard = bridge_handle.lock().await;
                let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                if let Some(b) = nets.first() {
                    match b.delete_fdt_entry(addr).await {
                        Ok(()) => bbmd_status.set(Some("FDT entry deleted".into())),
                        Err(e) => bbmd_status.set(Some(format!("Delete error: {e}"))),
                    }
                } else {
                    bbmd_status.set(Some("BACnet bridge not available".into()));
                }
                bbmd_loading.set(false);
            });
        }
    };

    // ── Who-Has handler ──
    let do_who_has = {
        let bridge_handle = state.bacnet_handle();
        move |_| {
            let bridge_handle = bridge_handle.clone();
            let by_name = *search_by_name.read();
            let name_val = who_has_name_input.read().clone();
            let type_val = who_has_type_input.read().clone();
            let instance_val = who_has_instance_input.read().clone();
            who_has_status.set(None);
            who_has_loading.set(true);
            spawn(async move {
                let guard = bridge_handle.lock().await;
                let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                if let Some(b) = nets.first() {
                    let result = if by_name {
                        if name_val.trim().is_empty() {
                            who_has_status.set(Some("Enter an object name".into()));
                            who_has_loading.set(false);
                            return;
                        }
                        b.who_has_by_name(&name_val, Duration::from_secs(5)).await
                    } else {
                        let obj_type = match parse_object_type(&type_val) {
                            Some(t) => t,
                            None => {
                                who_has_status
                                    .set(Some(format!("Unknown object type: {type_val}")));
                                who_has_loading.set(false);
                                return;
                            }
                        };
                        let instance: u32 = match instance_val.trim().parse() {
                            Ok(n) => n,
                            Err(_) => {
                                who_has_status.set(Some("Invalid instance number".into()));
                                who_has_loading.set(false);
                                return;
                            }
                        };
                        let object_id = ObjectId::new(obj_type, instance);
                        b.who_has_by_id(object_id, Duration::from_secs(5)).await
                    };
                    match result {
                        Ok(objects) => {
                            if objects.is_empty() {
                                who_has_status.set(Some("No results found".into()));
                            }
                            who_has_results.set(objects);
                        }
                        Err(e) => who_has_status.set(Some(format!("Who-Has error: {e}"))),
                    }
                } else {
                    who_has_status.set(Some("BACnet bridge not available".into()));
                }
                who_has_loading.set(false);
            });
        }
    };

    let bdt = bdt_entries.read();
    let fdt = fdt_entries.read();
    let results = who_has_results.read();
    let is_by_name = *search_by_name.read();

    rsx! {
        div { class: "bacnet-network-tools",
            // ── BBMD Management ──
            div { class: "bacnet-tool-section",
                h4 { class: "bacnet-tool-title", "BBMD Management" }
                div { class: "bacnet-tool-actions",
                    button {
                        class: "discovery-action-btn",
                        disabled: *bbmd_loading.read(),
                        onclick: read_bdt,
                        "Read BDT"
                    }
                    button {
                        class: "discovery-action-btn",
                        disabled: *bbmd_loading.read(),
                        onclick: read_fdt,
                        "Read FDT"
                    }
                }

                if let Some(ref msg) = *bbmd_status.read() {
                    div { class: "bacnet-tool-status", "{msg}" }
                }

                // BDT table
                if !bdt.is_empty() {
                    div { class: "bacnet-tool-result",
                        h5 { "Broadcast Distribution Table" }
                        table { class: "discovery-point-table",
                            thead {
                                tr {
                                    th { "Address" }
                                    th { "Mask" }
                                }
                            }
                            tbody {
                                for entry in bdt.iter() {
                                    tr {
                                        td { "{entry.address}" }
                                        td { "{entry.mask}" }
                                    }
                                }
                            }
                        }
                    }
                }

                // FDT table
                if !fdt.is_empty() {
                    div { class: "bacnet-tool-result",
                        h5 { "Foreign Device Table" }
                        table { class: "discovery-point-table",
                            thead {
                                tr {
                                    th { "Address" }
                                    th { "TTL (s)" }
                                    th { "Remaining (s)" }
                                }
                            }
                            tbody {
                                for entry in fdt.iter() {
                                    tr {
                                        td { "{entry.address}" }
                                        td { "{entry.ttl_seconds}" }
                                        td { "{entry.remaining_seconds}" }
                                    }
                                }
                            }
                        }
                    }
                }

                // Delete FDT entry
                div { class: "bacnet-tool-form",
                    input {
                        class: "discovery-input",
                        r#type: "text",
                        placeholder: "192.168.1.1:47808",
                        value: "{delete_fdt_input.read()}",
                        oninput: move |evt: Event<FormData>| delete_fdt_input.set(evt.value()),
                    }
                    button {
                        class: "discovery-action-btn warn",
                        disabled: *bbmd_loading.read(),
                        onclick: do_delete_fdt,
                        "Delete FDT Entry"
                    }
                }
            }

            // ── Who-Has Search ──
            div { class: "bacnet-tool-section",
                h4 { class: "bacnet-tool-title", "Who-Has Search" }
                div { class: "bacnet-tool-toggle",
                    button {
                        class: if is_by_name { "discovery-action-btn active" } else { "discovery-action-btn" },
                        onclick: move |_| search_by_name.set(true),
                        "By Name"
                    }
                    button {
                        class: if !is_by_name { "discovery-action-btn active" } else { "discovery-action-btn" },
                        onclick: move |_| search_by_name.set(false),
                        "By Object ID"
                    }
                }

                if is_by_name {
                    div { class: "bacnet-tool-form",
                        input {
                            class: "discovery-input",
                            r#type: "text",
                            placeholder: "Object name",
                            value: "{who_has_name_input.read()}",
                            oninput: move |evt: Event<FormData>| who_has_name_input.set(evt.value()),
                        }
                        button {
                            class: "discovery-action-btn",
                            disabled: *who_has_loading.read(),
                            onclick: do_who_has,
                            "Search"
                        }
                    }
                } else {
                    div { class: "bacnet-tool-form",
                        select {
                            class: "discovery-input",
                            value: "{who_has_type_input.read()}",
                            onchange: move |evt: Event<FormData>| who_has_type_input.set(evt.value()),
                            option { value: "AnalogInput", "Analog Input" }
                            option { value: "AnalogOutput", "Analog Output" }
                            option { value: "AnalogValue", "Analog Value" }
                            option { value: "BinaryInput", "Binary Input" }
                            option { value: "BinaryOutput", "Binary Output" }
                            option { value: "BinaryValue", "Binary Value" }
                            option { value: "MultiStateInput", "Multi-State Input" }
                            option { value: "MultiStateOutput", "Multi-State Output" }
                            option { value: "MultiStateValue", "Multi-State Value" }
                            option { value: "TrendLog", "Trend Log" }
                            option { value: "Schedule", "Schedule" }
                            option { value: "Calendar", "Calendar" }
                            option { value: "NotificationClass", "Notification Class" }
                            option { value: "File", "File" }
                        }
                        input {
                            class: "discovery-input",
                            r#type: "number",
                            placeholder: "Instance",
                            value: "{who_has_instance_input.read()}",
                            oninput: move |evt: Event<FormData>| who_has_instance_input.set(evt.value()),
                        }
                        button {
                            class: "discovery-action-btn",
                            disabled: *who_has_loading.read(),
                            onclick: do_who_has,
                            "Search"
                        }
                    }
                }

                if let Some(ref msg) = *who_has_status.read() {
                    div { class: "bacnet-tool-status", "{msg}" }
                }

                if !results.is_empty() {
                    div { class: "bacnet-tool-result",
                        table { class: "discovery-point-table",
                            thead {
                                tr {
                                    th { "Object ID" }
                                    th { "Device" }
                                    th { "Name" }
                                }
                            }
                            tbody {
                                for obj in results.iter() {
                                    {
                                        let dev_inst = obj.device_id.instance();
                                        let obj_id_str = format!("{:?}", obj.object_id);
                                        rsx! {
                                            tr {
                                                td { "{obj_id_str}" }
                                                td { "{dev_inst}" }
                                                td { "{obj.object_name}" }
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

fn parse_object_type(s: &str) -> Option<ObjectType> {
    match s {
        "AnalogInput" => Some(ObjectType::AnalogInput),
        "AnalogOutput" => Some(ObjectType::AnalogOutput),
        "AnalogValue" => Some(ObjectType::AnalogValue),
        "BinaryInput" => Some(ObjectType::BinaryInput),
        "BinaryOutput" => Some(ObjectType::BinaryOutput),
        "BinaryValue" => Some(ObjectType::BinaryValue),
        "MultiStateInput" => Some(ObjectType::MultiStateInput),
        "MultiStateOutput" => Some(ObjectType::MultiStateOutput),
        "MultiStateValue" => Some(ObjectType::MultiStateValue),
        "TrendLog" => Some(ObjectType::TrendLog),
        "Schedule" => Some(ObjectType::Schedule),
        "Calendar" => Some(ObjectType::Calendar),
        "NotificationClass" => Some(ObjectType::NotificationClass),
        "File" => Some(ObjectType::File),
        _ => None,
    }
}
