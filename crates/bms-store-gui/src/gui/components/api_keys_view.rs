//! API key management view.
//!
//! Lists existing keys, allows creating new keys (secret shown exactly once),
//! and revoking existing keys (with confirmation modal).

use dioxus::prelude::*;

use crate::gui::state::AppState;
use bms_store_storage::api_key_store::{ApiKeyInfo, CreatedApiKey};
use bms_store_storage::store::user_store::UserRole;

use super::preview_modal::{ChangeKind, PreviewModal, PreviewRow};

#[component]
pub fn ApiKeysView() -> Element {
    let state = use_context::<AppState>();
    let api_key_store = state.api_key_store.clone();

    let mut keys: Signal<Vec<ApiKeyInfo>> = use_signal(Vec::new);
    let mut version = use_signal(|| 0u32);
    let mut action_error: Signal<Option<String>> = use_signal(|| None);

    // Create-key form state.
    let mut show_create_form = use_signal(|| false);
    let mut new_name = use_signal(String::new);
    let mut new_role = use_signal(|| UserRole::Viewer);
    let mut creating = use_signal(|| false);

    // Show secret once after creation.
    let mut created_key: Signal<Option<CreatedApiKey>> = use_signal(|| None);

    // Revoke confirmation.
    let mut confirm_revoke: Signal<Option<ApiKeyInfo>> = use_signal(|| None);

    // Load keys.
    let store_load = api_key_store.clone();
    let _v = *version.read();
    let _ = use_resource(move || {
        let store = store_load.clone();
        let _v = _v;
        async move {
            let list = store.list().await;
            keys.set(list);
        }
    });

    let key_list = keys.read().clone();
    let is_creating = *creating.read();

    rsx! {
        div { class: "apikeys-section",
            div { class: "apikeys-header",
                h2 { "API Keys" }
                button {
                    class: "btn btn-sm btn-primary",
                    onclick: move |_| {
                        show_create_form.set(true);
                        new_name.set(String::new());
                        new_role.set(UserRole::Viewer);
                        action_error.set(None);
                    },
                    "+ New Key"
                }
            }

            if let Some(err) = &*action_error.read() {
                div { class: "apikeys-error", "{err}" }
            }

            // One-time secret display — shown after successful creation.
            if let Some(ck) = created_key.read().clone() {
                {
                    let ck = ck.clone();
                    rsx! {
                        div { class: "apikeys-secret-modal modal-overlay",
                            div { class: "modal-box apikeys-secret-box",
                                div { class: "modal-header",
                                    h3 { "Your New API Key" }
                                }
                                div { class: "modal-body",
                                    div { class: "apikeys-secret-warning",
                                        "Save this key now — it will NOT be shown again."
                                    }
                                    div { class: "apikeys-secret-meta",
                                        span { class: "apikeys-secret-label", "Name: " }
                                        span { "{ck.name}" }
                                    }
                                    div { class: "apikeys-secret-meta",
                                        span { class: "apikeys-secret-label", "Role: " }
                                        span { "{ck.role}" }
                                    }
                                    div { class: "apikeys-secret-key-wrap",
                                        span { class: "apikeys-secret-label", "Secret key:" }
                                        code { class: "apikeys-secret-value", "{ck.key}" }
                                    }
                                }
                                div { class: "modal-footer",
                                    button {
                                        class: "btn btn-primary",
                                        onclick: move |_| created_key.set(None),
                                        "I have saved the key"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Create form modal.
            if *show_create_form.read() {
                div { class: "modal-overlay",
                    onclick: move |_| show_create_form.set(false),
                    div {
                        class: "modal-box apikeys-create-modal",
                        onclick: |e| e.stop_propagation(),
                        div { class: "modal-header",
                            h3 { "Create API Key" }
                        }
                        div { class: "modal-body",
                            div { class: "form-row",
                                label { class: "form-label", "Key name" }
                                input {
                                    class: "form-input",
                                    r#type: "text",
                                    placeholder: "e.g. CI pipeline, monitoring service",
                                    value: "{new_name}",
                                    oninput: move |e| new_name.set(e.value()),
                                }
                            }
                            div { class: "form-row",
                                label { class: "form-label", "Role" }
                                select {
                                    class: "form-select",
                                    onchange: move |e| {
                                        let role = match e.value().as_str() {
                                            "Admin" => UserRole::Admin,
                                            "Operator" => UserRole::Operator,
                                            _ => UserRole::Viewer,
                                        };
                                        new_role.set(role);
                                    },
                                    option { value: "Viewer", selected: matches!(*new_role.read(), UserRole::Viewer), "Viewer" }
                                    option { value: "Operator", selected: matches!(*new_role.read(), UserRole::Operator), "Operator" }
                                    option { value: "Admin", selected: matches!(*new_role.read(), UserRole::Admin), "Admin" }
                                }
                            }
                        }
                        div { class: "modal-footer",
                            button {
                                class: "btn btn-secondary",
                                onclick: move |_| show_create_form.set(false),
                                "Cancel"
                            }
                            button {
                                class: "btn btn-primary",
                                disabled: is_creating || new_name.read().trim().is_empty(),
                                onclick: {
                                    let store = api_key_store.clone();
                                    move |_| {
                                        let store = store.clone();
                                        let name = new_name.read().trim().to_string();
                                        let role = new_role.read().clone();
                                        if name.is_empty() {
                                            return;
                                        }
                                        spawn(async move {
                                            creating.set(true);
                                            action_error.set(None);
                                            match store.create(&name, role).await {
                                                Ok(ck) => {
                                                    show_create_form.set(false);
                                                    created_key.set(Some(ck));
                                                    let next_v = version.read().wrapping_add(1);
                                                    version.set(next_v);
                                                }
                                                Err(e) => {
                                                    action_error.set(Some(format!("Failed to create key: {e}")));
                                                }
                                            }
                                            creating.set(false);
                                        });
                                    }
                                },
                                if is_creating { "Creating…" } else { "Create" }
                            }
                        }
                    }
                }
            }

            if key_list.is_empty() {
                div { class: "apikeys-empty",
                    p { "No API keys yet. Click \"+ New Key\" to create one." }
                    p { class: "apikeys-empty-hint",
                        "API keys allow programmatic access to the bms-store REST API
                        without a username and password."
                    }
                }
            } else {
                div { class: "apikeys-table-wrap",
                    table { class: "apikeys-table",
                        thead {
                            tr {
                                th { "Name" }
                                th { "Prefix" }
                                th { "Role" }
                                th { "Created" }
                                th { "Last Used" }
                                th { "Status" }
                                th { "Action" }
                            }
                        }
                        tbody {
                            for key in &key_list {
                                {
                                    let key = key.clone();
                                    let key_for_modal = key.clone();
                                    let status_class = if key.disabled {
                                        "apikeys-status-badge apikeys-status-disabled"
                                    } else {
                                        "apikeys-status-badge apikeys-status-active"
                                    };
                                    let status_label = if key.disabled { "Disabled" } else { "Active" };
                                    let created = format_ms(key.created_ms);
                                    let last_used = key.last_used_ms
                                        .map(format_ms)
                                        .unwrap_or_else(|| "Never".into());
                                    rsx! {
                                        tr { key: "{key.id}",
                                            td { class: "apikeys-name", "{key.name}" }
                                            td { class: "apikeys-prefix", code { "{key.prefix}…" } }
                                            td {
                                                span { class: "user-role-badge role-{key.role.label().to_lowercase()}", "{key.role}" }
                                            }
                                            td { "{created}" }
                                            td { "{last_used}" }
                                            td { span { class: status_class, "{status_label}" } }
                                            td {
                                                button {
                                                    class: "btn btn-xs btn-danger",
                                                    onclick: move |_| {
                                                        confirm_revoke.set(Some(key_for_modal.clone()));
                                                    },
                                                    "Revoke"
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

            // Revoke confirmation modal.
            if let Some(key_to_revoke) = confirm_revoke.read().clone() {
                {
                    let key = key_to_revoke.clone();
                    let store_rev = api_key_store.clone();
                    let rows = vec![PreviewRow {
                        id: key.id.clone(),
                        label: key.name.clone(),
                        before: format!("{} ({})", key.prefix, key.role),
                        after: "REVOKED".into(),
                        change_kind: ChangeKind::Remove,
                    }];
                    rsx! {
                        PreviewModal {
                            title: format!("Revoke API Key: {}", key.name),
                            rows,
                            on_confirm: move |_| {
                                let store = store_rev.clone();
                                let id = key.id.clone();
                                confirm_revoke.set(None);
                                spawn(async move {
                                    match store.delete(&id).await {
                                        Ok(_) => {
                                            action_error.set(None);
                                            let next_v = version.read().wrapping_add(1);
                                            version.set(next_v);
                                        }
                                        Err(e) => {
                                            action_error.set(Some(format!("Revoke failed: {e}")));
                                        }
                                    }
                                });
                            },
                            on_cancel: move |_| confirm_revoke.set(None),
                        }
                    }
                }
            }
        }
    }
}

fn format_ms(ms: i64) -> String {
    use std::time::{Duration, UNIX_EPOCH};
    let secs = (UNIX_EPOCH + Duration::from_millis(ms as u64))
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let h = (secs / 3600) % 24;
    let min = (secs / 60) % 60;
    let s = secs % 60;
    let (y, mo, d) = days_to_ymd(secs / 86400);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{min:02}:{s:02}")
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut year = 1970u64;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }
    let month_days: &[u64] = if is_leap(year) {
        &[31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        &[31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1u64;
    for &md in month_days {
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }
    (year, month, days + 1)
}

fn is_leap(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}
