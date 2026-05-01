use dioxus::prelude::*;

use bms_store_bridges::bridge::modbus::ModbusBridge;
use bms_store_storage::config::profile::ModbusRegisterType;
use crate::gui::state::AppState;

#[component]
pub fn ModbusDeviceRegisters(device_id: String) -> Element {
    let state = use_context::<AppState>();

    // Register read form state
    let mut start_addr = use_signal(|| "0".to_string());
    let mut reg_count = use_signal(|| "10".to_string());
    let mut reg_type = use_signal(|| "holding".to_string());
    let mut read_result: Signal<Vec<(u16, u16)>> = use_signal(Vec::new); // (addr, value)
    let mut read_bits_result: Signal<Vec<(u16, bool)>> = use_signal(Vec::new);
    let mut read_error: Signal<Option<String>> = use_signal(|| None);
    let mut read_loading = use_signal(|| false);

    // Device ID (FC43) state
    let mut fc43_vendor = use_signal(|| Option::<String>::None);
    let mut fc43_product = use_signal(|| Option::<String>::None);
    let mut fc43_revision = use_signal(|| Option::<String>::None);
    let mut fc43_error: Signal<Option<String>> = use_signal(|| None);
    let mut fc43_loading = use_signal(|| false);

    // FIFO state
    let mut fifo_addr = use_signal(|| "0".to_string());
    let mut fifo_result: Signal<Vec<u16>> = use_signal(Vec::new);
    let mut fifo_error: Signal<Option<String>> = use_signal(|| None);
    let mut fifo_loading = use_signal(|| false);

    rsx! {
        div { class: "modbus-device-registers",
            // Device Identification (FC43)
            div { class: "modbus-tool-section",
                h4 { class: "modbus-tool-title",
                    "Device Identification"
                    span { class: "modbus-tool-badge", "FC43" }
                }
                div { class: "modbus-tool-actions",
                    button {
                        class: "discovery-action-btn",
                        disabled: *fc43_loading.read(),
                        onclick: {
                            let bridge_handle = state.modbus_handle();
                            let dev_id = device_id.clone();
                            move |_| {
                                let bridge_handle = bridge_handle.clone();
                                let dev_id = dev_id.clone();
                                fc43_loading.set(true);
                                fc43_error.set(None);
                                if let Some(bridge_handle) = bridge_handle {
                                    spawn(async move {
                                        let guard = bridge_handle.lock().await;
                                        let b = guard.as_any().downcast_ref::<ModbusBridge>().unwrap();
                                        match b.read_device_identification(&dev_id).await {
                                            Ok(info) => {
                                                fc43_vendor.set(info.vendor);
                                                fc43_product.set(info.product);
                                                fc43_revision.set(info.revision);
                                            }
                                            Err(e) => fc43_error.set(Some(format!("{e}"))),
                                        }
                                        fc43_loading.set(false);
                                    });
                                } else {
                                    fc43_loading.set(false);
                                }
                            }
                        },
                        if *fc43_loading.read() { "Reading..." } else { "Read Device ID" }
                    }
                }
                if let Some(ref err) = *fc43_error.read() {
                    div { class: "modbus-tool-status error", "{err}" }
                }
                if fc43_vendor.read().is_some() || fc43_product.read().is_some() || fc43_revision.read().is_some() {
                    div { class: "modbus-tool-result",
                        table { class: "modbus-register-table",
                            tbody {
                                tr {
                                    td { "Vendor" }
                                    td { "{fc43_vendor.read().as_deref().unwrap_or(\"—\")}" }
                                }
                                tr {
                                    td { "Product" }
                                    td { "{fc43_product.read().as_deref().unwrap_or(\"—\")}" }
                                }
                                tr {
                                    td { "Revision" }
                                    td { "{fc43_revision.read().as_deref().unwrap_or(\"—\")}" }
                                }
                            }
                        }
                    }
                }
            }

            // Register Browser
            div { class: "modbus-tool-section",
                h4 { class: "modbus-tool-title", "Register Browser" }
                div { class: "modbus-tool-form",
                    select {
                        class: "discovery-input",
                        value: "{reg_type}",
                        onchange: move |e| reg_type.set(e.value()),
                        option { value: "holding", "Holding" }
                        option { value: "input", "Input" }
                        option { value: "coil", "Coil" }
                        option { value: "discrete", "Discrete Input" }
                    }
                    input {
                        class: "discovery-input",
                        r#type: "number",
                        placeholder: "Start addr",
                        value: "{start_addr}",
                        oninput: move |e| start_addr.set(e.value()),
                    }
                    input {
                        class: "discovery-input",
                        r#type: "number",
                        placeholder: "Count",
                        value: "{reg_count}",
                        oninput: move |e| reg_count.set(e.value()),
                    }
                    button {
                        class: "discovery-action-btn",
                        disabled: *read_loading.read(),
                        onclick: {
                            let bridge_handle = state.modbus_handle();
                            let dev_id = device_id.clone();
                            move |_| {
                                let bridge_handle = bridge_handle.clone();
                                let dev_id = dev_id.clone();
                                let start: u16 = start_addr.read().parse().unwrap_or(0);
                                let count: u16 = reg_count.read().parse().unwrap_or(10);
                                let rt_str = reg_type.read().clone();
                                read_loading.set(true);
                                read_error.set(None);
                                read_result.set(Vec::new());
                                read_bits_result.set(Vec::new());
                                if let Some(bridge_handle) = bridge_handle {
                                    spawn(async move {
                                        let guard = bridge_handle.lock().await;
                                        let b = guard.as_any().downcast_ref::<ModbusBridge>().unwrap();
                                        let is_bit = rt_str == "coil" || rt_str == "discrete";
                                        let modbus_rt = match rt_str.as_str() {
                                            "holding" => ModbusRegisterType::Holding,
                                            "input" => ModbusRegisterType::Input,
                                            "coil" => ModbusRegisterType::Coil,
                                            _ => ModbusRegisterType::DiscreteInput,
                                        };
                                        if is_bit {
                                            match b.read_bits(&dev_id, &modbus_rt, start, count).await {
                                                Ok(bits) => {
                                                    let items: Vec<(u16, bool)> = bits.into_iter()
                                                        .enumerate()
                                                        .map(|(i, v)| (start + i as u16, v))
                                                        .collect();
                                                    read_bits_result.set(items);
                                                }
                                                Err(e) => read_error.set(Some(format!("{e}"))),
                                            }
                                        } else {
                                            match b.read_registers(&dev_id, &modbus_rt, start, count).await {
                                                Ok(regs) => {
                                                    let items: Vec<(u16, u16)> = regs.into_iter()
                                                        .enumerate()
                                                        .map(|(i, v)| (start + i as u16, v))
                                                        .collect();
                                                    read_result.set(items);
                                                }
                                                Err(e) => read_error.set(Some(format!("{e}"))),
                                            }
                                        }
                                        read_loading.set(false);
                                    });
                                } else {
                                    read_loading.set(false);
                                }
                            }
                        },
                        if *read_loading.read() { "Reading..." } else { "Read" }
                    }
                }
                if let Some(ref err) = *read_error.read() {
                    div { class: "modbus-tool-status error", "{err}" }
                }
                if !read_result.read().is_empty() {
                    div { class: "modbus-tool-result",
                        table { class: "modbus-register-table",
                            thead {
                                tr {
                                    th { "Address" }
                                    th { "Raw Hex" }
                                    th { "Unsigned" }
                                    th { "Signed" }
                                }
                            }
                            tbody {
                                for (addr, val) in read_result.read().iter() {
                                    tr { key: "{addr}",
                                        td { "{addr}" }
                                        td { "0x{val:04X}" }
                                        td { "{val}" }
                                        td { "{*val as i16}" }
                                    }
                                }
                            }
                        }
                    }
                }
                if !read_bits_result.read().is_empty() {
                    div { class: "modbus-tool-result",
                        table { class: "modbus-register-table",
                            thead {
                                tr {
                                    th { "Address" }
                                    th { "Value" }
                                }
                            }
                            tbody {
                                for (addr, val) in read_bits_result.read().iter() {
                                    tr { key: "{addr}",
                                        td { "{addr}" }
                                        td { if *val { "ON (1)" } else { "OFF (0)" } }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // FIFO Queue Read (FC24)
            div { class: "modbus-tool-section",
                h4 { class: "modbus-tool-title",
                    "FIFO Queue"
                    span { class: "modbus-tool-badge", "FC24" }
                }
                div { class: "modbus-tool-form",
                    input {
                        class: "discovery-input",
                        r#type: "number",
                        placeholder: "FIFO address",
                        value: "{fifo_addr}",
                        oninput: move |e| fifo_addr.set(e.value()),
                    }
                    button {
                        class: "discovery-action-btn",
                        disabled: *fifo_loading.read(),
                        onclick: {
                            let bridge_handle = state.modbus_handle();
                            let dev_id = device_id.clone();
                            move |_| {
                                let bridge_handle = bridge_handle.clone();
                                let dev_id = dev_id.clone();
                                let addr: u16 = fifo_addr.read().parse().unwrap_or(0);
                                fifo_loading.set(true);
                                fifo_error.set(None);
                                if let Some(bridge_handle) = bridge_handle {
                                    spawn(async move {
                                        let guard = bridge_handle.lock().await;
                                        let b = guard.as_any().downcast_ref::<ModbusBridge>().unwrap();
                                        match b.read_fifo(&dev_id, addr).await {
                                            Ok(vals) => fifo_result.set(vals),
                                            Err(e) => fifo_error.set(Some(format!("{e}"))),
                                        }
                                        fifo_loading.set(false);
                                    });
                                } else {
                                    fifo_loading.set(false);
                                }
                            }
                        },
                        if *fifo_loading.read() { "Reading..." } else { "Read FIFO" }
                    }
                }
                if let Some(ref err) = *fifo_error.read() {
                    div { class: "modbus-tool-status error", "{err}" }
                }
                if !fifo_result.read().is_empty() {
                    div { class: "modbus-tool-result",
                        h5 { "Queue ({fifo_result.read().len()} values)" }
                        table { class: "modbus-register-table",
                            thead {
                                tr {
                                    th { "#" }
                                    th { "Value" }
                                    th { "Hex" }
                                }
                            }
                            tbody {
                                for (i, val) in fifo_result.read().iter().enumerate() {
                                    tr { key: "{i}",
                                        td { "{i}" }
                                        td { "{val}" }
                                        td { "0x{val:04X}" }
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
