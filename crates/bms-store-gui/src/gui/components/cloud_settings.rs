#[cfg(feature = "cloud")]
use dioxus::prelude::*;

#[cfg(feature = "cloud")]
use crate::auth::Permission;
#[cfg(feature = "cloud")]
use crate::cloud::{CloudBridgeConfig, CloudBridgeStatus, CloudProvider};
#[cfg(feature = "cloud")]
use crate::gui::state::AppState;
#[cfg(feature = "cloud")]
use bms_store_storage::store::audit_store::{AuditAction, AuditEntryBuilder};

/// Fields that contain secrets — redacted in the GUI edit form.
#[cfg(feature = "cloud")]
const SECRET_FIELDS: &[&str] = &[
    "key",
    "key_pem_path",
    "key_path",
    "cert_pem_path",
    "cert_path",
    "credentials_json_path",
    "private_key",
];

#[cfg(feature = "cloud")]
const REDACTED: &str = "***REDACTED***";

/// Redact secret fields in a config JSON string for display.
#[cfg(feature = "cloud")]
fn redact_config_for_display(config_json: &str) -> String {
    let mut val: serde_json::Value = serde_json::from_str(config_json).unwrap_or_default();
    redact_object(&mut val);
    serde_json::to_string_pretty(&val).unwrap_or_else(|_| config_json.to_string())
}

#[cfg(feature = "cloud")]
fn redact_object(val: &mut serde_json::Value) {
    if let Some(obj) = val.as_object_mut() {
        for &field in SECRET_FIELDS {
            if obj.contains_key(field) {
                obj.insert(field.to_string(), serde_json::json!(REDACTED));
            }
        }
        // Handle nested objects (e.g. auth_method)
        for (_, v) in obj.iter_mut() {
            if v.is_object() {
                redact_object(v);
            }
        }
    }
}

/// Merge user-edited config with the original, preserving redacted fields.
/// If the user left a field as REDACTED, keep the original value.
#[cfg(feature = "cloud")]
fn merge_with_original(edited: &str, original: &str) -> String {
    let mut edited_val: serde_json::Value = serde_json::from_str(edited).unwrap_or_default();
    let orig_val: serde_json::Value = serde_json::from_str(original).unwrap_or_default();
    restore_redacted(&mut edited_val, &orig_val);
    serde_json::to_string(&edited_val).unwrap_or_else(|_| edited.to_string())
}

#[cfg(feature = "cloud")]
fn restore_redacted(edited: &mut serde_json::Value, original: &serde_json::Value) {
    if let (Some(e_obj), Some(o_obj)) = (edited.as_object_mut(), original.as_object()) {
        for (key, e_val) in e_obj.iter_mut() {
            if e_val.as_str() == Some(REDACTED) {
                if let Some(orig) = o_obj.get(key) {
                    *e_val = orig.clone();
                }
            } else if e_val.is_object() {
                if let Some(orig) = o_obj.get(key) {
                    restore_redacted(e_val, orig);
                }
            }
        }
    }
}

#[cfg(feature = "cloud")]
#[derive(Debug, Clone, Copy, PartialEq)]
enum CloudTab {
    Bridges,
    Status,
}

