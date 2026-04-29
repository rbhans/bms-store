use dioxus::prelude::*;

use crate::gui::state::AppState;
use bms_store_bridges::plugin::archive::{self, PluginManifest};
// TODO(bms-store-gui): WasmSection and the wasm-plugins service module are not
// part of the bms-store-bridges public surface. The WASM plugin runtime UI
// (enable/disable/reload/uninstall/install-from-archive) is stubbed out until
// the wasm-plugins feature is re-introduced. See Task 11/12 cleanup plan.
/// Stub for the WASM plugin section metadata — real type lives in the wasm-plugins crate.
#[derive(Clone, Debug, Default)]
pub struct WasmSection {
    pub exports: Vec<String>,
    pub abi_version: String,
}

// stub service module so call sites below compile; all stubs log a warning.
mod service {
    #[allow(dead_code)]
    pub struct WasmOutcome {
        pub manifest: bms_store_bridges::plugin::archive::PluginManifest,
        pub load_error: Option<String>,
    }

    #[allow(dead_code)]
    #[derive(Default)]
    pub struct ReloadOutcome {
        pub status: String,
        pub message: Option<String>,
        pub load_error: Option<String>,
    }

    pub async fn install_plugin_archive(
        _path: &std::path::Path,
        _data_dir: &std::path::Path,
        _rt: Option<&std::sync::Arc<()>>,
    ) -> Result<WasmOutcome, String> {
        Err("WASM plugin service not available in this build (wasm-plugins feature disabled)".into())
    }

    pub async fn enable_wasm_plugin(
        _data_dir: &std::path::Path,
        _id: &str,
        _rt: Option<&std::sync::Arc<()>>,
    ) -> Result<ReloadOutcome, String> {
        Err("WASM plugin service not available in this build".into())
    }

    pub fn disable_wasm_plugin(
        _data_dir: &std::path::Path,
        _id: &str,
        _rt: Option<&std::sync::Arc<()>>,
    ) -> Result<(), String> {
        Err("WASM plugin service not available in this build".into())
    }

    pub async fn reload_wasm_plugin(
        _data_dir: &std::path::Path,
        _id: &str,
        _rt: &std::sync::Arc<()>,
    ) -> Result<ReloadOutcome, String> {
        Err("WASM plugin service not available in this build".into())
    }

    pub fn uninstall_wasm_plugin(
        _data_dir: &std::path::Path,
        _id: &str,
        _rt: Option<&std::sync::Arc<()>>,
    ) -> Result<(), String> {
        Err("WASM plugin service not available in this build".into())
    }
}
use bms_store_bridges::plugin::{
    load_plugin_settings, plugin_catalog, resolve_plugin_status, PluginInfo, PluginSettings,
    PluginStatus,
};

