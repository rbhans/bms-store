use dioxus::prelude::*;

use crate::gui::state::{
    snap, ActiveView, AppState, DashboardTool, DashboardWidget, DragOp, TrendRange, WidgetKind,
    WidgetSource, GRID_SNAP,
};
use bms_store_storage::store::history_store::{HistoryQuery, HistoryResult};

use crate::config::loader::LoadedDevice;

const COLORS: &[&str] = &[
    "#D4714E", "#7DB87D", "#5B9BD5", "#E8A87C", "#9B59B6", "#E67E22", "#2ECC71", "#3498DB",
];

/// Check if a device matches a search query for history browsers.
/// Matches device ID, profile name, and all point fields.
fn device_matches_history(dev: &LoadedDevice, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let q = query.to_lowercase();

    if dev.instance_id.to_lowercase().contains(&q) {
        return true;
    }
    if dev.profile.profile.name.to_lowercase().contains(&q) {
        return true;
    }
    if let Some(ref desc) = dev.profile.profile.description {
        if desc.to_lowercase().contains(&q) {
            return true;
        }
    }

    for pt in &dev.profile.points {
        if pt.history_exclude {
            continue;
        }
        if pt.id.to_lowercase().contains(&q) || pt.name.to_lowercase().contains(&q) {
            return true;
        }
        if let Some(ref units) = pt.units {
            if units.to_lowercase().contains(&q) {
                return true;
            }
        }
        if let Some(ref desc) = pt.description {
            if desc.to_lowercase().contains(&q) {
                return true;
            }
        }
    }
    false
}

/// Reusable search input for device browsers.
#[component]
fn BrowserSearch(search: Signal<String>) -> Element {
    let query = search.read().clone();
    rsx! {
        div { class: "sidebar-search",
            input {
                class: "sidebar-search-input",
                r#type: "text",
                placeholder: "Search devices, points...",
                value: "{query}",
                oninput: move |evt| search.set(evt.value()),
            }
            if !query.is_empty() {
                button {
                    class: "sidebar-search-clear",
                    onclick: move |_| search.set(String::new()),
                    "x"
                }
            }
        }
    }
}

// ================================================================
// Top-level TrendView — decides between default page and dashboard
// ================================================================

#[component]
pub fn TrendView() -> Element {
    let state = use_context::<AppState>();
    let active_id = state.active_dashboard_id.read().clone();

    if active_id.is_none() {
        // Default page: device/point browser with inline chart
        rsx! { DefaultHistoryPage {} }
    } else {
        // Dashboard editor with drag/drop widgets
        rsx! {
            DashboardDeviceBrowser {}
            div { class: "main-content",
                DashboardToolbar {}
                DashboardCanvas {}
            }
            DashboardProperties {}
        }
    }
}

// ================================================================
// Default History Page — equipment list + click-to-chart
// ================================================================

#[component]
fn DefaultHistoryPage() -> Element {
    let state = use_context::<AppState>();
    let qt_device = state.quick_trend_device.read().clone();
    let qt_point = state.quick_trend_point.read().clone();
    let search = use_signal(String::new);
    let query = search.read().clone();

    rsx! {
        div { class: "sidebar dash-device-browser",
            div { class: "details-header",
                span { "Equipment" }
            }
            BrowserSearch { search: search }
            div { class: "sidebar-content",
                {
                    let filtered: Vec<_> = state.loaded.devices.iter()
                        .filter(|d| device_matches_history(d, &query))
                        .collect();
                    if filtered.is_empty() && !query.is_empty() {
                        rsx! { div { class: "tree-empty-search", "No matches" } }
                    } else {
                        rsx! {
                            for dev in filtered {
                                DefaultDeviceNode {
                                    device_id: dev.instance_id.clone(),
                                    selected_point_key: qt_device.as_ref().zip(qt_point.as_ref()).map(|(d, p)| format!("{d}/{p}")),
                                    filter: query.clone(),
                                }
                            }
                        }
                    }
                }
            }
        }

        div { class: "main-content",
            if qt_device.is_some() && qt_point.is_some() {
                QuickTrendChart {}
            } else {
                div { class: "view-placeholder",
                    h2 { "History" }
                    p { "Select a point from a device to view its trend." }
                    p { class: "placeholder", "Or create a custom dashboard with the + button in the toolbar." }
                }
            }
        }
    }
}

#[component]
fn DefaultDeviceNode(
    device_id: String,
    selected_point_key: Option<String>,
    filter: String,
) -> Element {
    let has_filter = !filter.is_empty();
    let mut expanded = use_signal(|| true);
    // Auto-expand when searching
    let is_open = *expanded.read() || has_filter;

    let state = use_context::<AppState>();
    let device = state
        .loaded
        .devices
        .iter()
        .find(|d| d.instance_id == device_id);

    let Some(dev) = device else { return rsx! {} };

    let profile_name = dev.profile.profile.name.clone();
    let q = filter.to_lowercase();
    let visible_points: Vec<(String, String)> = dev
        .profile
        .points
        .iter()
        .filter(|p| !p.history_exclude)
        .filter(|p| {
            !has_filter
                || p.id.to_lowercase().contains(&q)
                || p.name.to_lowercase().contains(&q)
                || p.units.as_deref().unwrap_or("").to_lowercase().contains(&q)
                || p.description
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&q)
        })
        .map(|p| (p.id.clone(), p.name.clone()))
        .collect();

    rsx! {
        div { class: "dash-device-node",
            div {
                class: "tree-node-row",
                onclick: move |_| expanded.set(!is_open),
                span { class: if is_open { "tree-arrow open" } else { "tree-arrow" }, ">" }
                span { class: "tree-label", "{device_id}" }
                span { class: "tree-badge", "{profile_name}" }
            }
            if is_open {
                div { class: "dash-point-list",
                    for (pt_id, pt_name) in &visible_points {
                        {
                            let key = format!("{device_id}/{pt_id}");
                            let is_sel = selected_point_key.as_deref() == Some(&key);
                            rsx! {
                                DefaultPointItem {
                                    device_id: device_id.clone(),
                                    point_id: pt_id.clone(),
                                    point_name: pt_name.clone(),
                                    is_selected: is_sel,
                                }
                            }
                        }
                    }
                    if visible_points.is_empty() {
                        div { class: "dash-point-item muted", "No points" }
                    }
                }
            }
        }
    }
}

