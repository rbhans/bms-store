use serde::{Deserialize, Serialize};

use bms_store_storage::project::ProjectPaths;

// ----------------------------------------------------------------
// Data model
// ----------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    pub active_theme_id: String,
    pub base_mode: BaseMode,
    #[serde(default)]
    pub custom_themes: Vec<ThemeDefinition>,
    #[serde(default)]
    pub custom_logo: Option<String>,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            active_theme_id: "default".into(),
            base_mode: BaseMode::Dark,
            custom_themes: Vec::new(),
            custom_logo: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BaseMode {
    Dark,
    Light,
    System,
}

impl BaseMode {
    pub fn all() -> &'static [BaseMode] {
        &[Self::Dark, Self::Light, Self::System]
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Dark => "Dark",
            Self::Light => "Light",
            Self::System => "System",
        }
    }

    pub fn icon_path(&self) -> &'static str {
        match self {
            // Moon
            Self::Dark => "M12 3c-4.97 0-9 4.03-9 9s4.03 9 9 9 9-4.03 9-9c0-.46-.04-.92-.1-1.36-.98 1.37-2.58 2.26-4.4 2.26-2.98 0-5.4-2.42-5.4-5.4 0-1.81.89-3.42 2.26-4.4-.44-.06-.9-.1-1.36-.1z",
            // Sun
            Self::Light => "M6.76 4.84l-1.8-1.79-1.41 1.41 1.79 1.79 1.42-1.41zM4 10.5H1v2h3v-2zm9-9.95h-2V3.5h2V.55zm7.45 3.91l-1.41-1.41-1.79 1.79 1.41 1.41 1.79-1.79zm-3.21 13.7l1.79 1.8 1.41-1.41-1.8-1.79-1.4 1.4zM20 10.5v2h3v-2h-3zm-8-5c-3.31 0-6 2.69-6 6s2.69 6 6 6 6-2.69 6-6-2.69-6-6-6zm-1 16.95h2V19.5h-2v2.95zm-7.45-3.91l1.41 1.41 1.79-1.8-1.41-1.41-1.79 1.8z",
            // Monitor/system
            Self::System => "M21 2H3c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h7l-2 3v1h8v-1l-2-3h7c1.1 0 2-.9 2-2V4c0-1.1-.9-2-2-2zm0 12H3V4h18v10z",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeDefinition {
    pub id: String,
    pub name: String,
    pub is_preset: bool,
    pub dark: ThemeColors,
    pub light: ThemeColors,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeColors {
    pub bg_primary: String,
    pub bg_secondary: String,
    pub bg_surface: String,
    pub bg_hover: String,
    pub bg_toolbar: String,
    pub text_primary: String,
    pub text_secondary: String,
    pub text_muted: String,
    pub accent: String,
    pub accent_dim: String,
    pub accent_subtle: String,
    pub border: String,
    pub border_light: String,
    pub success: String,
    pub error: String,
    pub group_bg: String,
    pub selected_bg: String,
    pub value_color: String,
    pub input_bg: String,
    pub canvas_grid: String,
    pub scrollbar: String,
    pub scrollbar_hover: String,
}

impl ThemeColors {
    /// Returns the list of all CSS variable names (without --) and their values.
    pub fn as_pairs(&self) -> Vec<(&'static str, &str)> {
        vec![
            ("bg-primary", &self.bg_primary),
            ("bg-secondary", &self.bg_secondary),
            ("bg-surface", &self.bg_surface),
            ("bg-hover", &self.bg_hover),
            ("bg-toolbar", &self.bg_toolbar),
            ("text-primary", &self.text_primary),
            ("text-secondary", &self.text_secondary),
            ("text-muted", &self.text_muted),
            ("accent", &self.accent),
            ("accent-dim", &self.accent_dim),
            ("accent-subtle", &self.accent_subtle),
            ("border", &self.border),
            ("border-light", &self.border_light),
            ("success", &self.success),
            ("error", &self.error),
            ("group-bg", &self.group_bg),
            ("selected-bg", &self.selected_bg),
            ("value-color", &self.value_color),
            ("input-bg", &self.input_bg),
            ("canvas-grid", &self.canvas_grid),
            ("scrollbar", &self.scrollbar),
            ("scrollbar-hover", &self.scrollbar_hover),
        ]
    }
}

// ----------------------------------------------------------------
// Default colors (current terracotta)
// ----------------------------------------------------------------

fn default_dark() -> ThemeColors {
    ThemeColors {
        bg_primary: "#2B2927".into(),
        bg_secondary: "#333130".into(),
        bg_surface: "#3B3938".into(),
        bg_hover: "#454342".into(),
        bg_toolbar: "#232120".into(),
        text_primary: "#E8E4DF".into(),
        text_secondary: "#9B9590".into(),
        text_muted: "#6B6662".into(),
        accent: "#D4714E".into(),
        accent_dim: "#C04A22".into(),
        accent_subtle: "rgba(212, 113, 78, 0.12)".into(),
        border: "#3B3938".into(),
        border_light: "#454342".into(),
        success: "#7DB87D".into(),
        error: "#D4564E".into(),
        group_bg: "#302E2C".into(),
        selected_bg: "#3B3938".into(),
        value_color: "#E8A87C".into(),
        input_bg: "#232120".into(),
        canvas_grid: "rgba(255, 255, 255, 0.03)".into(),
        scrollbar: "#454342".into(),
        scrollbar_hover: "#6B6662".into(),
    }
}

fn default_light() -> ThemeColors {
    ThemeColors {
        bg_primary: "#F5F0E8".into(),
        bg_secondary: "#EDE7DD".into(),
        bg_surface: "#E4DDD2".into(),
        bg_hover: "#DDD5C9".into(),
        bg_toolbar: "#EDE7DD".into(),
        text_primary: "#2B1D11".into(),
        text_secondary: "#6B5D4F".into(),
        text_muted: "#9A8E82".into(),
        accent: "#C04A22".into(),
        accent_dim: "#A03D1C".into(),
        accent_subtle: "rgba(192, 74, 34, 0.08)".into(),
        border: "#D8D0C4".into(),
        border_light: "#C8BFB2".into(),
        success: "#4A8A4A".into(),
        error: "#C04040".into(),
        group_bg: "#E8E1D6".into(),
        selected_bg: "#DDD5C9".into(),
        value_color: "#A03D1C".into(),
        input_bg: "#FFFFFF".into(),
        canvas_grid: "rgba(0, 0, 0, 0.04)".into(),
        scrollbar: "#C8BFB2".into(),
        scrollbar_hover: "#9A8E82".into(),
    }
}

// ----------------------------------------------------------------
// Accent palette derivation
// ----------------------------------------------------------------

/// Parse a hex color string like "#D4714E" into (r, g, b).
fn parse_hex(hex: &str) -> Option<(u8, u8, u8)> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some((r, g, b))
}

/// Darken a color by a factor (0.0 = black, 1.0 = unchanged).
fn darken(r: u8, g: u8, b: u8, factor: f64) -> (u8, u8, u8) {
    (
        (r as f64 * factor) as u8,
        (g as f64 * factor) as u8,
        (b as f64 * factor) as u8,
    )
}

/// Lighten a color toward white by a factor (0.0 = unchanged, 1.0 = white).
fn lighten(r: u8, g: u8, b: u8, factor: f64) -> (u8, u8, u8) {
    (
        (r as f64 + (255.0 - r as f64) * factor) as u8,
        (g as f64 + (255.0 - g as f64) * factor) as u8,
        (b as f64 + (255.0 - b as f64) * factor) as u8,
    )
}

fn to_hex(r: u8, g: u8, b: u8) -> String {
    format!("#{:02X}{:02X}{:02X}", r, g, b)
}

/// From a single accent hex color, derive accent_dim, accent_subtle, and value_color
/// for both dark and light modes.
pub fn derive_accent_palette(accent_hex: &str) -> Option<(ThemeColors, ThemeColors)> {
    let (r, g, b) = parse_hex(accent_hex)?;

    let (dr, dg, db) = darken(r, g, b, 0.75);
    let (lr, lg, lb) = lighten(r, g, b, 0.45);

    let mut dark = default_dark();
    dark.accent = accent_hex.to_string();
    dark.accent_dim = to_hex(dr, dg, db);
    dark.accent_subtle = format!("rgba({r}, {g}, {b}, 0.12)");
    dark.value_color = to_hex(lr, lg, lb);

    let (dim_r, dim_g, dim_b) = darken(r, g, b, 0.85);
    let mut light = default_light();
    light.accent = accent_hex.to_string();
    light.accent_dim = to_hex(dim_r, dim_g, dim_b);
    light.accent_subtle = format!("rgba({r}, {g}, {b}, 0.08)");
    light.value_color = to_hex(dim_r, dim_g, dim_b);

    Some((dark, light))
}

// ----------------------------------------------------------------
// Built-in presets
// ----------------------------------------------------------------

pub fn builtin_presets() -> Vec<ThemeDefinition> {
    vec![
        // ── OpenCrate Terracotta (warm brown) ──
        ThemeDefinition {
            id: "default".into(),
            name: "OpenCrate Terracotta".into(),
            is_preset: true,
            dark: default_dark(),
            light: default_light(),
        },
        // ── Slate Blue (cool blue-gray) ──
        ThemeDefinition {
            id: "slate-blue".into(),
            name: "Slate Blue".into(),
            is_preset: true,
            dark: ThemeColors {
                bg_primary: "#1E2330".into(),
                bg_secondary: "#252B3A".into(),
                bg_surface: "#2D3444".into(),
                bg_hover: "#363E50".into(),
                bg_toolbar: "#181C28".into(),
                text_primary: "#E0E4ED".into(),
                text_secondary: "#8B92A5".into(),
                text_muted: "#5C6378".into(),
                accent: "#5B8DEF".into(),
                accent_dim: "#446AB3".into(),
                accent_subtle: "rgba(91, 141, 239, 0.12)".into(),
                border: "#2D3444".into(),
                border_light: "#363E50".into(),
                success: "#7DB87D".into(),
                error: "#D4564E".into(),
                group_bg: "#222838".into(),
                selected_bg: "#2D3444".into(),
                value_color: "#8DB4F5".into(),
                input_bg: "#181C28".into(),
                canvas_grid: "rgba(255, 255, 255, 0.03)".into(),
                scrollbar: "#363E50".into(),
                scrollbar_hover: "#5C6378".into(),
            },
            light: ThemeColors {
                bg_primary: "#F0F2F8".into(),
                bg_secondary: "#E6E9F2".into(),
                bg_surface: "#DCDFE9".into(),
                bg_hover: "#D2D6E2".into(),
                bg_toolbar: "#E6E9F2".into(),
                text_primary: "#1A1F2E".into(),
                text_secondary: "#5A6178".into(),
                text_muted: "#8B92A5".into(),
                accent: "#4A7ADB".into(),
                accent_dim: "#3B62B0".into(),
                accent_subtle: "rgba(74, 122, 219, 0.08)".into(),
                border: "#CDD1DC".into(),
                border_light: "#B8BDC8".into(),
                success: "#4A8A4A".into(),
                error: "#C04040".into(),
                group_bg: "#E0E3EC".into(),
                selected_bg: "#D2D6E2".into(),
                value_color: "#3B62B0".into(),
                input_bg: "#FFFFFF".into(),
                canvas_grid: "rgba(0, 0, 0, 0.04)".into(),
                scrollbar: "#B8BDC8".into(),
                scrollbar_hover: "#8B92A5".into(),
            },
        },
        // ── Forest Green (earthy green-gray) ──
        ThemeDefinition {
            id: "forest-green".into(),
            name: "Forest Green".into(),
            is_preset: true,
            dark: ThemeColors {
                bg_primary: "#1E2620".into(),
                bg_secondary: "#252E27".into(),
                bg_surface: "#2D372F".into(),
                bg_hover: "#36413A".into(),
                bg_toolbar: "#181F1A".into(),
                text_primary: "#DEE6DF".into(),
                text_secondary: "#8B9A8E".into(),
                text_muted: "#5F6E62".into(),
                accent: "#4CAF50".into(),
                accent_dim: "#39833C".into(),
                accent_subtle: "rgba(76, 175, 80, 0.12)".into(),
                border: "#2D372F".into(),
                border_light: "#36413A".into(),
                success: "#7DB87D".into(),
                error: "#D4564E".into(),
                group_bg: "#222A24".into(),
                selected_bg: "#2D372F".into(),
                value_color: "#82CC85".into(),
                input_bg: "#181F1A".into(),
                canvas_grid: "rgba(255, 255, 255, 0.03)".into(),
                scrollbar: "#36413A".into(),
                scrollbar_hover: "#5F6E62".into(),
            },
            light: ThemeColors {
                bg_primary: "#F0F5F0".into(),
                bg_secondary: "#E4ECE5".into(),
                bg_surface: "#D8E2D9".into(),
                bg_hover: "#CDD8CE".into(),
                bg_toolbar: "#E4ECE5".into(),
                text_primary: "#1A251B".into(),
                text_secondary: "#4E614F".into(),
                text_muted: "#7E917F".into(),
                accent: "#3D9141".into(),
                accent_dim: "#2F7032".into(),
                accent_subtle: "rgba(61, 145, 65, 0.08)".into(),
                border: "#C4D0C5".into(),
                border_light: "#ACBBAD".into(),
                success: "#4A8A4A".into(),
                error: "#C04040".into(),
                group_bg: "#DBE5DC".into(),
                selected_bg: "#CDD8CE".into(),
                value_color: "#2F7032".into(),
                input_bg: "#FFFFFF".into(),
                canvas_grid: "rgba(0, 0, 0, 0.04)".into(),
                scrollbar: "#ACBBAD".into(),
                scrollbar_hover: "#7E917F".into(),
            },
        },
        // ── Steel (neutral cool gray) ──
        ThemeDefinition {
            id: "steel".into(),
            name: "Steel".into(),
            is_preset: true,
            dark: ThemeColors {
                bg_primary: "#222426".into(),
                bg_secondary: "#2A2C2F".into(),
                bg_surface: "#323538".into(),
                bg_hover: "#3C3F43".into(),
                bg_toolbar: "#1B1D1F".into(),
                text_primary: "#E2E4E6".into(),
                text_secondary: "#8E9298".into(),
                text_muted: "#60656C".into(),
                accent: "#78909C".into(),
                accent_dim: "#5A6C75".into(),
                accent_subtle: "rgba(120, 144, 156, 0.12)".into(),
                border: "#323538".into(),
                border_light: "#3C3F43".into(),
                success: "#7DB87D".into(),
                error: "#D4564E".into(),
                group_bg: "#262829".into(),
                selected_bg: "#323538".into(),
                value_color: "#A0B8C4".into(),
                input_bg: "#1B1D1F".into(),
                canvas_grid: "rgba(255, 255, 255, 0.03)".into(),
                scrollbar: "#3C3F43".into(),
                scrollbar_hover: "#60656C".into(),
            },
            light: ThemeColors {
                bg_primary: "#F2F3F4".into(),
                bg_secondary: "#E8EAEB".into(),
                bg_surface: "#DFE1E3".into(),
                bg_hover: "#D4D7D9".into(),
                bg_toolbar: "#E8EAEB".into(),
                text_primary: "#1C1E20".into(),
                text_secondary: "#5A5E64".into(),
                text_muted: "#8E9298".into(),
                accent: "#607D8B".into(),
                accent_dim: "#4A626E".into(),
                accent_subtle: "rgba(96, 125, 139, 0.08)".into(),
                border: "#CFD2D4".into(),
                border_light: "#BCC0C3".into(),
                success: "#4A8A4A".into(),
                error: "#C04040".into(),
                group_bg: "#E2E4E5".into(),
                selected_bg: "#D4D7D9".into(),
                value_color: "#4A626E".into(),
                input_bg: "#FFFFFF".into(),
                canvas_grid: "rgba(0, 0, 0, 0.04)".into(),
                scrollbar: "#BCC0C3".into(),
                scrollbar_hover: "#8E9298".into(),
            },
        },
        // ── Teal (blue-green cool) ──
        ThemeDefinition {
            id: "teal".into(),
            name: "Teal".into(),
            is_preset: true,
            dark: ThemeColors {
                bg_primary: "#1C2626".into(),
                bg_secondary: "#232E2E".into(),
                bg_surface: "#2A3737".into(),
                bg_hover: "#334141".into(),
                bg_toolbar: "#162020".into(),
                text_primary: "#DEE8E8".into(),
                text_secondary: "#889C9C".into(),
                text_muted: "#5C7070".into(),
                accent: "#26A69A".into(),
                accent_dim: "#1C7D74".into(),
                accent_subtle: "rgba(38, 166, 154, 0.12)".into(),
                border: "#2A3737".into(),
                border_light: "#334141".into(),
                success: "#7DB87D".into(),
                error: "#D4564E".into(),
                group_bg: "#202B2B".into(),
                selected_bg: "#2A3737".into(),
                value_color: "#60C8BE".into(),
                input_bg: "#162020".into(),
                canvas_grid: "rgba(255, 255, 255, 0.03)".into(),
                scrollbar: "#334141".into(),
                scrollbar_hover: "#5C7070".into(),
            },
            light: ThemeColors {
                bg_primary: "#EEF6F5".into(),
                bg_secondary: "#E2EEEC".into(),
                bg_surface: "#D6E5E3".into(),
                bg_hover: "#CBDCD9".into(),
                bg_toolbar: "#E2EEEC".into(),
                text_primary: "#162220".into(),
                text_secondary: "#446560".into(),
                text_muted: "#78948F".into(),
                accent: "#1F8C82".into(),
                accent_dim: "#186B63".into(),
                accent_subtle: "rgba(31, 140, 130, 0.08)".into(),
                border: "#C2D3D0".into(),
                border_light: "#AAC0BC".into(),
                success: "#4A8A4A".into(),
                error: "#C04040".into(),
                group_bg: "#D9E8E5".into(),
                selected_bg: "#CBDCD9".into(),
                value_color: "#186B63".into(),
                input_bg: "#FFFFFF".into(),
                canvas_grid: "rgba(0, 0, 0, 0.04)".into(),
                scrollbar: "#AAC0BC".into(),
                scrollbar_hover: "#78948F".into(),
            },
        },
        // ── Midnight (deep purple-black) ──
        ThemeDefinition {
            id: "midnight".into(),
            name: "Midnight".into(),
            is_preset: true,
            dark: ThemeColors {
                bg_primary: "#1A1826".into(),
                bg_secondary: "#221F30".into(),
                bg_surface: "#2A273A".into(),
                bg_hover: "#343045".into(),
                bg_toolbar: "#14121E".into(),
                text_primary: "#E4E0F0".into(),
                text_secondary: "#908CA5".into(),
                text_muted: "#615D78".into(),
                accent: "#7C4DFF".into(),
                accent_dim: "#5D3ABF".into(),
                accent_subtle: "rgba(124, 77, 255, 0.12)".into(),
                border: "#2A273A".into(),
                border_light: "#343045".into(),
                success: "#7DB87D".into(),
                error: "#D4564E".into(),
                group_bg: "#1E1C2C".into(),
                selected_bg: "#2A273A".into(),
                value_color: "#A888FF".into(),
                input_bg: "#14121E".into(),
                canvas_grid: "rgba(255, 255, 255, 0.03)".into(),
                scrollbar: "#343045".into(),
                scrollbar_hover: "#615D78".into(),
            },
            light: ThemeColors {
                bg_primary: "#F3F0FA".into(),
                bg_secondary: "#EAE6F4".into(),
                bg_surface: "#E0DBEE".into(),
                bg_hover: "#D5D0E5".into(),
                bg_toolbar: "#EAE6F4".into(),
                text_primary: "#1A162A".into(),
                text_secondary: "#5A5470".into(),
                text_muted: "#8E88A2".into(),
                accent: "#6A3DE0".into(),
                accent_dim: "#5230B0".into(),
                accent_subtle: "rgba(106, 61, 224, 0.08)".into(),
                border: "#CFC9DE".into(),
                border_light: "#B8B2C8".into(),
                success: "#4A8A4A".into(),
                error: "#C04040".into(),
                group_bg: "#E3DEF0".into(),
                selected_bg: "#D5D0E5".into(),
                value_color: "#5230B0".into(),
                input_bg: "#FFFFFF".into(),
                canvas_grid: "rgba(0, 0, 0, 0.04)".into(),
                scrollbar: "#B8B2C8".into(),
                scrollbar_hover: "#8E88A2".into(),
            },
        },
    ]
}

// ----------------------------------------------------------------
// Resolve active theme
// ----------------------------------------------------------------

/// Find the active ThemeDefinition by ID (from presets or custom themes).
pub fn resolve_active_theme(config: &ThemeConfig) -> ThemeDefinition {
    let presets = builtin_presets();
    for p in &presets {
        if p.id == config.active_theme_id {
            return p.clone();
        }
    }
    for t in &config.custom_themes {
        if t.id == config.active_theme_id {
            return t.clone();
        }
    }
    // Fallback to default preset
    presets.into_iter().next().unwrap()
}

// ----------------------------------------------------------------
// CSS injection via document::eval
// ----------------------------------------------------------------

/// Build and inject a `<style>` tag that sets all CSS variables on `:root`.
/// For `BaseMode::System`, the override is removed so the media query works.
pub fn apply_theme_css(config: &ThemeConfig) {
    if config.base_mode == BaseMode::System && config.active_theme_id == "default" {
        // Remove override, let the @media query handle it
        let js = r#"
            (function() {
                var el = document.getElementById('opencrate-theme-override');
                if (el) el.remove();
            })();
        "#;
        dioxus::document::eval(js);
        return;
    }

    let theme = resolve_active_theme(config);

    let colors = match config.base_mode {
        BaseMode::Dark => &theme.dark,
        BaseMode::Light => &theme.light,
        BaseMode::System => {
            // For system mode with a non-default theme, we need both media queries
            let dark_css = build_vars_css(&theme.dark);
            let light_css = build_vars_css(&theme.light);
            let css = format!(
                "@media (prefers-color-scheme: dark) {{ :root {{ {dark_css} }} }} \
                 @media (prefers-color-scheme: light) {{ :root {{ {light_css} }} }}"
            );
            inject_style_tag(&css);
            return;
        }
    };

    let css = format!(":root {{ {} }}", build_vars_css(colors));
    inject_style_tag(&css);
}

fn build_vars_css(colors: &ThemeColors) -> String {
    colors
        .as_pairs()
        .iter()
        .map(|(name, val)| format!("--{name}: {val} !important;"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn inject_style_tag(css: &str) {
    // Escape for JS string literal
    let escaped = css
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "\\n");
    let js = format!(
        r#"(function() {{
            var el = document.getElementById('opencrate-theme-override');
            if (!el) {{
                el = document.createElement('style');
                el.id = 'opencrate-theme-override';
                document.head.appendChild(el);
            }}
            el.textContent = '{escaped}';
        }})();"#
    );
    dioxus::document::eval(&js);
}

// ----------------------------------------------------------------
// Persistence
// ----------------------------------------------------------------

const THEME_CONFIG_FILE: &str = "theme.json";

pub fn save_theme_config(paths: &ProjectPaths, config: &ThemeConfig) {
    let path = paths.data_dir.join(THEME_CONFIG_FILE);
    if let Ok(json) = serde_json::to_string_pretty(config) {
        let _ = std::fs::write(path, json);
    }
}

pub fn load_theme_config(paths: &ProjectPaths) -> ThemeConfig {
    let path = paths.data_dir.join(THEME_CONFIG_FILE);
    std::fs::read_to_string(path)
        .ok()
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}
