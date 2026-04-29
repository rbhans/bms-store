use dioxus::prelude::*;

use crate::bridge::bacnet::BacnetNetworks;
use crate::gui::state::AppState;

/// Format an epoch-millisecond timestamp as "YYYY-MM-DD HH:MM:SS".
fn format_epoch_ms(epoch_ms: i64) -> String {
    let total_secs = epoch_ms / 1000;
    let secs_in_day = total_secs.rem_euclid(86400);
    let days = (total_secs - secs_in_day) / 86400;

    let h = secs_in_day / 3600;
    let m = (secs_in_day % 3600) / 60;
    let s = secs_in_day % 60;

    let mut y: i64 = 1970;
    let mut remaining_days = days;

    loop {
        let days_in_year: i64 = if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
            366
        } else {
            365
        };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        y += 1;
    }

    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let month_days: [i64; 12] = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];

    let mut mon = 0usize;
    for (i, &md) in month_days.iter().enumerate() {
        if remaining_days < md {
            mon = i;
            break;
        }
        remaining_days -= md;
    }

    let day = remaining_days + 1;
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        y,
        mon + 1,
        day,
        h,
        m,
        s
    )
}

#[component]
pub fn BacnetDeviceTrends(device_instance: u32) -> Element {
    let state = use_context::<AppState>();

    // D1: Backfill reset state
    let mut backfill_reset_status: Signal<Option<String>> = use_signal(|| None);

    let mut tl_instance_input: Signal<String> = use_signal(|| "0".to_string());
    let mut record_count: Signal<Option<u32>> = use_signal(|| None);
    let mut count_error: Signal<Option<String>> = use_signal(|| None);
    let mut count_loading = use_signal(|| false);

    let mut start_index_input: Signal<String> = use_signal(|| "0".to_string());
    let mut read_count_input: Signal<String> = use_signal(|| "100".to_string());
    let mut trend_records: Signal<Vec<(i64, f64)>> = use_signal(Vec::new);
    let mut read_error: Signal<Option<String>> = use_signal(|| None);
    let mut read_loading = use_signal(|| false);

    rsx! {
        div { class: "bacnet-device-trends",
            // D1: Backfill All Trend Logs button
            div { class: "bacnet-tool-section",
                h4 { class: "bacnet-tool-title", "Trend Log Sync" }
                div { class: "bacnet-tool-form",
                    button {
                        class: "discovery-action-btn accept primary",
                        onclick: {
                            let bridge_handle = state.bacnet_handle();
                            move |_| {
                                let bridge_handle = bridge_handle.clone();
                                spawn(async move {
                                    let mut guard = bridge_handle.lock().await;
                                    let nets = guard.as_any_mut().downcast_mut::<BacnetNetworks>().unwrap();
                                    if let Some(b) = nets.bridge_for_device_mut(device_instance) {
                                        b.reset_trend_log_backfill();
                                        backfill_reset_status.set(Some("Backfill reset — next sync cycle will re-read all records".into()));
                                    }
                                });
                            }
                        },
                        "Backfill All Trend Logs"
                    }
                    {
                        let bridge_interval = state.bacnet_handle();
                        let mut interval_text = use_signal(String::new);
                        use_effect(move || {
                            let bridge_interval = bridge_interval.clone();
                            spawn(async move {
                                let guard = bridge_interval.lock().await;
                                let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                                if let Some(b) = nets.bridge_for_device(device_instance) {
                                    let secs = b.trend_log_sync_interval().as_secs();
                                    interval_text.set(format!("Sync interval: {secs}s"));
                                }
                            });
                        });
                        rsx! {
                            if !interval_text.read().is_empty() {
                                span { class: "bacnet-tool-status", "{interval_text}" }
                            }
                        }
                    }
                }
                if let Some(ref status) = *backfill_reset_status.read() {
                    div { class: "bacnet-tool-status", "{status}" }
                }
            }

            // Record Count section
            div { class: "bacnet-tool-section",
                h4 { class: "bacnet-tool-title",
                    "Record Count"
                    if let Some(count) = *record_count.read() {
                        span { class: "bacnet-tool-badge", "{count}" }
                    }
                }
                div { class: "bacnet-tool-form",
                    input {
                        r#type: "text",
                        class: "discovery-input",
                        placeholder: "TrendLog instance",
                        value: "{tl_instance_input}",
                        oninput: move |e| tl_instance_input.set(e.value()),
                    }
                    button {
                        class: "discovery-action-btn",
                        disabled: *count_loading.read(),
                        onclick: {
                            let bridge_handle = state.bacnet_handle();
                            move |_| {
                                let bridge_handle = bridge_handle.clone();
                                let tl_str = tl_instance_input.read().clone();
                                count_loading.set(true);
                                count_error.set(None);
                                spawn(async move {
                                    let tl_inst: u32 = match tl_str.parse() {
                                        Ok(v) => v,
                                        Err(_) => {
                                            count_error.set(Some("Invalid TrendLog instance".into()));
                                            count_loading.set(false);
                                            return;
                                        }
                                    };
                                    let guard = bridge_handle.lock().await;
                                    let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                                    if let Some(b) = nets.bridge_for_device(device_instance) {
                                        match b.trend_log_record_count(device_instance, tl_inst).await {
                                            Ok(count) => record_count.set(Some(count)),
                                            Err(e) => count_error.set(Some(format!("{e}"))),
                                        }
                                    }
                                    count_loading.set(false);
                                });
                            }
                        },
                        if *count_loading.read() { "Loading..." } else { "Get Count" }
                    }
                }

                if let Some(ref err) = *count_error.read() {
                    div { class: "bacnet-tool-status error", "Error: {err}" }
                }
            }

            // Read Records section
            div { class: "bacnet-tool-section",
                h4 { class: "bacnet-tool-title",
                    "Read Records"
                    if !trend_records.read().is_empty() {
                        span { class: "bacnet-tool-badge", "{trend_records.read().len()}" }
                    }
                }
                div { class: "bacnet-tool-form",
                    input {
                        r#type: "text",
                        class: "discovery-input",
                        placeholder: "Start index",
                        value: "{start_index_input}",
                        oninput: move |e| start_index_input.set(e.value()),
                    }
                    input {
                        r#type: "text",
                        class: "discovery-input",
                        placeholder: "Count",
                        value: "{read_count_input}",
                        oninput: move |e| read_count_input.set(e.value()),
                    }
                    button {
                        class: "discovery-action-btn",
                        disabled: *read_loading.read(),
                        onclick: {
                            let bridge_handle = state.bacnet_handle();
                            move |_| {
                                let bridge_handle = bridge_handle.clone();
                                let tl_str = tl_instance_input.read().clone();
                                let start_str = start_index_input.read().clone();
                                let count_str = read_count_input.read().clone();
                                read_loading.set(true);
                                read_error.set(None);
                                spawn(async move {
                                    let tl_inst: u32 = match tl_str.parse() {
                                        Ok(v) => v,
                                        Err(_) => {
                                            read_error.set(Some("Invalid TrendLog instance".into()));
                                            read_loading.set(false);
                                            return;
                                        }
                                    };
                                    let start: i32 = match start_str.parse() {
                                        Ok(v) => v,
                                        Err(_) => {
                                            read_error.set(Some("Invalid start index".into()));
                                            read_loading.set(false);
                                            return;
                                        }
                                    };
                                    let count: i16 = match count_str.parse() {
                                        Ok(v) => v,
                                        Err(_) => {
                                            read_error.set(Some("Invalid count".into()));
                                            read_loading.set(false);
                                            return;
                                        }
                                    };
                                    let guard = bridge_handle.lock().await;
                                    let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                                    if let Some(b) = nets.bridge_for_device(device_instance) {
                                        match b.read_trend_log(device_instance, tl_inst, start, count).await {
                                            Ok(records) => trend_records.set(records),
                                            Err(e) => read_error.set(Some(format!("{e}"))),
                                        }
                                    }
                                    read_loading.set(false);
                                });
                            }
                        },
                        if *read_loading.read() { "Reading..." } else { "Read" }
                    }
                }

                if let Some(ref err) = *read_error.read() {
                    div { class: "bacnet-tool-status error", "Error: {err}" }
                }

                if !trend_records.read().is_empty() {
                    div { class: "bacnet-tool-result",
                        table { class: "discovery-point-table",
                            thead {
                                tr {
                                    th { "Timestamp" }
                                    th { "Value" }
                                }
                            }
                            tbody {
                                for (ts, val) in trend_records.read().iter() {
                                    tr {
                                        td { "{format_epoch_ms(*ts)}" }
                                        td { "{val:.4}" }
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
