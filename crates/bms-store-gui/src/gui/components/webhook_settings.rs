use dioxus::prelude::*;

use crate::auth::Permission;
use crate::gui::state::AppState;
use crate::store::audit_store::{AuditAction, AuditEntryBuilder};
use crate::webhook::model::{Provider, WebhookDelivery, WebhookEndpoint};

#[derive(Debug, Clone, Copy, PartialEq)]
enum WebhookTab {
    Endpoints,
    DeliveryLog,
}

#[component]
pub fn WebhookSettingsView() -> Element {
    let state = use_context::<AppState>();
    let can_manage = state.has_permission(Permission::ManageWebhooks);
    let mut tab = use_signal(|| WebhookTab::Endpoints);
    let current_tab = *tab.read();

    rsx! {
        div { class: "alarm-routing-view",
            div { class: "alarm-tabs",
                button {
                    class: if current_tab == WebhookTab::Endpoints { "active" } else { "" },
                    onclick: move |_| tab.set(WebhookTab::Endpoints),
                    "Endpoints"
                }
                button {
                    class: if current_tab == WebhookTab::DeliveryLog { "active" } else { "" },
                    onclick: move |_| tab.set(WebhookTab::DeliveryLog),
                    "Delivery Log"
                }
            }

            div { class: "alarm-tab-content",
                match current_tab {
                    WebhookTab::Endpoints => rsx! { EndpointsTab { can_manage } },
                    WebhookTab::DeliveryLog => rsx! { DeliveryLogTab {} },
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Endpoints tab
// ----------------------------------------------------------------

#[component]
fn EndpointsTab(can_manage: bool) -> Element {
    let state = use_context::<AppState>();
    let ws = state.webhook_store.clone();
    let mut endpoints: Signal<Vec<WebhookEndpoint>> = use_signal(Vec::new);
    let mut show_form = use_signal(|| false);
    let mut editing_id: Signal<Option<String>> = use_signal(|| None);
    let mut version = use_signal(|| 0u64);

    // Form fields
    let mut f_name = use_signal(String::new);
    let mut f_provider = use_signal(|| "generic".to_string());
    let mut f_url = use_signal(String::new);
    let mut f_secret = use_signal(String::new);
    let mut f_on_alarm_raised = use_signal(|| true);
    let mut f_on_alarm_cleared = use_signal(|| true);
    let mut f_on_alarm_ack = use_signal(|| false);
    let mut f_on_device_down = use_signal(|| true);
    let mut f_on_device_recovered = use_signal(|| true);
    let mut f_on_fdd_fault_raised = use_signal(|| true);
    let mut f_on_fdd_fault_cleared = use_signal(|| true);
    let mut f_min_severity = use_signal(|| "info".to_string());
    let mut f_enabled = use_signal(|| true);
    // Preserved on edit — GUI has no controls for these, but must not erase them.
    let mut f_headers: Signal<Option<String>> = use_signal(|| None);
    let mut f_tag_filters: Signal<Option<String>> = use_signal(|| None);

    {
        let ws = ws.clone();
        let ver = *version.read();
        let _ = use_resource(move || {
            let ws = ws.clone();
            let _ver = ver;
            async move {
                endpoints.set(ws.list_endpoints().await);
            }
        });
    }

    let mut reset_form = move || {
        f_name.set(String::new());
        f_provider.set("generic".to_string());
        f_url.set(String::new());
        f_secret.set(String::new());
        f_on_alarm_raised.set(true);
        f_on_alarm_cleared.set(true);
        f_on_alarm_ack.set(false);
        f_on_device_down.set(true);
        f_on_device_recovered.set(true);
        f_on_fdd_fault_raised.set(true);
        f_on_fdd_fault_cleared.set(true);
        f_min_severity.set("info".to_string());
        f_enabled.set(true);
        f_headers.set(None);
        f_tag_filters.set(None);
        editing_id.set(None);
        show_form.set(false);
    };

    rsx! {
        div { class: "notification-section",
            div { style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px;",
                h3 { "Webhook Endpoints" }
                if can_manage {
                    button {
                        class: "btn btn-primary",
                        onclick: move |_| {
                            reset_form();
                            show_form.set(true);
                        },
                        "+ Add Endpoint"
                    }
                }
            }

            if *show_form.read() && can_manage {
                {
                    let ws = ws.clone();
                    let state2 = state.clone();
                    rsx! {
                        div { class: "notification-form",
                            h4 {
                                if editing_id.read().is_some() { "Edit Endpoint" } else { "New Endpoint" }
                            }
                            div { class: "form-row",
                                label { "Name" }
                                input {
                                    r#type: "text",
                                    value: "{f_name}",
                                    placeholder: "e.g. Slack Alerts",
                                    oninput: move |e| f_name.set(e.value()),
                                }
                            }
                            div { class: "form-row",
                                label { "Provider" }
                                select {
                                    value: "{f_provider}",
                                    onchange: move |e| f_provider.set(e.value()),
                                    for p in Provider::all() {
                                        option { value: "{p.as_str()}", "{p.label()}" }
                                    }
                                }
                            }
                            div { class: "form-row",
                                label { "URL" }
                                input {
                                    r#type: "text",
                                    value: "{f_url}",
                                    placeholder: "https://hooks.slack.com/services/...",
                                    oninput: move |e| f_url.set(e.value()),
                                }
                            }
                            div { class: "form-row",
                                label { "Secret (HMAC signing key)" }
                                input {
                                    r#type: "password",
                                    value: "{f_secret}",
                                    placeholder: "Optional",
                                    oninput: move |e| f_secret.set(e.value()),
                                }
                            }
                            div { class: "form-row",
                                label { "Min Severity" }
                                select {
                                    value: "{f_min_severity}",
                                    onchange: move |e| f_min_severity.set(e.value()),
                                    option { value: "info", "Info" }
                                    option { value: "warning", "Warning" }
                                    option { value: "critical", "Critical" }
                                    option { value: "life_safety", "Life Safety" }
                                }
                            }
                            div { class: "form-row",
                                label { "Events" }
                                div { style: "display: flex; gap: 16px; flex-wrap: wrap;",
                                    label { style: "display: flex; align-items: center; gap: 4px;",
                                        input {
                                            r#type: "checkbox",
                                            checked: *f_on_alarm_raised.read(),
                                            onchange: move |e| f_on_alarm_raised.set(e.checked()),
                                        }
                                        "Alarm Raised"
                                    }
                                    label { style: "display: flex; align-items: center; gap: 4px;",
                                        input {
                                            r#type: "checkbox",
                                            checked: *f_on_alarm_cleared.read(),
                                            onchange: move |e| f_on_alarm_cleared.set(e.checked()),
                                        }
                                        "Alarm Cleared"
                                    }
                                    label { style: "display: flex; align-items: center; gap: 4px;",
                                        input {
                                            r#type: "checkbox",
                                            checked: *f_on_alarm_ack.read(),
                                            onchange: move |e| f_on_alarm_ack.set(e.checked()),
                                        }
                                        "Alarm Acknowledged"
                                    }
                                    label { style: "display: flex; align-items: center; gap: 4px;",
                                        input {
                                            r#type: "checkbox",
                                            checked: *f_on_device_down.read(),
                                            onchange: move |e| f_on_device_down.set(e.checked()),
                                        }
                                        "Device Down"
                                    }
                                    label { style: "display: flex; align-items: center; gap: 4px;",
                                        input {
                                            r#type: "checkbox",
                                            checked: *f_on_device_recovered.read(),
                                            onchange: move |e| f_on_device_recovered.set(e.checked()),
                                        }
                                        "Device Recovered"
                                    }
                                    label { style: "display: flex; align-items: center; gap: 4px;",
                                        input {
                                            r#type: "checkbox",
                                            checked: *f_on_fdd_fault_raised.read(),
                                            onchange: move |e| f_on_fdd_fault_raised.set(e.checked()),
                                        }
                                        "FDD Fault Raised"
                                    }
                                    label { style: "display: flex; align-items: center; gap: 4px;",
                                        input {
                                            r#type: "checkbox",
                                            checked: *f_on_fdd_fault_cleared.read(),
                                            onchange: move |e| f_on_fdd_fault_cleared.set(e.checked()),
                                        }
                                        "FDD Fault Cleared"
                                    }
                                }
                            }
                            if editing_id.read().is_some() {
                                div { class: "form-row",
                                    label { "Enabled" }
                                    input {
                                        r#type: "checkbox",
                                        checked: *f_enabled.read(),
                                        onchange: move |e| f_enabled.set(e.checked()),
                                    }
                                }
                            }
                            div { class: "form-actions",
                                button {
                                    class: "btn btn-primary",
                                    disabled: f_name.read().is_empty() || f_url.read().is_empty(),
                                    onclick: move |_| {
                                        let ws = ws.clone();
                                        let state2 = state2.clone();
                                        let name = f_name.read().clone();
                                        let provider = f_provider.read().clone();
                                        let url = f_url.read().clone();
                                        let secret_val = f_secret.read().clone();
                                        let secret = if secret_val.is_empty() { None } else { Some(secret_val) };
                                        let on_ar = *f_on_alarm_raised.read();
                                        let on_ac = *f_on_alarm_cleared.read();
                                        let on_aa = *f_on_alarm_ack.read();
                                        let on_dd = *f_on_device_down.read();
                                        let on_dr = *f_on_device_recovered.read();
                                        let on_ffr = *f_on_fdd_fault_raised.read();
                                        let on_ffc = *f_on_fdd_fault_cleared.read();
                                        let sev = f_min_severity.read().clone();
                                        let enabled = *f_enabled.read();
                                        let headers = f_headers.read().clone();
                                        let tag_filters = f_tag_filters.read().clone();
                                        let edit_id = editing_id.read().clone();

                                        spawn(async move {
                                            if let Some(id) = edit_id {
                                                let _ = ws.update_endpoint(
                                                    &id, &name, &provider, &url,
                                                    headers.as_deref(), secret.as_deref(),
                                                    enabled, on_ar, on_ac, on_aa, on_dd, on_dr,
                                                    on_ffr, on_ffc,
                                                    &sev, tag_filters.as_deref(),
                                                ).await;
                                                let builder = AuditEntryBuilder::new(AuditAction::UpdateWebhook, "webhook").resource_id(&id);
                                                let _ = state2.audit_store.log_action(
                                                    &state2.current_user.read().as_ref().map(|u| u.id.as_str()).unwrap_or(""),
                                                    &state2.current_user.read().as_ref().map(|u| u.username.as_str()).unwrap_or(""),
                                                    builder,
                                                ).await;
                                            } else {
                                                let id = uuid::Uuid::new_v4().to_string();
                                                let _ = ws.create_endpoint(
                                                    &id, &name, &provider, &url,
                                                    headers.as_deref(), secret.as_deref(),
                                                    on_ar, on_ac, on_aa, on_dd, on_dr,
                                                    on_ffr, on_ffc,
                                                    &sev, tag_filters.as_deref(),
                                                ).await;
                                                let builder = AuditEntryBuilder::new(AuditAction::CreateWebhook, "webhook")
                                                    .resource_id(&id)
                                                    .details(&format!("name={}", name));
                                                let _ = state2.audit_store.log_action(
                                                    &state2.current_user.read().as_ref().map(|u| u.id.as_str()).unwrap_or(""),
                                                    &state2.current_user.read().as_ref().map(|u| u.username.as_str()).unwrap_or(""),
                                                    builder,
                                                ).await;
                                            }
                                            version.set(version() + 1);
                                            show_form.set(false);
                                            editing_id.set(None);
                                        });
                                    },
                                    "Save"
                                }
                                button {
                                    class: "btn",
                                    onclick: move |_| {
                                        reset_form();
                                    },
                                    "Cancel"
                                }
                            }
                        }
                    }
                }
            }

            // Endpoints list
            if endpoints.read().is_empty() {
                p { class: "empty-state", "No webhook endpoints configured." }
            } else {
                table { class: "data-table",
                    thead {
                        tr {
                            th { "Name" }
                            th { "Provider" }
                            th { "URL" }
                            th { "Events" }
                            th { "Status" }
                            if can_manage {
                                th { "Actions" }
                            }
                        }
                    }
                    tbody {
                        for ep in endpoints.read().iter() {
                            {
                                let ep_id = ep.id.clone();
                                let ep_clone = ep.clone();
                                let provider_label = Provider::from_str(&ep.provider)
                                    .map(|p| p.label())
                                    .unwrap_or("Generic");
                                let url_display = if ep.url.len() > 50 {
                                    format!("{}...", &ep.url[..50])
                                } else {
                                    ep.url.clone()
                                };
                                let mut event_parts = Vec::new();
                                if ep.on_alarm_raised { event_parts.push("Raised"); }
                                if ep.on_alarm_cleared { event_parts.push("Cleared"); }
                                if ep.on_alarm_acknowledged { event_parts.push("Ack"); }
                                if ep.on_device_down { event_parts.push("Down"); }
                                if ep.on_device_recovered { event_parts.push("Recovered"); }
                                if ep.on_fdd_fault_raised { event_parts.push("FDD Raised"); }
                                if ep.on_fdd_fault_cleared { event_parts.push("FDD Cleared"); }
                                let events_str = event_parts.join(", ");

                                rsx! {
                                    tr {
                                        td { "{ep.name}" }
                                        td {
                                            span { class: "protocol-badge", "{provider_label}" }
                                        }
                                        td { title: "{ep.url}", "{url_display}" }
                                        td { "{events_str}" }
                                        td {
                                            if ep.enabled {
                                                span { class: "severity-badge severity-info", "Enabled" }
                                            } else {
                                                span { class: "severity-badge", "Disabled" }
                                            }
                                        }
                                        if can_manage {
                                            td {
                                                div { style: "display: flex; gap: 4px;",
                                                    button {
                                                        class: "btn btn-sm",
                                                        onclick: {
                                                            let ep2 = ep_clone.clone();
                                                            move |_| {
                                                                f_name.set(ep2.name.clone());
                                                                f_provider.set(ep2.provider.clone());
                                                                f_url.set(ep2.url.clone());
                                                                f_secret.set(ep2.secret.clone().unwrap_or_default());
                                                                f_on_alarm_raised.set(ep2.on_alarm_raised);
                                                                f_on_alarm_cleared.set(ep2.on_alarm_cleared);
                                                                f_on_alarm_ack.set(ep2.on_alarm_acknowledged);
                                                                f_on_device_down.set(ep2.on_device_down);
                                                                f_on_device_recovered.set(ep2.on_device_recovered);
                                                                f_on_fdd_fault_raised.set(ep2.on_fdd_fault_raised);
                                                                f_on_fdd_fault_cleared.set(ep2.on_fdd_fault_cleared);
                                                                f_min_severity.set(ep2.min_severity.clone());
                                                                f_enabled.set(ep2.enabled);
                                                                f_headers.set(ep2.headers.clone());
                                                                f_tag_filters.set(ep2.tag_filters.clone());
                                                                editing_id.set(Some(ep2.id.clone()));
                                                                show_form.set(true);
                                                            }
                                                        },
                                                        "Edit"
                                                    }
                                                    button {
                                                        class: "btn btn-sm",
                                                        onclick: {
                                                            let ws = ws.clone();
                                                            let id = ep_id.clone();
                                                            move |_| {
                                                                let ws = ws.clone();
                                                                let id = id.clone();
                                                                spawn(async move {
                                                                    let _ = ws.delete_endpoint(&id).await;
                                                                    version.set(version() + 1);
                                                                });
                                                            }
                                                        },
                                                        "Delete"
                                                    }
                                                    button {
                                                        class: "btn btn-sm",
                                                        title: "Send test webhook",
                                                        onclick: {
                                                            let ws = ws.clone();
                                                            let id = ep_id.clone();
                                                            let ep_name = ep.name.clone();
                                                            let ep_provider = ep.provider.clone();
                                                            let ep_url = ep.url.clone();
                                                            let ep_secret = ep.secret.clone();
                                                            move |_| {
                                                                let ep_name = ep_name.clone();
                                                                let ep_provider = ep_provider.clone();
                                                                let ep_url = ep_url.clone();
                                                                let ep_secret = ep_secret.clone();
                                                                spawn(async move {
                                                                    let provider = Provider::from_str(&ep_provider).unwrap_or(Provider::Generic);
                                                                    let payload = crate::webhook::model::WebhookPayload {
                                                                        event_type: crate::webhook::model::WebhookEventType::AlarmRaised,
                                                                        alarm_id: Some(0),
                                                                        node_id: Some("test/test-point".into()),
                                                                        device_id: Some("test".into()),
                                                                        point_id: Some("test-point".into()),
                                                                        alarm_type: Some("high_limit".into()),
                                                                        severity: Some("warning".into()),
                                                                        trigger_value: Some(85.0),
                                                                        message: Some("Test webhook from OpenCrate BMS".into()),
                                                                        timestamp_ms: std::time::SystemTime::now()
                                                                            .duration_since(std::time::UNIX_EPOCH)
                                                                            .unwrap_or_default()
                                                                            .as_millis() as i64,
                                                                        project_name: "OpenCrate".into(),
                                                                    };
                                                                    let formatted = crate::webhook::providers::format_for_provider(
                                                                        provider, &payload, ep_secret.as_deref(), &ep_url,
                                                                    );
                                                                    let client = reqwest::Client::builder()
                                                                        .timeout(std::time::Duration::from_secs(10))
                                                                        .build()
                                                                        .unwrap_or_default();
                                                                    let req = client.post(&ep_url)
                                                                        .header("Content-Type", &formatted.content_type)
                                                                        .body(formatted.body);
                                                                    match req.send().await {
                                                                        Ok(resp) => {
                                                                            let status = resp.status().as_u16();
                                                                            tracing::info!(endpoint = %ep_name, http_status = status, "Test webhook sent");
                                                                        }
                                                                        Err(e) => {
                                                                            tracing::warn!(endpoint = %ep_name, error = %e, "Test webhook failed");
                                                                        }
                                                                    }
                                                                });
                                                            }
                                                        },
                                                        "Test"
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
}

// ----------------------------------------------------------------
// Delivery log tab
// ----------------------------------------------------------------

#[component]
fn DeliveryLogTab() -> Element {
    let state = use_context::<AppState>();
    let ws = state.webhook_store.clone();
    let mut deliveries: Signal<Vec<WebhookDelivery>> = use_signal(Vec::new);
    let mut status_filter: Signal<String> = use_signal(String::new);
    let mut version = use_signal(|| 0u64);

    {
        let ws = ws.clone();
        let filter = status_filter.read().clone();
        let ver = *version.read();
        let _ = use_resource(move || {
            let ws = ws.clone();
            let filter = filter.clone();
            let _ver = ver;
            async move {
                let sf = if filter.is_empty() {
                    None
                } else {
                    Some(filter.as_str())
                };
                deliveries.set(ws.list_deliveries(None, sf, 200).await);
            }
        });
    }

    rsx! {
        div { class: "notification-section",
            div { style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px;",
                h3 { "Delivery Log" }
                div { style: "display: flex; gap: 8px; align-items: center;",
                    label { "Status:" }
                    select {
                        value: "{status_filter}",
                        onchange: move |e| {
                            status_filter.set(e.value());
                            version.set(version() + 1);
                        },
                        option { value: "", "All" }
                        option { value: "delivered", "Delivered" }
                        option { value: "failed", "Failed" }
                        option { value: "retrying", "Retrying" }
                    }
                    button {
                        class: "btn btn-sm",
                        onclick: move |_| version.set(version() + 1),
                        "Refresh"
                    }
                }
            }

            if deliveries.read().is_empty() {
                p { class: "empty-state", "No delivery log entries." }
            } else {
                table { class: "data-table",
                    thead {
                        tr {
                            th { "Time" }
                            th { "Endpoint" }
                            th { "Event" }
                            th { "Status" }
                            th { "HTTP" }
                            th { "Error" }
                        }
                    }
                    tbody {
                        for d in deliveries.read().iter() {
                            {
                                let status_class = match d.status.as_str() {
                                    "delivered" => "severity-badge severity-info",
                                    "failed" => "severity-badge severity-critical",
                                    "retrying" => "severity-badge severity-warning",
                                    _ => "severity-badge",
                                };
                                let time_str = format_timestamp(d.timestamp_ms);
                                let http_str = d.http_status.map(|s| s.to_string()).unwrap_or_default();
                                let error_str = d.error.clone().unwrap_or_default();
                                let error_display = if error_str.len() > 80 {
                                    format!("{}...", &error_str[..80])
                                } else {
                                    error_str.clone()
                                };

                                rsx! {
                                    tr {
                                        td { "{time_str}" }
                                        td { "{d.endpoint_id}" }
                                        td { "{d.event_type}" }
                                        td {
                                            span { class: "{status_class}", "{d.status}" }
                                        }
                                        td { "{http_str}" }
                                        td { title: "{error_str}", "{error_display}" }
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

fn format_timestamp(ms: i64) -> String {
    let secs = ms / 1000;
    let total_secs = secs as u64;
    let time_secs = total_secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;
    let days = total_secs / 86400;

    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        y, m, d, hours, minutes, seconds
    )
}
