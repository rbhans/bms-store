//! Modal form for adding or editing a remote-site connection profile in the
//! launcher's Supervisor tab.
//!
//! On Test Connection: builds a `RemoteSiteClient::for_connect_test`, calls
//! `health()`, then attempts a login with the provided credentials. Reports
//! success / unreachable / auth failure inline. The form's "Save" button is
//! intentionally enabled even without a successful test — operators can save
//! a profile they intend to fix later.

use std::sync::Arc;

use dioxus::prelude::*;

use crate::supervisor::remote::client::RemoteSiteClient;
use crate::supervisor::remote::types::{RemoteCredentials, RemoteSiteError};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RemoteSiteFormData {
    pub config_id: String, // empty when creating
    pub name: String,
    pub base_url: String,
    pub username: String,
    pub password: String,
}

impl RemoteSiteFormData {
    pub fn is_valid(&self) -> bool {
        !self.name.trim().is_empty()
            && !self.base_url.trim().is_empty()
            && !self.username.trim().is_empty()
            && !self.password.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
enum TestStatus {
    Idle,
    Testing,
    Ok(String),
    Failed(String),
}

#[component]
pub fn RemoteSiteForm(
    initial: RemoteSiteFormData,
    on_save: EventHandler<RemoteSiteFormData>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut form = use_signal(|| initial.clone());
    let mut test_status = use_signal(|| TestStatus::Idle);

    let title = if initial.config_id.is_empty() {
        "Add Remote Site"
    } else {
        "Edit Remote Site"
    };

    let test_state = test_status.read().clone();

    rsx! {
        div { class: "login-backdrop",
            div { class: "login-card remote-site-form",
                h3 { "{title}" }

                label { "Name" }
                input {
                    r#type: "text",
                    placeholder: "HQ",
                    value: "{form.read().name}",
                    oninput: move |e| {
                        let mut f = form.write();
                        f.name = e.value();
                    },
                }

                label { "Base URL" }
                input {
                    r#type: "text",
                    placeholder: "http://localhost:8080",
                    value: "{form.read().base_url}",
                    oninput: move |e| {
                        let mut f = form.write();
                        f.base_url = e.value();
                    },
                }

                label { "Username" }
                input {
                    r#type: "text",
                    value: "{form.read().username}",
                    oninput: move |e| {
                        let mut f = form.write();
                        f.username = e.value();
                    },
                }

                label { "Password" }
                input {
                    r#type: "password",
                    value: "{form.read().password}",
                    oninput: move |e| {
                        let mut f = form.write();
                        f.password = e.value();
                    },
                }

                p { class: "text-muted text-xs",
                    "The supervisor stores credentials encrypted at rest under "
                    "~/.opencrate/supervisor.db. Plaintext lives in memory only "
                    "while the supervisor is running."
                }

                div { class: "remote-test-status",
                    match &test_state {
                        TestStatus::Idle => rsx! { span { class: "text-muted", "" } },
                        TestStatus::Testing => rsx! { span { class: "text-muted", "Testing connection…" } },
                        TestStatus::Ok(msg) => rsx! { span { class: "text-success", "✓ {msg}" } },
                        TestStatus::Failed(msg) => rsx! { span { class: "text-danger", "✗ {msg}" } },
                    }
                }

                div { class: "login-card-actions",
                    button {
                        class: "btn",
                        onclick: move |_| {
                            let f = form.read().clone();
                            if !f.is_valid() {
                                test_status.set(TestStatus::Failed("All fields required".into()));
                                return;
                            }
                            test_status.set(TestStatus::Testing);
                            spawn(async move {
                                let creds = RemoteCredentials {
                                    username: f.username.clone(),
                                    password: f.password.clone(),
                                };
                                let client = match RemoteSiteClient::for_connect_test(&f.base_url, creds) {
                                    Ok(c) => Arc::new(c),
                                    Err(e) => {
                                        test_status.set(TestStatus::Failed(format!("setup: {e}")));
                                        return;
                                    }
                                };
                                // Health check first.
                                if let Err(e) = client.health().await {
                                    test_status.set(TestStatus::Failed(describe(&e)));
                                    return;
                                }
                                // Then verify credentials by fetching system info.
                                match client.system_info().await {
                                    Ok(info) => {
                                        test_status.set(TestStatus::Ok(format!(
                                            "v{} — {}",
                                            info.version, info.scenario_name
                                        )));
                                    }
                                    Err(e) => {
                                        test_status.set(TestStatus::Failed(describe(&e)));
                                    }
                                }
                            });
                        },
                        "Test connection"
                    }
                    button {
                        class: "btn",
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }
                    button {
                        class: "btn btn-primary",
                        disabled: !form.read().is_valid(),
                        onclick: move |_| {
                            let f = form.read().clone();
                            if f.is_valid() {
                                on_save.call(f);
                            }
                        },
                        "Save"
                    }
                }
            }
        }
    }
}

fn describe(e: &RemoteSiteError) -> String {
    match e {
        RemoteSiteError::Unreachable(s) => format!("unreachable: {s}"),
        RemoteSiteError::AuthFailed => "auth failed (check username/password)".into(),
        RemoteSiteError::BadStatus(c) => format!("HTTP {c}"),
        RemoteSiteError::Decode(s) => format!("decode error: {s}"),
        RemoteSiteError::Timeout => "timeout".into(),
        RemoteSiteError::Setup(s) => format!("client setup: {s}"),
    }
}
