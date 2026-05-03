//! Bridge configuration view — register / edit / delete BACnet networks
//! and Modbus buses without hand-editing `scenario.json` and restarting.
//!
//! Reads + writes [`BridgeStore`]. Mutations emit a Toast warning to
//! restart bms-store for changes to activate.

use dioxus::prelude::*;

use bms_core::{Event, ToastLevel};
use bms_store_storage::store::audit_store::{AuditAction, AuditEntryBuilder};
use bms_store_storage::store::bridge_store::{
    BridgeStore, StoredBacnetNetwork, StoredModbusBus,
};

use crate::gui::state::AppState;

#[derive(Debug, Clone, Copy, PartialEq)]
enum BridgeTab {
    Bacnet,
    Modbus,
}

#[component]
pub fn BridgeSettingsView() -> Element {
    let mut tab = use_signal(|| BridgeTab::Bacnet);
    let current = *tab.read();

    rsx! {
        div { class: "alarm-routing-view",
            div { class: "view-header",
                h2 { "Bridge Configuration" }
                p { class: "view-subtitle",
                    "Register BACnet networks and Modbus buses. Changes take \
                     effect on next bms-store restart."
                }
            }
            div { class: "alarm-tabs",
                button {
                    class: if current == BridgeTab::Bacnet { "alarm-tab active" } else { "alarm-tab" },
                    onclick: move |_| tab.set(BridgeTab::Bacnet),
                    "BACnet Networks"
                }
                button {
                    class: if current == BridgeTab::Modbus { "alarm-tab active" } else { "alarm-tab" },
                    onclick: move |_| tab.set(BridgeTab::Modbus),
                    "Modbus Buses"
                }
            }
            div { class: "alarm-tab-content",
                match current {
                    BridgeTab::Bacnet => rsx! { BacnetTab {} },
                    BridgeTab::Modbus => rsx! { ModbusTab {} },
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// BACnet networks
// ---------------------------------------------------------------------------

#[component]
fn BacnetTab() -> Element {
    let state = use_context::<AppState>();
    let store = state.bridge_store.clone();
    let bus = state.event_bus.clone();
    let audit_store = state.audit_store.clone();
    let current_user = state.current_user;
    let mut networks: Signal<Vec<StoredBacnetNetwork>> = use_signal(Vec::new);
    let mut show_form = use_signal(|| false);
    let mut editing_id = use_signal(|| Option::<i64>::None);
    let mut name = use_signal(String::new);
    let mut mode = use_signal(|| "normal".to_string());
    let mut bbmd_addr = use_signal(String::new);
    let mut serial_port = use_signal(String::new);
    let mut baud_rate = use_signal(|| "38400".to_string());
    let mut server_device_instance = use_signal(String::new);
    let mut enabled = use_signal(|| true);
    let mut error = use_signal(|| Option::<String>::None);

    {
        let store = store.clone();
        let _ = use_resource(move || {
            let store = store.clone();
            async move {
                let list = store.list_bacnet_networks().await;
                networks.set(list);
            }
        });
    }

    let reload = {
        let store = store.clone();
        move || {
            let store = store.clone();
            spawn(async move {
                networks.set(store.list_bacnet_networks().await);
            });
        }
    };

    let begin_add = move |_| {
        editing_id.set(None);
        name.set(String::new());
        mode.set("normal".into());
        bbmd_addr.set(String::new());
        serial_port.set(String::new());
        baud_rate.set("38400".into());
        server_device_instance.set(String::new());
        enabled.set(true);
        error.set(None);
        show_form.set(true);
    };

    let cancel = move |_| {
        show_form.set(false);
        error.set(None);
    };

    let submit = {
        let store = store.clone();
        let bus = bus.clone();
        let audit_store = audit_store.clone();
        let reload = reload.clone();
        move |_| {
            let n = name.read().trim().to_string();
            if n.is_empty() {
                error.set(Some("Name is required.".into()));
                return;
            }
            // Build the JSON body from the form fields. We only set keys
            // that have non-empty values so scenario.rs's serde defaults apply.
            let mut cfg = serde_json::Map::new();
            cfg.insert("mode".into(), serde_json::json!(mode.read().clone()));
            let bbmd = bbmd_addr.read().trim().to_string();
            if !bbmd.is_empty() {
                cfg.insert("bbmd_addr".into(), serde_json::json!(bbmd));
            }
            let sp = serial_port.read().trim().to_string();
            if !sp.is_empty() {
                cfg.insert("serial_port".into(), serde_json::json!(sp));
                if let Ok(b) = baud_rate.read().trim().parse::<u32>() {
                    cfg.insert("baud_rate".into(), serde_json::json!(b));
                }
            }
            let sdi = server_device_instance.read().trim().to_string();
            if !sdi.is_empty() {
                if let Ok(v) = sdi.parse::<u32>() {
                    cfg.insert("server_device_instance".into(), serde_json::json!(v));
                }
            }
            let cfg_json = serde_json::Value::Object(cfg).to_string();
            let is_enabled = *enabled.read();
            let edit_id = *editing_id.read();
            let store = store.clone();
            let bus = bus.clone();
            let audit_store = audit_store.clone();
            let reload = reload.clone();
            let cu = current_user.read().clone();
            spawn(async move {
                let result = if let Some(id) = edit_id {
                    store
                        .update_bacnet_network(id, &cfg_json, is_enabled)
                        .await
                        .map(|_| id)
                        .map_err(|e| e.to_string())
                } else {
                    store
                        .create_bacnet_network(&n, &cfg_json, is_enabled)
                        .await
                        .map(|net| net.id)
                        .map_err(|e| e.to_string())
                };
                match result {
                    Ok(id) => {
                        let action = if edit_id.is_some() {
                            AuditAction::UpdateBacnetNetwork
                        } else {
                            AuditAction::CreateBacnetNetwork
                        };
                        if let Some(u) = cu {
                            let _ = audit_store
                                .log_action(
                                    &u.id,
                                    &u.username,
                                    AuditEntryBuilder::new(action, "bridge_bacnet")
                                        .resource_id(&id.to_string())
                                        .details(&n),
                                )
                                .await;
                        }
                        bus.publish(Event::toast(
                            ToastLevel::Warn,
                            "bridges",
                            format!("BACnet network `{n}` saved — restart bms-store to activate"),
                        ));
                        show_form.set(false);
                        reload();
                    }
                    Err(e) => error.set(Some(e)),
                }
            });
        }
    };

    let edit_one = {
        let store = store.clone();
        move |net: StoredBacnetNetwork| {
            editing_id.set(Some(net.id));
            name.set(net.name.clone());
            enabled.set(net.enabled);
            // Parse the persisted JSON back into form fields.
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&net.config_json) {
                if let Some(s) = v.get("mode").and_then(|m| m.as_str()) {
                    mode.set(s.to_string());
                }
                bbmd_addr.set(
                    v.get("bbmd_addr")
                        .and_then(|m| m.as_str())
                        .unwrap_or("")
                        .to_string(),
                );
                serial_port.set(
                    v.get("serial_port")
                        .and_then(|m| m.as_str())
                        .unwrap_or("")
                        .to_string(),
                );
                if let Some(b) = v.get("baud_rate").and_then(|m| m.as_u64()) {
                    baud_rate.set(b.to_string());
                }
                if let Some(s) = v.get("server_device_instance").and_then(|m| m.as_u64()) {
                    server_device_instance.set(s.to_string());
                }
            }
            error.set(None);
            show_form.set(true);
            // Suppress unused warning
            let _ = &store;
        }
    };

    let delete_one = {
        let store = store.clone();
        let bus = bus.clone();
        let audit_store = audit_store.clone();
        let reload = reload.clone();
        move |net: StoredBacnetNetwork| {
            let store = store.clone();
            let bus = bus.clone();
            let audit_store = audit_store.clone();
            let reload = reload.clone();
            let cu = current_user.read().clone();
            spawn(async move {
                let label = net.name.clone();
                if let Err(e) = store.delete_bacnet_network(net.id).await {
                    bus.publish(Event::toast(
                        ToastLevel::Error,
                        "bridges",
                        format!("Failed to delete BACnet network: {e}"),
                    ));
                    return;
                }
                if let Some(u) = cu {
                    let _ = audit_store
                        .log_action(
                            &u.id,
                            &u.username,
                            AuditEntryBuilder::new(
                                AuditAction::DeleteBacnetNetwork,
                                "bridge_bacnet",
                            )
                            .resource_id(&net.id.to_string())
                            .details(&label),
                        )
                        .await;
                }
                bus.publish(Event::toast(
                    ToastLevel::Warn,
                    "bridges",
                    format!("BACnet network `{label}` removed — restart bms-store"),
                ));
                reload();
            });
        }
    };

    let nets = networks.read().clone();
    let editing = (*editing_id.read()).is_some();
    let is_serial = mode.read().as_str() == "mstp";
    let is_normal = mode.read().as_str() == "normal";

    rsx! {
        div { class: "settings-actions",
            button { class: "btn btn-primary", onclick: begin_add, "+ Add BACnet Network" }
        }

        if *show_form.read() {
            div { class: "settings-form",
                h3 { if editing { "Edit BACnet Network" } else { "New BACnet Network" } }
                if let Some(msg) = error.read().as_ref() {
                    div { class: "form-error", "{msg}" }
                }
                div { class: "form-row",
                    label { "Name" }
                    input {
                        r#type: "text",
                        value: "{name}",
                        oninput: move |e| name.set(e.value()),
                        disabled: editing, // name is unique key — locked after create
                    }
                }
                div { class: "form-row",
                    label { "Mode" }
                    select {
                        value: "{mode}",
                        onchange: move |e| mode.set(e.value()),
                        option { value: "normal", "Normal (BACnet/IP)" }
                        option { value: "foreign", "Foreign Device (BBMD)" }
                        option { value: "sc", "BACnet/SC (WebSocket)" }
                        option { value: "mstp", "MS/TP (Serial)" }
                    }
                }
                if !is_normal && !is_serial {
                    div { class: "form-row",
                        label { "BBMD / Hub Address" }
                        input {
                            r#type: "text",
                            placeholder: "192.168.1.1:47808",
                            value: "{bbmd_addr}",
                            oninput: move |e| bbmd_addr.set(e.value()),
                        }
                    }
                }
                if is_serial {
                    div { class: "form-row",
                        label { "Serial Port" }
                        input {
                            r#type: "text",
                            placeholder: "/dev/ttyUSB0",
                            value: "{serial_port}",
                            oninput: move |e| serial_port.set(e.value()),
                        }
                    }
                    div { class: "form-row",
                        label { "Baud Rate" }
                        input {
                            r#type: "text",
                            value: "{baud_rate}",
                            oninput: move |e| baud_rate.set(e.value()),
                        }
                    }
                }
                div { class: "form-row",
                    label { "Server Device Instance (optional)" }
                    input {
                        r#type: "text",
                        placeholder: "e.g. 1234",
                        value: "{server_device_instance}",
                        oninput: move |e| server_device_instance.set(e.value()),
                    }
                }
                div { class: "form-row",
                    label {
                        input {
                            r#type: "checkbox",
                            checked: *enabled.read(),
                            onchange: move |e| enabled.set(e.value() == "true"),
                        }
                        " Enabled"
                    }
                }
                div { class: "form-actions",
                    button { class: "btn btn-primary", onclick: submit, "Save" }
                    button { class: "btn", onclick: cancel, "Cancel" }
                }
            }
        }

        table { class: "settings-table",
            thead {
                tr {
                    th { "Name" }
                    th { "Mode" }
                    th { "Address" }
                    th { "Enabled" }
                    th {}
                }
            }
            tbody {
                for net in nets {
                    {
                        let cfg: serde_json::Value =
                            serde_json::from_str(&net.config_json).unwrap_or(serde_json::Value::Null);
                        let mode_str = cfg.get("mode").and_then(|v| v.as_str()).unwrap_or("normal").to_string();
                        let addr = cfg
                            .get("bbmd_addr")
                            .or_else(|| cfg.get("serial_port"))
                            .or_else(|| cfg.get("hub_endpoint"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("—")
                            .to_string();
                        let net_for_edit = net.clone();
                        let net_for_del = net.clone();
                        let mut edit_one = edit_one.clone();
                        let mut delete_one = delete_one.clone();
                        rsx! {
                            tr { key: "{net.id}",
                                td { "{net.name}" }
                                td { "{mode_str}" }
                                td { "{addr}" }
                                td { if net.enabled { "✓" } else { "—" } }
                                td { class: "row-actions",
                                    button {
                                        class: "btn btn-sm",
                                        onclick: move |_| edit_one(net_for_edit.clone()),
                                        "Edit"
                                    }
                                    button {
                                        class: "btn btn-sm btn-danger",
                                        onclick: move |_| delete_one(net_for_del.clone()),
                                        "Delete"
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

// ---------------------------------------------------------------------------
// Modbus buses
// ---------------------------------------------------------------------------

#[component]
fn ModbusTab() -> Element {
    let state = use_context::<AppState>();
    let store = state.bridge_store.clone();
    let bus = state.event_bus.clone();
    let audit_store = state.audit_store.clone();
    let current_user = state.current_user;
    let mut buses: Signal<Vec<StoredModbusBus>> = use_signal(Vec::new);
    let mut show_form = use_signal(|| false);
    let mut editing_id = use_signal(|| Option::<i64>::None);
    let mut name = use_signal(String::new);
    let mut mode = use_signal(|| "tcp".to_string());
    let mut serial_port = use_signal(String::new);
    let mut baud_rate = use_signal(|| "9600".to_string());
    let mut timeout_ms = use_signal(|| "5000".to_string());
    let mut retry_count = use_signal(|| "3".to_string());
    let mut enabled = use_signal(|| true);
    let mut error = use_signal(|| Option::<String>::None);

    {
        let store = store.clone();
        let _ = use_resource(move || {
            let store = store.clone();
            async move {
                buses.set(store.list_modbus_buses().await);
            }
        });
    }

    let reload = {
        let store = store.clone();
        move || {
            let store = store.clone();
            spawn(async move {
                buses.set(store.list_modbus_buses().await);
            });
        }
    };

    let begin_add = move |_| {
        editing_id.set(None);
        name.set(String::new());
        mode.set("tcp".into());
        serial_port.set(String::new());
        baud_rate.set("9600".into());
        timeout_ms.set("5000".into());
        retry_count.set("3".into());
        enabled.set(true);
        error.set(None);
        show_form.set(true);
    };

    let cancel = move |_| {
        show_form.set(false);
        error.set(None);
    };

    let submit = {
        let store = store.clone();
        let bus = bus.clone();
        let audit_store = audit_store.clone();
        let reload = reload.clone();
        move |_| {
            let n = name.read().trim().to_string();
            if n.is_empty() {
                error.set(Some("Name is required.".into()));
                return;
            }
            let mut cfg = serde_json::Map::new();
            cfg.insert("mode".into(), serde_json::json!(mode.read().clone()));
            let sp = serial_port.read().trim().to_string();
            if mode.read().as_str() == "rtu" && !sp.is_empty() {
                cfg.insert("serial_port".into(), serde_json::json!(sp));
                if let Ok(b) = baud_rate.read().trim().parse::<u32>() {
                    cfg.insert("baud_rate".into(), serde_json::json!(b));
                }
            }
            if let Ok(t) = timeout_ms.read().trim().parse::<u64>() {
                cfg.insert("default_timeout_ms".into(), serde_json::json!(t));
            }
            if let Ok(r) = retry_count.read().trim().parse::<u8>() {
                cfg.insert("default_retry_count".into(), serde_json::json!(r));
            }
            let cfg_json = serde_json::Value::Object(cfg).to_string();
            let is_enabled = *enabled.read();
            let edit_id = *editing_id.read();
            let store = store.clone();
            let bus = bus.clone();
            let audit_store = audit_store.clone();
            let reload = reload.clone();
            let cu = current_user.read().clone();
            spawn(async move {
                let result = if let Some(id) = edit_id {
                    store
                        .update_modbus_bus(id, &cfg_json, is_enabled)
                        .await
                        .map(|_| id)
                        .map_err(|e| e.to_string())
                } else {
                    store
                        .create_modbus_bus(&n, &cfg_json, is_enabled)
                        .await
                        .map(|b| b.id)
                        .map_err(|e| e.to_string())
                };
                match result {
                    Ok(id) => {
                        let action = if edit_id.is_some() {
                            AuditAction::UpdateModbusBus
                        } else {
                            AuditAction::CreateModbusBus
                        };
                        if let Some(u) = cu {
                            let _ = audit_store
                                .log_action(
                                    &u.id,
                                    &u.username,
                                    AuditEntryBuilder::new(action, "bridge_modbus")
                                        .resource_id(&id.to_string())
                                        .details(&n),
                                )
                                .await;
                        }
                        bus.publish(Event::toast(
                            ToastLevel::Warn,
                            "bridges",
                            format!("Modbus bus `{n}` saved — restart bms-store to activate"),
                        ));
                        show_form.set(false);
                        reload();
                    }
                    Err(e) => error.set(Some(e)),
                }
            });
        }
    };

    let edit_one = move |b: StoredModbusBus| {
        editing_id.set(Some(b.id));
        name.set(b.name.clone());
        enabled.set(b.enabled);
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&b.config_json) {
            if let Some(s) = v.get("mode").and_then(|m| m.as_str()) {
                mode.set(s.to_string());
            }
            serial_port.set(
                v.get("serial_port")
                    .and_then(|m| m.as_str())
                    .unwrap_or("")
                    .to_string(),
            );
            if let Some(br) = v.get("baud_rate").and_then(|m| m.as_u64()) {
                baud_rate.set(br.to_string());
            }
            if let Some(t) = v.get("default_timeout_ms").and_then(|m| m.as_u64()) {
                timeout_ms.set(t.to_string());
            }
            if let Some(r) = v.get("default_retry_count").and_then(|m| m.as_u64()) {
                retry_count.set(r.to_string());
            }
        }
        error.set(None);
        show_form.set(true);
    };

    let delete_one = {
        let store = store.clone();
        let bus = bus.clone();
        let audit_store = audit_store.clone();
        let reload = reload.clone();
        move |b: StoredModbusBus| {
            let store = store.clone();
            let bus = bus.clone();
            let audit_store = audit_store.clone();
            let reload = reload.clone();
            let cu = current_user.read().clone();
            spawn(async move {
                let label = b.name.clone();
                if let Err(e) = store.delete_modbus_bus(b.id).await {
                    bus.publish(Event::toast(
                        ToastLevel::Error,
                        "bridges",
                        format!("Failed to delete Modbus bus: {e}"),
                    ));
                    return;
                }
                if let Some(u) = cu {
                    let _ = audit_store
                        .log_action(
                            &u.id,
                            &u.username,
                            AuditEntryBuilder::new(
                                AuditAction::DeleteModbusBus,
                                "bridge_modbus",
                            )
                            .resource_id(&b.id.to_string())
                            .details(&label),
                        )
                        .await;
                }
                bus.publish(Event::toast(
                    ToastLevel::Warn,
                    "bridges",
                    format!("Modbus bus `{label}` removed — restart bms-store"),
                ));
                reload();
            });
        }
    };

    let bs = buses.read().clone();
    let editing = (*editing_id.read()).is_some();
    let is_rtu = mode.read().as_str() == "rtu";

    let _ = &store; // silence unused warning when no spawn captures it

    rsx! {
        div { class: "settings-actions",
            button { class: "btn btn-primary", onclick: begin_add, "+ Add Modbus Bus" }
        }

        if *show_form.read() {
            div { class: "settings-form",
                h3 { if editing { "Edit Modbus Bus" } else { "New Modbus Bus" } }
                if let Some(msg) = error.read().as_ref() {
                    div { class: "form-error", "{msg}" }
                }
                div { class: "form-row",
                    label { "Name" }
                    input {
                        r#type: "text",
                        value: "{name}",
                        oninput: move |e| name.set(e.value()),
                        disabled: editing,
                    }
                }
                div { class: "form-row",
                    label { "Mode" }
                    select {
                        value: "{mode}",
                        onchange: move |e| mode.set(e.value()),
                        option { value: "tcp", "TCP" }
                        option { value: "rtu", "RTU (Serial)" }
                    }
                }
                if is_rtu {
                    div { class: "form-row",
                        label { "Serial Port" }
                        input {
                            r#type: "text",
                            placeholder: "/dev/ttyUSB0",
                            value: "{serial_port}",
                            oninput: move |e| serial_port.set(e.value()),
                        }
                    }
                    div { class: "form-row",
                        label { "Baud Rate" }
                        input {
                            r#type: "text",
                            value: "{baud_rate}",
                            oninput: move |e| baud_rate.set(e.value()),
                        }
                    }
                }
                div { class: "form-row",
                    label { "Default Timeout (ms)" }
                    input {
                        r#type: "text",
                        value: "{timeout_ms}",
                        oninput: move |e| timeout_ms.set(e.value()),
                    }
                }
                div { class: "form-row",
                    label { "Default Retry Count" }
                    input {
                        r#type: "text",
                        value: "{retry_count}",
                        oninput: move |e| retry_count.set(e.value()),
                    }
                }
                div { class: "form-row",
                    label {
                        input {
                            r#type: "checkbox",
                            checked: *enabled.read(),
                            onchange: move |e| enabled.set(e.value() == "true"),
                        }
                        " Enabled"
                    }
                }
                div { class: "form-actions",
                    button { class: "btn btn-primary", onclick: submit, "Save" }
                    button { class: "btn", onclick: cancel, "Cancel" }
                }
            }
        }

        table { class: "settings-table",
            thead {
                tr {
                    th { "Name" }
                    th { "Mode" }
                    th { "Port" }
                    th { "Enabled" }
                    th {}
                }
            }
            tbody {
                for b in bs {
                    {
                        let cfg: serde_json::Value =
                            serde_json::from_str(&b.config_json).unwrap_or(serde_json::Value::Null);
                        let mode_str = cfg.get("mode").and_then(|v| v.as_str()).unwrap_or("tcp").to_string();
                        let port = cfg
                            .get("serial_port")
                            .and_then(|v| v.as_str())
                            .unwrap_or("(TCP — set per-device)")
                            .to_string();
                        let bus_for_edit = b.clone();
                        let bus_for_del = b.clone();
                        let mut edit_one = edit_one.clone();
                        let mut delete_one = delete_one.clone();
                        rsx! {
                            tr { key: "{b.id}",
                                td { "{b.name}" }
                                td { "{mode_str}" }
                                td { "{port}" }
                                td { if b.enabled { "✓" } else { "—" } }
                                td { class: "row-actions",
                                    button {
                                        class: "btn btn-sm",
                                        onclick: move |_| edit_one(bus_for_edit.clone()),
                                        "Edit"
                                    }
                                    button {
                                        class: "btn btn-sm btn-danger",
                                        onclick: move |_| delete_one(bus_for_del.clone()),
                                        "Delete"
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

// Drop-in stub so we don't trigger the unused-import warning.
#[allow(dead_code)]
fn _bridge_store_marker(_b: BridgeStore) {}
