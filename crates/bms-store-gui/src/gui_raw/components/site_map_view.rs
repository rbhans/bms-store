use std::collections::{HashMap, HashSet};

use dioxus::prelude::*;

use crate::config::profile::PointValue;
use crate::gui::state::{
    find_building_children, save_mapbox_config, ActiveView, AppState, MapMarker, MapViewConfig,
    MapboxConfig, MarkerIcon, SiteMapData, StatusBinding,
};
use crate::store::point_store::{PointKey, PointStatusFlags};

/// Color presets for marker color picker.
const COLOR_PRESETS: &[(&str, &str)] = &[
    ("Blue", "#3b82f6"),
    ("Red", "#ef4444"),
    ("Green", "#22c55e"),
    ("Amber", "#f59e0b"),
    ("Purple", "#a855f7"),
    ("Cyan", "#06b6d4"),
    ("Pink", "#ec4899"),
    ("Orange", "#f97316"),
];

/// Map style options.
const STYLE_OPTIONS: &[(&str, &str)] = &[
    ("Dark", "mapbox://styles/mapbox/dark-v11"),
    (
        "Satellite Streets",
        "mapbox://styles/mapbox/satellite-streets-v12",
    ),
    ("Outdoors", "mapbox://styles/mapbox/outdoors-v12"),
    ("Light", "mapbox://styles/mapbox/light-v11"),
];

// ----------------------------------------------------------------
// Alarm / status helpers
// ----------------------------------------------------------------

/// Marker health status derived from bound points.
#[derive(Debug, Clone, Copy, PartialEq)]
enum MarkerHealth {
    /// No bindings configured — no status to show.
    Unknown,
    /// All bound points normal.
    Normal,
    /// At least one bound point has a warning flag (stale, overridden).
    Warning,
    /// At least one bound point is in alarm or fault.
    Alarm,
    /// At least one bound device is down / offline.
    Offline,
}

impl MarkerHealth {
    fn css_class(self) -> &'static str {
        match self {
            Self::Unknown => "",
            Self::Normal => "site-map-health-normal",
            Self::Warning => "site-map-health-warning",
            Self::Alarm => "site-map-health-alarm",
            Self::Offline => "site-map-health-offline",
        }
    }

    fn ring_color(self) -> &'static str {
        match self {
            Self::Unknown => "",
            Self::Normal => "#22c55e",
            Self::Warning => "#f59e0b",
            Self::Alarm => "#ef4444",
            Self::Offline => "#6b7280",
        }
    }

    #[allow(dead_code)]
    fn label(self) -> &'static str {
        match self {
            Self::Unknown => "",
            Self::Normal => "Normal",
            Self::Warning => "Warning",
            Self::Alarm => "Alarm",
            Self::Offline => "Offline",
        }
    }
}

/// Compute health for a marker from its status bindings.
fn compute_marker_health(marker: &MapMarker, state: &AppState) -> MarkerHealth {
    if marker.status_bindings.is_empty() {
        return MarkerHealth::Unknown;
    }
    let mut worst = MarkerHealth::Normal;
    for binding in &marker.status_bindings {
        if let Some((dev, pt)) = binding.point_key.split_once('/') {
            let key = PointKey {
                device_instance_id: dev.to_string(),
                point_id: pt.to_string(),
            };
            if let Some(tv) = state.store.get(&key) {
                let s = tv.status;
                if s.has(PointStatusFlags::DOWN) {
                    return MarkerHealth::Offline; // worst possible
                }
                if s.has(PointStatusFlags::ALARM) || s.has(PointStatusFlags::FAULT) {
                    worst = MarkerHealth::Alarm;
                } else if (s.has(PointStatusFlags::STALE) || s.has(PointStatusFlags::OVERRIDDEN))
                    && worst != MarkerHealth::Alarm
                {
                    worst = MarkerHealth::Warning;
                }
            }
        }
    }
    worst
}

/// Compute a status rollup summary for a marker's bound points.
fn compute_status_rollup(marker: &MapMarker, state: &AppState) -> StatusRollup {
    let mut rollup = StatusRollup::default();
    if marker.status_bindings.is_empty() {
        return rollup;
    }
    let mut seen_devices = HashSet::new();
    for binding in &marker.status_bindings {
        if let Some((dev, pt)) = binding.point_key.split_once('/') {
            rollup.total_points += 1;
            seen_devices.insert(dev.to_string());
            let key = PointKey {
                device_instance_id: dev.to_string(),
                point_id: pt.to_string(),
            };
            if let Some(tv) = state.store.get(&key) {
                let s = tv.status;
                if s.has(PointStatusFlags::ALARM) || s.has(PointStatusFlags::FAULT) {
                    rollup.alarm_count += 1;
                }
                if s.has(PointStatusFlags::DOWN) {
                    rollup.offline_count += 1;
                }
            } else {
                rollup.offline_count += 1;
            }
        }
    }
    rollup.total_devices = seen_devices.len() as u32;
    rollup
}

#[derive(Debug, Clone, Default)]
struct StatusRollup {
    total_points: u32,
    total_devices: u32,
    alarm_count: u32,
    offline_count: u32,
}

impl StatusRollup {
    fn summary_html(&self) -> String {
        if self.total_points == 0 {
            return String::new();
        }
        let mut parts = Vec::new();
        if self.alarm_count > 0 {
            parts.push(format!(
                "<span style='color:#ef4444'>{} alarm{}</span>",
                self.alarm_count,
                if self.alarm_count == 1 { "" } else { "s" }
            ));
        }
        if self.offline_count > 0 {
            parts.push(format!(
                "<span style='color:#6b7280'>{} offline</span>",
                self.offline_count
            ));
        }
        let online = self
            .total_points
            .saturating_sub(self.alarm_count + self.offline_count);
        parts.push(format!(
            "<span style='color:#22c55e'>{}/{} OK</span>",
            online, self.total_points
        ));
        format!(
            "<div class='site-map-popup-rollup'>{}</div>",
            parts.join(" &middot; ")
        )
    }

    fn list_summary(&self) -> String {
        if self.total_points == 0 {
            return String::new();
        }
        let mut parts = Vec::new();
        if self.alarm_count > 0 {
            parts.push(format!("{} alarm", self.alarm_count));
        }
        if self.offline_count > 0 {
            parts.push(format!("{} offline", self.offline_count));
        }
        let online = self
            .total_points
            .saturating_sub(self.alarm_count + self.offline_count);
        parts.push(format!("{}/{} OK", online, self.total_points));
        parts.join(" | ")
    }
}

