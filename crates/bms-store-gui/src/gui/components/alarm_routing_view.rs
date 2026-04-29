use dioxus::prelude::*;

use crate::auth::Permission;
use crate::gui::state::AppState;
use crate::notification::channel::{
    NotificationChannel, NotificationEventType, NotificationPayload,
};
use crate::notification::email::EmailChannel;
use crate::notification::sms::SmsChannel;
use crate::notification::webhook::WebhookChannel;
use bms_store_storage::store::audit_store::{AuditAction, AuditEntryBuilder};
use bms_store_storage::store::notification_store::{
    AlarmRecipient, ChannelType, DeliveryStatus, NotificationRecord, RoutingRule,
};

#[derive(Debug, Clone, Copy, PartialEq)]
enum RoutingTab {
    Recipients,
    Rules,
    Log,
}

#[component]
pub fn AlarmRoutingView() -> Element {
    let state = use_context::<AppState>();
    let can_manage = state.has_permission(Permission::ManageNotifications);
    let mut tab = use_signal(|| RoutingTab::Recipients);
    let current_tab = *tab.read();

    rsx! {
        div { class: "alarm-routing-view",
            div { class: "alarm-tabs",
                button {
                    class: if current_tab == RoutingTab::Recipients { "alarm-tab active" } else { "alarm-tab" },
                    onclick: move |_| tab.set(RoutingTab::Recipients),
                    "Recipients"
                }
                button {
                    class: if current_tab == RoutingTab::Rules { "alarm-tab active" } else { "alarm-tab" },
                    onclick: move |_| tab.set(RoutingTab::Rules),
                    "Routing Rules"
                }
                button {
                    class: if current_tab == RoutingTab::Log { "alarm-tab active" } else { "alarm-tab" },
                    onclick: move |_| tab.set(RoutingTab::Log),
                    "Notification Log"
                }
            }

            div { class: "alarm-tab-content",
                match current_tab {
                    RoutingTab::Recipients => rsx! { RecipientsTab { can_manage } },
                    RoutingTab::Rules => rsx! { RoutingRulesTab { can_manage } },
                    RoutingTab::Log => rsx! { NotificationLogTab {} },
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Recipients tab
// ----------------------------------------------------------------

#[component]
fn RecipientsTab(can_manage: bool) -> Element {
    let state = use_context::<AppState>();
    let ns = state.notification_store.clone();
    let audit_store = state.audit_store.clone();
    let current_user = state.current_user;
    let mut recipients: Signal<Vec<AlarmRecipient>> = use_signal(Vec::new);
    let mut show_form = use_signal(|| false);
    let mut editing_id = use_signal(|| Option::<i64>::None);
    let mut form_name = use_signal(String::new);
    let mut form_type = use_signal(|| ChannelType::Webhook);
    let mut form_address = use_signal(String::new);
    let mut form_config = use_signal(|| "{}".to_string());
    let mut status_msg: Signal<Option<String>> = use_signal(|| None);

    // Load recipients
    {
        let ns = ns.clone();
        let _ = use_resource(move || {
            let ns = ns.clone();
            async move {
                recipients.set(ns.list_recipients().await);
            }
        });
    }

    let on_add = move |_| {
        editing_id.set(None);
        form_name.set(String::new());
        form_type.set(ChannelType::Webhook);
        form_address.set(String::new());
        form_config.set("{}".to_string());
        show_form.set(true);
    };

    let mut on_edit = move |r: AlarmRecipient| {
        editing_id.set(Some(r.id));
        form_name.set(r.name.clone());
        form_type.set(r.channel_type.clone());
        form_address.set(r.address.clone());
        form_config.set(r.channel_config.clone());
        show_form.set(true);
    };

    rsx! {
        div { class: "section-header",
            h3 { "Notification Recipients" }
            if can_manage {
                button {
                    class: "btn btn-primary btn-sm",
                    onclick: on_add,
                    "+ Add Recipient"
                }
            }
        }

        if let Some(msg) = status_msg.read().as_ref() {
            div { class: "status-bar", "{msg}" }
        }

        if *show_form.read() && can_manage {
            {
                let ns = ns.clone();
                rsx! {
                    RecipientForm {
                        editing_id: *editing_id.read(),
                        name: form_name,
                        channel_type: form_type,
                        address: form_address,
                        config: form_config,
                        on_save: move |_| {
                            let ns = ns.clone();
                            let name = form_name.read().clone();
                            let ct = form_type.read().clone();
                            let addr = form_address.read().clone();
                            let cfg = form_config.read().clone();
                            let eid = *editing_id.read();
                            spawn(async move {
                                let result = if let Some(id) = eid {
                                    ns.update_recipient(id, &name, ct, &addr, &cfg, true).await
                                } else {
                                    ns.create_recipient(&name, ct, &addr, &cfg).await.map(|_| ())
                                };
                                match result {
                                    Ok(()) => {
                                        recipients.set(ns.list_recipients().await);
                                        show_form.set(false);
                                        status_msg.set(Some("Recipient saved".into()));
                                    }
                                    Err(e) => status_msg.set(Some(format!("Error: {e}"))),
                                }
                            });
                        },
                        on_cancel: move |_| show_form.set(false),
                    }
                }
            }
        }

        table { class: "data-table",
            thead {
                tr {
                    th { "Name" }
                    th { "Type" }
                    th { "Address" }
                    th { "Enabled" }
                    if can_manage { th { "Actions" } }
                }
            }
            tbody {
                for r in recipients.read().iter() {
                    {
                        let r_edit = r.clone();
                        let r_id = r.id;
                        let r_name = r.name.clone();
                        let r_addr = r.address.clone();
                        let ns_del = ns.clone();
                        let aud_test = audit_store.clone();
                        rsx! {
                            tr {
                                td { "{r.name}" }
                                td {
                                    span { class: "badge badge-{r.channel_type.as_str()}", "{r.channel_type.label()}" }
                                }
                                td { class: "monospace", "{r.address}" }
                                td { if r.enabled { "Yes" } else { "No" } }
                                if can_manage {
                                    td { class: "action-cell",
                                        button {
                                            class: "btn btn-sm",
                                            onclick: move |_| on_edit(r_edit.clone()),
                                            "Edit"
                                        }
                                        button {
                                            class: "btn btn-sm btn-danger",
                                            onclick: {
                                                let r_name = r_name.clone();
                                                move |_| {
                                                    let ns = ns_del.clone();
                                                    let name = r_name.clone();
                                                    spawn(async move {
                                                        if let Ok(()) = ns.delete_recipient(r_id).await {
                                                            recipients.set(ns.list_recipients().await);
                                                            status_msg.set(Some(format!("Deleted {name}")));
                                                        }
                                                    });
                                                }
                                            },
                                            "Delete"
                                        }
                                        button {
                                            class: "btn btn-sm",
                                            onclick: {
                                                let test_addr = r_addr.clone();
                                                let test_type = r.channel_type.clone();
                                                let test_config = r.channel_config.clone();
                                                let test_name = r.name.clone();
                                                move |_| {
                                                    let addr = test_addr.clone();
                                                    let ct = test_type.clone();
                                                    let cfg = test_config.clone();
                                                    let name = test_name.clone();
                                                    // Audit the test
                                                    {
                                                        let aud = aud_test.clone();
                                                        let user = current_user.read().clone();
                                                        let rid_str = r_id.to_string();
                                                        spawn(async move {
                                                            let (uid, uname) = match user.as_ref() {
                                                                Some(u) => (u.id.as_str(), u.username.as_str()),
                                                                None => ("system", "system"),
                                                            };
                                                            let _ = aud.log_action(uid, uname,
                                                                AuditEntryBuilder::new(AuditAction::TestNotification, "recipient")
                                                                    .resource_id(&rid_str),
                                                            ).await;
                                                        });
                                                    }
                                                    status_msg.set(Some(format!("Sending test to {addr}...")));
                                                    spawn(async move {
                                                        let payload = NotificationPayload {
                                                            alarm_id: 0,
                                                            alarm_config_id: 0,
                                                            device_id: "test-device".into(),
                                                            point_id: "test-point".into(),
                                                            alarm_type: "high_limit".into(),
                                                            severity: "warning".into(),
                                                            trigger_value: 85.0,
                                                            trigger_time_ms: std::time::SystemTime::now()
                                                                .duration_since(std::time::UNIX_EPOCH)
                                                                .unwrap_or_default()
                                                                .as_millis() as i64,
                                                            context_snapshot: "{}".into(),
                                                            event_type: NotificationEventType::Raised,
                                                            recipient_name: name,
                                                            project_name: "OpenCrate Test".into(),
                                                        };
                                                        let result: Result<(), crate::notification::channel::ChannelError> = match ct {
                                                            ChannelType::Webhook => WebhookChannel::new().send(&addr, &cfg, &payload).await,
                                                            ChannelType::Email => EmailChannel::new().send(&addr, &cfg, &payload).await,
                                                            ChannelType::Sms => SmsChannel::new().send(&addr, &cfg, &payload).await,
                                                        };
                                                        match result {
                                                            Ok(()) => status_msg.set(Some(format!("Test delivered to {addr}"))),
                                                            Err(e) => status_msg.set(Some(format!("Test failed: {e}"))),
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
                if recipients.read().is_empty() {
                    tr {
                        td { colspan: "5", class: "empty-row", "No recipients configured" }
                    }
                }
            }
        }
    }
}

#[component]
fn RecipientForm(
    editing_id: Option<i64>,
    name: Signal<String>,
    channel_type: Signal<ChannelType>,
    address: Signal<String>,
    config: Signal<String>,
    on_save: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "form-card",
            h4 { if editing_id.is_some() { "Edit Recipient" } else { "New Recipient" } }
            div { class: "form-row",
                label { "Name" }
                input {
                    r#type: "text",
                    value: "{name.read()}",
                    oninput: move |e| name.set(e.value()),
                    placeholder: "e.g. Operations Team",
                }
            }
            div { class: "form-row",
                label { "Channel Type" }
                select {
                    value: "{channel_type.read().as_str()}",
                    onchange: move |e| {
                        channel_type.set(match e.value().as_str() {
                            "email" => ChannelType::Email,
                            "sms" => ChannelType::Sms,
                            _ => ChannelType::Webhook,
                        });
                    },
                    option { value: "webhook", "Webhook" }
                    option { value: "email", "Email" }
                    option { value: "sms", "SMS" }
                }
            }
            div { class: "form-row",
                label { "Address" }
                input {
                    r#type: "text",
                    value: "{address.read()}",
                    oninput: move |e| address.set(e.value()),
                    placeholder: match *channel_type.read() {
                        ChannelType::Webhook => "https://example.com/webhook",
                        ChannelType::Email => "ops@example.com",
                        ChannelType::Sms => "+15551234567",
                    },
                }
            }
            div { class: "form-row",
                label { "Config (JSON)" }
                textarea {
                    value: "{config.read()}",
                    oninput: move |e| config.set(e.value()),
                    rows: "4",
                    spellcheck: "false",
                }
            }
            div { class: "form-actions",
                button { class: "btn btn-primary", onclick: move |_| on_save.call(()), "Save" }
                button { class: "btn", onclick: move |_| on_cancel.call(()), "Cancel" }
            }
        }
    }
}

// ----------------------------------------------------------------
// Routing Rules tab
// ----------------------------------------------------------------

#[component]
fn RoutingRulesTab(can_manage: bool) -> Element {
    let state = use_context::<AppState>();
    let ns = state.notification_store.clone();
    let mut rules: Signal<Vec<RoutingRule>> = use_signal(Vec::new);
    let mut recipients: Signal<Vec<AlarmRecipient>> = use_signal(Vec::new);
    let mut show_form = use_signal(|| false);
    let mut editing_id = use_signal(|| Option::<i64>::None);
    let mut form_recipient = use_signal(|| 0i64);
    let mut form_severity = use_signal(|| "info".to_string());
    let mut form_devices = use_signal(String::new);
    let mut form_types = use_signal(String::new);
    let mut form_tier = use_signal(|| 0u8);
    let mut form_delay = use_signal(|| 0u32);
    let mut form_on_clear = use_signal(|| true);
    let mut status_msg: Signal<Option<String>> = use_signal(|| None);

    {
        let ns = state.notification_store.clone();
        let _ = use_resource(move || {
            let ns = ns.clone();
            async move {
                rules.set(ns.list_rules().await);
                recipients.set(ns.list_recipients().await);
            }
        });
    }

    let on_add = move |_| {
        editing_id.set(None);
        let recips = recipients.read();
        form_recipient.set(recips.first().map(|r| r.id).unwrap_or(0));
        form_severity.set("info".into());
        form_devices.set(String::new());
        form_types.set(String::new());
        form_tier.set(0);
        form_delay.set(0);
        form_on_clear.set(true);
        show_form.set(true);
    };

    rsx! {
        div { class: "section-header",
            h3 { "Routing Rules" }
            if can_manage {
                button {
                    class: "btn btn-primary btn-sm",
                    onclick: on_add,
                    "+ Add Rule"
                }
            }
        }

        if let Some(msg) = status_msg.read().as_ref() {
            div { class: "status-bar", "{msg}" }
        }

        if *show_form.read() && can_manage {
            div { class: "form-card",
                h4 { if editing_id.read().is_some() { "Edit Rule" } else { "New Rule" } }
                div { class: "form-row",
                    label { "Recipient" }
                    select {
                        value: "{form_recipient.read()}",
                        onchange: move |e| {
                            if let Ok(v) = e.value().parse::<i64>() {
                                form_recipient.set(v);
                            }
                        },
                        for r in recipients.read().iter() {
                            option { value: "{r.id}", "{r.name} ({r.channel_type.label()})" }
                        }
                    }
                }
                div { class: "form-row",
                    label { "Min Severity" }
                    select {
                        value: "{form_severity.read()}",
                        onchange: move |e| form_severity.set(e.value()),
                        option { value: "info", "Info" }
                        option { value: "warning", "Warning" }
                        option { value: "critical", "Critical" }
                        option { value: "life_safety", "Life Safety" }
                    }
                }
                div { class: "form-row",
                    label { "Device Filter (comma-sep, empty=all)" }
                    input {
                        r#type: "text",
                        value: "{form_devices.read()}",
                        oninput: move |e| form_devices.set(e.value()),
                        placeholder: "ahu-1, vav-1",
                    }
                }
                div { class: "form-row",
                    label { "Alarm Type Filter (comma-sep, empty=all)" }
                    input {
                        r#type: "text",
                        value: "{form_types.read()}",
                        oninput: move |e| form_types.set(e.value()),
                        placeholder: "high_limit, low_limit",
                    }
                }
                div { class: "form-row",
                    label { "Escalation Tier (0=immediate)" }
                    input {
                        r#type: "number",
                        value: "{form_tier.read()}",
                        oninput: move |e| {
                            if let Ok(v) = e.value().parse::<u8>() {
                                form_tier.set(v);
                            }
                        },
                        min: "0",
                        max: "10",
                    }
                }
                div { class: "form-row",
                    label { "Escalation Delay (minutes)" }
                    input {
                        r#type: "number",
                        value: "{form_delay.read()}",
                        oninput: move |e| {
                            if let Ok(v) = e.value().parse::<u32>() {
                                form_delay.set(v);
                            }
                        },
                        min: "0",
                    }
                }
                div { class: "form-row",
                    label {
                        input {
                            r#type: "checkbox",
                            checked: *form_on_clear.read(),
                            onchange: move |e| form_on_clear.set(e.checked()),
                        }
                        " Notify on alarm clear"
                    }
                }
                div { class: "form-actions",
                    button {
                        class: "btn btn-primary",
                        onclick: {
                            let ns = ns.clone();
                            move |_| {
                                let ns = ns.clone();
                                let rid = *form_recipient.read();
                                let sev = form_severity.read().clone();
                                let devs = form_devices.read().clone();
                                let types = form_types.read().clone();
                                let tier = *form_tier.read();
                                let delay = *form_delay.read();
                                let on_clear = *form_on_clear.read();
                                let eid = *editing_id.read();
                                spawn(async move {
                                    let result = if let Some(id) = eid {
                                        ns.update_rule(id, &sev, &devs, &types, None, tier, delay, on_clear, true).await
                                    } else {
                                        ns.create_rule(rid, &sev, &devs, &types, None, tier, delay, on_clear).await.map(|_| ())
                                    };
                                    match result {
                                        Ok(()) => {
                                            rules.set(ns.list_rules().await);
                                            show_form.set(false);
                                            status_msg.set(Some("Rule saved".into()));
                                        }
                                        Err(e) => status_msg.set(Some(format!("Error: {e}"))),
                                    }
                                });
                            }
                        },
                        "Save"
                    }
                    button { class: "btn", onclick: move |_| show_form.set(false), "Cancel" }
                }
            }
        }

        table { class: "data-table",
            thead {
                tr {
                    th { "Recipient" }
                    th { "Min Severity" }
                    th { "Device Filter" }
                    th { "Tier" }
                    th { "Delay" }
                    th { "On Clear" }
                    th { "Enabled" }
                    if can_manage { th { "Actions" } }
                }
            }
            tbody {
                for rule in rules.read().iter() {
                    {
                        let rule_id = rule.id;
                        let recip_name = recipients.read().iter()
                            .find(|r| r.id == rule.recipient_id)
                            .map(|r| r.name.clone())
                            .unwrap_or_else(|| format!("#{}", rule.recipient_id));
                        let dev_filter_display = if rule.device_filter.is_empty() { "All".to_string() } else { rule.device_filter.clone() };
                        let ns_del = ns.clone();
                        rsx! {
                            tr {
                                td { "{recip_name}" }
                                td { "{rule.min_severity}" }
                                td { "{dev_filter_display}" }
                                td { "{rule.escalation_tier}" }
                                td { "{rule.escalation_delay_mins}m" }
                                td { if rule.notify_on_clear { "Yes" } else { "No" } }
                                td { if rule.enabled { "Yes" } else { "No" } }
                                if can_manage {
                                    td { class: "action-cell",
                                        button {
                                            class: "btn btn-sm btn-danger",
                                            onclick: move |_| {
                                                let ns = ns_del.clone();
                                                spawn(async move {
                                                    if let Ok(()) = ns.delete_rule(rule_id).await {
                                                        rules.set(ns.list_rules().await);
                                                        status_msg.set(Some("Rule deleted".into()));
                                                    }
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
                if rules.read().is_empty() {
                    tr {
                        td { colspan: "8", class: "empty-row", "No routing rules configured" }
                    }
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Notification Log tab
// ----------------------------------------------------------------

#[component]
fn NotificationLogTab() -> Element {
    let state = use_context::<AppState>();
    let ns = state.notification_store.clone();
    let mut records: Signal<Vec<NotificationRecord>> = use_signal(Vec::new);
    let mut filter_status = use_signal(String::new);

    {
        let ns = ns.clone();
        let _ = use_resource(move || {
            let ns = ns.clone();
            async move {
                records.set(ns.query_notification_log(500).await);
            }
        });
    }

    rsx! {
        div { class: "section-header",
            h3 { "Notification Log" }
            div { class: "filter-bar",
                select {
                    value: "{filter_status.read()}",
                    onchange: move |e| filter_status.set(e.value()),
                    option { value: "", "All Statuses" }
                    option { value: "pending", "Pending" }
                    option { value: "delivered", "Delivered" }
                    option { value: "failed", "Failed" }
                    option { value: "retrying", "Retrying" }
                }
                button {
                    class: "btn btn-sm",
                    onclick: {
                        let ns = ns.clone();
                        move |_| {
                            let ns = ns.clone();
                            spawn(async move {
                                records.set(ns.query_notification_log(500).await);
                            });
                        }
                    },
                    "Refresh"
                }
            }
        }

        table { class: "data-table",
            thead {
                tr {
                    th { "Time" }
                    th { "Alarm" }
                    th { "Channel" }
                    th { "Address" }
                    th { "Status" }
                    th { "Attempts" }
                    th { "Error" }
                }
            }
            tbody {
                {
                    let filter = filter_status.read().clone();
                    let filtered: Vec<_> = records.read().iter()
                        .filter(|r| filter.is_empty() || r.status.as_str() == filter)
                        .cloned()
                        .collect();
                    rsx! {
                        for rec in filtered.iter() {
                            {
                                let status_class = match rec.status {
                                    DeliveryStatus::Delivered => "badge badge-success",
                                    DeliveryStatus::Failed => "badge badge-danger",
                                    DeliveryStatus::Retrying => "badge badge-warning",
                                    DeliveryStatus::Pending => "badge badge-info",
                                };
                                let error_text = rec.last_error.as_deref().unwrap_or("-").to_string();
                                rsx! {
                                    tr {
                                        td { class: "monospace", "{format_datetime(rec.created_ms)}" }
                                        td { "#{rec.alarm_id}" }
                                        td {
                                            span { class: "badge badge-{rec.channel_type.as_str()}", "{rec.channel_type.label()}" }
                                        }
                                        td { class: "monospace", "{rec.address}" }
                                        td {
                                            span { class: "{status_class}", "{rec.status.as_str()}" }
                                        }
                                        td { "{rec.attempt_count}" }
                                        td { class: "error-text", "{error_text}" }
                                    }
                                }
                            }
                        }
                        if records.read().is_empty() {
                            tr {
                                td { colspan: "7", class: "empty-row", "No notifications sent yet" }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn format_datetime(ms: i64) -> String {
    let secs = ms / 1000;
    let hours = (secs / 3600) % 24;
    let mins = (secs / 60) % 60;
    let s = secs % 60;
    // Date portion
    let days_since_epoch = secs / 86400;
    let (y, m, d) = days_to_ymd(days_since_epoch);
    format!("{y:04}-{m:02}-{d:02} {hours:02}:{mins:02}:{s:02}")
}

fn days_to_ymd(days: i64) -> (i64, i64, i64) {
    // Simplified date calculation
    let mut y = 1970;
    let mut remaining = days;
    loop {
        let leap = if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
            366
        } else {
            365
        };
        if remaining < leap {
            break;
        }
        remaining -= leap;
        y += 1;
    }
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let month_days = [
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
    let mut m = 0;
    for md in &month_days {
        if remaining < *md {
            break;
        }
        remaining -= md;
        m += 1;
    }
    (y, m + 1, remaining + 1)
}
