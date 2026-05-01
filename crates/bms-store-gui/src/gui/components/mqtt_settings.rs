use dioxus::prelude::*;

use bms_store_storage::auth::Permission;
use crate::gui::state::AppState;
use bms_store_storage::mqtt::topic;
use bms_store_storage::store::audit_store::{AuditAction, AuditEntryBuilder};
use bms_store_storage::store::mqtt_store::{MqttBrokerConfig, MqttEventType, MqttTopicPattern};

#[derive(Debug, Clone, Copy, PartialEq)]
enum MqttTab {
    Brokers,
    Topics,
    Status,
}

#[component]
pub fn MqttSettingsView() -> Element {
    let state = use_context::<AppState>();
    let can_manage = state.has_permission(Permission::ManageMqtt);
    let mut tab = use_signal(|| MqttTab::Brokers);
    let current_tab = *tab.read();

    rsx! {
        div { class: "alarm-routing-view",
            div { class: "alarm-tabs",
                button {
                    class: if current_tab == MqttTab::Brokers { "alarm-tab active" } else { "alarm-tab" },
                    onclick: move |_| tab.set(MqttTab::Brokers),
                    "Brokers"
                }
                button {
                    class: if current_tab == MqttTab::Topics { "alarm-tab active" } else { "alarm-tab" },
                    onclick: move |_| tab.set(MqttTab::Topics),
                    "Topic Patterns"
                }
                button {
                    class: if current_tab == MqttTab::Status { "alarm-tab active" } else { "alarm-tab" },
                    onclick: move |_| tab.set(MqttTab::Status),
                    "Status"
                }
            }

            div { class: "alarm-tab-content",
                match current_tab {
                    MqttTab::Brokers => rsx! { BrokersTab { can_manage } },
                    MqttTab::Topics => rsx! { TopicsTab { can_manage } },
                    MqttTab::Status => rsx! { StatusTab {} },
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Brokers tab
// ----------------------------------------------------------------

#[component]
fn BrokersTab(can_manage: bool) -> Element {
    let state = use_context::<AppState>();
    let ms = state.mqtt_store.clone();
    let audit_store = state.audit_store.clone();
    let current_user = state.current_user;
    let mut brokers: Signal<Vec<MqttBrokerConfig>> = use_signal(Vec::new);
    let mut show_form = use_signal(|| false);
    let mut editing_id = use_signal(|| Option::<i64>::None);
    let mut form_name = use_signal(String::new);
    let mut form_host = use_signal(|| "localhost".to_string());
    let mut form_port = use_signal(|| "1883".to_string());
    let mut form_client_id = use_signal(String::new);
    let mut form_username = use_signal(String::new);
    let mut form_password = use_signal(String::new);
    let mut form_tls = use_signal(|| false);
    let mut form_keep_alive = use_signal(|| "30".to_string());
    let mut status_msg: Signal<Option<String>> = use_signal(|| None);
    let mut refresh = use_signal(|| 0u64);

    // Load brokers
    {
        let ms = ms.clone();
        let _ = use_resource(move || {
            let ms = ms.clone();
            let _r = *refresh.read();
            async move {
                brokers.set(ms.list_brokers().await);
            }
        });
    }

    let on_add = move |_| {
        editing_id.set(None);
        form_name.set(String::new());
        form_host.set("localhost".to_string());
        form_port.set("1883".to_string());
        form_client_id.set(String::new());
        form_username.set(String::new());
        form_password.set(String::new());
        form_tls.set(false);
        form_keep_alive.set("30".to_string());
        show_form.set(true);
    };

    let mut on_edit = move |b: MqttBrokerConfig| {
        editing_id.set(Some(b.id));
        form_name.set(b.name);
        form_host.set(b.host);
        form_port.set(b.port.to_string());
        form_client_id.set(b.client_id);
        form_username.set(b.username);
        form_password.set(b.password);
        form_tls.set(b.use_tls);
        form_keep_alive.set(b.keep_alive_secs.to_string());
        show_form.set(true);
    };

    rsx! {
        div { class: "section-header",
            h3 { "MQTT Brokers" }
            if can_manage {
                button {
                    class: "btn btn-primary btn-sm",
                    onclick: on_add,
                    "+ Add Broker"
                }
            }
        }

        if let Some(msg) = status_msg.read().as_ref() {
            div { class: "status-bar", "{msg}" }
        }

        if *show_form.read() && can_manage {
            div { class: "form-card",
                h4 { if editing_id.read().is_some() { "Edit Broker" } else { "New Broker" } }
                div { class: "form-row",
                    label { "Name" }
                    input {
                        r#type: "text",
                        placeholder: "My MQTT Broker",
                        value: "{form_name}",
                        oninput: move |e| form_name.set(e.value()),
                    }
                }
                div { class: "form-row",
                    label { "Host" }
                    input {
                        r#type: "text",
                        placeholder: "localhost",
                        value: "{form_host}",
                        oninput: move |e| form_host.set(e.value()),
                    }
                }
                div { class: "form-row",
                    label { "Port" }
                    input {
                        r#type: "number",
                        value: "{form_port}",
                        oninput: move |e| form_port.set(e.value()),
                    }
                }
                div { class: "form-row",
                    label { "Client ID (blank = auto)" }
                    input {
                        r#type: "text",
                        placeholder: "opencrate-xxxxx",
                        value: "{form_client_id}",
                        oninput: move |e| form_client_id.set(e.value()),
                    }
                }
                div { class: "form-row",
                    label { "Username" }
                    input {
                        r#type: "text",
                        value: "{form_username}",
                        oninput: move |e| form_username.set(e.value()),
                    }
                }
                div { class: "form-row",
                    label { "Password" }
                    input {
                        r#type: "password",
                        value: "{form_password}",
                        oninput: move |e| form_password.set(e.value()),
                    }
                }
                div { class: "form-row",
                    label { "Use TLS" }
                    input {
                        r#type: "checkbox",
                        checked: *form_tls.read(),
                        onchange: move |e: Event<FormData>| form_tls.set(e.checked()),
                    }
                }
                div { class: "form-row",
                    label { "Keep Alive (seconds)" }
                    input {
                        r#type: "number",
                        value: "{form_keep_alive}",
                        oninput: move |e| form_keep_alive.set(e.value()),
                    }
                }
                div { class: "form-actions",
                    button {
                        class: "btn btn-primary btn-sm",
                        onclick: {
                            let ms = ms.clone();
                            let audit_store = audit_store.clone();
                            move |_| {
                                let ms = ms.clone();
                                let audit_store = audit_store.clone();
                                let name = form_name.read().clone();
                                let host = form_host.read().clone();
                                let port: u16 = form_port.read().parse().unwrap_or(1883);
                                let client_id = form_client_id.read().clone();
                                let username = form_username.read().clone();
                                let password = form_password.read().clone();
                                let use_tls = *form_tls.read();
                                let keep_alive: u16 = form_keep_alive.read().parse().unwrap_or(30);
                                let edit_id = *editing_id.read();
                                let user = current_user.read().clone();
                                let (uid, uname) = match user.as_ref() {
                                    Some(u) => (u.id.clone(), u.username.clone()),
                                    None => ("system".into(), "system".into()),
                                };
                                spawn(async move {
                                    if let Some(id) = edit_id {
                                        if let Err(e) = ms.update_broker(id, &name, &host, port, &client_id, &username, &password, use_tls, true, keep_alive, true).await {
                                            status_msg.set(Some(format!("Error: {e}")));
                                            return;
                                        }
                                    } else {
                                        match ms.create_broker(&name, &host, port, &client_id, &username, &password, use_tls, true, keep_alive).await {
                                            Ok(broker_id) => {
                                                // Auto-create default topic patterns
                                                let _ = ms.create_topic_pattern(broker_id, MqttEventType::Value, topic::DEFAULT_VALUE_PATTERN, 0, false, "").await;
                                                let _ = ms.create_topic_pattern(broker_id, MqttEventType::Alarm, topic::DEFAULT_ALARM_PATTERN, 1, false, "").await;
                                                let _ = ms.create_topic_pattern(broker_id, MqttEventType::Status, topic::DEFAULT_STATUS_PATTERN, 1, true, "").await;
                                            }
                                            Err(e) => {
                                                status_msg.set(Some(format!("Error: {e}")));
                                                return;
                                            }
                                        }
                                    }
                                    let _ = audit_store.log_action(&uid, &uname,
                                        AuditEntryBuilder::new(AuditAction::ConfigureMqttBroker, "mqtt_broker").details(&name),
                                    ).await;
                                    show_form.set(false);
                                    status_msg.set(Some("Broker saved".into()));
                                    refresh.set(refresh() + 1);
                                });
                            }
                        },
                        "Save"
                    }
                    button {
                        class: "btn btn-sm",
                        onclick: move |_| show_form.set(false),
                        "Cancel"
                    }
                }
            }
        }

        table { class: "data-table",
            thead {
                tr {
                    th { "Name" }
                    th { "Host" }
                    th { "Port" }
                    th { "TLS" }
                    th { "Enabled" }
                    if can_manage { th { "Actions" } }
                }
            }
            tbody {
                for b in brokers.read().iter() {
                    {
                        let b_edit = b.clone();
                        let b_del = b.clone();
                        let ms_del = ms.clone();
                        let audit_del = audit_store.clone();
                        rsx! {
                            tr {
                                td { "{b.name}" }
                                td { "{b.host}" }
                                td { "{b.port}" }
                                td { if b.use_tls { "Yes" } else { "No" } }
                                td {
                                    span {
                                        class: if b.enabled { "badge badge-ok" } else { "badge badge-inactive" },
                                        if b.enabled { "Enabled" } else { "Disabled" }
                                    }
                                }
                                if can_manage {
                                    td { class: "action-cell",
                                        button {
                                            class: "btn btn-sm",
                                            onclick: move |_| on_edit(b_edit.clone()),
                                            "Edit"
                                        }
                                        button {
                                            class: "btn btn-sm btn-danger",
                                            onclick: {
                                                let ms_del = ms_del.clone();
                                                let audit_del = audit_del.clone();
                                                let name = b_del.name.clone();
                                                let id = b_del.id;
                                                let user = current_user.read().clone();
                                                let (uid, uname) = match user.as_ref() {
                                                    Some(u) => (u.id.clone(), u.username.clone()),
                                                    None => ("system".into(), "system".into()),
                                                };
                                                move |_| {
                                                    let ms = ms_del.clone();
                                                    let audit = audit_del.clone();
                                                    let name = name.clone();
                                                    let uid = uid.clone();
                                                    let uname = uname.clone();
                                                    spawn(async move {
                                                        if let Err(e) = ms.delete_broker(id).await {
                                                            status_msg.set(Some(format!("Error: {e}")));
                                                            return;
                                                        }
                                                        let _ = audit.log_action(&uid, &uname,
                                                            AuditEntryBuilder::new(AuditAction::DeleteMqttBroker, "mqtt_broker").details(&name),
                                                        ).await;
                                                        status_msg.set(Some("Broker deleted".into()));
                                                        refresh.set(refresh() + 1);
                                                    });
                                                }
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

        if brokers.read().is_empty() {
            div { class: "empty-state", "No MQTT brokers configured. Add a broker to start publishing building data." }
        }
    }
}

// ----------------------------------------------------------------
// Topic Patterns tab
// ----------------------------------------------------------------

#[component]
fn TopicsTab(can_manage: bool) -> Element {
    let state = use_context::<AppState>();
    let ms = state.mqtt_store.clone();
    let mut brokers: Signal<Vec<MqttBrokerConfig>> = use_signal(Vec::new);
    let mut selected_broker = use_signal(|| Option::<i64>::None);
    let mut topics: Signal<Vec<MqttTopicPattern>> = use_signal(Vec::new);
    let mut editing_id = use_signal(|| Option::<i64>::None);
    let mut form_pattern = use_signal(String::new);
    let mut form_qos = use_signal(|| "0".to_string());
    let mut form_retain = use_signal(|| false);
    let mut form_filter = use_signal(String::new);
    let mut status_msg: Signal<Option<String>> = use_signal(|| None);
    let mut refresh = use_signal(|| 0u64);

    // Load brokers
    {
        let ms = ms.clone();
        let _ = use_resource(move || {
            let ms = ms.clone();
            async move {
                let b = ms.list_brokers().await;
                if selected_broker.read().is_none() {
                    if let Some(first) = b.first() {
                        selected_broker.set(Some(first.id));
                    }
                }
                brokers.set(b);
            }
        });
    }

    // Load topics for selected broker
    {
        let ms = ms.clone();
        let _ = use_resource(move || {
            let ms = ms.clone();
            let broker_id = *selected_broker.read();
            let _r = *refresh.read();
            async move {
                if let Some(bid) = broker_id {
                    topics.set(ms.list_topic_patterns(bid).await);
                } else {
                    topics.set(Vec::new());
                }
            }
        });
    }

    rsx! {
        div { class: "section-header",
            h3 { "Topic Patterns" }
            div { class: "broker-selector",
                label { "Broker: " }
                select {
                    onchange: move |e| {
                        let v: i64 = e.value().parse().unwrap_or(0);
                        selected_broker.set(if v > 0 { Some(v) } else { None });
                        editing_id.set(None);
                    },
                    for b in brokers.read().iter() {
                        option {
                            value: "{b.id}",
                            selected: *selected_broker.read() == Some(b.id),
                            "{b.name}"
                        }
                    }
                }
            }
        }

        if let Some(msg) = status_msg.read().as_ref() {
            div { class: "status-bar", "{msg}" }
        }

        div { class: "help-text",
            "Available variables: "
            code { "{{device_id}}" }
            " "
            code { "{{point_id}}" }
            " "
            code { "{{node_id}}" }
            " "
            code { "{{severity}}" }
            " "
            code { "{{device_key}}" }
            " "
            code { "{{protocol}}" }
            " "
            code { "{{site_id}}" }
        }

        table { class: "data-table",
            thead {
                tr {
                    th { "Event Type" }
                    th { "Topic Pattern" }
                    th { "QoS" }
                    th { "Retain" }
                    th { "Node Filter" }
                    th { "Enabled" }
                    if can_manage { th { "Actions" } }
                }
            }
            tbody {
                for tp in topics.read().iter() {
                    {
                        let tp_edit = tp.clone();
                        let tp_id = tp.id;
                        let ms_upd = ms.clone();
                        rsx! {
                            tr {
                                td {
                                    span { class: "badge badge-info", "{tp.event_type.as_str()}" }
                                }
                                td {
                                    if *editing_id.read() == Some(tp.id) {
                                        input {
                                            r#type: "text",
                                            class: "inline-edit",
                                            value: "{form_pattern}",
                                            oninput: move |e| form_pattern.set(e.value()),
                                        }
                                    } else {
                                        code { "{tp.pattern}" }
                                    }
                                }
                                td {
                                    if *editing_id.read() == Some(tp.id) {
                                        select {
                                            value: "{form_qos}",
                                            onchange: move |e| form_qos.set(e.value()),
                                            option { value: "0", "0 (At most once)" }
                                            option { value: "1", "1 (At least once)" }
                                            option { value: "2", "2 (Exactly once)" }
                                        }
                                    } else {
                                        "{tp.qos}"
                                    }
                                }
                                td {
                                    if *editing_id.read() == Some(tp.id) {
                                        input {
                                            r#type: "checkbox",
                                            checked: *form_retain.read(),
                                            onchange: move |e: Event<FormData>| form_retain.set(e.checked()),
                                        }
                                    } else {
                                        if tp.retain { "Yes" } else { "No" }
                                    }
                                }
                                td {
                                    if *editing_id.read() == Some(tp.id) {
                                        input {
                                            r#type: "text",
                                            class: "inline-edit",
                                            placeholder: "comma-separated prefixes",
                                            value: "{form_filter}",
                                            oninput: move |e| form_filter.set(e.value()),
                                        }
                                    } else {
                                        if tp.node_filter.is_empty() {
                                            span { class: "text-muted", "all" }
                                        } else {
                                            "{tp.node_filter}"
                                        }
                                    }
                                }
                                td {
                                    span {
                                        class: if tp.enabled { "badge badge-ok" } else { "badge badge-inactive" },
                                        if tp.enabled { "On" } else { "Off" }
                                    }
                                }
                                if can_manage {
                                    td { class: "action-cell",
                                        if *editing_id.read() == Some(tp.id) {
                                            button {
                                                class: "btn btn-primary btn-sm",
                                                onclick: {
                                                    let ms = ms_upd.clone();
                                                    move |_| {
                                                        let ms = ms.clone();
                                                        let pattern = form_pattern.read().clone();
                                                        let qos: u8 = form_qos.read().parse().unwrap_or(0);
                                                        let retain = *form_retain.read();
                                                        let filter = form_filter.read().clone();
                                                        spawn(async move {
                                                            if let Err(e) = ms.update_topic_pattern(tp_id, &pattern, qos, retain, true, &filter).await {
                                                                status_msg.set(Some(format!("Error: {e}")));
                                                                return;
                                                            }
                                                            editing_id.set(None);
                                                            status_msg.set(Some("Pattern saved".into()));
                                                            refresh.set(refresh() + 1);
                                                        });
                                                    }
                                                },
                                                "Save"
                                            }
                                            button {
                                                class: "btn btn-sm",
                                                onclick: move |_| editing_id.set(None),
                                                "Cancel"
                                            }
                                        } else {
                                            button {
                                                class: "btn btn-sm",
                                                onclick: move |_| {
                                                    editing_id.set(Some(tp_edit.id));
                                                    form_pattern.set(tp_edit.pattern.clone());
                                                    form_qos.set(tp_edit.qos.to_string());
                                                    form_retain.set(tp_edit.retain);
                                                    form_filter.set(tp_edit.node_filter.clone());
                                                },
                                                "Edit"
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

        if topics.read().is_empty() && selected_broker.read().is_some() {
            div { class: "empty-state", "No topic patterns configured for this broker." }
        }
        if brokers.read().is_empty() {
            div { class: "empty-state", "Add a broker first to configure topic patterns." }
        }
    }
}

// ----------------------------------------------------------------
// Status tab
// ----------------------------------------------------------------

#[component]
fn StatusTab() -> Element {
    let state = use_context::<AppState>();
    let ms = state.mqtt_store.clone();
    let mut brokers: Signal<Vec<MqttBrokerConfig>> = use_signal(Vec::new);

    {
        let ms = ms.clone();
        let _ = use_resource(move || {
            let ms = ms.clone();
            async move {
                brokers.set(ms.list_brokers().await);
            }
        });
    }

    rsx! {
        div { class: "section-header",
            h3 { "Connection Status" }
        }

        table { class: "data-table",
            thead {
                tr {
                    th { "Broker" }
                    th { "Host" }
                    th { "Port" }
                    th { "TLS" }
                    th { "Enabled" }
                }
            }
            tbody {
                for b in brokers.read().iter() {
                    tr {
                        td { "{b.name}" }
                        td { "{b.host}" }
                        td { "{b.port}" }
                        td { if b.use_tls { "Yes" } else { "No" } }
                        td {
                            span {
                                class: if b.enabled { "badge badge-ok" } else { "badge badge-inactive" },
                                if b.enabled { "Enabled" } else { "Disabled" }
                            }
                        }
                    }
                }
            }
        }

        if brokers.read().is_empty() {
            div { class: "empty-state", "No MQTT brokers configured." }
        }
    }
}