// ----------------------------------------------------------------
// Main component
// ----------------------------------------------------------------

#[component]
pub fn SiteMapView(page_id: String) -> Element {
    let mut state = use_context::<AppState>();

    // Local UI state
    let mut editing = use_signal(|| false);
    let mut placing_marker = use_signal(|| false);
    let mut show_settings = use_signal(|| false);
    let mut show_marker_list = use_signal(|| true);
    let mut editing_marker_id = use_signal(|| Option::<String>::None);
    let mut map_initialized = use_signal(|| false);
    let mut search_filter = use_signal(String::new);
    let mut next_marker_id = use_signal(|| {
        let maps = state.site_maps.read();
        let mut max = 0u32;
        if let Some(data) = maps.get(&page_id) {
            for m in &data.markers {
                if let Some(n) = m.id.strip_prefix("m-").and_then(|s| s.parse::<u32>().ok()) {
                    max = max.max(n);
                }
            }
        }
        max + 1
    });

    // Ensure site map data exists
    {
        let maps = state.site_maps.read();
        if !maps.contains_key(&page_id) {
            drop(maps);
            state
                .site_maps
                .write()
                .entry(page_id.clone())
                .or_insert_with(SiteMapData::default);
        }
    }

    let site_data = state
        .site_maps
        .read()
        .get(&page_id)
        .cloned()
        .unwrap_or_default();
    let mapbox_cfg = state.mapbox_config.read().clone();
    let has_token = !mapbox_cfg.access_token.is_empty();
    let is_editing = *editing.read();
    let is_placing = *placing_marker.read();
    let settings_open = *show_settings.read();
    let marker_list_open = *show_marker_list.read();
    let edit_marker = editing_marker_id.read().clone();
    let filter_text = search_filter.read().to_lowercase();

    // Building children for linking dropdown
    let buildings = find_building_children(&state.nav_tree.read(), &page_id);

    // Read live store version to trigger re-renders for alarm/status changes
    let _store_ver = *state.store_version.read();

    // Compute per-marker health (reactive — recalculated on store version changes)
    let marker_healths: Vec<(String, MarkerHealth, StatusRollup)> = site_data
        .markers
        .iter()
        .map(|m| {
            let health = compute_marker_health(m, &state);
            let rollup = compute_status_rollup(m, &state);
            (m.id.clone(), health, rollup)
        })
        .collect();

    // Filter markers for list panel
    let filtered_markers: Vec<&MapMarker> = site_data
        .markers
        .iter()
        .filter(|m| {
            if filter_text.is_empty() {
                return true;
            }
            m.label.to_lowercase().contains(&filter_text)
                || m.address
                    .as_ref()
                    .map(|a| a.to_lowercase().contains(&filter_text))
                    .unwrap_or(false)
        })
        .collect();

    // Inject Mapbox GL JS + CSS and initialize map — runs only once on mount.
    let token = mapbox_cfg.access_token.clone();
    let style = mapbox_cfg.style.clone();
    let cfg = site_data.map_config.clone();
    let markers_for_init = site_data.markers.clone();
    use_hook(move || {
        if token.is_empty() {
            return;
        }
        spawn(async move {
            // Inject Mapbox CSS + JS if not already present
            document::eval(
                r#"
                if (!document.getElementById('mapbox-css')) {
                    var link = document.createElement('link');
                    link.id = 'mapbox-css';
                    link.rel = 'stylesheet';
                    link.href = 'https://api.mapbox.com/mapbox-gl-js/v3.4.0/mapbox-gl.css';
                    document.head.appendChild(link);
                }
                if (!document.getElementById('mapbox-js')) {
                    var script = document.createElement('script');
                    script.id = 'mapbox-js';
                    script.src = 'https://api.mapbox.com/mapbox-gl-js/v3.4.0/mapbox-gl.js';
                    document.head.appendChild(script);
                }
            "#,
            );

            // Wait for mapbox-gl to load
            let mut retries = 0;
            loop {
                let mut check = document::eval("dioxus.send(typeof mapboxgl !== 'undefined')");
                if let Ok(val) = check.recv::<bool>().await {
                    if val {
                        break;
                    }
                }
                retries += 1;
                if retries > 50 {
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }

            // Initialize map
            let init_js = format!(
                r#"
                if (window.__ocMap) {{
                    window.__ocMap.remove();
                }}
                window.__ocMarkers = {{}};
                mapboxgl.accessToken = '{token}';
                window.__ocMap = new mapboxgl.Map({{
                    container: 'mapbox-container',
                    style: '{style}',
                    center: [{lon}, {lat}],
                    zoom: {zoom},
                    pitch: {pitch},
                    bearing: {bearing},
                    projection: 'globe'
                }});
                window.__ocMap.addControl(new mapboxgl.NavigationControl(), 'bottom-right');
                window.__ocMap.on('load', function() {{
                    window.__ocMap.addSource('mapbox-dem', {{
                        type: 'raster-dem',
                        url: 'mapbox://mapbox.mapbox-terrain-dem-v1',
                        tileSize: 512,
                        maxzoom: 14
                    }});
                    window.__ocMap.setTerrain({{ source: 'mapbox-dem', exaggeration: 1.5 }});
                    window.__ocMap.setFog({{
                        color: '#1a1a2e',
                        'high-color': '#1a1a2e',
                        'horizon-blend': 0.02
                    }});

                    // 3D building extrusions
                    var layers = window.__ocMap.getStyle().layers;
                    var labelLayerId;
                    for (var i = 0; i < layers.length; i++) {{
                        if (layers[i].type === 'symbol' && layers[i].layout['text-field']) {{
                            labelLayerId = layers[i].id;
                            break;
                        }}
                    }}
                    window.__ocMap.addLayer({{
                        'id': '3d-buildings',
                        'source': 'composite',
                        'source-layer': 'building',
                        'filter': ['==', 'extrude', 'true'],
                        'type': 'fill-extrusion',
                        'minzoom': 15,
                        'paint': {{
                            'fill-extrusion-color': '#aaa',
                            'fill-extrusion-height': ['interpolate', ['linear'], ['zoom'], 15, 0, 15.05, ['get', 'height']],
                            'fill-extrusion-base': ['interpolate', ['linear'], ['zoom'], 15, 0, 15.05, ['get', 'min_height']],
                            'fill-extrusion-opacity': 0.6
                        }}
                    }}, labelLayerId);
                }});
                dioxus.send(true);
            "#,
                token = token,
                style = style,
                lon = cfg.center_lon,
                lat = cfg.center_lat,
                zoom = cfg.zoom,
                pitch = cfg.pitch,
                bearing = cfg.bearing,
            );
            let mut init = document::eval(&init_js);
            if init.recv::<bool>().await.is_ok() {
                map_initialized.set(true);
            }

            // Add existing markers and fit view to them
            for m in &markers_for_init {
                add_marker_js(m, false);
            }
            if !markers_for_init.is_empty() {
                if markers_for_init.len() == 1 {
                    fly_to(markers_for_init[0].lat, markers_for_init[0].lon);
                } else {
                    let mut min_lat = f64::MAX;
                    let mut max_lat = f64::MIN;
                    let mut min_lon = f64::MAX;
                    let mut max_lon = f64::MIN;
                    for m in &markers_for_init {
                        min_lat = min_lat.min(m.lat);
                        max_lat = max_lat.max(m.lat);
                        min_lon = min_lon.min(m.lon);
                        max_lon = max_lon.max(m.lon);
                    }
                    let js = format!(
                        "if (window.__ocMap) {{ window.__ocMap.fitBounds([[{min_lon}, {min_lat}], [{max_lon}, {max_lat}]], {{ padding: {{ top: 200, left: 220, bottom: 60, right: 60 }}, maxZoom: 16 }}); }}"
                    );
                    document::eval(&js);
                }
            }
        });
    });

    // Click event listener for placing markers + drag end events — runs once
    {
        let pid2 = page_id.clone();
        use_hook(move || {
            spawn(async move {
                // Wait for map init
                loop {
                    if *map_initialized.read() {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                }

                let mut eval = document::eval(
                    r#"
                    if (window.__ocClickHandler) {
                        window.__ocMap.off('click', window.__ocClickHandler);
                    }
                    window.__ocClickHandler = function(e) {
                        dioxus.send({ type: 'click', lat: e.lngLat.lat, lng: e.lngLat.lng });
                    };
                    window.__ocMap.on('click', window.__ocClickHandler);

                    // Drag end handler — called by marker setup
                    window.__ocDragEnd = function(id, lngLat) {
                        dioxus.send({ type: 'dragend', id: id, lat: lngLat.lat, lng: lngLat.lng });
                    };
                "#,
                );

                loop {
                    match eval.recv::<serde_json::Value>().await {
                        Ok(val) => {
                            let event_type = val.get("type").and_then(|v| v.as_str()).unwrap_or("");

                            match event_type {
                                "click" => {
                                    if !*placing_marker.read() {
                                        continue;
                                    }
                                    let lat =
                                        val.get("lat").and_then(|v| v.as_f64()).unwrap_or(0.0);
                                    let lon =
                                        val.get("lng").and_then(|v| v.as_f64()).unwrap_or(0.0);

                                    let id_num = *next_marker_id.read();
                                    next_marker_id.set(id_num + 1);
                                    let marker_id = format!("m-{id_num}");

                                    let new_marker = MapMarker {
                                        id: marker_id.clone(),
                                        label: format!("Building {}", id_num),
                                        lat,
                                        lon,
                                        address: None,
                                        building_nav_id: None,
                                        color: "#3b82f6".into(),
                                        icon: MarkerIcon::Circle,
                                        status_bindings: Vec::new(),
                                    };

                                    add_marker_js(&new_marker, false);

                                    let mut state = use_context::<AppState>();
                                    {
                                        let mut maps = state.site_maps.write();
                                        if let Some(data) = maps.get_mut(&pid2) {
                                            data.markers.push(new_marker);
                                        }
                                    }
                                    state.save_layout();
                                    placing_marker.set(false);
                                }
                                "dragend" => {
                                    let mid = val
                                        .get("id")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let lat =
                                        val.get("lat").and_then(|v| v.as_f64()).unwrap_or(0.0);
                                    let lon =
                                        val.get("lng").and_then(|v| v.as_f64()).unwrap_or(0.0);
                                    if !mid.is_empty() {
                                        let mut state = use_context::<AppState>();
                                        {
                                            let mut maps = state.site_maps.write();
                                            if let Some(data) = maps.get_mut(&pid2) {
                                                if let Some(m) =
                                                    data.markers.iter_mut().find(|m| m.id == mid)
                                                {
                                                    m.lat = lat;
                                                    m.lon = lon;
                                                }
                                            }
                                        }
                                        state.save_layout();
                                    }
                                }
                                _ => {}
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        });
    }

    // Live status polling + alarm ring updates — every 3 seconds
    {
        let pid3 = page_id.clone();
        use_hook(move || {
            spawn(async move {
                loop {
                    if *map_initialized.read() {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                }

                let state = use_context::<AppState>();
                let store = state.store.clone();
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    let maps = state.site_maps.read();
                    let Some(data) = maps.get(&pid3) else {
                        continue;
                    };
                    for marker in &data.markers {
                        if marker.status_bindings.is_empty() {
                            continue;
                        }

                        // Compute health for alarm ring
                        let health = compute_marker_health(marker, &state);
                        let rollup = compute_status_rollup(marker, &state);
                        let ring_color = health.ring_color();

                        // Build value lines
                        let mut lines = Vec::new();
                        for binding in &marker.status_bindings {
                            let val_str = if let Some((dev, pt)) = binding.point_key.split_once('/')
                            {
                                let key = PointKey {
                                    device_instance_id: dev.to_string(),
                                    point_id: pt.to_string(),
                                };
                                match store.get(&key) {
                                    Some(tv) => match &tv.value {
                                        PointValue::Bool(b) => {
                                            if *b {
                                                "ON".into()
                                            } else {
                                                "OFF".into()
                                            }
                                        }
                                        PointValue::Integer(i) => i.to_string(),
                                        PointValue::Float(f) => format!("{f:.1}"),
                                    },
                                    None => "\u{2014}".into(),
                                }
                            } else {
                                "\u{2014}".into()
                            };
                            lines.push(format!(
                                "<div class='site-map-popup-binding'><span class='site-map-popup-label'>{}</span><span class='site-map-popup-value'>{}</span></div>",
                                html_escape(&binding.label),
                                html_escape(&val_str)
                            ));
                        }
                        let bindings_html = lines.join("");
                        let rollup_html = rollup.summary_html();

                        // Update popup content + marker ring color
                        let js = format!(
                            r#"
                            (function() {{
                                var entry = window.__ocMarkers && window.__ocMarkers['{id}'];
                                if (!entry) return;
                                // Update popup
                                if (entry.popup) {{
                                    var base = entry.baseHtml || '';
                                    entry.popup.setHTML(base + '{rollup}' + '{bindings}');
                                }}
                                // Update alarm ring
                                var el = entry.marker.getElement();
                                if (el && '{ring}') {{
                                    el.style.boxShadow = '0 0 0 3px {ring}, 0 2px 6px rgba(0,0,0,0.4)';
                                }}
                            }})();
                            "#,
                            id = marker.id,
                            rollup = rollup_html.replace('\'', "\\'"),
                            bindings = bindings_html.replace('\'', "\\'"),
                            ring = ring_color,
                        );
                        document::eval(&js);
                    }
                }
            });
        });
    }

    // Toggle marker draggability when edit mode changes
    {
        use_effect(move || {
            if !*map_initialized.read() {
                return;
            }
            let draggable = *editing.read();
            let js = format!(
                r#"
                if (window.__ocMarkers) {{
                    Object.keys(window.__ocMarkers).forEach(function(id) {{
                        var entry = window.__ocMarkers[id];
                        if (entry && entry.marker) {{
                            entry.marker.setDraggable({draggable});
                        }}
                    }});
                }}
                "#,
                draggable = if draggable { "true" } else { "false" },
            );
            document::eval(&js);
        });
    }

    // Clone page_id for each closure that needs it
    let pid_edit_toggle = page_id.clone();
    let pid_fit = page_id.clone();
    let pid_3d = page_id.clone();
    let pid_settings = page_id.clone();
    let pid_edit_dialog = page_id.clone();

    rsx! {
        div { class: "site-map-root",
            // Map container
            div {
                id: "mapbox-container",
                class: "site-map-container",
            }

            // No-token overlay
            if !has_token {
                div { class: "site-map-no-token",
                    div { class: "site-map-no-token-card",
                        h3 { "Mapbox Access Token Required" }
                        p { "Enter your Mapbox access token to enable the interactive 3D map." }
                        button {
                            class: "btn btn-primary",
                            onclick: move |_| show_settings.set(true),
                            "Open Settings"
                        }
                    }
                }
            }

            // Toolbar overlay (top-left)
            div { class: "site-map-toolbar",
                button {
                    class: if is_editing { "site-map-btn site-map-btn-active" } else { "site-map-btn" },
                    title: "Toggle Edit Mode",
                    onclick: move |_| {
                        let was_editing = *editing.read();
                        editing.set(!was_editing);
                        if was_editing {
                            save_current_view(&pid_edit_toggle);
                        }
                    },
                    svg { view_box: "0 0 24 24", width: "16", height: "16",
                        path {
                            fill: "currentColor",
                            d: "M3 17.25V21h3.75L17.81 9.94l-3.75-3.75L3 17.25zM20.71 7.04c.39-.39.39-1.02 0-1.41l-2.34-2.34a.9959.9959 0 0 0-1.41 0l-1.83 1.83 3.75 3.75 1.83-1.83z",
                        }
                    }
                }

                if is_editing {
                    button {
                        class: if is_placing { "site-map-btn site-map-btn-active" } else { "site-map-btn" },
                        title: "Add Marker (click map to place)",
                        onclick: move |_| {
                            let v = *placing_marker.read();
                            placing_marker.set(!v);
                        },
                        svg { view_box: "0 0 24 24", width: "16", height: "16",
                            path {
                                fill: "currentColor",
                                d: "M12 2C8.13 2 5 5.13 5 9c0 5.25 7 13 7 13s7-7.75 7-13c0-3.87-3.13-7-7-7zm0 9.5c-1.38 0-2.5-1.12-2.5-2.5s1.12-2.5 2.5-2.5 2.5 1.12 2.5 2.5-1.12 2.5-2.5 2.5z",
                            }
                        }
                    }
                }

                button {
                    class: "site-map-btn",
                    title: "Fit All Markers",
                    onclick: move |_| {
                        let state = use_context::<AppState>();
                        fit_all_markers(&state, &pid_fit);
                    },
                    svg { view_box: "0 0 24 24", width: "16", height: "16",
                        path {
                            fill: "currentColor",
                            d: "M15 3l2.3 2.3-2.89 2.87 1.42 1.42L18.7 6.7 21 9V3h-6zM3 9l2.3-2.3 2.87 2.89 1.42-1.42L6.7 5.3 9 3H3v6zm6 12l-2.3-2.3 2.89-2.87-1.42-1.42L5.3 17.3 3 15v6h6zm12-6l-2.3 2.3-2.87-2.89-1.42 1.42 2.89 2.87L15 21h6v-6z",
                        }
                    }
                }

                button {
                    class: "site-map-btn",
                    title: "Toggle 3D Tilt",
                    onclick: move |_| {
                        let mut state = use_context::<AppState>();
                        toggle_3d(&pid_3d, &mut state);
                    },
                    "3D"
                }

                button {
                    class: "site-map-btn",
                    title: "Toggle Marker List",
                    onclick: move |_| {
                        let v = *show_marker_list.read();
                        show_marker_list.set(!v);
                    },
                    svg { view_box: "0 0 24 24", width: "16", height: "16",
                        path {
                            fill: "currentColor",
                            d: "M3 13h2v-2H3v2zm0 4h2v-2H3v2zm0-8h2V7H3v2zm4 4h14v-2H7v2zm0 4h14v-2H7v2zM7 7v2h14V7H7z",
                        }
                    }
                }

                button {
                    class: "site-map-btn",
                    title: "Map Settings",
                    onclick: move |_| {
                        let v = *show_settings.read();
                        show_settings.set(!v);
                    },
                    svg { view_box: "0 0 24 24", width: "16", height: "16",
                        path {
                            fill: "currentColor",
                            d: "M19.14 12.94c.04-.3.06-.61.06-.94 0-.32-.02-.64-.07-.94l2.03-1.58a.49.49 0 0 0 .12-.61l-1.92-3.32a.488.488 0 0 0-.59-.22l-2.39.96c-.5-.38-1.03-.7-1.62-.94l-.36-2.54a.484.484 0 0 0-.48-.41h-3.84c-.24 0-.43.17-.47.41l-.36 2.54c-.59.24-1.13.57-1.62.94l-2.39-.96c-.22-.08-.47 0-.59.22L2.74 8.87c-.12.21-.08.47.12.61l2.03 1.58c-.05.3-.07.62-.07.94s.02.64.07.94l-2.03 1.58a.49.49 0 0 0-.12.61l1.92 3.32c.12.22.37.29.59.22l2.39-.96c.5.38 1.03.7 1.62.94l.36 2.54c.05.24.24.41.48.41h3.84c.24 0 .44-.17.47-.41l.36-2.54c.59-.24 1.13-.56 1.62-.94l2.39.96c.22.08.47 0 .59-.22l1.92-3.32c.12-.22.07-.47-.12-.61l-2.01-1.58zM12 15.6c-1.98 0-3.6-1.62-3.6-3.6s1.62-3.6 3.6-3.6 3.6 1.62 3.6 3.6-1.62 3.6-3.6 3.6z",
                        }
                    }
                }
            }

            // Placing marker hint
            if is_placing {
                div { class: "site-map-placing-hint",
                    "Click on the map to place a marker"
                }
            }

            // Edit mode hint
            if is_editing && !is_placing {
                div { class: "site-map-edit-hint",
                    "Edit mode — drag markers to reposition"
                }
            }

            // Marker list panel (left sidebar overlay)
            if marker_list_open && !site_data.markers.is_empty() {
                div { class: "site-map-marker-list",
                    div { class: "site-map-marker-list-header",
                        span { "Markers ({site_data.markers.len()})" }
                    }

                    // Search/filter
                    if site_data.markers.len() > 3 {
                        div { class: "site-map-marker-search",
                            input {
                                r#type: "text",
                                placeholder: "Filter markers...",
                                value: "{search_filter}",
                                oninput: move |e| search_filter.set(e.value()),
                            }
                        }
                    }

                    div { class: "site-map-marker-list-body",
                        for marker in filtered_markers.iter() {
                            {
                                let lat = marker.lat;
                                let lon = marker.lon;
                                let mid_edit = marker.id.clone();
                                let mid_del = marker.id.clone();
                                let pid_del = page_id.clone();
                                let color = marker.color.clone();
                                let building_label = marker.building_nav_id.as_ref().and_then(|bid| {
                                    buildings.iter().find(|(id, _)| id == bid).map(|(_, l)| l.clone())
                                });
                                let (health, rollup) = marker_healths
                                    .iter()
                                    .find(|(id, _, _)| id == &marker.id)
                                    .map(|(_, h, r)| (*h, r.clone()))
                                    .unwrap_or((MarkerHealth::Unknown, StatusRollup::default()));
                                let health_class = health.css_class();
                                let rollup_text = rollup.list_summary();
                                let building_nav_target = marker.building_nav_id.clone();
                                rsx! {
                                    div {
                                        class: "site-map-marker-item",
                                        onclick: move |_| fly_to(lat, lon),
                                        div {
                                            class: "site-map-marker-dot {health_class}",
                                            style: "background: {color};",
                                        }
                                        div { class: "site-map-marker-info",
                                            div { class: "site-map-marker-label", "{marker.label}" }
                                            if let Some(ref addr) = marker.address {
                                                div { class: "site-map-marker-addr", "{addr}" }
                                            }
                                            if !rollup_text.is_empty() {
                                                div { class: "site-map-marker-status", "{rollup_text}" }
                                            }
                                            if let Some(ref bl) = building_label {
                                                div { class: "site-map-marker-link",
                                                    span {
                                                        onclick: {
                                                            let nav_target = building_nav_target.clone();
                                                            move |e: Event<MouseData>| {
                                                                e.stop_propagation();
                                                                if let Some(ref bid) = nav_target {
                                                                    let mut state = use_context::<AppState>();
                                                                    state.active_view.set(ActiveView::Page(bid.clone()));
                                                                }
                                                            }
                                                        },
                                                        "\u{2192} {bl}"
                                                    }
                                                }
                                            }
                                        }
                                        button {
                                            class: "site-map-marker-edit-btn",
                                            onclick: move |e| {
                                                e.stop_propagation();
                                                editing_marker_id.set(Some(mid_edit.clone()));
                                            },
                                            title: "Edit",
                                            "..."
                                        }
                                        button {
                                            class: "site-map-marker-del-btn",
                                            onclick: move |e| {
                                                e.stop_propagation();
                                                remove_marker_js(&mid_del);
                                                let mut state = use_context::<AppState>();
                                                let mut maps = state.site_maps.write();
                                                if let Some(data) = maps.get_mut(&pid_del) {
                                                    data.markers.retain(|m| m.id != mid_del);
                                                }
                                                drop(maps);
                                                state.save_layout();
                                            },
                                            title: "Delete",
                                            "x"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Settings panel
            if settings_open {
                SettingsPanel {
                    on_close: move |_| show_settings.set(false),
                    page_id: pid_settings.clone(),
                }
            }

            // Marker edit dialog
            if let Some(ref marker_id) = edit_marker {
                MarkerEditDialog {
                    marker_id: marker_id.clone(),
                    page_id: pid_edit_dialog.clone(),
                    buildings: buildings.clone(),
                    on_close: move |_| editing_marker_id.set(None),
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Settings panel
// ----------------------------------------------------------------

#[component]
fn SettingsPanel(on_close: EventHandler<()>, page_id: String) -> Element {
    let state = use_context::<AppState>();
    let cfg = state.mapbox_config.read().clone();
    let mut token_input = use_signal(|| cfg.access_token.clone());
    let mut style_input = use_signal(|| cfg.style.clone());

    rsx! {
        div { class: "site-map-settings-backdrop",
            onclick: move |_| on_close.call(()),
            div {
                class: "site-map-settings-panel",
                onclick: move |e| e.stop_propagation(),
                h3 { "Map Settings" }

                div { class: "site-map-settings-group",
                    label { "Mapbox Access Token" }
                    input {
                        r#type: "text",
                        placeholder: "pk.eyJ1...",
                        value: "{token_input}",
                        oninput: move |e| token_input.set(e.value()),
                    }
                }

                div { class: "site-map-settings-group",
                    label { "Map Style" }
                    select {
                        value: "{style_input}",
                        onchange: move |e| style_input.set(e.value()),
                        for (name, url) in STYLE_OPTIONS.iter() {
                            option {
                                value: "{url}",
                                selected: *url == style_input.read().as_str(),
                                "{name}"
                            }
                        }
                    }
                }

                div { class: "site-map-settings-actions",
                    button {
                        class: "btn btn-primary",
                        onclick: move |_| {
                            let mut state = use_context::<AppState>();
                            let new_cfg = MapboxConfig {
                                access_token: token_input.read().clone(),
                                style: style_input.read().clone(),
                            };
                            save_mapbox_config(&state.project_paths, &new_cfg);
                            state.mapbox_config.set(new_cfg);
                            on_close.call(());
                        },
                        "Save"
                    }
                    button {
                        class: "btn",
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Marker edit dialog
// ----------------------------------------------------------------

#[component]
fn MarkerEditDialog(
    marker_id: String,
    page_id: String,
    buildings: Vec<(String, String)>,
    on_close: EventHandler<()>,
) -> Element {
    let state = use_context::<AppState>();

    let marker = state
        .site_maps
        .read()
        .get(&page_id)
        .and_then(|d| d.markers.iter().find(|m| m.id == marker_id).cloned());

    let Some(marker) = marker else {
        return rsx! {};
    };

    let mut label = use_signal(|| marker.label.clone());
    let mut address = use_signal(|| marker.address.clone().unwrap_or_default());
    let mut lat = use_signal(|| format!("{:.6}", marker.lat));
    let mut lon = use_signal(|| format!("{:.6}", marker.lon));
    let mut color = use_signal(|| marker.color.clone());
    let mut icon = use_signal(|| marker.icon);
    let mut building_link = use_signal(|| marker.building_nav_id.clone().unwrap_or_default());
    let mut bindings = use_signal(|| marker.status_bindings.clone());
    let mut geocode_status = use_signal(|| Option::<String>::None);

    // Point search for adding bindings
    let mut binding_search = use_signal(String::new);
    let mut binding_search_open = use_signal(|| false);

    // Build searchable point list: Vec<(point_key "dev/pt", display_label)>
    let mut point_list_sig: Signal<Vec<(String, String)>> = use_signal(Vec::new);
    {
        let ns = state.node_store.clone();
        let store = state.store.clone();
        let _ = use_resource(move || {
            let ns = ns.clone();
            let store = store.clone();
            async move {
                // Get device/equip display names
                let mut dev_names: HashMap<String, String> = HashMap::new();
                let equips = ns.list_nodes(Some("equip"), None).await;
                for n in equips {
                    if !n.dis.is_empty() {
                        dev_names.insert(n.id.clone(), n.dis);
                    }
                }
                // Get point display names
                let mut pt_names: HashMap<String, String> = HashMap::new();
                let pts = ns.list_nodes(Some("point"), None).await;
                for n in pts {
                    if !n.dis.is_empty() {
                        pt_names.insert(n.id.clone(), n.dis);
                    }
                }
                let vpts = ns.list_nodes(Some("virtual_point"), None).await;
                for n in vpts {
                    if !n.dis.is_empty() {
                        pt_names.insert(n.id.clone(), n.dis);
                    }
                }
                // Build list from all point keys: "DeviceName / PointName"
                let mut list: Vec<(String, String)> = store
                    .all_keys()
                    .into_iter()
                    .map(|k| {
                        let key = format!("{}/{}", k.device_instance_id, k.point_id);
                        let dev_label = dev_names
                            .get(&k.device_instance_id)
                            .cloned()
                            .unwrap_or_else(|| k.device_instance_id.clone());
                        let pt_label = pt_names
                            .get(&key)
                            .cloned()
                            .unwrap_or_else(|| k.point_id.clone());
                        let label = format!("{} / {}", dev_label, pt_label);
                        (key, label)
                    })
                    .collect();
                list.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
                point_list_sig.set(list);
            }
        });
    }

    let mid = marker_id.clone();
    let pid = page_id.clone();
    let orig_marker = marker.clone();

    rsx! {
        div { class: "site-map-settings-backdrop",
            onclick: move |_| on_close.call(()),
            div {
                class: "site-map-edit-dialog",
                onclick: move |e| e.stop_propagation(),
                h3 { "Edit Marker" }

                div { class: "site-map-edit-row",
                    label { "Label" }
                    input {
                        r#type: "text",
                        value: "{label}",
                        oninput: move |e| label.set(e.value()),
                    }
                }

                div { class: "site-map-edit-row",
                    label { "Address" }
                    div { class: "site-map-edit-addr-row",
                        input {
                            r#type: "text",
                            placeholder: "123 Main St, City, State",
                            value: "{address}",
                            oninput: move |e| address.set(e.value()),
                        }
                        button {
                            class: "btn btn-sm",
                            onclick: {
                                let token = state.mapbox_config.read().access_token.clone();
                                move |_| {
                                    let addr = address.read().clone();
                                    let token = token.clone();
                                    if addr.is_empty() || token.is_empty() {
                                        return;
                                    }
                                    geocode_status.set(Some("Looking up...".into()));
                                    spawn(async move {
                                        match geocode(&addr, &token).await {
                                            Some((glat, glon)) => {
                                                lat.set(format!("{glat:.6}"));
                                                lon.set(format!("{glon:.6}"));
                                                geocode_status.set(Some(format!("Found: {glat:.4}, {glon:.4}")));
                                            }
                                            None => {
                                                geocode_status.set(Some("Address not found".into()));
                                            }
                                        }
                                    });
                                }
                            },
                            "Lookup"
                        }
                    }
                    if let Some(ref status) = *geocode_status.read() {
                        span { class: "site-map-geocode-status", "{status}" }
                    }
                }

                div { class: "site-map-edit-coords",
                    div { class: "site-map-edit-row",
                        label { "Latitude" }
                        input {
                            r#type: "text",
                            value: "{lat}",
                            oninput: move |e| lat.set(e.value()),
                        }
                    }
                    div { class: "site-map-edit-row",
                        label { "Longitude" }
                        input {
                            r#type: "text",
                            value: "{lon}",
                            oninput: move |e| lon.set(e.value()),
                        }
                    }
                }

                div { class: "site-map-edit-row",
                    label { "Color" }
                    div { class: "site-map-color-swatches",
                        for (_name, hex) in COLOR_PRESETS.iter() {
                            {
                                let hex = hex.to_string();
                                let hex2 = hex.clone();
                                let name = _name.to_string();
                                rsx! {
                                    button {
                                        class: if color.read().as_str() == hex.as_str() { "site-map-swatch site-map-swatch-active" } else { "site-map-swatch" },
                                        style: "background: {hex};",
                                        title: "{name}",
                                        onclick: move |_| color.set(hex2.clone()),
                                    }
                                }
                            }
                        }
                    }
                }

                div { class: "site-map-edit-row",
                    label { "Icon Shape" }
                    div { class: "site-map-icon-options",
                        for opt in MarkerIcon::all().iter() {
                            {
                                let opt = *opt;
                                rsx! {
                                    button {
                                        class: if *icon.read() == opt { "site-map-icon-btn site-map-icon-btn-active" } else { "site-map-icon-btn" },
                                        onclick: move |_| icon.set(opt),
                                        "{opt.label()}"
                                    }
                                }
                            }
                        }
                    }
                }

                div { class: "site-map-edit-row",
                    label { "Link to Building" }
                    select {
                        value: "{building_link}",
                        onchange: move |e| building_link.set(e.value()),
                        option { value: "", "(none)" }
                        for (bid, blabel) in buildings.iter() {
                            option {
                                value: "{bid}",
                                "{blabel}"
                            }
                        }
                    }
                }

                // Status bindings
                div { class: "site-map-edit-row",
                    label { "Status Points" }
                    div { class: "site-map-bindings-list",
                        for (i, binding) in bindings.read().iter().enumerate() {
                            div { class: "site-map-binding-item",
                                span { class: "site-map-binding-label", "{binding.label}" }
                                span { class: "site-map-binding-key", "{binding.point_key}" }
                                button {
                                    class: "site-map-binding-remove",
                                    onclick: move |_| {
                                        bindings.write().remove(i);
                                    },
                                    "×"
                                }
                            }
                        }
                        div { class: "site-map-binding-search-wrap",
                            input {
                                r#type: "text",
                                placeholder: "Search points...",
                                value: "{binding_search}",
                                oninput: move |e| {
                                    binding_search.set(e.value());
                                    binding_search_open.set(!e.value().is_empty());
                                },
                                onfocus: move |_| {
                                    if !binding_search.read().is_empty() {
                                        binding_search_open.set(true);
                                    }
                                },
                            }
                            if *binding_search_open.read() {
                                {
                                    let query = binding_search.read().to_lowercase();
                                    let existing: HashSet<String> = bindings.read().iter().map(|b| b.point_key.clone()).collect();
                                    let all_pts = point_list_sig.read();
                                    let results: Vec<&(String, String)> = all_pts.iter()
                                        .filter(|(key, label)| {
                                            !existing.contains(key) &&
                                            (label.to_lowercase().contains(&query) || key.to_lowercase().contains(&query))
                                        })
                                        .take(8)
                                        .collect();
                                    if results.is_empty() {
                                        rsx! {
                                            div { class: "site-map-binding-results",
                                                div { class: "site-map-binding-no-results", "No matching points" }
                                            }
                                        }
                                    } else {
                                        rsx! {
                                            div { class: "site-map-binding-results",
                                                for (key, result_label) in results.iter() {
                                                    {
                                                        let pk = key.to_string();
                                                        let lbl = result_label.to_string();
                                                        rsx! {
                                                            button {
                                                                class: "site-map-binding-result",
                                                                onclick: move |_| {
                                                                    bindings.write().push(StatusBinding {
                                                                        point_key: pk.clone(),
                                                                        label: lbl.clone(),
                                                                    });
                                                                    binding_search.set(String::new());
                                                                    binding_search_open.set(false);
                                                                },
                                                                span { class: "site-map-binding-result-label", "{result_label}" }
                                                                span { class: "site-map-binding-result-key", "{key}" }
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

                div { class: "site-map-settings-actions",
                    button {
                        class: "btn btn-primary",
                        onclick: {
                            let mid = mid.clone();
                            let pid = pid.clone();
                            let orig_marker = orig_marker.clone();
                            move |_| {
                                let new_lat = lat.read().parse::<f64>().unwrap_or(orig_marker.lat);
                                let new_lon = lon.read().parse::<f64>().unwrap_or(orig_marker.lon);
                                let new_color = color.read().clone();
                                let new_label = label.read().clone();
                                let new_icon = *icon.read();
                                let addr_val = address.read().clone();
                                let blink = building_link.read().clone();
                                let new_bindings = bindings.read().clone();

                                let updated = MapMarker {
                                    id: mid.clone(),
                                    label: new_label,
                                    lat: new_lat,
                                    lon: new_lon,
                                    address: if addr_val.is_empty() { None } else { Some(addr_val) },
                                    building_nav_id: if blink.is_empty() { None } else { Some(blink) },
                                    color: new_color,
                                    icon: new_icon,
                                    status_bindings: new_bindings,
                                };

                                // Update JS marker
                                remove_marker_js(&mid);
                                add_marker_js(&updated, false);

                                // Update store
                                let mut state = use_context::<AppState>();
                                {
                                    let mut maps = state.site_maps.write();
                                    if let Some(data) = maps.get_mut(&pid) {
                                        if let Some(m) = data.markers.iter_mut().find(|m| m.id == mid) {
                                            *m = updated;
                                        }
                                    }
                                }
                                state.save_layout();
                                on_close.call(());
                            }
                        },
                        "Save"
                    }
                    button {
                        class: "btn",
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// JS bridge helpers
// ----------------------------------------------------------------

fn add_marker_js(marker: &MapMarker, draggable: bool) {
    let popup_base = format!(
        "<div class='site-map-popup'><strong>{}</strong>{}</div>",
        html_escape(&marker.label),
        marker
            .address
            .as_ref()
            .map(|a| format!("<div class='site-map-popup-addr'>{}</div>", html_escape(a)))
            .unwrap_or_default(),
    );

    let building_link = marker
        .building_nav_id
        .as_ref()
        .map(|bid| {
            format!(
                "<div class='site-map-popup-link' data-building='{}'>\u{2192} Go to Building</div>",
                html_escape(bid)
            )
        })
        .unwrap_or_default();

    let icon_shape = match marker.icon {
        MarkerIcon::Circle => "50%",
        MarkerIcon::Pin => "50% 50% 0 50%",
        MarkerIcon::Square => "2px",
        MarkerIcon::Diamond => "50% 0 50% 0",
    };

    let drag_str = if draggable { "true" } else { "false" };

    let js = format!(
        r#"
        (function() {{
            if (!window.__ocMap || !window.__ocMarkers) return;
            var el = document.createElement('div');
            el.style.width = '20px';
            el.style.height = '20px';
            el.style.backgroundColor = '{color}';
            el.style.borderRadius = '{shape}';
            el.style.border = '2px solid white';
            el.style.cursor = 'pointer';
            el.style.boxShadow = '0 2px 6px rgba(0,0,0,0.4)';
            el.style.transition = 'box-shadow 0.3s ease';
            {diamond_transform}
            var popup = new mapboxgl.Popup({{ offset: 15, closeButton: false, maxWidth: '280px' }})
                .setHTML('{popup_html}');
            var marker = new mapboxgl.Marker({{ element: el, draggable: {drag} }})
                .setLngLat([{lon}, {lat}])
                .setPopup(popup)
                .addTo(window.__ocMap);
            marker.on('dragend', function() {{
                var lngLat = marker.getLngLat();
                if (window.__ocDragEnd) window.__ocDragEnd('{id}', lngLat);
            }});
            window.__ocMarkers['{id}'] = {{ marker: marker, popup: popup, baseHtml: '{popup_base}' }};
        }})();
        "#,
        id = marker.id,
        color = marker.color,
        lat = marker.lat,
        lon = marker.lon,
        shape = icon_shape,
        drag = drag_str,
        diamond_transform = if marker.icon == MarkerIcon::Diamond {
            "el.style.transform = 'rotate(45deg)';"
        } else {
            ""
        },
        popup_html = format!("{}{}", popup_base, building_link).replace('\'', "\\'"),
        popup_base = popup_base.replace('\'', "\\'"),
    );
    document::eval(&js);
}

fn remove_marker_js(id: &str) {
    let js = format!(
        r#"
        if (window.__ocMarkers && window.__ocMarkers['{id}']) {{
            window.__ocMarkers['{id}'].marker.remove();
            delete window.__ocMarkers['{id}'];
        }}
        "#,
        id = id,
    );
    document::eval(&js);
}

fn fly_to(lat: f64, lon: f64) {
    let js = format!(
        "if (window.__ocMap) {{ window.__ocMap.flyTo({{ center: [{lon}, {lat}], zoom: 15, essential: true }}); }}"
    );
    document::eval(&js);
}

fn fit_all_markers(state: &AppState, page_id: &str) {
    let maps = state.site_maps.read();
    let Some(data) = maps.get(page_id) else {
        return;
    };
    if data.markers.is_empty() {
        return;
    }
    if data.markers.len() == 1 {
        fly_to(data.markers[0].lat, data.markers[0].lon);
        return;
    }
    let mut min_lat = f64::MAX;
    let mut max_lat = f64::MIN;
    let mut min_lon = f64::MAX;
    let mut max_lon = f64::MIN;
    for m in &data.markers {
        min_lat = min_lat.min(m.lat);
        max_lat = max_lat.max(m.lat);
        min_lon = min_lon.min(m.lon);
        max_lon = max_lon.max(m.lon);
    }
    let js = format!(
        "if (window.__ocMap) {{ window.__ocMap.fitBounds([[{min_lon}, {min_lat}], [{max_lon}, {max_lat}]], {{ padding: {{ top: 200, left: 220, bottom: 60, right: 60 }}, maxZoom: 16 }}); }}"
    );
    document::eval(&js);
}

fn toggle_3d(page_id: &str, state: &mut AppState) {
    let current_pitch = state
        .site_maps
        .read()
        .get(page_id)
        .map(|d| d.map_config.pitch)
        .unwrap_or(0.0);
    let new_pitch = if current_pitch > 10.0 { 0.0 } else { 60.0 };
    let js = format!(
        "if (window.__ocMap) {{ window.__ocMap.easeTo({{ pitch: {new_pitch}, duration: 1000 }}); }}"
    );
    document::eval(&js);
    let mut maps = state.site_maps.write();
    if let Some(data) = maps.get_mut(page_id) {
        data.map_config.pitch = new_pitch;
    }
}

fn save_current_view(page_id: &str) {
    let pid = page_id.to_string();
    spawn(async move {
        let mut eval = document::eval(
            r#"
            if (window.__ocMap) {
                var c = window.__ocMap.getCenter();
                var z = window.__ocMap.getZoom();
                var p = window.__ocMap.getPitch();
                var b = window.__ocMap.getBearing();
                dioxus.send({ lat: c.lat, lng: c.lng, zoom: z, pitch: p, bearing: b });
            } else {
                dioxus.send(null);
            }
            "#,
        );
        if let Ok(val) = eval.recv::<serde_json::Value>().await {
            if val.is_null() {
                return;
            }
            let new_config = MapViewConfig {
                center_lat: val.get("lat").and_then(|v| v.as_f64()).unwrap_or(39.8283),
                center_lon: val.get("lng").and_then(|v| v.as_f64()).unwrap_or(-98.5795),
                zoom: val.get("zoom").and_then(|v| v.as_f64()).unwrap_or(4.0),
                pitch: val.get("pitch").and_then(|v| v.as_f64()).unwrap_or(0.0),
                bearing: val.get("bearing").and_then(|v| v.as_f64()).unwrap_or(0.0),
            };
            let mut state = use_context::<AppState>();
            {
                let mut maps = state.site_maps.write();
                if let Some(data) = maps.get_mut(&pid) {
                    data.map_config = new_config;
                }
            }
            state.save_layout();
        }
    });
}

/// Geocode an address using the Mapbox Geocoding API.
async fn geocode(address: &str, token: &str) -> Option<(f64, f64)> {
    let encoded = address.replace(' ', "%20");
    let url = format!(
        "https://api.mapbox.com/geocoding/v5/mapbox.places/{encoded}.json?access_token={token}&limit=1"
    );
    let js = format!(
        r#"
        fetch('{url}')
            .then(r => r.json())
            .then(data => {{
                if (data.features && data.features.length > 0) {{
                    var coords = data.features[0].center;
                    dioxus.send({{ lon: coords[0], lat: coords[1] }});
                }} else {{
                    dioxus.send(null);
                }}
            }})
            .catch(() => dioxus.send(null));
        "#,
        url = url,
    );
    let mut eval = document::eval(&js);
    match eval.recv::<serde_json::Value>().await {
        Ok(val) if !val.is_null() => {
            let lat = val.get("lat")?.as_f64()?;
            let lon = val.get("lon")?.as_f64()?;
            Some((lat, lon))
        }
        _ => None,
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
