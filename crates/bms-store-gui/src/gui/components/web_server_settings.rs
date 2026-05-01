use dioxus::prelude::*;

use bms_store_storage::config::scenario::WebServerConfig;
use crate::gui::state::AppState;

/// Load web server config from the project data directory.
/// Falls back to scenario settings, then defaults.
pub fn load_web_server_config(
    paths: &bms_store_storage::project::ProjectPaths,
    scenario_cfg: Option<&WebServerConfig>,
) -> WebServerConfig {
    let path = paths.data_dir.join("web_server.json");
    std::fs::read_to_string(path)
        .ok()
        .and_then(|data| serde_json::from_str(&data).ok())
        .or_else(|| scenario_cfg.cloned())
        .unwrap_or_default()
}

/// Save web server config to the project data directory.
pub fn save_web_server_config(paths: &bms_store_storage::project::ProjectPaths, config: &WebServerConfig) {
    let path = paths.data_dir.join("web_server.json");
    if let Ok(json) = serde_json::to_string_pretty(config) {
        let _ = std::fs::write(path, json);
    }
}

#[component]
pub fn WebServerSettingsView() -> Element {
    let state = use_context::<AppState>();
    let paths = state.project_paths.clone();

    let scenario_web = state
        .loaded
        .config
        .settings
        .as_ref()
        .and_then(|s| s.web_server.clone());

    let mut config = use_signal(|| load_web_server_config(&paths, scenario_web.as_ref()));
    let mut saved_msg = use_signal(|| Option::<String>::None);

    let cfg = config.read().clone();

    rsx! {
        div { class: "web-server-settings",
            h2 { class: "theme-settings-title", "Web Server" }

            // ── HTTP Settings ──
            div { class: "theme-section",
                h3 { class: "theme-section-title", "HTTP" }
                div { class: "ws-form-grid",
                    // HTTP Enabled
                    div { class: "ws-form-row",
                        label { class: "ws-label", "Enabled" }
                        input {
                            r#type: "checkbox",
                            checked: cfg.http_enabled,
                            onchange: move |evt: Event<FormData>| {
                                let mut c = config.read().clone();
                                c.http_enabled = evt.checked();
                                config.set(c);
                            },
                        }
                    }
                    // HTTP Port
                    div { class: "ws-form-row",
                        label { class: "ws-label", "Port" }
                        input {
                            r#type: "number",
                            class: "ws-input ws-input-short",
                            value: "{cfg.http_port}",
                            oninput: move |evt: Event<FormData>| {
                                if let Ok(port) = evt.value().parse::<u16>() {
                                    let mut c = config.read().clone();
                                    c.http_port = port;
                                    config.set(c);
                                }
                            },
                        }
                    }
                    // Listen Address
                    div { class: "ws-form-row",
                        label { class: "ws-label", "Listen Address" }
                        input {
                            r#type: "text",
                            class: "ws-input",
                            value: "{cfg.listen_addr}",
                            oninput: move |evt: Event<FormData>| {
                                let mut c = config.read().clone();
                                c.listen_addr = evt.value().to_string();
                                config.set(c);
                            },
                        }
                    }
                }
            }

            // ── HTTPS Settings ──
            div { class: "theme-section",
                h3 { class: "theme-section-title", "HTTPS / TLS" }
                div { class: "ws-form-grid",
                    // HTTPS Enabled
                    div { class: "ws-form-row",
                        label { class: "ws-label", "Enabled" }
                        input {
                            r#type: "checkbox",
                            checked: cfg.https_enabled,
                            onchange: move |evt: Event<FormData>| {
                                let mut c = config.read().clone();
                                c.https_enabled = evt.checked();
                                config.set(c);
                            },
                        }
                    }
                    // HTTPS Port
                    div { class: "ws-form-row",
                        label { class: "ws-label", "Port" }
                        input {
                            r#type: "number",
                            class: "ws-input ws-input-short",
                            value: "{cfg.https_port}",
                            oninput: move |evt: Event<FormData>| {
                                if let Ok(port) = evt.value().parse::<u16>() {
                                    let mut c = config.read().clone();
                                    c.https_port = port;
                                    config.set(c);
                                }
                            },
                        }
                    }
                    // Certificate File
                    div { class: "ws-form-row",
                        label { class: "ws-label", "Certificate (PEM)" }
                        input {
                            r#type: "text",
                            class: "ws-input",
                            placeholder: "/path/to/cert.pem",
                            value: "{cfg.cert_file.as_deref().unwrap_or_default()}",
                            oninput: move |evt: Event<FormData>| {
                                let mut c = config.read().clone();
                                let v = evt.value().to_string();
                                c.cert_file = if v.is_empty() { None } else { Some(v) };
                                config.set(c);
                            },
                        }
                    }
                    // Key File
                    div { class: "ws-form-row",
                        label { class: "ws-label", "Private Key (PEM)" }
                        input {
                            r#type: "text",
                            class: "ws-input",
                            placeholder: "/path/to/key.pem",
                            value: "{cfg.key_file.as_deref().unwrap_or_default()}",
                            oninput: move |evt: Event<FormData>| {
                                let mut c = config.read().clone();
                                let v = evt.value().to_string();
                                c.key_file = if v.is_empty() { None } else { Some(v) };
                                config.set(c);
                            },
                        }
                    }
                    // Redirect HTTP to HTTPS
                    div { class: "ws-form-row",
                        label { class: "ws-label", "Redirect HTTP → HTTPS" }
                        input {
                            r#type: "checkbox",
                            checked: cfg.redirect_to_https,
                            onchange: move |evt: Event<FormData>| {
                                let mut c = config.read().clone();
                                c.redirect_to_https = evt.checked();
                                config.set(c);
                            },
                        }
                    }
                }
            }

            // ── Save / Status ──
            div { class: "ws-actions",
                button {
                    class: "btn btn-primary",
                    onclick: move |_| {
                        let c = config.read().clone();
                        save_web_server_config(&paths, &c);
                        saved_msg.set(Some("Saved. Restart the server for changes to take effect.".into()));
                    },
                    "Save"
                }
                if let Some(ref msg) = *saved_msg.read() {
                    span { class: "ws-saved-msg", "{msg}" }
                }
            }

            // ── Info ──
            div { class: "ws-info",
                p { class: "ws-info-text",
                    "Changes to web server settings require a restart. "
                    "The server reads this configuration on startup."
                }
                p { class: "ws-info-text",
                    "CLI flag "
                    code { "--api-addr" }
                    " overrides these settings."
                }
            }
        }
    }
}
