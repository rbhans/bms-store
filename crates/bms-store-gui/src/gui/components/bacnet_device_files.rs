use dioxus::prelude::*;

use crate::bridge::bacnet::BacnetNetworks;
use crate::gui::state::AppState;
use rustbac_client::{AtomicReadFileResult, AtomicWriteFileResult};

/// Parse a hex string (e.g., "48656C6C6F") into bytes.
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

#[component]
pub fn BacnetDeviceFiles(device_instance: u32) -> Element {
    let state = use_context::<AppState>();

    // Stream upload state
    let mut upload_file_instance: Signal<String> = use_signal(|| "0".to_string());
    let mut upload_start: Signal<String> = use_signal(|| "0".to_string());
    let mut upload_hex_data: Signal<String> = use_signal(String::new);
    let mut upload_status: Signal<Option<String>> = use_signal(|| None);
    let mut upload_loading = use_signal(|| false);

    // Record read state
    let mut record_read_file_instance: Signal<String> = use_signal(|| "0".to_string());
    let mut record_read_start: Signal<String> = use_signal(|| "0".to_string());
    let mut record_read_count: Signal<String> = use_signal(|| "10".to_string());
    let mut record_read_result: Signal<Option<RecordReadDisplay>> = use_signal(|| None);
    let mut record_read_error: Signal<Option<String>> = use_signal(|| None);
    let mut record_read_loading = use_signal(|| false);

    // Record write state
    let mut record_write_file_instance: Signal<String> = use_signal(|| "0".to_string());
    let mut record_write_start: Signal<String> = use_signal(|| "0".to_string());
    let mut record_write_hex: Signal<String> = use_signal(String::new);
    let mut record_write_status: Signal<Option<String>> = use_signal(|| None);
    let mut record_write_loading = use_signal(|| false);

    rsx! {
        div { class: "bacnet-device-files",
            div { class: "bacnet-tool-section",
                h4 { class: "bacnet-tool-title", "Stream Upload" }

                div { class: "bacnet-tool-form",
                    input {
                        r#type: "text",
                        class: "discovery-input",
                        placeholder: "File instance",
                        value: "{upload_file_instance}",
                        oninput: move |e| upload_file_instance.set(e.value()),
                    }
                    input {
                        r#type: "text",
                        class: "discovery-input",
                        placeholder: "Start position",
                        value: "{upload_start}",
                        oninput: move |e| upload_start.set(e.value()),
                    }
                }
                div { class: "bacnet-tool-form",
                    textarea {
                        class: "discovery-input",
                        placeholder: "Hex data (e.g., 48656C6C6F)",
                        rows: "3",
                        value: "{upload_hex_data}",
                        oninput: move |e| upload_hex_data.set(e.value()),
                    }
                }
                div { class: "bacnet-tool-form",
                    button {
                        class: "discovery-action-btn",
                        disabled: *upload_loading.read(),
                        onclick: {
                            let bridge_handle = state.bacnet_handle();
                            move |_| {
                                let bridge_handle = bridge_handle.clone();
                                let fi_str = upload_file_instance.read().clone();
                                let start_str = upload_start.read().clone();
                                let hex_str = upload_hex_data.read().clone();
                                upload_loading.set(true);
                                upload_status.set(None);
                                spawn(async move {
                                    let file_inst: u32 = match fi_str.parse() {
                                        Ok(v) => v,
                                        Err(_) => {
                                            upload_status.set(Some("Invalid file instance".into()));
                                            upload_loading.set(false);
                                            return;
                                        }
                                    };
                                    let start: i32 = match start_str.parse() {
                                        Ok(v) => v,
                                        Err(_) => {
                                            upload_status.set(Some("Invalid start position".into()));
                                            upload_loading.set(false);
                                            return;
                                        }
                                    };
                                    let data = match hex_to_bytes(&hex_str) {
                                        Some(d) => d,
                                        None => {
                                            upload_status.set(Some("Invalid hex data".into()));
                                            upload_loading.set(false);
                                            return;
                                        }
                                    };
                                    let guard = bridge_handle.lock().await;
                                    let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                                    if let Some(b) = nets.bridge_for_device(device_instance) {
                                        match b.write_file_stream(device_instance, file_inst, start, &data).await {
                                            Ok(result) => {
                                                let pos = match result {
                                                    AtomicWriteFileResult::Stream { file_start_position } => file_start_position,
                                                    AtomicWriteFileResult::Record { file_start_record } => file_start_record,
                                                };
                                                upload_status.set(Some(format!("Upload OK, start position: {pos}")));
                                            }
                                            Err(e) => upload_status.set(Some(format!("Upload failed: {e}"))),
                                        }
                                    }
                                    upload_loading.set(false);
                                });
                            }
                        },
                        if *upload_loading.read() { "Uploading..." } else { "Upload" }
                    }
                }

                if let Some(ref status) = *upload_status.read() {
                    div { class: "bacnet-tool-status", "{status}" }
                }
            }

            div { class: "bacnet-tool-section",
                h4 { class: "bacnet-tool-title",
                    "Record Read"
                    if let Some(ref r) = *record_read_result.read() {
                        span { class: "bacnet-tool-badge", "{r.records.len()} records" }
                    }
                }

                div { class: "bacnet-tool-form",
                    input {
                        r#type: "text",
                        class: "discovery-input",
                        placeholder: "File instance",
                        value: "{record_read_file_instance}",
                        oninput: move |e| record_read_file_instance.set(e.value()),
                    }
                    input {
                        r#type: "text",
                        class: "discovery-input",
                        placeholder: "Start record",
                        value: "{record_read_start}",
                        oninput: move |e| record_read_start.set(e.value()),
                    }
                    input {
                        r#type: "text",
                        class: "discovery-input",
                        placeholder: "Count",
                        value: "{record_read_count}",
                        oninput: move |e| record_read_count.set(e.value()),
                    }
                    button {
                        class: "discovery-action-btn",
                        disabled: *record_read_loading.read(),
                        onclick: {
                            let bridge_handle = state.bacnet_handle();
                            move |_| {
                                let bridge_handle = bridge_handle.clone();
                                let fi_str = record_read_file_instance.read().clone();
                                let start_str = record_read_start.read().clone();
                                let count_str = record_read_count.read().clone();
                                record_read_loading.set(true);
                                record_read_error.set(None);
                                spawn(async move {
                                    let file_inst: u32 = match fi_str.parse() {
                                        Ok(v) => v,
                                        Err(_) => {
                                            record_read_error.set(Some("Invalid file instance".into()));
                                            record_read_loading.set(false);
                                            return;
                                        }
                                    };
                                    let start: i32 = match start_str.parse() {
                                        Ok(v) => v,
                                        Err(_) => {
                                            record_read_error.set(Some("Invalid start record".into()));
                                            record_read_loading.set(false);
                                            return;
                                        }
                                    };
                                    let count: u32 = match count_str.parse() {
                                        Ok(v) => v,
                                        Err(_) => {
                                            record_read_error.set(Some("Invalid count".into()));
                                            record_read_loading.set(false);
                                            return;
                                        }
                                    };
                                    let guard = bridge_handle.lock().await;
                                    let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                                    if let Some(b) = nets.bridge_for_device(device_instance) {
                                        match b.read_file_record(device_instance, file_inst, start, count).await {
                                            Ok(result) => {
                                                let display = match result {
                                                    AtomicReadFileResult::Record {
                                                        end_of_file,
                                                        file_start_record,
                                                        returned_record_count,
                                                        file_record_data,
                                                    } => RecordReadDisplay {
                                                        end_of_file,
                                                        start_record: file_start_record,
                                                        count: returned_record_count,
                                                        records: file_record_data
                                                            .iter()
                                                            .map(|r| bytes_to_hex(r))
                                                            .collect(),
                                                    },
                                                    AtomicReadFileResult::Stream {
                                                        end_of_file,
                                                        file_data,
                                                        ..
                                                    } => RecordReadDisplay {
                                                        end_of_file,
                                                        start_record: 0,
                                                        count: 1,
                                                        records: vec![bytes_to_hex(&file_data)],
                                                    },
                                                };
                                                record_read_result.set(Some(display));
                                            }
                                            Err(e) => record_read_error.set(Some(format!("{e}"))),
                                        }
                                    }
                                    record_read_loading.set(false);
                                });
                            }
                        },
                        if *record_read_loading.read() { "Reading..." } else { "Read Records" }
                    }
                }

                if let Some(ref err) = *record_read_error.read() {
                    div { class: "bacnet-tool-status", "Error: {err}" }
                }

                if let Some(ref result) = *record_read_result.read() {
                    div { class: "bacnet-tool-status",
                        "Start record: {result.start_record}, Count: {result.count}, EOF: {result.end_of_file}"
                    }
                    div { class: "discovery-point-table-wrapper",
                        table { class: "discovery-point-table",
                            thead {
                                tr {
                                    th { "#" }
                                    th { "Data (hex)" }
                                }
                            }
                            tbody {
                                for (i, hex) in result.records.iter().enumerate() {
                                    tr {
                                        td { "{result.start_record + i as i32}" }
                                        td { style: "font-family: monospace; word-break: break-all;", "{hex}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            div { class: "bacnet-tool-section",
                h4 { class: "bacnet-tool-title", "Record Write" }

                div { class: "bacnet-tool-form",
                    input {
                        r#type: "text",
                        class: "discovery-input",
                        placeholder: "File instance",
                        value: "{record_write_file_instance}",
                        oninput: move |e| record_write_file_instance.set(e.value()),
                    }
                    input {
                        r#type: "text",
                        class: "discovery-input",
                        placeholder: "Start record",
                        value: "{record_write_start}",
                        oninput: move |e| record_write_start.set(e.value()),
                    }
                }
                div { class: "bacnet-tool-form",
                    textarea {
                        class: "discovery-input",
                        placeholder: "Hex record data (one record per line, e.g., 48656C6C6F)",
                        rows: "3",
                        value: "{record_write_hex}",
                        oninput: move |e| record_write_hex.set(e.value()),
                    }
                }
                div { class: "bacnet-tool-form",
                    button {
                        class: "discovery-action-btn",
                        disabled: *record_write_loading.read(),
                        onclick: {
                            let bridge_handle = state.bacnet_handle();
                            move |_| {
                                let bridge_handle = bridge_handle.clone();
                                let fi_str = record_write_file_instance.read().clone();
                                let start_str = record_write_start.read().clone();
                                let hex_lines = record_write_hex.read().clone();
                                record_write_loading.set(true);
                                record_write_status.set(None);
                                spawn(async move {
                                    let file_inst: u32 = match fi_str.parse() {
                                        Ok(v) => v,
                                        Err(_) => {
                                            record_write_status.set(Some("Invalid file instance".into()));
                                            record_write_loading.set(false);
                                            return;
                                        }
                                    };
                                    let start: i32 = match start_str.parse() {
                                        Ok(v) => v,
                                        Err(_) => {
                                            record_write_status.set(Some("Invalid start record".into()));
                                            record_write_loading.set(false);
                                            return;
                                        }
                                    };
                                    let mut records_vec: Vec<Vec<u8>> = Vec::new();
                                    for line in hex_lines.lines() {
                                        let line = line.trim();
                                        if line.is_empty() {
                                            continue;
                                        }
                                        match hex_to_bytes(line) {
                                            Some(bytes) => records_vec.push(bytes),
                                            None => {
                                                record_write_status.set(Some(format!("Invalid hex on line: {line}")));
                                                record_write_loading.set(false);
                                                return;
                                            }
                                        }
                                    }
                                    if records_vec.is_empty() {
                                        record_write_status.set(Some("No records to write".into()));
                                        record_write_loading.set(false);
                                        return;
                                    }
                                    let record_slices: Vec<&[u8]> = records_vec.iter().map(|r| r.as_slice()).collect();
                                    let guard = bridge_handle.lock().await;
                                    let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                                    if let Some(b) = nets.bridge_for_device(device_instance) {
                                        match b.write_file_record(device_instance, file_inst, start, &record_slices).await {
                                            Ok(result) => {
                                                let pos = match result {
                                                    AtomicWriteFileResult::Record { file_start_record } => file_start_record,
                                                    AtomicWriteFileResult::Stream { file_start_position } => file_start_position,
                                                };
                                                record_write_status.set(Some(format!(
                                                    "Write OK, {} records written, start record: {pos}",
                                                    records_vec.len()
                                                )));
                                            }
                                            Err(e) => record_write_status.set(Some(format!("Write failed: {e}"))),
                                        }
                                    }
                                    record_write_loading.set(false);
                                });
                            }
                        },
                        if *record_write_loading.read() { "Writing..." } else { "Write Records" }
                    }
                }

                if let Some(ref status) = *record_write_status.read() {
                    div { class: "bacnet-tool-status", "{status}" }
                }
            }
        }
    }
}

/// Display model for record-read results.
#[derive(Clone)]
struct RecordReadDisplay {
    end_of_file: bool,
    start_record: i32,
    count: u32,
    records: Vec<String>,
}