#[component]
fn DefaultPointItem(
    device_id: String,
    point_id: String,
    point_name: String,
    is_selected: bool,
) -> Element {
    let mut state = use_context::<AppState>();

    rsx! {
        div {
            class: if is_selected { "dash-point-item selected" } else { "dash-point-item" },
            onclick: move |_| {
                state.quick_trend_device.set(Some(device_id.clone()));
                state.quick_trend_point.set(Some(point_id.clone()));
            },
            span { class: "dash-point-name", "{point_name}" }
            span { class: "dash-point-id", "{point_id}" }
        }
    }
}

#[component]
fn QuickTrendChart() -> Element {
    let state = use_context::<AppState>();
    let device_id = state.quick_trend_device.read().clone().unwrap_or_default();
    let point_id = state.quick_trend_point.read().clone().unwrap_or_default();
    let range = *state.quick_trend_range.read();

    // Get point name
    let point_name = state
        .loaded
        .devices
        .iter()
        .find(|d| d.instance_id == device_id)
        .and_then(|d| d.profile.points.iter().find(|p| p.id == point_id))
        .map(|p| p.name.clone())
        .unwrap_or_else(|| point_id.clone());

    let units = state
        .loaded
        .devices
        .iter()
        .find(|d| d.instance_id == device_id)
        .and_then(|d| d.profile.points.iter().find(|p| p.id == point_id))
        .and_then(|p| p.units.clone())
        .unwrap_or_default();

    let hs = state.history_store.clone();
    let dev = device_id.clone();
    let pt = point_id.clone();

    let history_data = use_resource(move || {
        let hs = hs.clone();
        let dev = dev.clone();
        let pt = pt.clone();
        async move {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64;
            let start = now - range.millis();
            hs.query(HistoryQuery {
                device_id: dev,
                point_id: pt,
                start_ms: start,
                end_ms: now,
                max_results: None,
            })
            .await
            .ok()
        }
    });

    rsx! {
        div { class: "quick-trend",
            div { class: "quick-trend-header",
                div { class: "quick-trend-title",
                    h3 { "{point_name}" }
                    span { class: "quick-trend-subtitle", "{device_id} — {point_id}" }
                    if !units.is_empty() {
                        span { class: "quick-trend-units", "({units})" }
                    }
                }
                div { class: "trend-range-bar",
                    for r in TrendRange::all() {
                        QuickRangeButton { range: *r, is_active: *r == range }
                    }
                }
                QuickExportButton { device_id: device_id.clone(), point_id: point_id.clone(), range: range }
            }

            div { class: "quick-trend-chart",
                match &*history_data.read() {
                    Some(Some(result)) if !result.samples.is_empty() => {
                        let src = WidgetSource {
                            device_id: device_id.clone(),
                            point_id: point_id.clone(),
                            label: point_name.clone(),
                            color: COLORS[0].to_string(),
                        };
                        rsx! { FullChart { results: vec![(src, result.clone())], range: range } }
                    },
                    Some(Some(_)) => rsx! {
                        div { class: "trend-empty-inline",
                            "No data yet for this time range. Data is collected over time."
                        }
                    },
                    Some(None) => rsx! {
                        div { class: "trend-empty-inline", "Error loading data." }
                    },
                    None => rsx! {
                        div { class: "trend-empty-inline", "Loading..." }
                    },
                }
            }
        }
    }
}

#[component]
fn QuickRangeButton(range: TrendRange, is_active: bool) -> Element {
    let mut state = use_context::<AppState>();
    rsx! {
        button {
            class: if is_active { "trend-range-btn active" } else { "trend-range-btn" },
            onclick: move |_| state.quick_trend_range.set(range),
            "{range.label()}"
        }
    }
}

// ================================================================
// Full-size chart (reused by both quick trend and chart widgets)
// ================================================================

