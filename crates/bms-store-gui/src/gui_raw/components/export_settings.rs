use dioxus::prelude::*;

use crate::auth::Permission;
use crate::export::{ConnectorType, ExportConnectorConfig, ExportStatus};
use crate::gui::state::AppState;
use crate::store::audit_store::{AuditAction, AuditEntryBuilder};

#[derive(Debug, Clone, Copy, PartialEq)]
enum ExportTab {
    Connectors,
    Status,
}

#[component]
pub fn ExportSettingsView() -> Element {
    let state = use_context::<AppState>();
    let can_manage = state.has_permission(Permission::ManageExport);
    let mut tab = use_signal(|| ExportTab::Connectors);
    let current_tab = *tab.read();

    rsx! {
        div { class: "alarm-routing-view",
            div { class: "alarm-tabs",
                button {
                    class: if current_tab == ExportTab::Connectors { "alarm-tab active" } else { "alarm-tab" },
                    onclick: move |_| tab.set(ExportTab::Connectors),
                    "Connectors"
                }
                button {
                    class: if current_tab == ExportTab::Status { "alarm-tab active" } else { "alarm-tab" },
                    onclick: move |_| tab.set(ExportTab::Status),
                    "Status"
                }
            }

            div { class: "alarm-tab-content",
                match current_tab {
                    ExportTab::Connectors => rsx! { ConnectorsTab { can_manage } },
                    ExportTab::Status => rsx! { StatusTab {} },
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Connectors tab
// ----------------------------------------------------------------

#[component]
fn ConnectorsTab(can_manage: bool) -> Element {
    let state = use_context::<AppState>();
    let es = state.export_store.clone();
    let audit_store = state.audit_store.clone();
    let current_user = state.current_user;
    let mut connectors: Signal<Vec<ExportConnectorConfig>> = use_signal(Vec::new);
    let mut show_form = use_signal(|| false);
    let mut editing_id = use_signal(|| Option::<String>::None);
    let mut form_name = use_signal(String::new);
    let mut form_type = use_signal(|| "influxdb".to_string());
    // InfluxDB fields
    let mut form_url = use_signal(|| "http://localhost:8086".to_string());
    let mut form_token = use_signal(String::new);
    let mut form_org = use_signal(String::new);
    let mut form_bucket = use_signal(String::new);
    let mut form_on_values = use_signal(|| true);
    let mut form_on_alarms = use_signal(|| true);
    let mut form_on_fdd = use_signal(|| true);
    let mut status_msg: Signal<Option<String>> = use_signal(|| None);
    let mut refresh = use_signal(|| 0u64);

    // Load connectors
    {
        let es = es.clone();
        let _r = *refresh.read();
        use_effect(move || {
            let es = es.clone();
            spawn(async move {
                connectors.set(es.list_connectors().await);
            });
        });
    }

    let new_connector = move |_| {
        form_name.set(String::new());
        form_type.set("influxdb".to_string());
        form_url.set("http://localhost:8086".to_string());
        form_token.set(String::new());
        form_org.set(String::new());
        form_bucket.set(String::new());
        form_on_values.set(true);
        form_on_alarms.set(true);
        form_on_fdd.set(true);
        editing_id.set(None);
        show_form.set(true);
        status_msg.set(None);
    };

    let save_connector = move |_| {
        let es = es.clone();
        let audit = audit_store.clone();
        let name = form_name.read().clone();
        let ctype = form_type.read().clone();
        let url = form_url.read().clone();
        let token = form_token.read().clone();
        let org = form_org.read().clone();
        let bucket = form_bucket.read().clone();
        let on_values = *form_on_values.read();
        let on_alarms = *form_on_alarms.read();
        let on_fdd = *form_on_fdd.read();
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
            let config = serde_json::json!({
                "url": url,
                "token": token,
                "org": org,
                "bucket": bucket,
            });
            let config_str = serde_json::to_string(&config).unwrap_or_default();

            let result = if let Some(id) = &edit_id {
                es.update_connector(
                    id,
                    &name,
                    &ctype,
                    &config_str,
                    true,
                    on_values,
                    on_alarms,
                    on_fdd,
                )
                .await
            } else {
                let id = format!(
                    "exp-{}",
                    uuid::Uuid::new_v4()
                        .to_string()
                        .split('-')
                        .next()
                        .unwrap_or("0")
                );
                es.create_connector(
                    &id,
                    &name,
                    &ctype,
                    &config_str,
                    on_values,
                    on_alarms,
                    on_fdd,
                )
                .await
            };

            match result {
                Ok(()) => {
                    let action = if edit_id.is_some() {
                        AuditAction::UpdateExportConnector
                    } else {
                        AuditAction::CreateExportConnector
                    };
                    let _ = audit
                        .log_action(
                            &user_id,
                            &user_name,
                            AuditEntryBuilder::new(action, "export_connector")
                                .details(&format!("Saved connector '{}'", name)),
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

    rsx! {
        div { class: "alarm-routing-section",
            div { style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px;",
                h3 { "Export Connectors" }
                if can_manage {
                    button { class: "btn btn-primary", onclick: new_connector, "+ Add Connector" }
                }
            }

            if *show_form.read() {
                div { class: "alarm-routing-form",
                    h4 { if editing_id.read().is_some() { "Edit Connector" } else { "New Connector" } }
                    div { class: "form-row",
                        label { "Name" }
                        input {
                            value: "{form_name}",
                            oninput: move |e| form_name.set(e.value()),
                            placeholder: "e.g. InfluxDB Production",
                        }
                    }
                    div { class: "form-row",
                        label { "Type" }
                        select {
                            value: "{form_type}",
                            onchange: move |e| form_type.set(e.value()),
                            option { value: "influxdb", "InfluxDB" }
                        }
                    }
                    div { class: "form-row",
                        label { "URL" }
                        input {
                            value: "{form_url}",
                            oninput: move |e| form_url.set(e.value()),
                            placeholder: "http://localhost:8086",
                        }
                    }
                    div { class: "form-row",
                        label { "Token" }
                        input {
                            r#type: "password",
                            value: "{form_token}",
                            oninput: move |e| form_token.set(e.value()),
                        }
                    }
                    div { class: "form-row",
                        label { "Org" }
                        input {
                            value: "{form_org}",
                            oninput: move |e| form_org.set(e.value()),
                        }
                    }
                    div { class: "form-row",
                        label { "Bucket" }
                        input {
                            value: "{form_bucket}",
                            oninput: move |e| form_bucket.set(e.value()),
                        }
                    }
                    div { class: "form-row",
                        label {
                            input {
                                r#type: "checkbox",
                                checked: *form_on_values.read(),
                                onchange: move |e| form_on_values.set(e.checked()),
                            }
                            " Export values"
                        }
                        label {
                            input {
                                r#type: "checkbox",
                                checked: *form_on_alarms.read(),
                                onchange: move |e| form_on_alarms.set(e.checked()),
                            }
                            " Export alarms"
                        }
                        label {
                            input {
                                r#type: "checkbox",
                                checked: *form_on_fdd.read(),
                                onchange: move |e| form_on_fdd.set(e.checked()),
                            }
                            " Export FDD faults"
                        }
                    }
                    if let Some(msg) = status_msg.read().as_ref() {
                        div { class: "form-error", "{msg}" }
                    }
                    div { class: "form-actions",
                        button { class: "btn btn-primary", onclick: save_connector, "Save" }
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
                        th { "Type" }
                        th { "Enabled" }
                        th { "Values" }
                        th { "Alarms" }
                        th { "FDD" }
                        if can_manage { th { "Actions" } }
                    }
                }
                tbody {
                    for connector in connectors.read().iter() {
                        {
                            let cid = connector.id.clone();
                            let cname = connector.name.clone();
                            let ctype = connector.connector_type.clone();
                            let enabled = connector.enabled;
                            let on_v = connector.on_values;
                            let on_a = connector.on_alarms;
                            let on_f = connector.on_fdd;
                            let es_del = state.export_store.clone();
                            let audit_del = state.audit_store.clone();
                            let user_id_del = current_user.read().as_ref().map(|u| u.id.clone()).unwrap_or_default();
                            let user_name_del = current_user.read().as_ref().map(|u| u.username.clone()).unwrap_or_default();
                            let type_badge_class = if ctype == "influxdb" { "badge badge-blue" } else { "badge badge-amber" };

                            rsx! {
                                tr {
                                    td { "{cname}" }
                                    td {
                                        span { class: type_badge_class, "{ctype}" }
                                    }
                                    td { if enabled { "Yes" } else { "No" } }
                                    td { if on_v { "✓" } else { "—" } }
                                    td { if on_a { "✓" } else { "—" } }
                                    td { if on_f { "✓" } else { "—" } }
                                    if can_manage {
                                        td {
                                            button {
                                                class: "btn btn-small btn-danger",
                                                onclick: move |_| {
                                                    let es = es_del.clone();
                                                    let audit = audit_del.clone();
                                                    let uid = user_id_del.clone();
                                                    let uname = user_name_del.clone();
                                                    let id = cid.clone();
                                                    spawn(async move {
                                                        let _ = es.delete_connector(&id).await;
                                                        let _ = audit.log_action(&uid, &uname,
                                                            AuditEntryBuilder::new(AuditAction::DeleteExportConnector, "export_connector")
                                                                .details(&format!("Deleted connector '{}'", id)),
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

            if connectors.read().is_empty() {
                div { class: "empty-state",
                    p { "No export connectors configured." }
                    p { "Add an InfluxDB or PostgreSQL connector to stream history data to an external database." }
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Status tab
// ----------------------------------------------------------------

#[component]
fn StatusTab() -> Element {
    let state = use_context::<AppState>();
    let es = state.export_store.clone();
    let mut statuses: Signal<Vec<ExportStatus>> = use_signal(Vec::new);
    let mut connectors: Signal<Vec<ExportConnectorConfig>> = use_signal(Vec::new);
    let mut refresh = use_signal(|| 0u64);

    {
        let es = es.clone();
        let _r = *refresh.read();
        use_effect(move || {
            let es = es.clone();
            spawn(async move {
                statuses.set(es.list_statuses().await);
                connectors.set(es.list_connectors().await);
            });
        });
    }

    let connector_name = |id: &str| -> String {
        connectors
            .read()
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| id.to_string())
    };

    rsx! {
        div { class: "alarm-routing-section",
            div { style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px;",
                h3 { "Connector Status" }
                button {
                    class: "btn",
                    onclick: move |_| refresh.set(refresh.cloned() + 1),
                    "Refresh"
                }
            }

            table { class: "alarm-table",
                thead {
                    tr {
                        th { "Connector" }
                        th { "State" }
                        th { "Last Sync" }
                        th { "Rows Exported" }
                        th { "Last Error" }
                    }
                }
                tbody {
                    for status in statuses.read().iter() {
                        {
                            let name = connector_name(&status.connector_id);
                            let state_class = match status.state.as_str() {
                                "idle" => "badge badge-green",
                                "syncing" | "backfilling" => "badge badge-blue",
                                "error" => "badge badge-red",
                                _ => "badge",
                            };
                            let last_sync = if status.last_sync_ms > 0 {
                                format_timestamp(status.last_sync_ms)
                            } else {
                                "Never".to_string()
                            };
                            let error_display = status.last_error.clone().unwrap_or_else(|| "—".to_string());

                            rsx! {
                                tr {
                                    td { "{name}" }
                                    td {
                                        span { class: state_class, "{status.state}" }
                                    }
                                    td { "{last_sync}" }
                                    td { "{status.rows_exported}" }
                                    td { class: "error-cell", "{error_display}" }
                                }
                            }
                        }
                    }
                }
            }

            if statuses.read().is_empty() {
                div { class: "empty-state",
                    p { "No connector statuses available. Add a connector first." }
                }
            }
        }
    }
}

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