/// Inline config section view for managing plugins.
/// Shown as a tab in the Config page.
#[component]
pub fn PluginManagerView() -> Element {
    let state = use_context::<AppState>();
    let catalog = plugin_catalog();

    let settings = use_signal(|| load_plugin_settings(&state.project_paths.data_dir));
    let mut refresh = use_signal(|| 0u32);
    let _v = *refresh.read();

    let mut install_error: Signal<Option<String>> = use_signal(|| None);
    let err = install_error.read().clone();

    rsx! {
        div { class: "plugin-manager-inline",
            // Header row
            div { class: "plugin-manager-inline-header",
                div {
                    h3 { "Plugins" }
                    p { class: "config-hint",
                        "Manage optional plugins. Install from "
                        code { ".ocplugin" }
                        " files or configure built-in plugins."
                    }
                }
                button {
                    class: "config-btn",
                    onclick: {
                        let data_dir = state.project_paths.data_dir.clone();
                        // TODO(bms-store-gui): wasm_runtime not available without wasm-plugins feature.
                        let rt: Option<std::sync::Arc<()>> = None;
                        move |_| {
                            let data_dir = data_dir.clone();
                            let rt = rt.clone();
                            spawn(async move {
                                let picked = rfd::AsyncFileDialog::new()
                                    .add_filter("OpenCrate Plugin", &["ocplugin"])
                                    .set_title("Install Plugin")
                                    .pick_file()
                                    .await;

                                if let Some(file) = picked {
                                    let path = file.path().to_path_buf();
                                    match service::install_plugin_archive(&path, &data_dir, rt.as_ref()).await {
                                        Ok(outcome) => {
                                            tracing::info!(
                                                plugin = outcome.manifest.plugin.id,
                                                version = outcome.manifest.plugin.version,
                                                "Plugin installed from archive"
                                            );
                                            if let Some(e) = outcome.load_error {
                                                install_error.set(Some(format!("Installed, but load failed: {e}")));
                                            } else {
                                                install_error.set(None);
                                            }
                                            let v = *refresh.read();
                                            refresh.set(v + 1);
                                        }
                                        Err(e) => {
                                            install_error.set(Some(format!("Install failed: {e}")));
                                        }
                                    }
                                }
                            });
                        }
                    },
                    "Install from File..."
                }
            }

            // Error display
            if let Some(ref msg) = err {
                div { class: "plugin-manager-error", "{msg}" }
            }

            // Built-in plugin list
            div { class: "plugin-manager-list",
                for info in &catalog {
                    {
                        let info = info.clone();
                        let data_installed = is_data_installed(&state, &info);
                        let status = resolve_plugin_status(&info, data_installed, &settings.read());
                        rsx! {
                            PluginRow {
                                info: info.clone(),
                                status,
                                settings,
                                refresh,
                                install_error,
                            }
                        }
                    }
                }
            }

            // WASM plugins section
            {
                let wasm_plugins = discover_wasm_plugins(&state.project_paths.data_dir);
                let mut settings = settings;
                if !wasm_plugins.is_empty() {
                    rsx! {
                        h4 { class: "plugin-section-header", "WASM Plugins" }
                        div { class: "plugin-manager-list",
                            for (plugin_id, manifest, wasm) in wasm_plugins.iter() {
                                {
                                    let pid = plugin_id.clone();
                                    let name = manifest.plugin.name.clone();
                                    let version = manifest.plugin.version.clone();
                                    let desc = manifest.plugin.description.clone();
                                    let exports = wasm.exports.join(", ");
                                    let abi = wasm.abi_version.clone();
                                    let disabled = settings.read().disabled.contains(&pid);
                                    rsx! {
                                        div { class: "plugin-row",
                                            div { class: "plugin-row-info",
                                                div { class: "plugin-row-header",
                                                    span { class: "plugin-name", "{name}" }
                                                    span { class: "plugin-version", "v{version}" }
                                                    span { class: "plugin-kind-badge plugin-kind-wasm", "WASM" }
                                                    if disabled {
                                                        span { class: "plugin-status-badge plugin-status-disabled", "Disabled" }
                                                    } else {
                                                        span { class: "plugin-status-badge plugin-status-active", "Loaded" }
                                                    }
                                                }
                                                p { class: "plugin-description", "{desc}" }
                                                p { class: "plugin-meta",
                                                    "Exports: "
                                                    code { "{exports}" }
                                                    " · ABI: "
                                                    code { "{abi}" }
                                                }
                                            }
                                            div { class: "plugin-row-actions",
                                                if disabled {
                                                    button {
                                                        class: "config-btn config-btn-primary",
                                                        onclick: {
                                                            let pid = pid.clone();
                                                            let data_dir = state.project_paths.data_dir.clone();
                                                            // TODO(bms-store-gui): wasm_runtime not available without wasm-plugins feature.
                        let rt: Option<std::sync::Arc<()>> = None;
                                                            move |_| {
                                                                let pid = pid.clone();
                                                                let data_dir = data_dir.clone();
                                                                let rt = rt.clone();
                                                                let mut settings = settings;
                                                                let mut err_sig = install_error;
                                                                let mut refresh = refresh;
                                                                dioxus::prelude::spawn(async move {
                                                                    match service::enable_wasm_plugin(&data_dir, &pid, rt.as_ref()).await {
                                                                        Ok(outcome) => {
                                                                            let s = load_plugin_settings(&data_dir);
                                                                            settings.set(s);
                                                                            if let Some(e) = outcome.load_error {
                                                                                err_sig.set(Some(format!("Enable load failed: {e}")));
                                                                            } else {
                                                                                err_sig.set(None);
                                                                            }
                                                                            let v = *refresh.read();
                                                                            refresh.set(v + 1);
                                                                        }
                                                                        Err(e) => err_sig.set(Some(format!("Enable failed: {e}"))),
                                                                    }
                                                                });
                                                            }
                                                        },
                                                        "Enable"
                                                    }
                                                } else {
                                                    button {
                                                        class: "config-btn",
                                                        onclick: {
                                                            let pid = pid.clone();
                                                            let data_dir = state.project_paths.data_dir.clone();
                                                            // TODO(bms-store-gui): wasm_runtime not available without wasm-plugins feature.
                        let rt: Option<std::sync::Arc<()>> = None;
                                                            move |_| {
                                                                match service::disable_wasm_plugin(&data_dir, &pid, rt.as_ref()) {
                                                                    Ok(()) => {
                                                                        let s = load_plugin_settings(&data_dir);
                                                                        settings.set(s);
                                                                        install_error.set(None);
                                                                        let v = *refresh.read();
                                                                        refresh.set(v + 1);
                                                                    }
                                                                    Err(e) => install_error.set(Some(format!("Disable failed: {e}"))),
                                                                }
                                                            }
                                                        },
                                                        "Disable"
                                                    }
                                                    // Reload — re-reads plugin.toml, re-checks ABI,
                                                    // swaps the live instance without restarting the
                                                    // host. Only shown when the plugin is enabled.
                                                    // (The `desktop` feature pulls in `wasm-plugins`,
                                                    // so `state.wasm_runtime` is always available in
                                                    // this code path.)
                                                    button {
                                                        class: "config-btn",
                                                        onclick: {
                                                            let pid = pid.clone();
                                                            let data_dir = state.project_paths.data_dir.clone();
                                                            // TODO(bms-store-gui): wasm_runtime not available without wasm-plugins feature.
                        let rt: Option<std::sync::Arc<()>> = None;
                                                            move |_| {
                                                                let pid = pid.clone();
                                                                let data_dir = data_dir.clone();
                                                                let rt = rt.clone();
                                                                let mut err_sig = install_error;
                                                                let mut refresh = refresh;
                                                                dioxus::prelude::spawn(async move {
                                                                    let Some(rt) = rt else {
                                                                        err_sig.set(Some("WASM runtime not initialized".into()));
                                                                        return;
                                                                    };
                                                                    match service::reload_wasm_plugin(&data_dir, &pid, &rt).await {
                                                                        Ok(outcome) if outcome.status == "reloaded" => {
                                                                            err_sig.set(None);
                                                                            let v = *refresh.read();
                                                                            refresh.set(v + 1);
                                                                        }
                                                                        Ok(outcome) => {
                                                                            err_sig.set(Some(outcome.message.unwrap_or_else(|| format!(
                                                                                "Reload skipped: {}",
                                                                                outcome.status
                                                                            ))));
                                                                        }
                                                                        Err(e) => {
                                                                            err_sig.set(Some(format!(
                                                                                "Reload failed: {e}"
                                                                            )));
                                                                        }
                                                                    }
                                                                });
                                                            }
                                                        },
                                                        "Reload"
                                                    }
                                                }
                                                button {
                                                    class: "config-btn config-btn-danger",
                                                        onclick: {
                                                        let pid = pid.clone();
                                                        let data_dir = state.project_paths.data_dir.clone();
                                                        // TODO(bms-store-gui): wasm_runtime not available without wasm-plugins feature.
                        let rt: Option<std::sync::Arc<()>> = None;
                                                        move |_| {
                                                            match service::uninstall_wasm_plugin(&data_dir, &pid, rt.as_ref()) {
                                                                Ok(()) => {
                                                                    let s = load_plugin_settings(&data_dir);
                                                                    settings.set(s);
                                                                    install_error.set(None);
                                                                    let v = *refresh.read();
                                                                    refresh.set(v + 1);
                                                                }
                                                                Err(e) => {
                                                                    install_error.set(Some(format!("Uninstall failed: {e}")));
                                                                }
                                                            }
                                                        }
                                                    },
                                                    "Uninstall"
                                                }
                                            }
                                        }
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

/// A single row in the plugin list.
#[component]
fn PluginRow(
    info: PluginInfo,
    status: PluginStatus,
    mut settings: Signal<PluginSettings>,
    mut refresh: Signal<u32>,
    mut install_error: Signal<Option<String>>,
) -> Element {
    let mut state = use_context::<AppState>();
    let status_class = status.css_class();
    let status_label = status.label();
    let has_config = info.config_section.is_some();
    let plugin_id = info.id.to_string();

    rsx! {
        div { class: "plugin-row",
            // Info section
            div { class: "plugin-row-info",
                div { class: "plugin-row-header",
                    span { class: "plugin-name", "{info.name}" }
                    span { class: "plugin-status-badge {status_class}", "{status_label}" }
                }
                p { class: "plugin-description", "{info.description}" }
                if let Some(flag) = info.feature_flag {
                    if !info.compiled_in {
                        p { class: "plugin-feature-hint",
                            "Requires: "
                            code { "--features {flag}" }
                        }
                    }
                }
            }

            // Actions
            div { class: "plugin-row-actions",
                match status {
                    PluginStatus::NotCompiled => rsx! {
                        // No actions — needs recompilation
                    },
                    PluginStatus::Available => rsx! {
                        // Not installed yet — offer install (navigate to plugin config section)
                        if has_config {
                            button {
                                class: "config-btn config-btn-primary",
                                onclick: {
                                    let section = info.config_section.map(String::from);
                                    move |_| {
                                        if let Some(ref s) = section {
                                            state.pending_config_section.set(Some(s.clone()));
                                        }
                                    }
                                },
                                "Install"
                            }
                        }
                    },
                    PluginStatus::Disabled => rsx! {
                        button {
                            class: "config-btn config-btn-primary",
                            onclick: {
                                let pid = plugin_id.clone();
                                let data_dir = state.project_paths.data_dir.clone();
                                // TODO(bms-store-gui): wasm_runtime not available without wasm-plugins feature.
                        let rt: Option<std::sync::Arc<()>> = None;
                                move |_| {
                                    let pid = pid.clone();
                                    let data_dir = data_dir.clone();
                                    let rt = rt.clone();
                                    let mut settings = settings;
                                    let mut install_error = install_error;
                                    let mut refresh = refresh;
                                    dioxus::prelude::spawn(async move {
                                        match service::enable_wasm_plugin(&data_dir, &pid, rt.as_ref()).await {
                                            Ok(outcome) => {
                                                settings.set(load_plugin_settings(&data_dir));
                                                if let Some(e) = outcome.load_error {
                                                    install_error.set(Some(format!("Enable load failed: {e}")));
                                                } else {
                                                    install_error.set(None);
                                                }
                                                let v = *refresh.read();
                                                refresh.set(v + 1);
                                            }
                                            Err(e) => install_error.set(Some(format!("Enable failed: {e}"))),
                                        }
                                    });
                                }
                            },
                            "Enable"
                        }
                        if has_config {
                            button {
                                class: "config-btn",
                                onclick: {
                                    let section = info.config_section.map(String::from);
                                    move |_| {
                                        if let Some(ref s) = section {
                                            state.pending_config_section.set(Some(s.clone()));
                                        }
                                    }
                                },
                                "Settings"
                            }
                        }
                    },
                    PluginStatus::Active => rsx! {
                        button {
                            class: "config-btn",
                            onclick: {
                                let pid = plugin_id.clone();
                                let data_dir = state.project_paths.data_dir.clone();
                                // TODO(bms-store-gui): wasm_runtime not available without wasm-plugins feature.
                        let rt: Option<std::sync::Arc<()>> = None;
                                move |_| {
                                    match service::disable_wasm_plugin(&data_dir, &pid, rt.as_ref()) {
                                        Ok(()) => {
                                            settings.set(load_plugin_settings(&data_dir));
                                            install_error.set(None);
                                            let v = *refresh.read();
                                            refresh.set(v + 1);
                                        }
                                        Err(e) => install_error.set(Some(format!("Disable failed: {e}"))),
                                    }
                                }
                            },
                            "Disable"
                        }
                        if has_config {
                            button {
                                class: "config-btn",
                                onclick: {
                                    let section = info.config_section.map(String::from);
                                    move |_| {
                                        if let Some(ref s) = section {
                                            state.pending_config_section.set(Some(s.clone()));
                                        }
                                    }
                                },
                                "Settings"
                            }
                        }
                        button {
                            class: "config-btn config-btn-danger",
                            onclick: {
                                let pid = plugin_id.clone();
                                let data_dir = state.project_paths.data_dir.clone();
                                // TODO(bms-store-gui): wasm_runtime not available without wasm-plugins feature.
                        let rt: Option<std::sync::Arc<()>> = None;
                                move |_| {
                                    match service::uninstall_wasm_plugin(&data_dir, &pid, rt.as_ref()) {
                                        Ok(()) => {
                                            settings.set(load_plugin_settings(&data_dir));
                                            install_error.set(None);
                                            let v = *refresh.read();
                                            refresh.set(v + 1);
                                        }
                                        Err(e) => {
                                            install_error.set(Some(format!("Uninstall failed: {e}")));
                                        }
                                    }
                                }
                            },
                            "Uninstall"
                        }
                    },
                }
            }
        }
    }
}

/// Discover WASM plugins installed in data/plugins/*/plugin.toml.
///
/// TODO(bms-store-gui): `archive::read_manifest_from_str` and `PluginManifest::wasm`
/// are not part of the bms-store-bridges public surface (they belong to the
/// wasm-plugins feature that was not extracted). Returns empty until the
/// wasm-plugins feature is re-introduced in bms-store-bridges.
fn discover_wasm_plugins(_data_dir: &std::path::Path) -> Vec<(String, PluginManifest, WasmSection)> {
    // Stubbed: WASM plugin discovery requires `archive::read_manifest_from_str`
    // and `PluginManifest::wasm` which are absent without wasm-plugins feature.
    Vec::new()
}

/// Check if a plugin's data is installed in the current project.
fn is_data_installed(state: &AppState, info: &PluginInfo) -> bool {
    match info.id {
        #[cfg(feature = "atlas")]
        "atlas" => {
            let path = state.project_paths.db_path("bas-atlas.db");
            bms_store_storage::atlas::db::AtlasDb::is_available(&path)
        }
        _ => {
            // Check for a saved manifest file from .ocplugin install
            let manifest_path = state
                .project_paths
                .data_dir
                .join(format!("plugin-{}.toml", info.id));
            manifest_path.exists()
        }
    }
}