#[component]
fn FullChart(results: Vec<(WidgetSource, HistoryResult)>, range: TrendRange) -> Element {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let t_start = now - range.millis();
    let t_span = range.millis() as f64;

    let mut y_min = f64::INFINITY;
    let mut y_max = f64::NEG_INFINITY;
    for (_, r) in &results {
        for s in &r.samples {
            if s.value < y_min {
                y_min = s.value;
            }
            if s.value > y_max {
                y_max = s.value;
            }
        }
    }
    if y_min == f64::INFINITY {
        y_min = 0.0;
        y_max = 100.0;
    }
    let y_range = if (y_max - y_min).abs() < 0.001 {
        1.0
    } else {
        y_max - y_min
    };
    let pad = y_range * 0.1;
    y_min -= pad;
    y_max += pad;
    let y_span = y_max - y_min;

    let w = 800.0_f64;
    let h = 400.0_f64;
    let ml = 55.0_f64;
    let mr = 20.0_f64;
    let mt = 15.0_f64;
    let mb = 35.0_f64;
    let pw = w - ml - mr;
    let ph = h - mt - mb;

    let series: Vec<(String, String, String)> = results
        .iter()
        .map(|(src, r)| {
            let pts: String = r
                .samples
                .iter()
                .map(|s| {
                    let x = ml + ((s.timestamp_ms - t_start) as f64 / t_span) * pw;
                    let y = mt + (1.0 - (s.value - y_min) / y_span) * ph;
                    format!("{x:.1},{y:.1}")
                })
                .collect::<Vec<_>>()
                .join(" ");
            (src.label.clone(), src.color.clone(), pts)
        })
        .collect();

    let y_ticks: Vec<(f64, String)> = (0..=4)
        .map(|i| {
            let frac = i as f64 / 4.0;
            let val = y_min + frac * y_span;
            let y = mt + (1.0 - frac) * ph;
            (y, format!("{val:.1}"))
        })
        .collect();

    let x_ticks: Vec<(f64, String)> = (0..=4)
        .map(|i| {
            let frac = i as f64 / 4.0;
            let x = ml + frac * pw;
            let ts_ms = t_start + (frac * t_span) as i64;
            (x, format_time_label(ts_ms, range))
        })
        .collect();

    rsx! {
        div { class: "full-chart-wrap",
            svg {
                class: "full-chart-svg",
                view_box: "0 0 {w} {h}",

                rect {
                    x: "{ml}", y: "{mt}", width: "{pw}", height: "{ph}",
                    fill: "var(--bg-surface)", rx: "2",
                }

                for (y, label) in &y_ticks {
                    line {
                        x1: "{ml}", y1: "{y}", x2: "{ml + pw}", y2: "{y}",
                        stroke: "var(--border)", stroke_width: "0.5",
                    }
                    text {
                        x: "{ml - 6.0}", y: "{y + 3.5}",
                        text_anchor: "end", fill: "var(--text-secondary)", font_size: "11",
                        "{label}"
                    }
                }

                for (x, label) in &x_ticks {
                    text {
                        x: "{x}", y: "{mt + ph + 20.0}",
                        text_anchor: "middle", fill: "var(--text-secondary)", font_size: "11",
                        "{label}"
                    }
                }

                for (_, color, pts) in &series {
                    if !pts.is_empty() {
                        polyline {
                            points: "{pts}",
                            fill: "none", stroke: "{color}", stroke_width: "2",
                            stroke_linejoin: "round",
                        }
                    }
                }
            }

            div { class: "widget-legend",
                for (label, color, _) in &series {
                    div { class: "trend-legend-item",
                        span { class: "trend-swatch", style: "background: {color}" }
                        span { "{label}" }
                    }
                }
            }
        }
    }
}

fn format_time_label(ts_ms: i64, range: TrendRange) -> String {
    let secs = ts_ms / 1000;
    let hours = (secs % 86400) / 3600;
    let minutes = (secs % 3600) / 60;
    match range {
        TrendRange::Hour1 | TrendRange::Hour4 | TrendRange::Hour24 => {
            format!("{hours:02}:{minutes:02}")
        }
        TrendRange::Day7 | TrendRange::Day30 => {
            let days = secs / 86400;
            let day_in_year = days % 365;
            let month = (day_in_year / 30) + 1;
            let day = (day_in_year % 30) + 1;
            format!("{month:02}/{day:02}")
        }
    }
}

// ================================================================
// Dashboard device browser (left pane)
// ================================================================