#[cfg(feature = "cloud")]
#[component]
pub fn CloudSettingsView() -> Element {
    let state = use_context::<AppState>();
    let can_manage = state.has_permission(Permission::ManageCloud);
    let mut tab = use_signal(|| CloudTab::Bridges);
    let current_tab = *tab.read();

    rsx! {
        div { class: "alarm-routing-view",
            div { class: "alarm-tabs",
                button {
                    class: if current_tab == CloudTab::Bridges { "alarm-tab active" } else { "alarm-tab" },
                    onclick: move |_| tab.set(CloudTab::Bridges),
                    "Bridges"
                }
                button {
                    class: if current_tab == CloudTab::Status { "alarm-tab active" } else { "alarm-tab" },
                    onclick: move |_| tab.set(CloudTab::Status),
                    "Status"
                }
            }

            div { class: "alarm-tab-content",
                match current_tab {
                    CloudTab::Bridges => rsx! { BridgesTab { can_manage } },
                    CloudTab::Status => rsx! { StatusTab {} },
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Bridges tab
// ----------------------------------------------------------------

#[cfg(feature = "cloud")]
#[component]
fn BridgesTab(can_manage: bool) -> Element {
    let state = use_context::<AppState>();
    let cs = state.cloud_store.clone();
    let audit_store = state.audit_store.clone();
    let current_user = state.current_user;
    let mut bridges: Signal<Vec<CloudBridgeConfig>> = use_signal(Vec::new);
    let mut show_form = use_signal(|| false);
    let mut editing_id = use_signal(|| Option::<String>::None);
    let mut form_name = use_signal(String::new);
    let mut form_provider = use_signal(|| "aws_iot_core".to_string());
    let mut form_config = use_signal(|| "{}".to_string());
    let mut form_enabled = use_signal(|| true);
    let mut form_on_values = use_signal(|| true);
    let mut form_on_alarms = use_signal(|| true);
    let mut form_on_fdd = use_signal(|| true);
    let mut form_on_device_status = use_signal(|| true);
    let mut status_msg: Signal<Option<String>> = use_signal(|| None);
    let mut refresh = use_signal(|| 0u64);

    // Load bridges
    {
        let cs = cs.clone();
        let _r = *refresh.read();
        use_effect(move || {
            let cs = cs.clone();
            spawn(async move {
                bridges.set(cs.list_bridges().await);
            });
        });
    }

    let new_bridge = move |_| {
        form_name.set(String::new());
        form_provider.set("aws_iot_core".to_string());
        form_config.set("{}".to_string());
        form_enabled.set(true);
        form_on_values.set(true);
        form_on_alarms.set(true);
        form_on_fdd.set(true);
        form_on_device_status.set(true);
        editing_id.set(None);
        show_form.set(true);
        status_msg.set(None);
    };

    let save_bridge = move |_| {
        let cs = cs.clone();
        let audit = audit_store.clone();
        let name = form_name.read().clone();
        let provider = form_provider.read().clone();
        let config = form_config.read().clone();
        let enabled = *form_enabled.read();
        let on_values = *form_on_values.read();
        let on_alarms = *form_on_alarms.read();
        let on_fdd = *form_on_fdd.read();
        let on_device_status = *form_on_device_status.read();
        let edit_id = editing_id.read().clone();
        let user_id = current_user
            .read()
            .as_ref()
            .map(|u| u.id.clone())
            .unwrap_or_default();
        let user_name = current_user
            .read()
            .as_ref()
            .map(|u| u.username.clone())
            .unwrap_or_default();

        spawn(async move {
            // Validate JSON config
            if serde_json::from_str::<serde_json::Value>(&config).is_err() {
                status_msg.set(Some("Error: Config must be valid JSON".to_string()));
                return;
            }

            let result = if let Some(id) = &edit_id {
                // Merge redacted fields back from stored config
                let final_config = if let Some(existing) = cs.get_bridge(id).await {
                    merge_with_original(&config, &existing.config)
                } else {
                    config.clone()
                };
                cs.update_bridge(
                    id,
                    &name,
                    &provider,
                    &final_config,
                    enabled,
                    on_values,
                    on_alarms,
                    on_fdd,
                    on_device_status,
                )
                .await
            } else {
                let id = format!(
                    "cloud-{}",
                    uuid::Uuid::new_v4()
                        .to_string()
                        .split('-')
                        .next()
                        .unwrap_or("0")
                );
                cs.create_bridge(
                    &id,
                    &name,
                    &provider,
                    &config,
                    on_values,
                    on_alarms,
                    on_fdd,
                    on_device_status,
                )
                .await
            };

            match result {
                Ok(()) => {
                    let action = if edit_id.is_some() {
                        AuditAction::UpdateCloudBridge
                    } else {
                        AuditAction::CreateCloudBridge
                    };
                    let _ = audit
                        .log_action(
                            &user_id,
                            &user_name,
                            AuditEntryBuilder::new(action, "cloud_bridge")
                                .details(&format!("Saved cloud bridge '{}'", name)),
                        )
                        .await;
                    show_form.set(false);
                    refresh.set(refresh.cloned() + 1);
                    status_msg.set(None);
                }
                Err(e) => {
                    status_msg.set(Some(format!("Error: {e}")));
                }
            }
        });
    };

    let test_connection = {
        let cs_test = state.cloud_store.clone();
        let audit_test = state.audit_store.clone();
        move |_| {
            let provider = form_provider.read().clone();
            let config = form_config.read().clone();
            let name = form_name.read().clone();
            let audit = audit_test.clone();
            let _cs = cs_test.clone();
            let user_id = current_user
                .read()
                .as_ref()
                .map(|u| u.id.clone())
                .unwrap_or_default();
            let user_name = current_user
                .read()
                .as_ref()
                .map(|u| u.username.clone())
                .unwrap_or_default();

            spawn(async move {
                status_msg.set(Some("Testing connection...".to_string()));
                match crate::cloud::build_connector(&provider, &config) {
                    Ok(mut connector) => {
                        // Must call connect() before test_connection() — for
                        // the implemented providers, test_connection() reports
                        // "not connected" unless connect() has already run.
                        // Mirrors the API route at src/api/routes/cloud.rs.
                        if let Err(e) = connector.connect().await {
                            status_msg.set(Some(format!("Connect failed: {e}")));
                            return;
                        }
                        let result = connector.test_connection().await;
                        connector.close().await;
                        match result {
                            Ok(()) => {
                                let _ = audit
                                    .log_action(
                                        &user_id,
                                        &user_name,
                                        AuditEntryBuilder::new(
                                            AuditAction::TestCloudBridge,
                                            "cloud_bridge",
                                        )
                                        .details(&format!("Test passed for '{}'", name)),
                                    )
                                    .await;
                                status_msg.set(Some("Connection test passed.".to_string()));
                            }
                            Err(e) => {
                                status_msg.set(Some(format!("Test failed: {e}")));
                            }
                        }
                    }
                    Err(e) => {
                        status_msg.set(Some(format!("Config error: {e}")));
                    }
                }
            });
        }
    };

    rsx! {
        div { class: "alarm-routing-section",
            div { style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px;",
                h3 { "Cloud Bridges" }
                if can_manage {
                    button { class: "btn btn-primary", onclick: new_bridge, "+ Add Bridge" }
                }
            }

            if *show_form.read() {
                div { class: "alarm-routing-form",
                    h4 { if editing_id.read().is_some() { "Edit Bridge" } else { "New Bridge" } }
                    div { class: "form-row",
                        label { "Name" }
                        input {
                            value: "{form_name}",
                            oninput: move |e| form_name.set(e.value()),
                            placeholder: "e.g. AWS Production",
                        }
                    }
                    div { class: "form-row",
                        label { "Provider" }
                        select {
                            value: "{form_provider}",
                            onchange: move |e| form_provider.set(e.value()),
                            for p in CloudProvider::all() {
                                option { value: "{p.as_str()}", "{p.label()}" }
                            }
                        }
                    }
                    div { class: "form-row",
                        label { "Config (JSON)" }
                        textarea {
                            value: "{form_config}",
                            oninput: move |e| form_config.set(e.value()),
                            rows: "8",
                            style: "font-family: monospace; width: 100%; resize: vertical;",
                            placeholder: "Provider-specific configuration as JSON...",
                        }
                    }
                    div { class: "form-row",
                        label {
                            input {
                                r#type: "checkbox",
                                checked: *form_enabled.read(),
                                onchange: move |e| form_enabled.set(e.checked()),
                            }
                            " Enabled"
                        }
                    }
                    div { class: "form-row",
                        label {
                            input {
                                r#type: "checkbox",
                                checked: *form_on_values.read(),
                                onchange: move |e| form_on_values.set(e.checked()),
                            }
                            " Publish values"
                        }
                        label {
                            input {
                                r#type: "checkbox",
                                checked: *form_on_alarms.read(),
                                onchange: move |e| form_on_alarms.set(e.checked()),
                            }
                            " Publish alarms"
                        }
                        label {
                            input {
                                r#type: "checkbox",
                                checked: *form_on_fdd.read(),
                                onchange: move |e| form_on_fdd.set(e.checked()),
                            }
                            " Publish FDD faults"
                        }
                        label {
                            input {
                                r#type: "checkbox",
                                checked: *form_on_device_status.read(),
                                onchange: move |e| form_on_device_status.set(e.checked()),
                            }
                            " Publish device status"
                        }
                    }
                    if let Some(msg) = status_msg.read().as_ref() {
                        div { class: "form-error", "{msg}" }
                    }
                    div { class: "form-actions",
                        button { class: "btn btn-primary", onclick: save_bridge, "Save" }
                        if can_manage {
                            button { class: "btn", onclick: test_connection, "Test Connection" }
                        }
                        button {
                            class: "btn",
                            onclick: move |_| show_form.set(false),
                            "Cancel"
                        }
                    }
                }
            }

            table { class: "alarm-table",
                thead {
                    tr {
                        th { "Name" }
                        th { "Provider" }
                        th { "Enabled" }
                        th { "Values" }
                        th { "Alarms" }
                        th { "FDD" }
                        th { "Status" }
                        if can_manage { th { "Actions" } }
                    }
                }
                tbody {
                    for bridge in bridges.read().iter() {
                        {
                            let bid = bridge.id.clone();
                            let bname = bridge.name.clone();
                            let bprovider = bridge.provider.clone();
                            let enabled = bridge.enabled;
                            let on_v = bridge.on_values;
                            let on_a = bridge.on_alarms;
                            let on_f = bridge.on_fdd;
                            let on_ds = bridge.on_device_status;
                            let provider_label = CloudProvider::from_str(&bprovider)
                                .map(|p| p.label())
                                .unwrap_or("Unknown");
                            let provider_color = provider_badge_color(&bprovider);
                            let cs_del = state.cloud_store.clone();
                            let audit_del = state.audit_store.clone();
                            let user_id_del = current_user.read().as_ref().map(|u| u.id.clone()).unwrap_or_default();
                            let user_name_del = current_user.read().as_ref().map(|u| u.username.clone()).unwrap_or_default();

                            // Clone values for edit closure
                            let edit_bid = bid.clone();
                            let edit_name = bname.clone();
                            let edit_provider = bprovider.clone();
                            let edit_config = redact_config_for_display(&bridge.config);
                            let edit_enabled = enabled;
                            let edit_on_v = on_v;
                            let edit_on_a = on_a;
                            let edit_on_f = on_f;
                            let edit_on_ds = on_ds;

                            rsx! {
                                tr {
                                    td { "{bname}" }
                                    td {
                                        span {
                                            class: "badge",
                                            style: "background: {provider_color}; color: #fff;",
                                            "{provider_label}"
                                        }
                                    }
                                    td { if enabled { "Yes" } else { "No" } }
                                    td { if on_v { "Yes" } else { "No" } }
                                    td { if on_a { "Yes" } else { "No" } }
                                    td { if on_f { "Yes" } else { "No" } }
                                    td { if on_ds { "Yes" } else { "No" } }
                                    if can_manage {
                                        td {
                                            button {
                                                class: "btn btn-small",
                                                onclick: move |_| {
                                                    editing_id.set(Some(edit_bid.clone()));
                                                    form_name.set(edit_name.clone());
                                                    form_provider.set(edit_provider.clone());
                                                    form_config.set(edit_config.clone());
                                                    form_enabled.set(edit_enabled);
                                                    form_on_values.set(edit_on_v);
                                                    form_on_alarms.set(edit_on_a);
                                                    form_on_fdd.set(edit_on_f);
                                                    form_on_device_status.set(edit_on_ds);
                                                    show_form.set(true);
                                                    status_msg.set(None);
                                                },
                                                "Edit"
                                            }
                                            button {
                                                class: "btn btn-small btn-danger",
                                                onclick: move |_| {
                                                    let cs = cs_del.clone();
                                                    let audit = audit_del.clone();
                                                    let uid = user_id_del.clone();
                                                    let uname = user_name_del.clone();
                                                    let id = bid.clone();
                                                    spawn(async move {
                                                        let _ = cs.delete_bridge(&id).await;
                                                        let _ = audit.log_action(&uid, &uname,
                                                            AuditEntryBuilder::new(AuditAction::DeleteCloudBridge, "cloud_bridge")
                                                                .details(&format!("Deleted cloud bridge '{}'", id)),
                                                        ).await;
                                                        refresh.set(refresh.cloned() + 1);
                                                    });
                                                },
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

            if bridges.read().is_empty() {
                div { class: "empty-state",
                    p { "No cloud bridges configured." }
                    p { "Add a bridge to stream telemetry and events to AWS IoT Core, Azure IoT Hub, or Google Cloud Pub/Sub." }
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Status tab
// ----------------------------------------------------------------

#[cfg(feature = "cloud")]
#[component]
fn StatusTab() -> Element {
    let state = use_context::<AppState>();
    let cs = state.cloud_store.clone();
    let mut statuses: Signal<Vec<CloudBridgeStatus>> = use_signal(Vec::new);
    let mut bridges: Signal<Vec<CloudBridgeConfig>> = use_signal(Vec::new);
    let mut refresh = use_signal(|| 0u64);

    {
        let cs = cs.clone();
        let _r = *refresh.read();
        use_effect(move || {
            let cs = cs.clone();
            spawn(async move {
                statuses.set(cs.list_statuses().await);
                bridges.set(cs.list_bridges().await);
            });
        });
    }

    let bridge_name = |id: &str| -> String {
        bridges
            .read()
            .iter()
            .find(|b| b.id == id)
            .map(|b| b.name.clone())
            .unwrap_or_else(|| id.to_string())
    };

    let bridge_provider = |id: &str| -> String {
        bridges
            .read()
            .iter()
            .find(|b| b.id == id)
            .map(|b| b.provider.clone())
            .unwrap_or_default()
    };

    rsx! {
        div { class: "alarm-routing-section",
            div { style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px;",
                h3 { "Bridge Status" }
                button {
                    class: "btn",
                    onclick: move |_| refresh.set(refresh.cloned() + 1),
                    "Refresh"
                }
            }

            table { class: "alarm-table",
                thead {
                    tr {
                        th { "Bridge" }
                        th { "Provider" }
                        th { "State" }
                        th { "Last Publish" }
                        th { "Messages" }
                        th { "Last Error" }
                    }
                }
                tbody {
                    for status in statuses.read().iter() {
                        {
                            let name = bridge_name(&status.bridge_id);
                            let provider_str = bridge_provider(&status.bridge_id);
                            let provider_label = CloudProvider::from_str(&provider_str)
                                .map(|p| p.label())
                                .unwrap_or("Unknown");
                            let provider_color = provider_badge_color(&provider_str);
                            let state_class = match status.state.as_str() {
                                "idle" => "badge badge-green",
                                "publishing" => "badge badge-blue",
                                "error" => "badge badge-red",
                                "disconnected" => "badge",
                                _ => "badge",
                            };
                            let last_publish = if status.last_publish_ms > 0 {
                                format_timestamp(status.last_publish_ms)
                            } else {
                                "Never".to_string()
                            };
                            let error_display = status.last_error.clone().unwrap_or_else(|| "\u{2014}".to_string());

                            rsx! {
                                tr {
                                    td { "{name}" }
                                    td {
                                        span {
                                            class: "badge",
                                            style: "background: {provider_color}; color: #fff;",
                                            "{provider_label}"
                                        }
                                    }
                                    td {
                                        span { class: state_class, "{status.state}" }
                                    }
                                    td { "{last_publish}" }
                                    td { "{status.messages_published}" }
                                    td { class: "error-cell", "{error_display}" }
                                }
                            }
                        }
                    }
                }
            }

            if statuses.read().is_empty() {
                div { class: "empty-state",
                    p { "No bridge statuses available. Add and enable a cloud bridge first." }
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------

#[cfg(feature = "cloud")]
fn provider_badge_color(provider: &str) -> &'static str {
    match provider {
        "aws_iot_core" => "#FF9900",
        "azure_iot_hub" => "#0078D4",
        "google_pubsub" => "#4285F4",
        _ => "#666666",
    }
}

#[cfg(feature = "cloud")]
fn format_timestamp(ms: i64) -> String {
    let secs = ms / 1000;
    let mins_ago = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
        - secs)
        / 60;

    if mins_ago < 1 {
        "Just now".to_string()
    } else if mins_ago < 60 {
        format!("{mins_ago}m ago")
    } else if mins_ago < 1440 {
        format!("{}h ago", mins_ago / 60)
    } else {
        format!("{}d ago", mins_ago / 1440)
    }
}
