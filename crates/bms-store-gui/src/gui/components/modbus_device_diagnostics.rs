use dioxus::prelude::*;

use crate::bridge::modbus::ModbusBridge;
use crate::gui::state::AppState;

#[component]
pub fn ModbusDeviceDiagnostics(device_id: String) -> Element {
    let state = use_context::<AppState>();

    // Echo test (FC8, sub 0x0000)
    let mut echo_data = use_signal(|| "0".to_string());
    let mut echo_result: Signal<Option<(u16, u16)>> = use_signal(|| None);
    let mut echo_error: Signal<Option<String>> = use_signal(|| None);
    let mut echo_loading = use_signal(|| false);

    // Bus counters (FC8, sub 0x000B-0x0012)
    let mut counters: Signal<Vec<(&'static str, Option<u16>)>> = use_signal(Vec::new);
    let mut counters_error: Signal<Option<String>> = use_signal(|| None);
    let mut counters_loading = use_signal(|| false);

    // Mask write (FC22)
    let mut mask_addr = use_signal(|| "0".to_string());
    let mut mask_and = use_signal(|| "FFFF".to_string());
    let mut mask_or = use_signal(|| "0000".to_string());
    let mut mask_error: Signal<Option<String>> = use_signal(|| None);
    let mut mask_status: Signal<Option<String>> = use_signal(|| None);
    let mut mask_loading = use_signal(|| false);

    rsx! {
        div { class: "modbus-device-diagnostics",
            // Echo Test
            div { class: "modbus-tool-section",
                h4 { class: "modbus-tool-title",
                    "Echo Test"
                    span { class: "modbus-tool-badge", "FC8" }
                }
                div { class: "modbus-tool-form",
                    input {
                        class: "discovery-input",
                        r#type: "number",
                        placeholder: "Data (u16)",
                        value: "{echo_data}",
                        oninput: move |e| echo_data.set(e.value()),
                    }
                    button {
                        class: "discovery-action-btn",
                        disabled: *echo_loading.read(),
                        onclick: {
                            let bridge_handle = state.modbus_handle();
                            let dev_id = device_id.clone();
                            move |_| {
                                let bridge_handle = bridge_handle.clone();
                                let dev_id = dev_id.clone();
                                let data: u16 = echo_data.read().parse().unwrap_or(0);
                                echo_loading.set(true);
                                echo_error.set(None);
                                if let Some(bridge_handle) = bridge_handle {
                                    spawn(async move {
                                        let guard = bridge_handle.lock().await;
                                        let b = guard.as_any().downcast_ref::<ModbusBridge>().unwrap();
                                        match b.diagnostics(&dev_id, 0x0000, data).await {
                                            Ok(resp) => echo_result.set(Some(resp)),
                                            Err(e) => echo_error.set(Some(format!("{e}"))),
                                        }
                                        echo_loading.set(false);
                                    });
                                } else {
                                    echo_loading.set(false);
                                }
                            }
                        },
                        if *echo_loading.read() { "Sending..." } else { "Send Echo" }
                    }
                }
                if let Some(ref err) = *echo_error.read() {
                    div { class: "modbus-tool-status error", "{err}" }
                }
                if let Some((sub, data)) = *echo_result.read() {
                    div { class: "modbus-tool-status",
                        "Response: sub=0x{sub:04X}, data=0x{data:04X}"
                    }
                }
            }

            // Bus Counters
            div { class: "modbus-tool-section",
                h4 { class: "modbus-tool-title",
                    "Bus Counters"
                    span { class: "modbus-tool-badge", "FC8" }
                }
                div { class: "modbus-tool-actions",
                    button {
                        class: "discovery-action-btn",
                        disabled: *counters_loading.read(),
                        onclick: {
                            let bridge_handle = state.modbus_handle();
                            let dev_id = device_id.clone();
                            move |_| {
                                let bridge_handle = bridge_handle.clone();
                                let dev_id = dev_id.clone();
                                counters_loading.set(true);
                                counters_error.set(None);
                                if let Some(bridge_handle) = bridge_handle {
                                    spawn(async move {
                                        let guard = bridge_handle.lock().await;
                                        let b = guard.as_any().downcast_ref::<ModbusBridge>().unwrap();
                                        let counter_defs: Vec<(&str, u16)> = vec![
                                            ("Bus Message Count", 0x000B),
                                            ("Bus Comm Error Count", 0x000C),
                                            ("Bus Exception Error Count", 0x000D),
                                            ("Server Message Count", 0x000E),
                                            ("Server No Response Count", 0x000F),
                                            ("Server NAK Count", 0x0010),
                                            ("Server Busy Count", 0x0011),
                                            ("Bus Char Overrun Count", 0x0012),
                                        ];
                                        let mut results = Vec::new();
                                        for (name, sub) in counter_defs {
                                            match b.diagnostics(&dev_id, sub, 0).await {
                                                Ok((_sub, data)) => results.push((name, Some(data))),
                                                Err(_) => results.push((name, None)),
                                            }
                                        }
                                        counters.set(results);
                                        counters_loading.set(false);
                                    });
                                } else {
                                    counters_loading.set(false);
                                }
                            }
                        },
                        if *counters_loading.read() { "Reading..." } else { "Read Counters" }
                    }
                    button {
                        class: "discovery-action-btn",
                        onclick: {
                            let bridge_handle = state.modbus_handle();
                            let dev_id = device_id.clone();
                            move |_| {
                                let bridge_handle = bridge_handle.clone();
                                let dev_id = dev_id.clone();
                                if let Some(bridge_handle) = bridge_handle {
                                    spawn(async move {
                                        let guard = bridge_handle.lock().await;
                                        let b = guard.as_any().downcast_ref::<ModbusBridge>().unwrap();
                                        let _ = b.diagnostics(&dev_id, 0x000A, 0).await;
                                        counters.set(Vec::new());
                                    });
                                } else {
                                    counters.set(Vec::new());
                                }
                            }
                        },
                        "Clear Counters"
                    }
                }
                if let Some(ref err) = *counters_error.read() {
                    div { class: "modbus-tool-status error", "{err}" }
                }
                if !counters.read().is_empty() {
                    div { class: "modbus-tool-result",
                        table { class: "modbus-register-table",
                            thead {
                                tr {
                                    th { "Counter" }
                                    th { "Value" }
                                }
                            }
                            tbody {
                                for (name, val) in counters.read().iter() {
                                    tr {
                                        td { "{name}" }
                                        td {
                                            match val {
                                                Some(v) => format!("{v}"),
                                                None => "N/A".into(),
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Mask Write Register (FC22)
            div { class: "modbus-tool-section",
                h4 { class: "modbus-tool-title",
                    "Mask Write Register"
                    span { class: "modbus-tool-badge", "FC22" }
                }
                div { class: "modbus-tool-form",
                    label { class: "modbus-tool-label", "Addr" }
                    input {
                        class: "discovery-input short",
                        r#type: "number",
                        placeholder: "0",
                        value: "{mask_addr}",
                        oninput: move |e| mask_addr.set(e.value()),
                    }
                    label { class: "modbus-tool-label", "AND" }
                    input {
                        class: "discovery-input",
                        placeholder: "FFFF",
                        value: "{mask_and}",
                        oninput: move |e| mask_and.set(e.value()),
                    }
                    label { class: "modbus-tool-label", "OR" }
                    input {
                        class: "discovery-input",
                        placeholder: "0000",
                        value: "{mask_or}",
                        oninput: move |e| mask_or.set(e.value()),
                    }
                    button {
                        class: "discovery-action-btn",
                        disabled: *mask_loading.read(),
                        onclick: {
                            let bridge_handle = state.modbus_handle();
                            let dev_id = device_id.clone();
                            move |_| {
                                let bridge_handle = bridge_handle.clone();
                                let dev_id = dev_id.clone();
                                let addr: u16 = mask_addr.read().parse().unwrap_or(0);
                                let and_mask = u16::from_str_radix(mask_and.read().trim_start_matches("0x"), 16).unwrap_or(0xFFFF);
                                let or_mask = u16::from_str_radix(mask_or.read().trim_start_matches("0x"), 16).unwrap_or(0x0000);
                                mask_loading.set(true);
                                mask_error.set(None);
                                mask_status.set(None);
                                if let Some(bridge_handle) = bridge_handle {
                                    spawn(async move {
                                        let guard = bridge_handle.lock().await;
                                        let b = guard.as_any().downcast_ref::<ModbusBridge>().unwrap();
                                        match b.mask_write_register(&dev_id, addr, and_mask, or_mask).await {
                                            Ok(()) => mask_status.set(Some("Write successful".into())),
                                            Err(e) => mask_error.set(Some(format!("{e}"))),
                                        }
                                        mask_loading.set(false);
                                    });
                                } else {
                                    mask_loading.set(false);
                                }
                            }
                        },
                        if *mask_loading.read() { "Writing..." } else { "Write" }
                    }
                }
                div { class: "modbus-tool-status",
                    "Result = (Register AND and_mask) OR (or_mask AND NOT and_mask)"
                }
                if let Some(ref err) = *mask_error.read() {
                    div { class: "modbus-tool-status error", "{err}" }
                }
                if let Some(ref msg) = *mask_status.read() {
                    div { class: "modbus-tool-status", "{msg}" }
                }
            }
        }
    }
}