#[component]
fn DashboardDeviceBrowser() -> Element {
    let state = use_context::<AppState>();
    let search = use_signal(String::new);
    let query = search.read().clone();

    rsx! {
        div { class: "sidebar dash-device-browser",
            div { class: "details-header", span { "Devices" } }
            BrowserSearch { search: search }
            div { class: "sidebar-content",
                {
                    let filtered: Vec<_> = state.loaded.devices.iter()
                        .filter(|d| device_matches_history(d, &query))
                        .collect();
                    if filtered.is_empty() && !query.is_empty() {
                        rsx! { div { class: "tree-empty-search", "No matches" } }
                    } else {
                        rsx! {
                            for dev in filtered {
                                DashDeviceNode { device_id: dev.instance_id.clone(), filter: query.clone() }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn DashDeviceNode(device_id: String, filter: String) -> Element {
    let has_filter = !filter.is_empty();
    let state = use_context::<AppState>();
    let mut expanded = use_signal(|| false);
    // Auto-expand when searching
    let is_open = *expanded.read() || has_filter;

    let device = state
        .loaded
        .devices
        .iter()
        .find(|d| d.instance_id == device_id);
    let Some(dev) = device else { return rsx! {} };

    let profile_name = dev.profile.profile.name.clone();
    let q = filter.to_lowercase();
    let visible_points: Vec<(String, String)> = dev
        .profile
        .points
        .iter()
        .filter(|p| !p.history_exclude)
        .filter(|p| {
            !has_filter
                || p.id.to_lowercase().contains(&q)
                || p.name.to_lowercase().contains(&q)
                || p.units.as_deref().unwrap_or("").to_lowercase().contains(&q)
                || p.description
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&q)
        })
        .map(|p| (p.id.clone(), p.name.clone()))
        .collect();

    rsx! {
        div { class: "dash-device-node",
            div {
                class: "tree-node-row",
                onclick: move |_| expanded.set(!is_open),
                span { class: if is_open { "tree-arrow open" } else { "tree-arrow" }, ">" }
                span { class: "tree-label", "{device_id}" }
                span { class: "tree-badge", "{profile_name}" }
            }
            if is_open {
                div { class: "dash-point-list",
                    for (pt_id, pt_name) in &visible_points {
                        DashPointItem { device_id: device_id.clone(), point_id: pt_id.clone(), point_name: pt_name.clone() }
                    }
                    if visible_points.is_empty() {
                        div { class: "dash-point-item muted", "No points" }
                    }
                }
            }
        }
    }
}

#[component]
fn DashPointItem(device_id: String, point_id: String, point_name: String) -> Element {
    let mut state = use_context::<AppState>();
    rsx! {
        div {
            class: "dash-point-item",
            title: "Click to add to selected widget",
            onclick: move |_| {
                add_source_to_selected_widget(&mut state, &device_id, &point_id, &point_name);
            },
            span { class: "dash-point-name", "{point_name}" }
            span { class: "dash-point-id", "{point_id}" }
        }
    }
}

fn add_source_to_selected_widget(
    state: &mut AppState,
    device_id: &str,
    point_id: &str,
    label: &str,
) {
    let dash_id = match state.active_dashboard_id.read().clone() {
        Some(id) => id,
        None => return,
    };
    let widget_id = match state.selected_widget.read().clone() {
        Some(id) => id,
        None => return,
    };
    let mut dashboards = state.dashboards.read().clone();
    if let Some(dash) = dashboards.iter_mut().find(|d| d.id == dash_id) {
        if let Some(widget) = dash.widgets.iter_mut().find(|w| w.id == widget_id) {
            if !widget
                .sources
                .iter()
                .any(|s| s.device_id == device_id && s.point_id == point_id)
            {
                let ci = widget.sources.len();
                widget.sources.push(WidgetSource {
                    device_id: device_id.to_string(),
                    point_id: point_id.to_string(),
                    label: label.to_string(),
                    color: COLORS[ci % COLORS.len()].to_string(),
                });
            }
        }
    }
    state.dashboards.set(dashboards);
}

// ================================================================
// Dashboard toolbar
// ================================================================

#[component]
fn DashboardToolbar() -> Element {
    let mut state = use_context::<AppState>();
    let current_tool = *state.dashboard_tool.read();
    let dashboards = state.dashboards.read().clone();
    let active_id = state.active_dashboard_id.read().clone();
    let dash_name = active_id
        .as_ref()
        .and_then(|id| dashboards.iter().find(|d| d.id == *id))
        .map(|d| d.name.clone())
        .unwrap_or_default();

    rsx! {
        div { class: "canvas-toolbar",
            button {
                class: if matches!(current_tool, DashboardTool::Select) { "canvas-tool-btn active" } else { "canvas-tool-btn" },
                title: "Select", onclick: move |_| state.dashboard_tool.set(DashboardTool::Select),
                svg { width: "16", height: "16", view_box: "0 0 24 24", fill: "currentColor",
                    path { d: "M3 3l7.07 16.97 2.51-7.39 7.39-2.51L3 3z" }
                }
            }
            span { class: "canvas-toolbar-divider" }
            for kind in WidgetKind::all() {
                WidgetToolButton { kind: *kind, current_tool: current_tool }
            }
            span { class: "canvas-toolbar-divider" }
            input {
                class: "dash-name-input", value: "{dash_name}",
                onchange: move |evt: Event<FormData>| {
                    let name = evt.value();
                    let aid = state.active_dashboard_id.read().clone();
                    if let Some(id) = aid {
                        let mut dashes = state.dashboards.read().clone();
                        if let Some(d) = dashes.iter_mut().find(|d| d.id == id) { d.name = name; }
                        state.dashboards.set(dashes);
                    }
                },
            }
        }
    }
}

#[component]
fn WidgetToolButton(kind: WidgetKind, current_tool: DashboardTool) -> Element {
    let mut state = use_context::<AppState>();
    let is_active = matches!(current_tool, DashboardTool::AddWidget(wk) if wk == kind);
    rsx! {
        button {
            class: if is_active { "canvas-tool-btn active" } else { "canvas-tool-btn" },
            title: "{kind.label()}", onclick: move |_| state.dashboard_tool.set(DashboardTool::AddWidget(kind)),
            svg { width: "16", height: "16", view_box: "0 0 24 24", fill: "currentColor",
                path { d: "{kind.icon_path()}" }
            }
            span { class: "canvas-tool-hint", "{kind.label()}" }
        }
    }
}

// ================================================================
// Dashboard canvas — absolute positioned widgets with drag support
// ================================================================

#[component]
fn DashboardCanvas() -> Element {
    let mut state = use_context::<AppState>();
    let dashboards = state.dashboards.read().clone();
    let active_id = state.active_dashboard_id.read().clone();
    let selected = state.selected_widget.read().clone();
    let tool = *state.dashboard_tool.read();

    let is_dragging = state.drag_op.read().is_some();

    let dash_id = match active_id {
        Some(ref id) => id.clone(),
        None => return rsx! {},
    };
    let dash = match dashboards.iter().find(|d| d.id == dash_id) {
        Some(d) => d.clone(),
        None => return rsx! {},
    };
    let widgets = dash.widgets.clone();

    rsx! {
        div {
            class: if is_dragging { "dash-canvas-abs dragging" } else { "dash-canvas-abs" },
            // Click to add widget when tool is active
            onclick: move |evt: Event<MouseData>| {
                if let DashboardTool::AddWidget(kind) = tool {
                    let coords = evt.element_coordinates();
                    let mx = coords.x;
                    let my = coords.y;

                    let wid_num = *state.next_widget_id.read();
                    state.next_widget_id.set(wid_num + 1);
                    let widget_id = format!("w-{wid_num}");
                    let (dw, dh) = match kind {
                        WidgetKind::Chart => (400.0, 280.0),
                        WidgetKind::Gauge => (200.0, 200.0),
                        WidgetKind::Table => (400.0, 250.0),
                        WidgetKind::Value => (200.0, 120.0),
                    };
                    let new_widget = DashboardWidget {
                        id: widget_id.clone(), kind,
                        x: snap(mx), y: snap(my), w: snap(dw), h: snap(dh),
                        sources: Vec::new(), range: TrendRange::Hour1,
                    };
                    let mut dashes = state.dashboards.read().clone();
                    if let Some(d) = dashes.iter_mut().find(|d| d.id == dash_id) {
                        d.widgets.push(new_widget);
                    }
                    state.dashboards.set(dashes);
                    state.selected_widget.set(Some(widget_id));
                    state.dashboard_tool.set(DashboardTool::Select);
                }
            },
            // Mouse move for drag — use page coordinates to avoid reference-frame jumps
            onmousemove: {
                let mut move_state = use_context::<AppState>();
                move |evt: Event<MouseData>| {
                    let drag = move_state.drag_op.read().clone();
                    if let Some(op) = drag {
                        let page = evt.page_coordinates();
                        let px = page.x;
                        let py = page.y;
                        match op {
                            DragOp::Move { ref widget_id, start_page_x, start_page_y, orig_x, orig_y } => {
                                let nx = snap((orig_x + px - start_page_x).max(0.0));
                                let ny = snap((orig_y + py - start_page_y).max(0.0));
                                update_widget_field(&mut move_state, widget_id, |w| { w.x = nx; w.y = ny; });
                            }
                            DragOp::Resize { ref widget_id, start_page_x, start_page_y, orig_w, orig_h } => {
                                let nw = snap((orig_w + px - start_page_x).max(GRID_SNAP * 5.0));
                                let nh = snap((orig_h + py - start_page_y).max(GRID_SNAP * 3.0));
                                update_widget_field(&mut move_state, widget_id, |w| { w.w = nw; w.h = nh; });
                            }
                        }
                    }
                }
            },
            onmouseup: {
                let mut up_state = use_context::<AppState>();
                move |_| {
                    up_state.drag_op.set(None);
                }
            },
            onmouseleave: {
                let mut leave_state = use_context::<AppState>();
                move |_| {
                    leave_state.drag_op.set(None);
                }
            },

            for widget in &widgets {
                WidgetCell {
                    widget: widget.clone(),
                    is_selected: selected.as_deref() == Some(&widget.id),
                }
            }

            if widgets.is_empty() {
                div { class: "dash-canvas-empty",
                    p { "Click a widget type above, then click here to place it." }
                }
            }
        }
    }
}

#[component]
fn WidgetCell(widget: DashboardWidget, is_selected: bool) -> Element {
    let mut state = use_context::<AppState>();
    let wid = widget.id.clone();
    let wx = widget.x;
    let wy = widget.y;
    let ww = widget.w;
    let wh = widget.h;
    let style = format!("left: {wx}px; top: {wy}px; width: {ww}px; height: {wh}px;");

    rsx! {
        div {
            class: if is_selected { "dash-widget-abs selected" } else { "dash-widget-abs" },
            style: "{style}",
            // Click to select
            onclick: move |evt| {
                evt.stop_propagation();
                state.selected_widget.set(Some(wid.clone()));
            },
            // Mouse down on widget body to start move
            onmousedown: {
                let wid2 = widget.id.clone();
                move |evt: Event<MouseData>| {
                    evt.stop_propagation();
                    let page = evt.page_coordinates();
                    state.drag_op.set(Some(DragOp::Move {
                        widget_id: wid2.clone(),
                        start_page_x: page.x,
                        start_page_y: page.y,
                        orig_x: wx,
                        orig_y: wy,
                    }));
                    state.selected_widget.set(Some(wid2.clone()));
                }
            },

            WidgetRenderer { widget: widget.clone() }

            // Resize handle (bottom-right corner)
            if is_selected {
                ResizeHandle { widget_id: widget.id.clone(), w: ww, h: wh }
            }
        }
    }
}

#[component]
fn ResizeHandle(widget_id: String, w: f64, h: f64) -> Element {
    let mut state = use_context::<AppState>();
    rsx! {
        div {
            class: "resize-handle",
            onmousedown: move |evt: Event<MouseData>| {
                evt.stop_propagation();
                let page = evt.page_coordinates();
                state.drag_op.set(Some(DragOp::Resize {
                    widget_id: widget_id.clone(),
                    start_page_x: page.x,
                    start_page_y: page.y,
                    orig_w: w,
                    orig_h: h,
                }));
            },
        }
    }
}

// ================================================================
// Widget rendering (shared by dashboard)
// ================================================================

#[component]
fn WidgetRenderer(widget: DashboardWidget) -> Element {
    match widget.kind {
        WidgetKind::Chart => rsx! { ChartWidget { widget: widget } },
        WidgetKind::Gauge => rsx! { GaugeWidget { widget: widget } },
        WidgetKind::Table => rsx! { TableWidget { widget: widget } },
        WidgetKind::Value => rsx! { ValueWidget { widget: widget } },
    }
}

#[component]
fn ChartWidget(widget: DashboardWidget) -> Element {
    let state = use_context::<AppState>();
    if widget.sources.is_empty() {
        return rsx! {
            div { class: "widget-empty",
                span { class: "widget-kind-label", "Chart" }
                p { "Select this widget, then click points in the device browser." }
            }
        };
    }
    let hs = state.history_store.clone();
    let sources = widget.sources.clone();
    let range = widget.range;
    let history_data = use_resource(move || {
        let hs = hs.clone();
        let srcs = sources.clone();
        async move {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64;
            let start = now - range.millis();
            let mut results = Vec::new();
            for src in &srcs {
                if let Ok(r) = hs
                    .query(HistoryQuery {
                        device_id: src.device_id.clone(),
                        point_id: src.point_id.clone(),
                        start_ms: start,
                        end_ms: now,
                        max_results: None,
                    })
                    .await
                {
                    results.push((src.clone(), r));
                }
            }
            results
        }
    });
    let data = history_data.read();
    match &*data {
        Some(results) => rsx! { FullChart { results: results.clone(), range: range } },
        None => rsx! { div { class: "widget-loading", "Loading..." } },
    }
}

#[component]
fn GaugeWidget(widget: DashboardWidget) -> Element {
    let state = use_context::<AppState>();
    let _v = state.store_version.read();
    if widget.sources.is_empty() {
        return rsx! { div { class: "widget-empty", span { class: "widget-kind-label", "Gauge" } p { "Add a point." } } };
    }
    let src = &widget.sources[0];
    let key = bms_store_storage::store::point_store::PointKey {
        device_instance_id: src.device_id.clone(),
        point_id: src.point_id.clone(),
    };
    let val = state
        .store
        .get(&key)
        .map(|v| v.value.as_f64())
        .unwrap_or(0.0);
    let angle = (val.clamp(0.0, 100.0) / 100.0) * 180.0;
    let rad = angle.to_radians();
    let (cx, cy, r) = (100.0_f64, 110.0_f64, 80.0_f64);
    let (ex, ey) = (cx + r * (-rad).cos(), cy - r * rad.sin());
    let large = if angle > 90.0 { 1 } else { 0 };
    let color = src.color.clone();
    let label = src.label.clone();
    rsx! {
        div { class: "widget-gauge-wrap",
            svg { class: "widget-gauge-svg", view_box: "0 0 200 130",
                path { d: "M {cx - r} {cy} A {r} {r} 0 0 1 {cx + r} {cy}", fill: "none", stroke: "var(--border)", stroke_width: "8", stroke_linecap: "round" }
                path { d: "M {cx - r} {cy} A {r} {r} 0 {large} 1 {ex} {ey}", fill: "none", stroke: "{color}", stroke_width: "8", stroke_linecap: "round" }
                text { x: "{cx}", y: "{cy - 10.0}", text_anchor: "middle", fill: "var(--text-primary)", font_size: "24", font_weight: "600", "{val:.1}" }
                text { x: "{cx}", y: "{cy + 10.0}", text_anchor: "middle", fill: "var(--text-secondary)", font_size: "11", "{label}" }
            }
        }
    }
}

#[component]
fn TableWidget(widget: DashboardWidget) -> Element {
    let state = use_context::<AppState>();
    let _v = state.store_version.read();
    if widget.sources.is_empty() {
        return rsx! { div { class: "widget-empty", span { class: "widget-kind-label", "Table" } p { "Add points." } } };
    }
    let rows: Vec<(String, String, String, String)> = widget
        .sources
        .iter()
        .map(|src| {
            let key = bms_store_storage::store::point_store::PointKey {
                device_instance_id: src.device_id.clone(),
                point_id: src.point_id.clone(),
            };
            let val = state
                .store
                .get(&key)
                .map(|v| format!("{:.2}", v.value.as_f64()))
                .unwrap_or_else(|| "—".into());
            (
                src.label.clone(),
                src.device_id.clone(),
                val,
                src.color.clone(),
            )
        })
        .collect();
    rsx! {
        div { class: "widget-table-wrap",
            table { class: "widget-table",
                thead { tr { th { "Point" } th { "Device" } th { "Value" } } }
                tbody {
                    for (label, dev_id, val, color) in &rows {
                        tr { td { style: "color: {color}", "{label}" } td { "{dev_id}" } td { class: "col-value", "{val}" } }
                    }
                }
            }
        }
    }
}

#[component]
fn ValueWidget(widget: DashboardWidget) -> Element {
    let state = use_context::<AppState>();
    let _v = state.store_version.read();
    if widget.sources.is_empty() {
        return rsx! { div { class: "widget-empty", span { class: "widget-kind-label", "Value" } p { "Add a point." } } };
    }
    let cards: Vec<(String, String, String, String)> = widget
        .sources
        .iter()
        .map(|src| {
            let key = bms_store_storage::store::point_store::PointKey {
                device_instance_id: src.device_id.clone(),
                point_id: src.point_id.clone(),
            };
            let val = state
                .store
                .get(&key)
                .map(|v| format!("{:.1}", v.value.as_f64()))
                .unwrap_or_else(|| "—".into());
            (
                val,
                src.label.clone(),
                src.device_id.clone(),
                src.color.clone(),
            )
        })
        .collect();
    rsx! {
        div { class: "widget-value-wrap",
            for (val, label, dev_id, color) in &cards {
                div { class: "widget-value-card",
                    span { class: "widget-value-number", style: "color: {color}", "{val}" }
                    span { class: "widget-value-label", "{label}" }
                    span { class: "widget-value-device", "{dev_id}" }
                }
            }
        }
    }
}

// ================================================================
// Right pane: Widget properties
// ================================================================

#[component]
fn DashboardProperties() -> Element {
    let state = use_context::<AppState>();
    let selected = state.selected_widget.read().clone();
    let active_id = state.active_dashboard_id.read().clone();
    let dashboards = state.dashboards.read().clone();
    let widget = active_id.as_ref().and_then(|did| {
        dashboards.iter().find(|d| d.id == *did).and_then(|d| {
            selected
                .as_ref()
                .and_then(|wid| d.widgets.iter().find(|w| w.id == *wid).cloned())
        })
    });
    rsx! {
        div { class: "details-pane dash-properties",
            div { class: "details-header", span { "Properties" } }
            if let Some(w) = widget {
                WidgetPropertiesPanel { widget: w }
            } else {
                div { class: "point-detail-body", p { class: "placeholder", "Select a widget to edit its properties." } }
            }
        }
    }
}

#[component]
fn WidgetPropertiesPanel(widget: DashboardWidget) -> Element {
    let wid = widget.id.clone();
    rsx! {
        div { class: "point-detail-body",
            h4 { class: "detail-point-name", "{widget.kind.label()}" }
            if matches!(widget.kind, WidgetKind::Chart) {
                RangeSelector { widget_id: wid.clone(), current_range: widget.range }
            }
            SourcesList { widget_id: wid.clone(), sources: widget.sources.clone() }
            WidgetExportButton { sources: widget.sources.clone(), range: widget.range }
            DeleteWidgetButton { widget_id: wid }
        }
    }
}

#[component]
fn RangeSelector(widget_id: String, current_range: TrendRange) -> Element {
    rsx! {
        div { class: "props-section",
            label { "Time Range" }
            div { class: "trend-range-bar",
                for range in TrendRange::all() {
                    RangeButton { widget_id: widget_id.clone(), range: *range, is_active: *range == current_range }
                }
            }
        }
    }
}

#[component]
fn RangeButton(widget_id: String, range: TrendRange, is_active: bool) -> Element {
    let mut state = use_context::<AppState>();
    rsx! {
        button {
            class: if is_active { "trend-range-btn active" } else { "trend-range-btn" },
            onclick: move |_| { update_widget_field(&mut state, &widget_id, |w| w.range = range); },
            "{range.label()}"
        }
    }
}

#[component]
fn SourcesList(widget_id: String, sources: Vec<WidgetSource>) -> Element {
    rsx! {
        div { class: "props-section",
            h4 { class: "props-subhead", "Data Sources" }
            if sources.is_empty() { p { class: "placeholder", "Click points in the device browser to add." } }
            for (i, src) in sources.iter().enumerate() {
                SourceRow { widget_id: widget_id.clone(), index: i, source: src.clone() }
            }
        }
    }
}

#[component]
fn SourceRow(widget_id: String, index: usize, source: WidgetSource) -> Element {
    let mut state = use_context::<AppState>();
    rsx! {
        div { class: "props-source-row",
            span { class: "trend-swatch", style: "background: {source.color}" }
            span { class: "props-source-label", "{source.label}" }
            span { class: "props-source-device", "{source.device_id}" }
            button {
                class: "nav-delete-btn visible",
                onclick: move |_| {
                    update_widget_field(&mut state, &widget_id, |w| { if index < w.sources.len() { w.sources.remove(index); } });
                },
                "x"
            }
        }
    }
}

#[component]
fn DeleteWidgetButton(widget_id: String) -> Element {
    let mut state = use_context::<AppState>();
    rsx! {
        button {
            class: "props-delete-btn",
            onclick: move |_| {
                let did = state.active_dashboard_id.read().clone();
                if let Some(id) = did {
                    let mut dashes = state.dashboards.read().clone();
                    if let Some(d) = dashes.iter_mut().find(|d| d.id == id) { d.widgets.retain(|w| w.id != widget_id); }
                    state.dashboards.set(dashes);
                    state.selected_widget.set(None);
                }
            },
            "Delete Widget"
        }
    }
}

fn update_widget_field(
    state: &mut AppState,
    widget_id: &str,
    f: impl FnOnce(&mut DashboardWidget),
) {
    let did = match state.active_dashboard_id.read().clone() {
        Some(id) => id,
        None => return,
    };
    let mut dashboards = state.dashboards.read().clone();
    if let Some(dash) = dashboards.iter_mut().find(|d| d.id == did) {
        if let Some(widget) = dash.widgets.iter_mut().find(|w| w.id == widget_id) {
            f(widget);
        }
    }
    state.dashboards.set(dashboards);
}

// ================================================================
// CSV Export
// ================================================================

/// Format epoch millis as ISO 8601 UTC string.
fn format_iso8601(ts_ms: i64) -> String {
    let ts = ts_ms / 1000;
    let ms = (ts_ms % 1000).unsigned_abs();
    // Days since epoch
    let mut days = ts / 86400;
    let day_secs = ts % 86400;
    let hours = day_secs / 3600;
    let minutes = (day_secs % 3600) / 60;
    let seconds = day_secs % 60;

    // Year/month/day from days since 1970-01-01
    let mut year: i64 = 1970;
    loop {
        let days_in_year: i64 = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }
    let month_days: &[i64] = if is_leap(year) {
        &[31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        &[31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1;
    for &md in month_days {
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }
    let day = days + 1;
    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}.{ms:03}Z")
}

fn is_leap(y: i64) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}

/// Format a single point's history as CSV.
fn format_csv_single(result: &HistoryResult, label: &str) -> String {
    let mut csv = format!("Timestamp,{label}\n");
    for s in &result.samples {
        csv.push_str(&format!("{},{}\n", format_iso8601(s.timestamp_ms), s.value));
    }
    csv
}

/// Format multiple points' history as CSV with aligned timestamps.
fn format_csv_multi(results: &[(WidgetSource, HistoryResult)]) -> String {
    use std::collections::BTreeMap;

    // Collect all unique timestamps, sorted
    let mut all_ts: Vec<i64> = results
        .iter()
        .flat_map(|(_, r)| r.samples.iter().map(|s| s.timestamp_ms))
        .collect();
    all_ts.sort_unstable();
    all_ts.dedup();

    // Build lookup per series: timestamp → value
    let lookups: Vec<BTreeMap<i64, f64>> = results
        .iter()
        .map(|(_, r)| {
            r.samples
                .iter()
                .map(|s| (s.timestamp_ms, s.value))
                .collect()
        })
        .collect();

    // Header
    let headers: Vec<String> = results
        .iter()
        .map(|(src, r)| format!("{}/{} ({})", r.device_id, r.point_id, src.label))
        .collect();
    let mut csv = format!("Timestamp,{}\n", headers.join(","));

    // Rows
    for ts in &all_ts {
        csv.push_str(&format_iso8601(*ts));
        for lookup in &lookups {
            match lookup.get(ts) {
                Some(v) => csv.push_str(&format!(",{v}")),
                None => csv.push(','),
            }
        }
        csv.push('\n');
    }
    csv
}

/// Show a save dialog and write CSV content to the chosen file.
#[cfg(feature = "desktop")]
fn save_csv_to_file(csv_content: String, default_name: &str) {
    let name = default_name.to_string();
    spawn(async move {
        let path = tokio::task::spawn_blocking(move || {
            rfd::FileDialog::new()
                .add_filter("CSV", &["csv"])
                .set_file_name(&name)
                .save_file()
        })
        .await
        .ok()
        .flatten();

        if let Some(p) = path {
            let _ = tokio::fs::write(p, csv_content).await;
        }
    });
}

/// Export button for the quick trend (single point).
#[component]
fn QuickExportButton(device_id: String, point_id: String, range: TrendRange) -> Element {
    let state = use_context::<AppState>();
    let hs = state.history_store.clone();
    let dev = device_id.clone();
    let pt = point_id.clone();
    let label = state
        .loaded
        .devices
        .iter()
        .find(|d| d.instance_id == device_id)
        .and_then(|d| d.profile.points.iter().find(|p| p.id == point_id))
        .map(|p| p.name.clone())
        .unwrap_or_else(|| point_id.clone());

    rsx! {
        button {
            class: "export-csv-btn",
            title: "Export to CSV",
            onclick: move |_| {
                let hs = hs.clone();
                let dev = dev.clone();
                let pt = pt.clone();
                let label = label.clone();
                spawn(async move {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
                    let start = now - range.millis();
                    if let Ok(result) = hs.query(HistoryQuery {
                        device_id: dev.clone(), point_id: pt.clone(),
                        start_ms: start, end_ms: now, max_results: Some(0),
                    }).await {
                        let csv = format_csv_single(&result, &label);
                        let filename = format!("{dev}_{pt}_{}.csv", range.label());
                        #[cfg(feature = "desktop")]
                        save_csv_to_file(csv, &filename);
                    }
                });
            },
            svg {
                width: "14", height: "14", view_box: "0 0 24 24", fill: "currentColor",
                path { d: "M19 9h-4V3H9v6H5l7 7 7-7zM5 18v2h14v-2H5z" }
            }
            " Export CSV"
        }
    }
}

/// Export button for a dashboard widget (multiple sources).
#[component]
fn WidgetExportButton(sources: Vec<WidgetSource>, range: TrendRange) -> Element {
    let state = use_context::<AppState>();
    let hs = state.history_store.clone();

    rsx! {
        button {
            class: "export-csv-btn",
            title: "Export widget data to CSV",
            onclick: move |_| {
                let hs = hs.clone();
                let srcs = sources.clone();
                spawn(async move {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
                    let start = now - range.millis();
                    let mut results = Vec::new();
                    for src in &srcs {
                        if let Ok(r) = hs.query(HistoryQuery {
                            device_id: src.device_id.clone(), point_id: src.point_id.clone(),
                            start_ms: start, end_ms: now, max_results: Some(0),
                        }).await {
                            results.push((src.clone(), r));
                        }
                    }
                    if results.is_empty() { return; }
                    let csv = if results.len() == 1 {
                        let (src, r) = &results[0];
                        format_csv_single(r, &src.label)
                    } else {
                        format_csv_multi(&results)
                    };
                    #[cfg(feature = "desktop")]
                    save_csv_to_file(csv, "widget-export.csv");
                });
            },
            svg {
                width: "14", height: "14", view_box: "0 0 24 24", fill: "currentColor",
                path { d: "M19 9h-4V3H9v6H5l7 7 7-7zM5 18v2h14v-2H5z" }
            }
            " Export CSV"
        }
    }
}

/// Navigate to trend view with a specific device and point pre-selected.
pub fn navigate_to_trend(state: &mut AppState, device_id: &str, point_id: &str) {
    if state.dashboards.read().is_empty() {
        let dash = crate::gui::state::TrendDashboard {
            id: "dash-1".into(),
            name: "Dashboard 1".into(),
            widgets: Vec::new(),
        };
        state.dashboards.set(vec![dash]);
        state.active_dashboard_id.set(Some("dash-1".into()));
    }
    let dash_id = {
        let existing = state.active_dashboard_id.read().clone();
        if let Some(id) = existing {
            id
        } else {
            let id = match state.dashboards.read().first() {
                Some(d) => d.id.clone(),
                None => return,
            };
            state.active_dashboard_id.set(Some(id.clone()));
            id
        }
    };
    let label = state
        .loaded
        .devices
        .iter()
        .find(|d| d.instance_id == device_id)
        .and_then(|d| d.profile.points.iter().find(|p| p.id == point_id))
        .map(|p| p.name.clone())
        .unwrap_or_else(|| point_id.to_string());
    let wid_num = *state.next_widget_id.read();
    state.next_widget_id.set(wid_num + 1);
    let widget_id = format!("w-{wid_num}");
    let widget = DashboardWidget {
        id: widget_id.clone(),
        kind: WidgetKind::Chart,
        x: 20.0,
        y: 20.0,
        w: 500.0,
        h: 300.0,
        sources: vec![WidgetSource {
            device_id: device_id.to_string(),
            point_id: point_id.to_string(),
            label,
            color: COLORS[0].to_string(),
        }],
        range: TrendRange::Hour1,
    };
    let mut dashboards = state.dashboards.read().clone();
    if let Some(d) = dashboards.iter_mut().find(|d| d.id == dash_id) {
        d.widgets.push(widget);
    }
    state.dashboards.set(dashboards);
    state.selected_widget.set(Some(widget_id));
    state.active_view.set(ActiveView::History);
}
