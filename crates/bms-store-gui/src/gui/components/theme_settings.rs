use dioxus::prelude::*;

use crate::gui::state::AppState;
use crate::gui::theme::{builtin_presets, derive_accent_palette, BaseMode, ThemeDefinition};

#[component]
pub fn ThemeSettingsView() -> Element {
    let mut state = use_context::<AppState>();
    let config = state.theme_config.read().clone();
    let presets = builtin_presets();

    let mut custom_accent = use_signal(|| {
        // Initialize from current accent
        let theme = crate::gui::theme::resolve_active_theme(&config);
        theme.dark.accent.clone()
    });

    let mut save_name = use_signal(String::new);
    let mut show_advanced = use_signal(|| false);

    // Project name editing
    let mut project_name = use_signal(|| state.project_meta.name.clone());
    let mut name_saved = use_signal(|| false);

    rsx! {
        div { class: "theme-settings",
            h2 { class: "theme-settings-title", "Appearance" }

            // ── Project Name ──
            div { class: "theme-section",
                h3 { class: "theme-section-title", "Project Name" }
                div { class: "form-row",
                    input {
                        r#type: "text",
                        value: "{project_name}",
                        oninput: move |e| {
                            project_name.set(e.value());
                            name_saved.set(false);
                        },
                    }
                    button {
                        class: "btn btn-primary btn-sm",
                        disabled: *name_saved.read(),
                        onclick: {
                            let project_id = state.project_meta.id.clone();
                            let project_root = state.project_paths.root.clone();
                            move |_| {
                                let new_name = project_name.read().trim().to_string();
                                if new_name.is_empty() {
                                    return;
                                }
                                match bms_store_storage::project::rename_project(&project_id, &new_name, &project_root) {
                                    Ok(()) => {
                                        state.project_meta.name = new_name;
                                        name_saved.set(true);
                                    }
                                    Err(e) => {
                                        tracing::error!("Failed to rename project: {e}");
                                    }
                                }
                            }
                        },
                        if *name_saved.read() { "Saved" } else { "Rename" }
                    }
                }
            }

            // ── Base Mode ──
            div { class: "theme-section",
                h3 { class: "theme-section-title", "Mode" }
                div { class: "theme-mode-row",
                    for mode in BaseMode::all() {
                        {
                            let m = *mode;
                            let is_active = config.base_mode == m;
                            rsx! {
                                button {
                                    class: if is_active { "theme-mode-btn active" } else { "theme-mode-btn" },
                                    onclick: move |_| {
                                        let mut cfg = state.theme_config.read().clone();
                                        cfg.base_mode = m;
                                        state.theme_config.set(cfg);
                                    },
                                    svg {
                                        width: "16",
                                        height: "16",
                                        view_box: "0 0 24 24",
                                        fill: "currentColor",
                                        path { d: "{m.icon_path()}" }
                                    }
                                    span { "{m.label()}" }
                                }
                            }
                        }
                    }
                }
            }

            // ── Theme Presets ──
            div { class: "theme-section",
                h3 { class: "theme-section-title", "Theme" }
                div { class: "theme-preset-grid",
                    for preset in &presets {
                        {
                            let pid = preset.id.clone();
                            let pname = preset.name.clone();
                            let accent = preset.dark.accent.clone();
                            let accent_for_click = accent.clone();
                            let accent_dim = preset.dark.accent_dim.clone();
                            let bg_dark = preset.dark.bg_primary.clone();
                            let bg_light = preset.light.bg_primary.clone();
                            let is_active = config.active_theme_id == pid;
                            rsx! {
                                button {
                                    class: if is_active { "theme-preset-card active" } else { "theme-preset-card" },
                                    onclick: move |_| {
                                        let mut cfg = state.theme_config.read().clone();
                                        cfg.active_theme_id = pid.clone();
                                        state.theme_config.set(cfg);
                                        custom_accent.set(accent_for_click.clone());
                                    },
                                    div { class: "theme-preset-swatch",
                                        div {
                                            class: "swatch-bg-pair",
                                            div {
                                                class: "swatch-half",
                                                style: "background: {bg_dark};",
                                            }
                                            div {
                                                class: "swatch-half",
                                                style: "background: {bg_light};",
                                            }
                                        }
                                        div {
                                            class: "swatch-circle",
                                            style: "background: {accent};",
                                        }
                                        div {
                                            class: "swatch-circle",
                                            style: "background: {accent_dim};",
                                        }
                                    }
                                    span { class: "theme-preset-name", "{pname}" }
                                    if is_active {
                                        svg {
                                            class: "theme-check",
                                            width: "14",
                                            height: "14",
                                            view_box: "0 0 24 24",
                                            fill: "var(--accent)",
                                            path { d: "M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z" }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Custom themes
                    for ct in &config.custom_themes {
                        {
                            let cid = ct.id.clone();
                            let cname = ct.name.clone();
                            let accent = ct.dark.accent.clone();
                            let accent_dim = ct.dark.accent_dim.clone();
                            let is_active = config.active_theme_id == cid;
                            let del_id = cid.clone();
                            rsx! {
                                div { class: "theme-preset-card-wrap",
                                    button {
                                        class: if is_active { "theme-preset-card active" } else { "theme-preset-card" },
                                        onclick: move |_| {
                                            let mut cfg = state.theme_config.read().clone();
                                            cfg.active_theme_id = cid.clone();
                                            state.theme_config.set(cfg);
                                        },
                                        div { class: "theme-preset-swatch",
                                            div {
                                                class: "swatch-circle",
                                                style: "background: {accent};",
                                            }
                                            div {
                                                class: "swatch-circle",
                                                style: "background: {accent_dim};",
                                            }
                                        }
                                        span { class: "theme-preset-name", "{cname}" }
                                        if is_active {
                                            svg {
                                                class: "theme-check",
                                                width: "14",
                                                height: "14",
                                                view_box: "0 0 24 24",
                                                fill: "var(--accent)",
                                                path { d: "M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z" }
                                            }
                                        }
                                    }
                                    button {
                                        class: "theme-delete-btn",
                                        title: "Delete theme",
                                        onclick: move |_| {
                                            let mut cfg = state.theme_config.read().clone();
                                            cfg.custom_themes.retain(|t| t.id != del_id);
                                            if cfg.active_theme_id == del_id {
                                                cfg.active_theme_id = "default".into();
                                            }
                                            state.theme_config.set(cfg);
                                        },
                                        "×"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ── Accent Color ──
            div { class: "theme-section",
                h3 { class: "theme-section-title", "Accent Color" }
                div { class: "theme-accent-row",
                    input {
                        r#type: "color",
                        class: "theme-color-input",
                        value: "{custom_accent}",
                        oninput: move |evt: Event<FormData>| {
                            let hex = evt.value().to_string();
                            custom_accent.set(hex.clone());
                            if let Some((dark, light)) = derive_accent_palette(&hex) {
                                let mut cfg = state.theme_config.read().clone();
                                // Create or update a "custom-accent" theme
                                let custom_id = "custom-accent".to_string();
                                cfg.custom_themes.retain(|t| t.id != custom_id);
                                cfg.custom_themes.push(ThemeDefinition {
                                    id: custom_id.clone(),
                                    name: "Custom Accent".into(),
                                    is_preset: false,
                                    dark,
                                    light,
                                });
                                cfg.active_theme_id = custom_id;
                                state.theme_config.set(cfg);
                            }
                        },
                    }
                    span { class: "theme-accent-label", "{custom_accent}" }
                }
            }

            // ── Custom Logo ──
            div { class: "theme-section",
                h3 { class: "theme-section-title", "Custom Logo" }
                div { class: "theme-logo-row",
                    if let Some(ref logo_path) = config.custom_logo {
                        div { class: "theme-logo-preview",
                            img {
                                src: "{logo_path}",
                                width: "40",
                                height: "40",
                            }
                        }
                        button {
                            class: "btn btn-small",
                            onclick: move |_| {
                                let mut cfg = state.theme_config.read().clone();
                                cfg.custom_logo = None;
                                state.theme_config.set(cfg);
                            },
                            "Remove Logo"
                        }
                    } else {
                        span { class: "theme-logo-placeholder", "No custom logo" }
                    }
                    button {
                        class: "btn btn-small",
                        onclick: move |_| {
                            let paths = state.project_paths.clone();
                            spawn(async move {
                                let file = rfd::AsyncFileDialog::new()
                                    .add_filter("Images", &["png", "jpg", "jpeg", "svg"])
                                    .pick_file()
                                    .await;
                                if let Some(f) = file {
                                    let src = f.path().to_path_buf();
                                    let ext = src.extension()
                                        .and_then(|e| e.to_str())
                                        .unwrap_or("png");
                                    let dest = paths.data_dir.join(format!("logo.{ext}"));
                                    if std::fs::copy(&src, &dest).is_ok() {
                                        let dest_str = dest.to_string_lossy().to_string();
                                        let mut cfg = state.theme_config.read().clone();
                                        cfg.custom_logo = Some(dest_str);
                                        state.theme_config.set(cfg);
                                    }
                                }
                            });
                        },
                        "Upload Logo"
                    }
                }
            }

            // ── Advanced Colors (collapsible) ──
            div { class: "theme-section",
                button {
                    class: "theme-advanced-toggle",
                    onclick: move |_| show_advanced.toggle(),
                    if *show_advanced.read() {
                        "▾ Advanced Colors"
                    } else {
                        "▸ Advanced Colors"
                    }
                }

                if *show_advanced.read() {
                    AdvancedColorEditor {}
                }
            }

            // ── Save as Custom Theme ──
            div { class: "theme-section",
                h3 { class: "theme-section-title", "Save Current as Theme" }
                div { class: "theme-save-row",
                    input {
                        class: "theme-name-input",
                        placeholder: "Theme name...",
                        value: "{save_name}",
                        oninput: move |evt: Event<FormData>| save_name.set(evt.value().to_string()),
                    }
                    button {
                        class: "btn btn-primary btn-small",
                        disabled: save_name.read().trim().is_empty(),
                        onclick: move |_| {
                            let name = save_name.read().trim().to_string();
                            if name.is_empty() { return; }
                            let mut cfg = state.theme_config.read().clone();
                            let current = crate::gui::theme::resolve_active_theme(&cfg);
                            let new_id = format!("custom-{}", uuid::Uuid::new_v4());
                            cfg.custom_themes.push(ThemeDefinition {
                                id: new_id.clone(),
                                name: name.clone(),
                                is_preset: false,
                                dark: current.dark.clone(),
                                light: current.light.clone(),
                            });
                            cfg.active_theme_id = new_id;
                            state.theme_config.set(cfg);
                            save_name.set(String::new());
                        },
                        "Save"
                    }
                }
            }
        }
    }
}

/// Advanced color editor: shows all 22 CSS variables with color inputs.
#[component]
fn AdvancedColorEditor() -> Element {
    let state = use_context::<AppState>();
    let mut theme_sig = state.theme_config;
    let config = theme_sig.read().clone();
    let theme = crate::gui::theme::resolve_active_theme(&config);
    let is_dark = config.base_mode != BaseMode::Light;
    let colors = if is_dark { &theme.dark } else { &theme.light };

    let labels = [
        ("bg_primary", "Background Primary"),
        ("bg_secondary", "Background Secondary"),
        ("bg_surface", "Surface"),
        ("bg_hover", "Hover"),
        ("bg_toolbar", "Toolbar"),
        ("text_primary", "Text Primary"),
        ("text_secondary", "Text Secondary"),
        ("text_muted", "Text Muted"),
        ("accent", "Accent"),
        ("accent_dim", "Accent Dim"),
        ("accent_subtle", "Accent Subtle"),
        ("border", "Border"),
        ("border_light", "Border Light"),
        ("success", "Success"),
        ("error", "Error"),
        ("group_bg", "Group Background"),
        ("selected_bg", "Selected Background"),
        ("value_color", "Value Color"),
        ("input_bg", "Input Background"),
        ("canvas_grid", "Canvas Grid"),
        ("scrollbar", "Scrollbar"),
        ("scrollbar_hover", "Scrollbar Hover"),
    ];

    let pairs = colors.as_pairs();

    rsx! {
        div { class: "theme-advanced-grid",
            for (i, (field_name, label)) in labels.iter().enumerate() {
                {
                    let current_val = pairs.get(i).map(|(_, v)| v.to_string()).unwrap_or_default();
                    let is_rgba = current_val.starts_with("rgba");
                    let display_val = current_val.clone();
                    let field = field_name.to_string();
                    rsx! {
                        div { class: "theme-color-row",
                            span { class: "theme-color-label", "{label}" }
                            if is_rgba {
                                input {
                                    class: "theme-color-text",
                                    value: "{display_val}",
                                    oninput: {
                                        let field = field.clone();
                                        move |evt: Event<FormData>| {
                                            update_color_field(&mut theme_sig, &field, &evt.value());
                                        }
                                    },
                                }
                            } else {
                                input {
                                    r#type: "color",
                                    class: "theme-color-input",
                                    value: "{display_val}",
                                    oninput: {
                                        let field = field.clone();
                                        move |evt: Event<FormData>| {
                                            update_color_field(&mut theme_sig, &field, &evt.value());
                                        }
                                    },
                                }
                                span { class: "theme-color-hex", "{display_val}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn update_color_field(
    theme_sig: &mut Signal<crate::gui::theme::ThemeConfig>,
    field: &str,
    value: &str,
) {
    let mut cfg = theme_sig.read().clone();
    let mut theme = crate::gui::theme::resolve_active_theme(&cfg);

    // Ensure we're editing a custom theme
    if theme.is_preset {
        let new_id = format!("custom-{}", uuid::Uuid::new_v4());
        theme.id = new_id.clone();
        theme.name = format!("{} (Custom)", theme.name);
        theme.is_preset = false;
        cfg.custom_themes.push(theme.clone());
        cfg.active_theme_id = new_id;
    }

    let is_dark = cfg.base_mode != BaseMode::Light;
    let colors = if is_dark {
        &mut theme.dark
    } else {
        &mut theme.light
    };

    match field {
        "bg_primary" => colors.bg_primary = value.into(),
        "bg_secondary" => colors.bg_secondary = value.into(),
        "bg_surface" => colors.bg_surface = value.into(),
        "bg_hover" => colors.bg_hover = value.into(),
        "bg_toolbar" => colors.bg_toolbar = value.into(),
        "text_primary" => colors.text_primary = value.into(),
        "text_secondary" => colors.text_secondary = value.into(),
        "text_muted" => colors.text_muted = value.into(),
        "accent" => colors.accent = value.into(),
        "accent_dim" => colors.accent_dim = value.into(),
        "accent_subtle" => colors.accent_subtle = value.into(),
        "border" => colors.border = value.into(),
        "border_light" => colors.border_light = value.into(),
        "success" => colors.success = value.into(),
        "error" => colors.error = value.into(),
        "group_bg" => colors.group_bg = value.into(),
        "selected_bg" => colors.selected_bg = value.into(),
        "value_color" => colors.value_color = value.into(),
        "input_bg" => colors.input_bg = value.into(),
        "canvas_grid" => colors.canvas_grid = value.into(),
        "scrollbar" => colors.scrollbar = value.into(),
        "scrollbar_hover" => colors.scrollbar_hover = value.into(),
        _ => {}
    }

    // Update the custom theme in config
    if let Some(ct) = cfg
        .custom_themes
        .iter_mut()
        .find(|t| t.id == cfg.active_theme_id)
    {
        if is_dark {
            ct.dark = theme.dark;
        } else {
            ct.light = theme.light;
        }
    }

    theme_sig.set(cfg);
}
