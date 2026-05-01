use std::collections::HashMap;

use dioxus::prelude::*;

use bms_store_bridges::bridge::modbus::ModbusBridge;
use bms_store_storage::config::profile::{match_profile, DeviceProfile};
use bms_store_storage::discovery::model::{DeviceState, DiscoveredDevice, DiscoveredPoint, PointKindHint};
use crate::gui::state::AppState;

use super::discovery_utils::{
    bump, extract_bacnet_instance, extract_modbus_instance_id, kind_label, network_badge_class,
    segmentation_label, DeviceDetailTab,
};

use super::bacnet_device_alarms::BacnetDeviceAlarms;
use super::bacnet_device_cov::BacnetDeviceAdvanced;
use super::bacnet_device_files::BacnetDeviceFiles;
use super::bacnet_device_trends::BacnetDeviceTrends;
use super::discovery_bacnet_ops::{render_bacnet_management, render_bacnet_objects};
use super::modbus_device_diagnostics::ModbusDeviceDiagnostics;
use super::modbus_device_registers::ModbusDeviceRegisters;

/// Renders the device detail right pane (header + tab bar + tab content).
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_device_detail(
    state: &AppState,
    selected_dev: Option<&DiscoveredDevice>,
    points: &[DiscoveredPoint],
    user_is_admin: bool,
    mut detail_tab: Signal<DeviceDetailTab>,
    mut refresh_counter: Signal<u64>,
    mut editing_device_name: Signal<bool>,
    mut device_name_draft: Signal<String>,
    mut selected_point_ids: Signal<std::collections::HashSet<String>>,
    mut editing_point_id: Signal<Option<String>>,
    mut point_name_draft: Signal<String>,
    mut point_units_draft: Signal<String>,
    mut point_desc_draft: Signal<String>,
    mut point_state_labels_draft: Signal<HashMap<String, String>>,
    mut point_kind_editing: Signal<PointKindHint>,
    mut bulk_units_draft: Signal<String>,
    mut bulk_status: Signal<Option<String>>,
    event_infos: Signal<Vec<bms_store_bridges::bridge::bacnet::BacnetEventInfo>>,
    trend_logs: Signal<Vec<(u32, String)>>,
    create_object_type: Signal<String>,
    delete_object_input: Signal<String>,
    commission_status: Signal<Option<String>>,
    modbus_profiles: Signal<Vec<DeviceProfile>>,
    mut selected_profile_idx: Signal<Option<usize>>,
    mut profile_apply_status: Signal<Option<String>>,
    selected_points_signal: Signal<Vec<DiscoveredPoint>>,
) -> Element {
    let detail_dev_state = selected_dev.map(|d| d.state);
    let detail_dev_id = selected_dev.map(|d| d.id.clone());
    let detail_display = selected_dev.map(|d| d.display_name.clone());
    let detail_proto = selected_dev.map(|d| d.protocol.as_str());
    let detail_dev_protocol = selected_dev.map(|d| d.protocol.as_str());
    let detail_addr = selected_dev.map(|d| d.address.clone());
    let detail_vendor = selected_dev.and_then(|d| d.vendor.clone());
    let detail_model = selected_dev.and_then(|d| d.model.clone());
    let detail_state_str = selected_dev.map(|d| d.state.as_str());

    let is_bacnet_accepted = detail_dev_state == Some(DeviceState::Accepted)
        && detail_dev_id
            .as_ref()
            .map(|id| id.starts_with("bacnet-"))
            .unwrap_or(false);

    let is_modbus_accepted = detail_dev_state == Some(DeviceState::Accepted)
        && detail_dev_id
            .as_ref()
            .map(|id| id.starts_with("modbus-"))
            .unwrap_or(false);

    let current_detail = *detail_tab.read();

    // Compute available tabs for selected device
    let available_tabs = match (detail_dev_protocol, detail_dev_state) {
        (Some(proto), Some(st)) => super::discovery_utils::tabs_for_device(proto, st),
        _ => vec![DeviceDetailTab::Overview],
    };

    if let Some(ref display) = detail_display {
        let display = display.clone();
        let name_svc1 = state.discovery_service.clone();
        let name_svc2 = state.discovery_service.clone();
        let name_id1 = detail_dev_id.clone().unwrap_or_default();
        let name_id2 = name_id1.clone();
        rsx! {
            // Device header (always visible)
            div { class: "discovery-detail-header",
                if *editing_device_name.read() {
                    div { class: "discovery-name-edit",
                        input {
                            class: "discovery-name-input",
                            value: "{device_name_draft.read()}",
                            oninput: move |evt: Event<FormData>| device_name_draft.set(evt.value()),
                            onkeypress: move |evt: Event<KeyboardData>| {
                                if evt.key() == Key::Enter {
                                    let svc = name_svc1.clone();
                                    let id = name_id1.clone();
                                    let name = device_name_draft.read().clone();
                                    spawn(async move {
                                        let _ = svc.update_device_name(&id, &name).await;
                                        editing_device_name.set(false);
                                        bump(&mut refresh_counter);
                                    });
                                }
                            },
                        }
                        button {
                            class: "discovery-name-save",
                            onclick: move |_| {
                                let svc = name_svc2.clone();
                                let id = name_id2.clone();
                                let name = device_name_draft.read().clone();
                                spawn(async move {
                                    let _ = svc.update_device_name(&id, &name).await;
                                    editing_device_name.set(false);
                                    bump(&mut refresh_counter);
                                });
                            },
                            "Save"
                        }
                        button {
                            class: "discovery-name-cancel",
                            onclick: move |_| editing_device_name.set(false),
                            "Cancel"
                        }
                    }
                } else {
                    div { class: "discovery-name-row",
                        h3 { "{display}" }
                        if user_is_admin {
                            button {
                                class: "discovery-edit-btn",
                                title: "Edit name",
                                onclick: move |_| {
                                    device_name_draft.set(display.clone());
                                    editing_device_name.set(true);
                                },
                                "Edit"
                            }
                        }
                    }
                }
                div { class: "discovery-detail-meta",
                    if let Some(proto) = detail_proto {
                        span {
                            class: if proto == "bacnet" { "discovery-meta-chip protocol-bacnet" } else { "discovery-meta-chip protocol-modbus" },
                            "{proto}"
                        }
                    }
                    if let Some(ref addr) = detail_addr {
                        span { class: "discovery-meta-chip", "Address: {addr}" }
                    }
                    if let Some(ref v) = detail_vendor {
                        span { class: "discovery-meta-chip", "Vendor: {v}" }
                    }
                    if let Some(ref m) = detail_model {
                        span { class: "discovery-meta-chip", "Model: {m}" }
                    }
                    if let Some(st) = detail_state_str {
                        span { class: "discovery-meta-chip", "State: {st}" }
                    }
                    // B4: Show network_id in detail meta
                    {
                        let net = selected_dev.map(|d| d.network_id.clone()).unwrap_or_default();
                        if !net.is_empty() {
                            rsx! {
                                span { class: "discovery-network-badge {network_badge_class(&net)}", "Network: {net}" }
                            }
                        } else {
                            rsx! {}
                        }
                    }
                }

                // Accept/Ignore for pending devices (admin only for accept)
                if detail_dev_state == Some(DeviceState::Discovered) && user_is_admin {
                    div { class: "discovery-detail-actions",
                        {
                            let accept_id = detail_dev_id.clone().unwrap_or_default();
                            let ignore_id = accept_id.clone();
                            let svc = state.discovery_service.clone();
                            let svc2 = state.discovery_service.clone();
                            let accept_audit = state.clone();
                            rsx! {
                                button {
                                    class: "discovery-action-btn accept primary",
                                    onclick: move |_| {
                                        let svc = svc.clone();
                                        let id = accept_id.clone();
                                        let audit_state = accept_audit.clone();
                                        spawn(async move {
                                            if let Err(e) = svc.accept_device(&id).await {
                                                eprintln!("Accept failed: {e}");
                                                audit_state.audit(
                                                    bms_store_storage::store::audit_store::AuditEntryBuilder::new(
                                                        bms_store_storage::store::audit_store::AuditAction::AcceptDevice, "device",
                                                    ).resource_id(&id).failure(&format!("{e}")),
                                                );
                                            } else {
                                                audit_state.audit(
                                                    bms_store_storage::store::audit_store::AuditEntryBuilder::new(
                                                        bms_store_storage::store::audit_store::AuditAction::AcceptDevice, "device",
                                                    ).resource_id(&id),
                                                );
                                            }
                                            bump(&mut refresh_counter);
                                        });
                                    },
                                    "Accept Device"
                                }
                                button {
                                    class: "discovery-action-btn ignore",
                                    onclick: move |_| {
                                        let svc2 = svc2.clone();
                                        let id = ignore_id.clone();
                                        spawn(async move {
                                            let _ = svc2.ignore_device(&id).await;
                                            bump(&mut refresh_counter);
                                        });
                                    },
                                    "Ignore"
                                }
                            }
                        }
                    }
                }
            }

            // Detail tab bar (protocol-aware)
            if available_tabs.len() > 1 {
                div { class: "discovery-detail-tab-bar",
                    for tab in available_tabs.iter() {
                        {
                            let t = *tab;
                            rsx! {
                                button {
                                    class: if current_detail == t { "discovery-detail-tab active" } else { "discovery-detail-tab" },
                                    onclick: move |_| detail_tab.set(t),
                                    "{t.label()}"
                                }
                            }
                        }
                    }
                }
            }

            // Detail tab content
            div { class: "discovery-detail-body",
                match current_detail {
                    DeviceDetailTab::Overview => rsx! {
                        // C1: Device Properties section (from protocol_meta)
                        {
                            let meta = selected_dev.map(|d| &d.protocol_meta);
                            let dev_protocol = detail_dev_protocol;
                            let dev_network = selected_dev.map(|d| d.network_id.clone()).unwrap_or_default();
                            rsx! {
                                if dev_protocol == Some("bacnet") {
                                    if let Some(meta) = meta {
                                        {
                                            let location = meta.get("location").and_then(|v| v.as_str());
                                            let description = meta.get("description").and_then(|v| v.as_str());
                                            let max_apdu = meta.get("max_apdu").and_then(|v| v.as_u64());
                                            let segmentation = meta.get("segmentation").and_then(|v| v.as_u64());
                                            let protocol_version = meta.get("protocol_version").and_then(|v| v.as_u64());
                                            let app_sw_version = meta.get("app_software_version").and_then(|v| v.as_str());
                                            let has_props = location.is_some() || description.is_some() || max_apdu.is_some()
                                                || segmentation.is_some() || protocol_version.is_some() || app_sw_version.is_some();
                                            if has_props {
                                                rsx! {
                                                    div { class: "discovery-props-section",
                                                        h4 { "Device Properties" }
                                                        table { class: "discovery-props-table",
                                                            tbody {
                                                                if !dev_network.is_empty() {
                                                                    tr {
                                                                        td { class: "discovery-prop-key", "Network" }
                                                                        td { "{dev_network}" }
                                                                    }
                                                                }
                                                                if let Some(loc) = location {
                                                                    tr {
                                                                        td { class: "discovery-prop-key", "Location" }
                                                                        td { "{loc}" }
                                                                    }
                                                                }
                                                                if let Some(desc) = description {
                                                                    tr {
                                                                        td { class: "discovery-prop-key", "Description" }
                                                                        td { "{desc}" }
                                                                    }
                                                                }
                                                                if let Some(apdu) = max_apdu {
                                                                    tr {
                                                                        td { class: "discovery-prop-key", "Max APDU" }
                                                                        td { "{apdu}" }
                                                                    }
                                                                }
                                                                if let Some(seg) = segmentation {
                                                                    tr {
                                                                        td { class: "discovery-prop-key", "Segmentation" }
                                                                        td { "{segmentation_label(seg as u32)}" }
                                                                    }
                                                                }
                                                                if let Some(pv) = protocol_version {
                                                                    tr {
                                                                        td { class: "discovery-prop-key", "Protocol Version" }
                                                                        td { "{pv}" }
                                                                    }
                                                                }
                                                                if let Some(sw) = app_sw_version {
                                                                    tr {
                                                                        td { class: "discovery-prop-key", "Software Version" }
                                                                        td { "{sw}" }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            } else if !dev_network.is_empty() {
                                                rsx! {
                                                    div { class: "discovery-props-section",
                                                        h4 { "Device Properties" }
                                                        table { class: "discovery-props-table",
                                                            tbody {
                                                                tr {
                                                                    td { class: "discovery-prop-key", "Network" }
                                                                    td { "{dev_network}" }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            } else {
                                                rsx! {}
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // D2: Modbus profile library dropdown (for Modbus devices without points)
                        if detail_dev_protocol == Some("modbus") && points.is_empty() {
                            {
                                let profiles = modbus_profiles.read();
                                let dev_vendor = selected_dev.and_then(|d| d.vendor.clone()).unwrap_or_default();
                                let dev_model = selected_dev.and_then(|d| d.model.clone()).unwrap_or_default();
                                let profile_dev_id = detail_dev_id.clone().unwrap_or_default();
                                let profile_svc = state.discovery_service.clone();

                                // Sort profiles by match score
                                let mut scored: Vec<(usize, f64, String)> = profiles.iter().enumerate()
                                    .map(|(i, p)| {
                                        let score = match_profile(p, &dev_vendor, &dev_model);
                                        (i, score, p.profile.name.clone())
                                    })
                                    .collect();
                                scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

                                if !scored.is_empty() {
                                    rsx! {
                                        div { class: "discovery-profile-section",
                                            h4 { "Apply Device Profile" }
                                            p { class: "discovery-hint", "Select a profile to automatically configure registers for this device." }
                                            div { class: "discovery-profile-row",
                                                select {
                                                    class: "discovery-input discovery-profile-select",
                                                    value: (*selected_profile_idx.read()).map(|i| i.to_string()).unwrap_or_default(),
                                                    onchange: move |e| {
                                                        let val = e.value();
                                                        selected_profile_idx.set(val.parse().ok());
                                                    },
                                                    option { value: "", "Select a profile..." }
                                                    for (idx, score, name) in scored.iter() {
                                                        {
                                                            let idx_val = *idx;
                                                            let pct = (*score * 100.0) as u32;
                                                            let label = if pct > 0 {
                                                                format!("{name} ({pct}% match)")
                                                            } else {
                                                                name.clone()
                                                            };
                                                            rsx! {
                                                                option { value: "{idx_val}", "{label}" }
                                                            }
                                                        }
                                                    }
                                                }
                                                button {
                                                    class: "discovery-action-btn accept primary",
                                                    disabled: selected_profile_idx.read().is_none(),
                                                    onclick: {
                                                        let svc = profile_svc.clone();
                                                        let dev_id = profile_dev_id.clone();
                                                        move |_| {
                                                            if let Some(idx) = *selected_profile_idx.read() {
                                                                let profiles = modbus_profiles.read();
                                                                if let Some(profile) = profiles.get(idx) {
                                                                    let svc = svc.clone();
                                                                    let dev = dev_id.clone();
                                                                    let p = profile.clone();
                                                                    spawn(async move {
                                                                        match svc.apply_modbus_profile(&dev, &p).await {
                                                                            Ok(n) => profile_apply_status.set(Some(format!("Applied {n} point(s)"))),
                                                                            Err(e) => profile_apply_status.set(Some(format!("Error: {e}"))),
                                                                        }
                                                                        selected_profile_idx.set(None);
                                                                        bump(&mut refresh_counter);
                                                                    });
                                                                }
                                                            }
                                                        }
                                                    },
                                                    "Apply"
                                                }
                                            }
                                            if let Some(ref status) = *profile_apply_status.read() {
                                                div { class: "discovery-status-msg", "{status}" }
                                            }
                                        }
                                    }
                                } else {
                                    rsx! {}
                                }
                            }
                        }

                        // Point editing panel (shown when a point is selected for editing)
                        if let Some(ref edit_pid) = *editing_point_id.read() {
                            {
                                let edit_pid = edit_pid.clone();
                                let dev_id = detail_dev_id.clone().unwrap_or_default();
                                let svc = state.discovery_service.clone();
                                rsx! {
                                    div { class: "discovery-point-edit",
                                        h4 { "Edit Point" }
                                        div { class: "login-field",
                                            label { "Display Name" }
                                            input {
                                                r#type: "text",
                                                value: "{point_name_draft.read()}",
                                                oninput: move |evt: Event<FormData>| point_name_draft.set(evt.value()),
                                            }
                                        }
                                        div { class: "login-field",
                                            label { "Units" }
                                            input {
                                                r#type: "text",
                                                value: "{point_units_draft.read()}",
                                                oninput: move |evt: Event<FormData>| point_units_draft.set(evt.value()),
                                            }
                                        }
                                        div { class: "login-field",
                                            label { "Description" }
                                            input {
                                                r#type: "text",
                                                value: "{point_desc_draft.read()}",
                                                oninput: move |evt: Event<FormData>| point_desc_draft.set(evt.value()),
                                            }
                                        }
                                        // State labels for binary/multistate
                                        {
                                            let pk = *point_kind_editing.read();
                                            rsx! {
                                                if pk == PointKindHint::Binary {
                                                    div { class: "login-field",
                                                        label { "State Labels" }
                                                        div { class: "discovery-state-label-row",
                                                            span { class: "discovery-state-label-key", "True:" }
                                                            input {
                                                                class: "discovery-state-label-input",
                                                                r#type: "text",
                                                                placeholder: "On",
                                                                value: "{point_state_labels_draft.read().get(\"true\").cloned().unwrap_or_default()}",
                                                                oninput: move |evt: Event<FormData>| {
                                                                    point_state_labels_draft.write().insert("true".to_string(), evt.value());
                                                                },
                                                            }
                                                        }
                                                        div { class: "discovery-state-label-row",
                                                            span { class: "discovery-state-label-key", "False:" }
                                                            input {
                                                                class: "discovery-state-label-input",
                                                                r#type: "text",
                                                                placeholder: "Off",
                                                                value: "{point_state_labels_draft.read().get(\"false\").cloned().unwrap_or_default()}",
                                                                oninput: move |evt: Event<FormData>| {
                                                                    point_state_labels_draft.write().insert("false".to_string(), evt.value());
                                                                },
                                                            }
                                                        }
                                                    }
                                                } else if pk == PointKindHint::Multistate {
                                                    div { class: "login-field",
                                                        label { "State Labels" }
                                                        {
                                                            let labels = point_state_labels_draft.read().clone();
                                                            let mut keys: Vec<String> = labels.keys().cloned().collect();
                                                            keys.sort_by(|a, b| a.parse::<u32>().unwrap_or(u32::MAX).cmp(&b.parse::<u32>().unwrap_or(u32::MAX)));
                                                            if keys.is_empty() {
                                                                keys = vec!["1".to_string(), "2".to_string(), "3".to_string()];
                                                            }
                                                            rsx! {
                                                                for key in keys.iter() {
                                                                    {
                                                                        let k = key.clone();
                                                                        let k2 = key.clone();
                                                                        let val = labels.get(key).cloned().unwrap_or_default();
                                                                        rsx! {
                                                                            div { class: "discovery-state-label-row",
                                                                                span { class: "discovery-state-label-key", "{k}:" }
                                                                                input {
                                                                                    class: "discovery-state-label-input",
                                                                                    r#type: "text",
                                                                                    placeholder: "State {k2}",
                                                                                    value: "{val}",
                                                                                    oninput: move |evt: Event<FormData>| {
                                                                                        point_state_labels_draft.write().insert(k.clone(), evt.value());
                                                                                    },
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
                                        div { class: "discovery-point-edit-actions",
                                            button {
                                                class: "discovery-action-btn accept primary",
                                                onclick: move |_| {
                                                    let svc = svc.clone();
                                                    let dev = dev_id.clone();
                                                    let pid = edit_pid.clone();
                                                    let name = point_name_draft.read().clone();
                                                    let units = point_units_draft.read().clone();
                                                    let desc = point_desc_draft.read().clone();
                                                    let labels = point_state_labels_draft.read().clone();
                                                    let pk = *point_kind_editing.read();
                                                    spawn(async move {
                                                        let name_opt = if name.is_empty() { None } else { Some(name.as_str()) };
                                                        let units_opt = if units.is_empty() { None } else { Some(units.as_str()) };
                                                        let desc_opt = if desc.is_empty() { None } else { Some(desc.as_str()) };
                                                        let sl_opt = if pk == PointKindHint::Binary || pk == PointKindHint::Multistate {
                                                            let non_empty: HashMap<String, String> = labels.into_iter().filter(|(_, v)| !v.is_empty()).collect();
                                                            if non_empty.is_empty() { Some(None) } else { Some(Some(non_empty)) }
                                                        } else {
                                                            None
                                                        };
                                                        let sl_ref = sl_opt.as_ref().map(|opt| opt.as_ref());
                                                        let _ = svc.update_point(&dev, &pid, name_opt, units_opt, desc_opt, sl_ref).await;
                                                        editing_point_id.set(None);
                                                        bump(&mut refresh_counter);
                                                    });
                                                },
                                                "Save"
                                            }
                                            button {
                                                class: "discovery-action-btn ignore",
                                                onclick: move |_| editing_point_id.set(None),
                                                "Cancel"
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Point table
                        if !points.is_empty() {
                            div { class: "discovery-point-table-wrapper",
                                // Bulk edit bar (shown when points are selected)
                                {
                                    let sel_count = selected_point_ids.read().len();
                                    let dev_id_bulk = detail_dev_id.clone().unwrap_or_default();
                                    let svc_bulk = state.discovery_service.clone();
                                    rsx! {
                                        div { class: "discovery-bulk-bar",
                                            div { class: "discovery-bulk-select-actions",
                                                span { class: "discovery-bulk-label",
                                                    if sel_count > 0 {
                                                        "{sel_count} selected"
                                                    } else {
                                                        "Points ({points.len()})"
                                                    }
                                                }
                                                button {
                                                    class: "btn btn-sm",
                                                    onclick: move |_| {
                                                        let all: std::collections::HashSet<String> = selected_points_signal.read().iter().map(|p| p.id.clone()).collect();
                                                        selected_point_ids.set(all);
                                                    },
                                                    "All"
                                                }
                                                button {
                                                    class: "btn btn-sm",
                                                    onclick: move |_| {
                                                        let filtered: std::collections::HashSet<String> = selected_points_signal.read().iter()
                                                            .filter(|p| p.point_kind == bms_store_storage::discovery::model::PointKindHint::Analog)
                                                            .map(|p| p.id.clone()).collect();
                                                        selected_point_ids.set(filtered);
                                                    },
                                                    "Analog"
                                                }
                                                button {
                                                    class: "btn btn-sm",
                                                    onclick: move |_| {
                                                        let filtered: std::collections::HashSet<String> = selected_points_signal.read().iter()
                                                            .filter(|p| p.point_kind == bms_store_storage::discovery::model::PointKindHint::Binary)
                                                            .map(|p| p.id.clone()).collect();
                                                        selected_point_ids.set(filtered);
                                                    },
                                                    "Binary"
                                                }
                                                if sel_count > 0 {
                                                    button {
                                                        class: "btn btn-sm",
                                                        onclick: move |_| {
                                                            selected_point_ids.set(std::collections::HashSet::new());
                                                        },
                                                        "Clear"
                                                    }
                                                }
                                            }
                                            if sel_count > 0 && user_is_admin {
                                                div { class: "discovery-bulk-edit",
                                                    input {
                                                        class: "discovery-bulk-input",
                                                        r#type: "text",
                                                        placeholder: "Set units...",
                                                        value: "{bulk_units_draft.read()}",
                                                        oninput: move |evt: Event<FormData>| bulk_units_draft.set(evt.value()),
                                                    }
                                                    button {
                                                        class: "discovery-action-btn accept primary",
                                                        disabled: bulk_units_draft.read().is_empty(),
                                                        onclick: move |_| {
                                                            let svc = svc_bulk.clone();
                                                            let dev = dev_id_bulk.clone();
                                                            let ids: Vec<String> = selected_point_ids.read().iter().cloned().collect();
                                                            let units = bulk_units_draft.read().clone();
                                                            spawn(async move {
                                                                match svc.bulk_update_points(&dev, &ids, Some(&units), None).await {
                                                                    Ok(n) => bulk_status.set(Some(format!("Updated {n} point(s)"))),
                                                                    Err(e) => bulk_status.set(Some(format!("Error: {e}"))),
                                                                }
                                                                selected_point_ids.set(std::collections::HashSet::new());
                                                                bulk_units_draft.set(String::new());
                                                                bump(&mut refresh_counter);
                                                            });
                                                        },
                                                        "Apply Units"
                                                    }
                                                    if let Some(ref msg) = *bulk_status.read() {
                                                        span { class: "discovery-bulk-status", "{msg}" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                table { class: "discovery-point-table",
                                    thead {
                                        tr {
                                            if user_is_admin {
                                                th { class: "discovery-col-check", "" }
                                            }
                                            th { "Name" }
                                            th { "Description" }
                                            th { "Units" }
                                            th { "Kind" }
                                            th { "Writable" }
                                        }
                                    }
                                    tbody {
                                        for pt in points.iter() {
                                            {
                                                let pid = pt.id.clone();
                                                let pid_toggle = pid.clone();
                                                let pid_edit = pid.clone();
                                                let is_checked = selected_point_ids.read().contains(&pid);
                                                let is_editing = editing_point_id.read().as_deref() == Some(&pid);
                                                let pt_name = pt.display_name.clone();
                                                let pt_units = pt.units.clone().unwrap_or_default();
                                                let pt_desc = pt.description.clone().unwrap_or_default();
                                                let row_class = if is_editing {
                                                    "discovery-point-row selected"
                                                } else if is_checked {
                                                    "discovery-point-row checked"
                                                } else {
                                                    "discovery-point-row"
                                                };
                                                rsx! {
                                                    tr {
                                                        key: "{pid}",
                                                        class: row_class,
                                                        if user_is_admin {
                                                            td { class: "discovery-col-check",
                                                                input {
                                                                    r#type: "checkbox",
                                                                    checked: is_checked,
                                                                    onchange: move |_| {
                                                                        let mut set = selected_point_ids.write();
                                                                        if set.contains(&pid_toggle) {
                                                                            set.remove(&pid_toggle);
                                                                        } else {
                                                                            set.insert(pid_toggle.clone());
                                                                        }
                                                                    },
                                                                }
                                                            }
                                                        }
                                                        td {
                                                            class: "discovery-point-name-cell",
                                                            onclick: {
                                                                let pt_labels = pt.state_labels.clone().unwrap_or_default();
                                                                let pt_kind_val = pt.point_kind;
                                                                move |_| {
                                                                    if user_is_admin {
                                                                        point_name_draft.set(pt_name.clone());
                                                                        point_units_draft.set(pt_units.clone());
                                                                        point_desc_draft.set(pt_desc.clone());
                                                                        point_state_labels_draft.set(pt_labels.clone());
                                                                        point_kind_editing.set(pt_kind_val);
                                                                        editing_point_id.set(Some(pid_edit.clone()));
                                                                    }
                                                                }
                                                            },
                                                            "{pt.display_name}"
                                                        }
                                                        td { class: "text-muted", "{pt.description.as_deref().unwrap_or(\"—\")}" }
                                                        td { "{pt.units.as_deref().unwrap_or(\"—\")}" }
                                                        td {
                                                            span { class: "discovery-kind-badge", "{kind_label(pt.point_kind)}" }
                                                        }
                                                        td {
                                                            if pt.writable { "Yes" } else { "—" }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            div { class: "discovery-tab-empty",
                                if detail_dev_protocol == Some("modbus") {
                                    p { "No registers probed yet." }
                                    p { class: "discovery-hint",
                                        "Modbus devices don't self-describe their registers. "
                                        "Click below to probe this device and discover available registers."
                                    }
                                    {
                                        let probe_dev_id = detail_dev_id.clone().unwrap_or_default();
                                        let probe_svc = state.discovery_service.clone();
                                        let probe_bridge = state.modbus_handle();
                                        rsx! {
                                            button {
                                                class: "btn btn-primary",
                                                onclick: move |_| {
                                                    let dev = probe_dev_id.clone();
                                                    let svc = probe_svc.clone();
                                                    let bridge_handle = probe_bridge.clone();
                                                    if let Some(bridge_handle) = bridge_handle {
                                                        spawn(async move {
                                                            let guard = bridge_handle.lock().await;
                                                            let bridge = guard.as_any().downcast_ref::<ModbusBridge>().unwrap();
                                                            match svc.probe_modbus_registers(&dev, bridge).await {
                                                                Ok(n) => tracing::info!(device = dev, points = n, "Register probe complete"),
                                                                Err(e) => tracing::error!(device = dev, "Register probe failed: {e}"),
                                                            }
                                                            drop(guard);
                                                            bump(&mut refresh_counter);
                                                        });
                                                    }
                                                },
                                                "Probe Registers"
                                            }
                                        }
                                    }
                                } else {
                                    "No points discovered for this device."
                                }
                            }
                        }
                    },
                    DeviceDetailTab::BacnetManagement if is_bacnet_accepted => rsx! {
                        { render_bacnet_management(state, &detail_dev_id, event_infos, trend_logs) }
                    },
                    DeviceDetailTab::BacnetAlarms if is_bacnet_accepted => rsx! {
                        if let Some(inst) = detail_dev_id.as_ref().and_then(|id| extract_bacnet_instance(id)) {
                            BacnetDeviceAlarms { device_instance: inst }
                        }
                    },
                    DeviceDetailTab::BacnetTrends if is_bacnet_accepted => rsx! {
                        if let Some(inst) = detail_dev_id.as_ref().and_then(|id| extract_bacnet_instance(id)) {
                            BacnetDeviceTrends { device_instance: inst }
                        }
                    },
                    DeviceDetailTab::BacnetFiles if is_bacnet_accepted => rsx! {
                        if let Some(inst) = detail_dev_id.as_ref().and_then(|id| extract_bacnet_instance(id)) {
                            BacnetDeviceFiles { device_instance: inst }
                        }
                    },
                    DeviceDetailTab::BacnetAdvanced if is_bacnet_accepted => rsx! {
                        if let Some(inst) = detail_dev_id.as_ref().and_then(|id| extract_bacnet_instance(id)) {
                            BacnetDeviceAdvanced { device_instance: inst }
                        }
                    },
                    DeviceDetailTab::BacnetObjects if is_bacnet_accepted => rsx! {
                        if let Some(inst) = detail_dev_id.as_ref().and_then(|id| extract_bacnet_instance(id)) {
                            { render_bacnet_objects(state, inst, create_object_type, delete_object_input, commission_status) }
                        }
                    },
                    DeviceDetailTab::ModbusRegisters if is_modbus_accepted => rsx! {
                        if let Some(ref dev_id) = detail_dev_id {
                            {
                                let instance_id = extract_modbus_instance_id(dev_id);
                                rsx! {
                                    ModbusDeviceRegisters { device_id: instance_id }
                                }
                            }
                        }
                    },
                    DeviceDetailTab::ModbusDiagnostics if is_modbus_accepted => rsx! {
                        if let Some(ref dev_id) = detail_dev_id {
                            {
                                let instance_id = extract_modbus_instance_id(dev_id);
                                rsx! {
                                    ModbusDeviceDiagnostics { device_id: instance_id }
                                }
                            }
                        }
                    },
                    _ => rsx! {
                        if !points.is_empty() {
                            div { class: "discovery-point-table-wrapper",
                                h4 { "Points ({points.len()})" }
                                table { class: "discovery-point-table",
                                    thead {
                                        tr {
                                            th { "Name" }
                                            th { "Description" }
                                            th { "Units" }
                                            th { "Kind" }
                                            th { "Writable" }
                                        }
                                    }
                                    tbody {
                                        for pt in points.iter() {
                                            tr { key: "{pt.id}",
                                                td { "{pt.display_name}" }
                                                td { class: "text-muted", "{pt.description.as_deref().unwrap_or(\"—\")}" }
                                                td { "{pt.units.as_deref().unwrap_or(\"—\")}" }
                                                td {
                                                    span { class: "discovery-kind-badge", "{kind_label(pt.point_kind)}" }
                                                }
                                                td {
                                                    if pt.writable { "Yes" } else { "—" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },
                }
            }
        }
    } else {
        rsx! {}
    }
}
