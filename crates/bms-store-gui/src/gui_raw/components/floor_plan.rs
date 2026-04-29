use std::collections::HashMap;

use dioxus::prelude::*;

use crate::config::profile::PointValue;
use crate::gui::components::building_tree::collect_spatial_ancestors;
use crate::gui::state::{
    insert_nav_child, remove_nav_node, update_nav_node, AppState, CanvasSelection, CanvasTool,
    EquipLabelConfig, EquipSymbol, Equipment, LabelPlacement, NavNode, NavNodeKind, PageData,
    SetpointSource, Zone, ZoneLabelConfig,
};
use crate::store::point_store::PointKey;

/// Default canvas coordinate space.
const CANVAS_W: f64 = 1920.0;
const CANVAS_H: f64 = 1080.0;

/// Build nav node label and kind for a zone.
/// `device_list` maps device IDs to display names for the nav label.
fn zone_nav_info(zone: &Zone, device_list: &[(String, String)]) -> (String, NavNodeKind) {
    if let Some(ref dev_id) = zone.device_id {
        let dev_display = device_list
            .iter()
            .find(|(id, _)| id == dev_id)
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| dev_id.clone());
        let mut prefix_parts = Vec::new();
        if !zone.room_number.is_empty() {
            prefix_parts.push(zone.room_number.clone());
        }
        if !zone.label.is_empty() {
            prefix_parts.push(zone.label.clone());
        }
        let label = if prefix_parts.is_empty() {
            dev_display
        } else {
            format!("{} — {}", prefix_parts.join(" "), dev_display)
        };
        (
            label,
            NavNodeKind::Device {
                device_id: dev_id.clone(),
            },
        )
    } else {
        let label = if zone.label.is_empty() {
            "New Zone".into()
        } else {
            zone.label.clone()
        };
        (label, NavNodeKind::Page)
    }
}

/// Polygon centroid for label placement.
fn centroid(pts: &[(f64, f64)]) -> (f64, f64) {
    if pts.is_empty() {
        return (0.0, 0.0);
    }
    let n = pts.len() as f64;
    let sx: f64 = pts.iter().map(|p| p.0).sum();
    let sy: f64 = pts.iter().map(|p| p.1).sum();
    (sx / n, sy / n)
}

/// Format polygon points for SVG `points` attribute.
fn svg_points(pts: &[(f64, f64)]) -> String {
    pts.iter()
        .map(|(x, y)| format!("{x},{y}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Point-in-polygon test (ray casting algorithm).
fn point_in_polygon(px: f64, py: f64, poly: &[(f64, f64)]) -> bool {
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = poly[i];
        let (xj, yj) = poly[j];
        if ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Convert element coordinates (CSS px relative to SVG) to viewBox (canvas) coordinates.
/// Accounts for `preserveAspectRatio="xMidYMid meet"`.
fn elem_to_canvas(
    elem_x: f64,
    elem_y: f64,
    svg_w: f64,
    svg_h: f64,
    vb_x: f64,
    vb_y: f64,
    vb_w: f64,
    vb_h: f64,
) -> (f64, f64) {
    if svg_w <= 0.0 || svg_h <= 0.0 {
        return (elem_x, elem_y);
    }
    let scale = (svg_w / vb_w).min(svg_h / vb_h);
    let rendered_w = vb_w * scale;
    let rendered_h = vb_h * scale;
    let offset_x = (svg_w - rendered_w) / 2.0;
    let offset_y = (svg_h - rendered_h) / 2.0;
    (
        vb_x + (elem_x - offset_x) / scale,
        vb_y + (elem_y - offset_y) / scale,
    )
}

/// Measure the SVG element's bounding rect via JS eval + dioxus.send().
fn measure_svg_rect(mut svg_rect: Signal<(f64, f64, f64, f64)>) {
    spawn(async move {
        let mut eval = document::eval(
            r#"var svg = document.getElementById('floor-svg');
            if (svg) {
                var r = svg.getBoundingClientRect();
                dioxus.send([r.left, r.top, r.width, r.height]);
            } else {
                dioxus.send([0, 0, 0, 0]);
            }"#,
        );
        if let Ok(val) = eval.recv::<serde_json::Value>().await {
            if let Some(arr) = val.as_array() {
                let left = arr[0].as_f64().unwrap_or(0.0);
                let top = arr[1].as_f64().unwrap_or(0.0);
                let width = arr[2].as_f64().unwrap_or(1.0);
                let height = arr[3].as_f64().unwrap_or(1.0);
                svg_rect.set((left, top, width, height));
            }
        }
    });
}

/// Temperature deviation → fill color.
/// Returns an rgba string: blue (cold) → green (comfort) → red (hot).
fn deviation_color(deviation: f64) -> String {
    // Clamp to ±5 range
    let d = deviation.clamp(-5.0, 5.0);
    let t = (d + 5.0) / 10.0; // 0.0 = cold, 0.5 = comfort, 1.0 = hot

    // HSL interpolation: 210° (blue) → 120° (green) → 0° (red)
    let hue = if t <= 0.5 {
        // blue → green
        210.0 - (210.0 - 120.0) * (t / 0.5)
    } else {
        // green → red
        120.0 - 120.0 * ((t - 0.5) / 0.5)
    };

    format!("hsla({hue:.0}, 70%, 50%, 0.25)")
}

/// What the user is currently dragging on the canvas.
#[derive(Debug, Clone, PartialEq)]
enum DragAction {
    None,
    /// Dragging a vertex: (zone_id, vertex_index)
    Vertex(String, usize),
    /// Dragging a whole zone by its body
    ZoneBody(String, f64, f64),
    /// Dragging an equipment symbol: (equip_id, offset_x, offset_y)
    EquipmentMove(String, f64, f64),
}

// ----------------------------------------------------------------
// Main component
// ----------------------------------------------------------------

#[component]
pub fn FloorPlanCanvas(page_id: String) -> Element {
    let mut state = use_context::<AppState>();

    // Canvas-local state
    let mut editing = use_signal(|| false);
    let mut tool = use_signal(|| CanvasTool::Select);
    let mut selection = use_signal(|| CanvasSelection::None);
    let mut next_id = use_signal(|| {
        let pages = state.pages.read();
        let mut max_id = 0u32;
        if let Some(data) = pages.get(&page_id) {
            for z in &data.zones {
                if let Some(n) = z.id.strip_prefix("c-").and_then(|s| s.parse::<u32>().ok()) {
                    max_id = max_id.max(n);
                }
            }
            for e in &data.equipment {
                if let Some(n) = e.id.strip_prefix("c-").and_then(|s| s.parse::<u32>().ok()) {
                    max_id = max_id.max(n);
                }
            }
        }
        max_id + 1
    });

    // ViewBox state (zoom/pan)
    let mut vb_x = use_signal(|| 0.0f64);
    let mut vb_y = use_signal(|| 0.0f64);
    let mut vb_w = use_signal(|| CANVAS_W);
    let mut vb_h = use_signal(|| CANVAS_H);

    // Polygon drawing state
    let mut drawing_pts = use_signal(Vec::<(f64, f64)>::new);
    let mut mouse_canvas = use_signal(|| (0.0f64, 0.0f64));

    // Panning state
    let mut pan_active = use_signal(|| false);
    let mut pan_start_client = use_signal(|| (0.0f64, 0.0f64));
    let mut pan_start_vb = use_signal(|| (0.0f64, 0.0f64));

    // Drag state for vertex/zone editing
    let mut drag_action = use_signal(|| DragAction::None);

    // Tooltip state: (text, client_x, client_y)
    let mut tooltip = use_signal(|| Option::<(String, f64, f64)>::None);

    // SVG bounding rect (left, top, width, height) measured via JS eval
    let svg_rect = use_signal(|| (0.0f64, 0.0f64, 0.0f64, 0.0f64));

    // Measure on mount
    use_effect(move || {
        spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            measure_svg_rect(svg_rect);
        });
    });

    // Ensure page data exists
    {
        let pages = state.pages.read();
        if !pages.contains_key(&page_id) {
            drop(pages);
            state
                .pages
                .write()
                .entry(page_id.clone())
                .or_insert_with(PageData::default);
        }
    }

    let page = state
        .pages
        .read()
        .get(&page_id)
        .cloned()
        .unwrap_or_default();

    let is_editing = *editing.read();
    let current_tool = *tool.read();
    let current_selection = selection.read().clone();
    let sel_for_mousedown = current_selection.clone();
    let sel_for_key = current_selection.clone();
    let draw_pts = drawing_pts.read().clone();
    let (mouse_cx, mouse_cy) = *mouse_canvas.read();

    // Read store version to trigger re-renders on live data changes
    let _store_ver = *state.store_version.read();
    let _node_ver = *state.node_version.read();

    // Build device list from NodeStore (includes both scenario and discovered/accepted devices)
    let mut device_list_sig: Signal<Vec<(String, String)>> = use_signal(Vec::new);
    {
        let ns = state.node_store.clone();
        let _ = use_resource(move || {
            let ns = ns.clone();
            let _nv = _node_ver;
            async move {
                let equips = ns.list_nodes(Some("equip"), None).await;
                let list: Vec<(String, String)> = equips
                    .into_iter()
                    .map(|n| {
                        let label = if n.dis.is_empty() {
                            n.id.clone()
                        } else {
                            n.dis.clone()
                        };
                        (n.id, label)
                    })
                    .collect();
                device_list_sig.set(list);
            }
        });
    }
    let device_list = device_list_sig.read().clone();

    // Build a map of node_id -> display_name for all point nodes (used by zone/equip properties)
    let mut point_names_sig: Signal<HashMap<String, String>> = use_signal(HashMap::new);
    {
        let ns = state.node_store.clone();
        let _ = use_resource(move || {
            let ns = ns.clone();
            let _nv = _node_ver;
            async move {
                let pts = ns.list_nodes(Some("point"), None).await;
                let mut map: HashMap<String, String> = pts
                    .into_iter()
                    .map(|n| (n.id.clone(), if n.dis.is_empty() { n.id } else { n.dis }))
                    .collect();
                let vpts = ns.list_nodes(Some("virtual_point"), None).await;
                for n in vpts {
                    map.insert(n.id.clone(), if n.dis.is_empty() { n.id } else { n.dis });
                }
                point_names_sig.set(map);
            }
        });
    }
    let point_names = point_names_sig.read().clone();

    let mut alloc_id = move || -> String {
        let id = *next_id.read();
        next_id.set(id + 1);
        format!("c-{id}")
    };

    // ---- Background image picker ----
    let pid_bg = page_id.clone();
    let set_background = move |_| {
        let pid = pid_bg.clone();
        spawn(async move {
            let path = tokio::task::spawn_blocking(|| {
                rfd::FileDialog::new()
                    .add_filter("Images", &["png", "jpg", "jpeg", "svg", "webp", "bmp"])
                    .pick_file()
            })
            .await
            .ok()
            .flatten();

            if let Some(p) = path {
                let path_str = p.to_string_lossy().to_string();
                let mut s = use_context::<AppState>();
                let mut pages = s.pages.write();
                if let Some(data) = pages.get_mut(&pid) {
                    data.background = Some(path_str);
                }
            }
        });
    };

    // ---- Coordinate conversion helper ----
    let to_canvas = move |elem_x: f64, elem_y: f64| -> (f64, f64) {
        let (_, _, rw, rh) = *svg_rect.read();
        elem_to_canvas(
            elem_x,
            elem_y,
            rw,
            rh,
            *vb_x.read(),
            *vb_y.read(),
            *vb_w.read(),
            *vb_h.read(),
        )
    };

    let css_scale = move || -> f64 {
        let (_, _, rw, rh) = *svg_rect.read();
        let vw = *vb_w.read();
        let vh = *vb_h.read();
        if rw <= 0.0 || rh <= 0.0 {
            return 1.0;
        }
        (rw / vw).min(rh / vh)
    };

    // ---- Compute zone fill colors (temp deviation or static) ----
    let zone_fills: Vec<(String, String)> = page
        .zones
        .iter()
        .map(|zone| {
            let fill = compute_zone_fill(zone, &state);
            (zone.id.clone(), fill)
        })
        .collect();

    // ---- Mouse handlers ----

    let mut start_pan = move |e: &Event<MouseData>| {
        let client = e.data().client_coordinates();
        pan_active.set(true);
        pan_start_client.set((client.x, client.y));
        pan_start_vb.set((*vb_x.read(), *vb_y.read()));
    };

    let onmousedown = {
        let pid = page_id.clone();
        move |e: Event<MouseData>| {
            if e.data().trigger_button() == Some(dioxus::html::input_data::MouseButton::Auxiliary) {
                start_pan(&e);
                return;
            }

            if !*editing.read() {
                // In view mode, single click on a zone → open device in detail pane
                if e.data().trigger_button() == Some(dioxus::html::input_data::MouseButton::Primary)
                {
                    let elem = e.data().element_coordinates();
                    let (cx, cy) = to_canvas(elem.x, elem.y);
                    let page = state.pages.read().get(&pid).cloned().unwrap_or_default();
                    let mut hit = false;
                    for zone in page.zones.iter().rev() {
                        if point_in_polygon(cx, cy, &zone.points) {
                            if let Some(ref dev_id) = zone.device_id {
                                state.selected_device.set(Some(dev_id.clone()));
                                state.selected_point.set(None);
                                state.detail_open.set(true);
                            }
                            hit = true;
                            break;
                        }
                    }
                    if !hit {
                        start_pan(&e);
                    }
                    return;
                }
                start_pan(&e);
                return;
            }

            let elem = e.data().element_coordinates();
            let (cx, cy) = to_canvas(elem.x, elem.y);
            let active_tool = *tool.read();

            match active_tool {
                CanvasTool::PlaceEquipment => {
                    let new_id = alloc_id();
                    let equip = Equipment {
                        id: new_id.clone(),
                        label: "Equipment".into(),
                        device_id: None,
                        x: cx,
                        y: cy,
                        label_config: EquipLabelConfig::default(),
                        symbol: EquipSymbol::Gear,
                    };
                    let mut pages = state.pages.write();
                    if let Some(data) = pages.get_mut(&pid) {
                        data.equipment.push(equip);
                    }
                    selection.set(CanvasSelection::Equipment(new_id));
                    tool.set(CanvasTool::Select);
                }
                CanvasTool::Pan => {}
                CanvasTool::Select => {
                    let page = state.pages.read().get(&pid).cloned().unwrap_or_default();

                    // Check if clicking on a vertex of the selected zone
                    if let CanvasSelection::Zone(ref sel_id) = sel_for_mousedown {
                        if let Some(zone) = page.zones.iter().find(|z| &z.id == sel_id) {
                            let hit_r = *vb_w.read() * 0.008;
                            for (i, &(vx, vy)) in zone.points.iter().enumerate() {
                                let dist = ((cx - vx).powi(2) + (cy - vy).powi(2)).sqrt();
                                if dist < hit_r {
                                    drag_action.set(DragAction::Vertex(sel_id.clone(), i));
                                    return;
                                }
                            }
                            // Check midpoint handles (add vertex)
                            for i in 0..zone.points.len() {
                                let j = (i + 1) % zone.points.len();
                                let mx = (zone.points[i].0 + zone.points[j].0) / 2.0;
                                let my = (zone.points[i].1 + zone.points[j].1) / 2.0;
                                let dist = ((cx - mx).powi(2) + (cy - my).powi(2)).sqrt();
                                if dist < hit_r {
                                    // Insert new vertex after i
                                    let mut pages = state.pages.write();
                                    if let Some(data) = pages.get_mut(&pid) {
                                        if let Some(z) =
                                            data.zones.iter_mut().find(|z| z.id == *sel_id)
                                        {
                                            z.points.insert(j, (mx, my));
                                            drop(pages);
                                            drag_action.set(DragAction::Vertex(sel_id.clone(), j));
                                            return;
                                        }
                                    }
                                }
                            }
                            // Check if inside selected zone body → drag whole zone
                            if point_in_polygon(cx, cy, &zone.points) {
                                let (ccx, ccy) = centroid(&zone.points);
                                drag_action.set(DragAction::ZoneBody(
                                    sel_id.clone(),
                                    cx - ccx,
                                    cy - ccy,
                                ));
                                return;
                            }
                        }
                    }

                    // Check if clicking on the selected equipment → drag it
                    if let CanvasSelection::Equipment(ref sel_id) = sel_for_mousedown {
                        if let Some(eq) = page.equipment.iter().find(|e| &e.id == sel_id) {
                            let dist = ((cx - eq.x).powi(2) + (cy - eq.y).powi(2)).sqrt();
                            if dist < 30.0 {
                                drag_action.set(DragAction::EquipmentMove(
                                    sel_id.clone(),
                                    cx - eq.x,
                                    cy - eq.y,
                                ));
                                return;
                            }
                        }
                    }

                    // Normal hit-test for selection
                    let mut found = false;

                    for eq in page.equipment.iter().rev() {
                        let dist = ((cx - eq.x).powi(2) + (cy - eq.y).powi(2)).sqrt();
                        if dist < 30.0 {
                            selection.set(CanvasSelection::Equipment(eq.id.clone()));
                            found = true;
                            break;
                        }
                    }

                    if !found {
                        for zone in page.zones.iter().rev() {
                            if point_in_polygon(cx, cy, &zone.points) {
                                selection.set(CanvasSelection::Zone(zone.id.clone()));
                                found = true;
                                break;
                            }
                        }
                    }

                    if !found {
                        selection.set(CanvasSelection::None);
                    }

                    // Remeasure SVG after selection change (panel may appear/disappear)
                    spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        measure_svg_rect(svg_rect);
                    });
                }
                _ => {}
            }
        }
    };

    let onmousemove = {
        let pid = page_id.clone();
        move |e: Event<MouseData>| {
            let client = e.data().client_coordinates();
            let elem = e.data().element_coordinates();

            if *pan_active.read() {
                let scale = css_scale();
                let (sc_x, sc_y) = *pan_start_client.read();
                let (sv_x, sv_y) = *pan_start_vb.read();
                vb_x.set(sv_x - (client.x - sc_x) / scale);
                vb_y.set(sv_y - (client.y - sc_y) / scale);
                return;
            }

            let (cx, cy) = to_canvas(elem.x, elem.y);

            // Handle vertex/zone dragging
            match &*drag_action.read() {
                DragAction::Vertex(zone_id, idx) => {
                    let zid = zone_id.clone();
                    let i = *idx;
                    let mut pages = state.pages.write();
                    if let Some(data) = pages.get_mut(&pid) {
                        if let Some(z) = data.zones.iter_mut().find(|z| z.id == zid) {
                            if i < z.points.len() {
                                z.points[i] = (cx, cy);
                            }
                        }
                    }
                    return;
                }
                DragAction::ZoneBody(zone_id, off_x, off_y) => {
                    let zid = zone_id.clone();
                    let ox = *off_x;
                    let oy = *off_y;
                    let mut pages = state.pages.write();
                    if let Some(data) = pages.get_mut(&pid) {
                        if let Some(z) = data.zones.iter_mut().find(|z| z.id == zid) {
                            let (old_cx, old_cy) = centroid(&z.points);
                            let new_cx = cx - ox;
                            let new_cy = cy - oy;
                            let dx = new_cx - old_cx;
                            let dy = new_cy - old_cy;
                            for pt in z.points.iter_mut() {
                                pt.0 += dx;
                                pt.1 += dy;
                            }
                        }
                    }
                    return;
                }
                DragAction::EquipmentMove(equip_id, off_x, off_y) => {
                    let eid = equip_id.clone();
                    let ox = *off_x;
                    let oy = *off_y;
                    let mut pages = state.pages.write();
                    if let Some(data) = pages.get_mut(&pid) {
                        if let Some(eq) = data.equipment.iter_mut().find(|e| e.id == eid) {
                            eq.x = cx - ox;
                            eq.y = cy - oy;
                        }
                    }
                    return;
                }
                DragAction::None => {}
            }

            mouse_canvas.set((cx, cy));
        }
    };

    let onmouseup = move |_e: Event<MouseData>| {
        pan_active.set(false);
        drag_action.set(DragAction::None);
    };

    let onmouseenter = move |_e: Event<MouseData>| {
        measure_svg_rect(svg_rect);
    };

    // Click for polygon drawing
    let pid_click = page_id.clone();
    let mut state_click = state.clone();
    let device_list_click = device_list.clone();
    let onclick = move |e: Event<MouseData>| {
        if !*editing.read() || *tool.read() != CanvasTool::DrawZone {
            return;
        }
        if *pan_active.read() {
            return;
        }
        let elem = e.data().element_coordinates();
        let (cx, cy) = to_canvas(elem.x, elem.y);

        let mut pts = drawing_pts.write();

        // Check if clicking near the first point to close
        if pts.len() >= 3 {
            let (fx, fy) = pts[0];
            let dist = ((cx - fx).powi(2) + (cy - fy).powi(2)).sqrt();
            let close_threshold = *vb_w.read() * 0.015;
            if dist < close_threshold {
                let final_pts: Vec<(f64, f64)> = pts.clone();
                drop(pts);

                let new_id = alloc_id();
                let nav_id = state_click.alloc_node_id();
                let zone = Zone {
                    id: new_id.clone(),
                    label: "New Zone".into(),
                    room_number: String::new(),
                    device_id: None,
                    points: final_pts,
                    color: "#d4714e33".into(),
                    temp_point_id: None,
                    setpoint_source: None,
                    label_config: ZoneLabelConfig::default(),
                    nav_node_id: Some(nav_id.clone()),
                };
                let (nav_label, nav_kind) = zone_nav_info(&zone, &device_list_click);
                let nav_node = NavNode {
                    id: nav_id,
                    label: nav_label,
                    kind: nav_kind,
                    children: Vec::new(),
                };
                let mut tree = state_click.nav_tree.write();
                insert_nav_child(&mut tree, &pid_click, nav_node);
                drop(tree);
                let mut pages = state_click.pages.write();
                if let Some(data) = pages.get_mut(&pid_click) {
                    data.zones.push(zone);
                }
                drawing_pts.write().clear();
                selection.set(CanvasSelection::Zone(new_id));
                tool.set(CanvasTool::Select);
                return;
            }
        }

        pts.push((cx, cy));
    };

    // Double-click to close polygon
    let pid_dbl = page_id.clone();
    let mut state_dbl = state.clone();
    let device_list_dbl = device_list.clone();
    let ondoubleclick = move |_e: Event<MouseData>| {
        if !*editing.read() || *tool.read() != CanvasTool::DrawZone {
            return;
        }
        let pts = drawing_pts.read().clone();
        if pts.len() >= 3 {
            let new_id = alloc_id();
            let nav_id = state_dbl.alloc_node_id();
            let zone = Zone {
                id: new_id.clone(),
                label: "New Zone".into(),
                room_number: String::new(),
                device_id: None,
                points: pts,
                color: "#d4714e33".into(),
                temp_point_id: None,
                setpoint_source: None,
                label_config: ZoneLabelConfig::default(),
                nav_node_id: Some(nav_id.clone()),
            };
            let (nav_label, nav_kind) = zone_nav_info(&zone, &device_list_dbl);
            let nav_node = NavNode {
                id: nav_id,
                label: nav_label,
                kind: nav_kind,
                children: Vec::new(),
            };
            let mut tree = state_dbl.nav_tree.write();
            insert_nav_child(&mut tree, &pid_dbl, nav_node);
            drop(tree);
            let mut pages = state_dbl.pages.write();
            if let Some(data) = pages.get_mut(&pid_dbl) {
                data.zones.push(zone);
            }
            drawing_pts.write().clear();
            selection.set(CanvasSelection::Zone(new_id));
            tool.set(CanvasTool::Select);
        }
    };

    // Wheel → zoom
    let onwheel = move |e: Event<WheelData>| {
        e.prevent_default();
        let delta_y = e.data().delta().strip_units().y;
        let factor = if delta_y > 0.0 { 1.1 } else { 1.0 / 1.1 };

        let elem = e.data().element_coordinates();
        let (mx, my) = to_canvas(elem.x, elem.y);

        let old_w = *vb_w.read();
        let old_h = *vb_h.read();
        let old_x = *vb_x.read();
        let old_y = *vb_y.read();

        vb_w.set(old_w * factor);
        vb_h.set(old_h * factor);
        vb_x.set(mx - (mx - old_x) * factor);
        vb_y.set(my - (my - old_y) * factor);
    };

    // Keyboard: Escape / Delete
    let pid_key = page_id.clone();
    let onkeydown = move |e: Event<KeyboardData>| {
        match e.data().key() {
            Key::Escape => {
                drawing_pts.write().clear();
                let ct = *tool.read();
                if ct == CanvasTool::DrawZone || ct == CanvasTool::PlaceEquipment {
                    tool.set(CanvasTool::Select);
                }
            }
            Key::Delete => {
                // Delete selected vertex or whole zone/equipment
                if !*editing.read() {
                    return;
                }
                match &sel_for_key {
                    CanvasSelection::Zone(zid) => {
                        let zid = zid.clone();
                        let mut pages = state.pages.write();
                        if let Some(data) = pages.get_mut(&pid_key) {
                            // Capture device_id before removing zone
                            let device_id = data
                                .zones
                                .iter()
                                .find(|z| z.id == zid)
                                .and_then(|z| z.device_id.clone());
                            // Remove nav node for this zone
                            let nav_id = data
                                .zones
                                .iter()
                                .find(|z| z.id == zid)
                                .and_then(|z| z.nav_node_id.clone());
                            data.zones.retain(|z| z.id != zid);
                            drop(pages);
                            if let Some(nid) = nav_id {
                                let mut tree = state.nav_tree.write();
                                remove_nav_node(&mut tree, &nid);
                            }
                            // Clean up spatial refs on the bound device and remove the space node
                            let space_id = format!("space-{}-{}", pid_key, zid);
                            spawn(async move {
                                let st = use_context::<AppState>();
                                let ns = st.node_store.clone();
                                // Clear all spatial refs from bound device
                                if let Some(ref dev_id) = device_id {
                                    let _ = ns.remove_ref(dev_id, "spaceRef").await;
                                    let _ = ns.remove_ref(dev_id, "floorRef").await;
                                    let _ = ns.remove_ref(dev_id, "buildingRef").await;
                                    let _ = ns.remove_ref(dev_id, "siteRef").await;
                                }
                                // Also clear any other equipment pointing at this space
                                let stale = ns.find_by_ref("spaceRef", &space_id).await;
                                for node in &stale {
                                    let _ = ns.remove_ref(&node.id, "spaceRef").await;
                                }
                                // Delete the synthetic space node
                                let _ = ns.delete_node(&space_id).await;
                            });
                        }
                        selection.set(CanvasSelection::None);
                    }
                    CanvasSelection::Equipment(eid) => {
                        let eid = eid.clone();
                        let mut pages = state.pages.write();
                        if let Some(data) = pages.get_mut(&pid_key) {
                            data.equipment.retain(|e| e.id != eid);
                        }
                        selection.set(CanvasSelection::None);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    };

    let fit_view = move |_| {
        vb_x.set(0.0);
        vb_y.set(0.0);
        vb_w.set(CANVAS_W);
        vb_h.set(CANVAS_H);
    };

    // Properties panel data
    let selected_zone: Option<Zone> = match &current_selection {
        CanvasSelection::Zone(id) => page.zones.iter().find(|z| &z.id == id).cloned(),
        _ => None,
    };
    let selected_equip: Option<Equipment> = match &current_selection {
        CanvasSelection::Equipment(id) => page.equipment.iter().find(|e| &e.id == id).cloned(),
        _ => None,
    };
    let has_props = is_editing && (selected_zone.is_some() || selected_equip.is_some());
    let pid_props = page_id.clone();

    let vx = *vb_x.read();
    let vy = *vb_y.read();
    let vw = *vb_w.read();
    let vh = *vb_h.read();
    let view_box = format!("{vx} {vy} {vw} {vh}");

    let svg_class = match (is_editing, current_tool) {
        (true, CanvasTool::DrawZone) | (true, CanvasTool::PlaceEquipment) => {
            "floor-svg cursor-crosshair"
        }
        (_, CanvasTool::Pan) | (false, _) => "floor-svg cursor-grab",
        _ => "floor-svg",
    };

    rsx! {
        div {
            class: "floor-plan-editor",
            tabindex: "0",
            onkeydown: onkeydown,

            // Canvas toolbar
            div { class: "canvas-toolbar",
                button {
                    class: if is_editing { "canvas-tool-btn edit-toggle active" } else { "canvas-tool-btn edit-toggle" },
                    title: if is_editing { "Exit Edit Mode" } else { "Enter Edit Mode" },
                    onclick: move |_| {
                        let entering = !*editing.read();
                        editing.set(entering);
                        if !entering {
                            tool.set(CanvasTool::Select);
                            drawing_pts.write().clear();
                            selection.set(CanvasSelection::None);
                            drag_action.set(DragAction::None);
                            state.save_layout();
                        }
                        spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                            measure_svg_rect(svg_rect);
                        });
                    },
                    svg { width: "16", height: "16", view_box: "0 0 24 24", fill: "currentColor",
                        path { d: "M3 17.25V21h3.75L17.81 9.94l-3.75-3.75L3 17.25zM20.71 7.04a1 1 0 000-1.41l-2.34-2.34a1 1 0 00-1.41 0l-1.83 1.83 3.75 3.75 1.83-1.83z" }
                    }
                    if is_editing {
                        span { class: "edit-badge", "Editing" }
                    }
                }

                if is_editing {
                    span { class: "canvas-toolbar-divider" }

                    button {
                        class: if current_tool == CanvasTool::Select { "canvas-tool-btn active" } else { "canvas-tool-btn" },
                        title: "Select",
                        onclick: move |_| { tool.set(CanvasTool::Select); drawing_pts.write().clear(); },
                        svg { width: "16", height: "16", view_box: "0 0 24 24", fill: "currentColor",
                            path { d: "M7 2l12 11.2-5.8.5 3.3 7.3-2.2 1-3.2-7.4L7 18.5V2z" }
                        }
                    }

                    button {
                        class: if current_tool == CanvasTool::DrawZone { "canvas-tool-btn active" } else { "canvas-tool-btn" },
                        title: "Draw Zone",
                        onclick: move |_| { tool.set(CanvasTool::DrawZone); selection.set(CanvasSelection::None); },
                        svg { width: "16", height: "16", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2",
                            polygon { points: "12,2 22,8.5 22,15.5 12,22 2,15.5 2,8.5" }
                        }
                    }

                    button {
                        class: if current_tool == CanvasTool::PlaceEquipment { "canvas-tool-btn active" } else { "canvas-tool-btn" },
                        title: "Place Equipment",
                        onclick: move |_| { tool.set(CanvasTool::PlaceEquipment); selection.set(CanvasSelection::None); drawing_pts.write().clear(); },
                        svg { width: "16", height: "16", view_box: "0 0 24 24", fill: "currentColor",
                            path { d: "M19.14 12.94c.04-.3.06-.61.06-.94 0-.32-.02-.64-.07-.94l2.03-1.58a.49.49 0 00.12-.61l-1.92-3.32a.49.49 0 00-.59-.22l-2.39.96c-.5-.38-1.03-.7-1.62-.94l-.36-2.54a.484.484 0 00-.48-.41h-3.84c-.24 0-.43.17-.47.41l-.36 2.54c-.59.24-1.13.57-1.62.94l-2.39-.96a.49.49 0 00-.59.22L2.74 8.87c-.12.21-.08.47.12.61l2.03 1.58c-.05.3-.07.62-.07.94s.02.64.07.94l-2.03 1.58a.49.49 0 00-.12.61l1.92 3.32c.12.22.37.29.59.22l2.39-.96c.5.38 1.03.7 1.62.94l.36 2.54c.05.24.24.41.48.41h3.84c.24 0 .44-.17.47-.41l.36-2.54c.59-.24 1.13-.56 1.62-.94l2.39.96c.22.08.47 0 .59-.22l1.92-3.32c.12-.22.07-.47-.12-.61l-2.01-1.58zM12 15.6A3.6 3.6 0 1112 8.4a3.6 3.6 0 010 7.2z" }
                        }
                    }

                    span { class: "canvas-toolbar-divider" }

                    button {
                        class: "canvas-tool-btn",
                        title: "Set Background Image",
                        onclick: set_background,
                        svg { width: "16", height: "16", view_box: "0 0 24 24", fill: "currentColor",
                            path { d: "M21 19V5c0-1.1-.9-2-2-2H5c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2zM8.5 13.5l2.5 3.01L14.5 12l4.5 6H5l3.5-4.5z" }
                        }
                    }
                }


                button {
                    class: "canvas-tool-btn",
                    title: "Fit to View",
                    onclick: fit_view,
                    svg { width: "16", height: "16", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2",
                        rect { x: "3", y: "3", width: "18", height: "18", rx: "2" }
                        polyline { points: "9,3 9,9 3,9" }
                        polyline { points: "15,3 15,9 21,9" }
                        polyline { points: "9,21 9,15 3,15" }
                        polyline { points: "15,21 15,15 21,15" }
                    }
                }

                if is_editing {
                    span { class: "canvas-tool-hint",
                        match current_tool {
                            CanvasTool::Select => "Click zone to select · Drag vertices to edit · Del to remove",
                            CanvasTool::DrawZone => "Click to add points · Double-click or click start to close · Esc to cancel",
                            CanvasTool::PlaceEquipment => "Click to place · Esc to cancel",
                            CanvasTool::Pan => "Drag to pan · Scroll to zoom",
                        }
                    }
                } else {
                    span { class: "canvas-tool-hint", "Scroll to zoom · Drag to pan · Click Edit to modify" }
                }
            }

            // Canvas body
            div { class: "canvas-body",
                svg {
                    id: "floor-svg",
                    class: svg_class,
                    view_box: "{view_box}",
                    "preserveAspectRatio": "xMidYMid meet",
                    onmousedown: onmousedown,
                    onmousemove: onmousemove,
                    onmouseup: onmouseup,
                    onmouseenter: onmouseenter,
                    onclick: onclick,
                    ondoubleclick: ondoubleclick,
                    onwheel: onwheel,

                    // Background image
                    if let Some(ref bg) = page.background {
                        image {
                            href: "{bg}",
                            x: "0",
                            y: "0",
                            width: "{CANVAS_W}",
                            height: "{CANVAS_H}",
                            "preserveAspectRatio": "xMidYMid meet",
                            style: "pointer-events:none;",
                        }
                    }

                    // Completed zones
                    for zone in &page.zones {
                        {
                            let zid = zone.id.clone();
                            let is_sel = matches!(&current_selection, CanvasSelection::Zone(id) if id == &zone.id);
                            let pts_str = svg_points(&zone.points);
                            let (lx, ly) = centroid(&zone.points);
                            let stroke_w = if is_sel { "4" } else { "2" };
                            let stroke_color = if is_sel { "#d4714e" } else { "#c04a22" };
                            let cfg = &zone.label_config;
                            let mut label_parts: Vec<String> = Vec::new();
                            if cfg.show_room_number && !zone.room_number.is_empty() {
                                label_parts.push(zone.room_number.clone());
                            }
                            if cfg.show_label && !zone.label.is_empty() {
                                label_parts.push(zone.label.clone());
                            }
                            let label_text = label_parts.join(" - ");
                            let temp_text = if cfg.show_temp {
                                zone_temp_reading(zone, &state)
                                    .map(|t| format!("{t:.1}°"))
                                    .unwrap_or_default()
                            } else {
                                String::new()
                            };
                            let font_sz = cfg.font_size;
                            let font_color = cfg.font_color.clone();
                            let fill = zone_fills
                                .iter()
                                .find(|(id, _)| id == &zone.id)
                                .map(|(_, f)| f.clone())
                                .unwrap_or_else(|| zone.color.clone());

                            // Build tooltip text from tooltip flags
                            let mut tooltip_parts: Vec<String> = Vec::new();
                            if cfg.tooltip_room_number && !zone.room_number.is_empty() {
                                tooltip_parts.push(zone.room_number.clone());
                            }
                            if cfg.tooltip_label && !zone.label.is_empty() {
                                tooltip_parts.push(zone.label.clone());
                            }
                            if cfg.tooltip_temp {
                                if let Some(t) = zone_temp_reading(zone, &state) {
                                    tooltip_parts.push(format!("{t:.1}°"));
                                }
                            }
                            let tooltip_text = tooltip_parts.join(" — ");

                            rsx! {
                                polygon {
                                    key: "{zid}-poly",
                                    points: "{pts_str}",
                                    fill: "{fill}",
                                    stroke: stroke_color,
                                    stroke_width: stroke_w,
                                    style: "pointer-events:none;",
                                }
                                // Invisible hover overlay for tooltips and click (view mode only).
                                if !is_editing && (!tooltip_text.is_empty() || zone.device_id.is_some()) {
                                    {
                                        let dev_click = zone.device_id.clone();
                                        let tt = tooltip_text.clone();
                                        rsx! {
                                            polygon {
                                                key: "{zid}-hover",
                                                points: "{pts_str}",
                                                fill: "rgba(0,0,0,0)",
                                                stroke: "none",
                                                style: "pointer-events:all; cursor:pointer;",
                                                onmouseenter: move |e: Event<MouseData>| {
                                                    if !tt.is_empty() {
                                                        let client = e.data().client_coordinates();
                                                        tooltip.set(Some((tt.clone(), client.x, client.y)));
                                                    }
                                                },
                                                onmousemove: move |e: Event<MouseData>| {
                                                    let cur = tooltip.read().clone();
                                                    if let Some((text, _, _)) = cur {
                                                        let client = e.data().client_coordinates();
                                                        tooltip.set(Some((text, client.x, client.y)));
                                                    }
                                                },
                                                onmouseleave: move |_| {
                                                    tooltip.set(None);
                                                },
                                                onmousedown: move |e: Event<MouseData>| {
                                                    e.stop_propagation();
                                                    tooltip.set(None);
                                                    if e.data().trigger_button() == Some(dioxus::html::input_data::MouseButton::Primary) {
                                                        if let Some(ref dev_id) = dev_click {
                                                            state.selected_device.set(Some(dev_id.clone()));
                                                            state.selected_point.set(None);
                                                            state.detail_open.set(true);
                                                        }
                                                    }
                                                },
                                            }
                                        }
                                    }
                                }
                                if !label_text.is_empty() || !temp_text.is_empty() {
                                    {
                                        // Offset lines vertically if showing both
                                        let has_both = !label_text.is_empty() && !temp_text.is_empty();
                                        let label_y = if has_both { ly - font_sz * 0.6 } else { ly };
                                        let temp_y = if has_both { ly + font_sz * 0.6 } else { ly };
                                        rsx! {
                                            if !label_text.is_empty() {
                                                text {
                                                    key: "{zid}-label",
                                                    x: "{lx}",
                                                    y: "{label_y}",
                                                    text_anchor: "middle",
                                                    dominant_baseline: "central",
                                                    fill: "{font_color}",
                                                    font_size: "{font_sz}",
                                                    style: "pointer-events:none;",
                                                    "{label_text}"
                                                }
                                            }
                                            if !temp_text.is_empty() {
                                                text {
                                                    key: "{zid}-temp",
                                                    x: "{lx}",
                                                    y: "{temp_y}",
                                                    text_anchor: "middle",
                                                    dominant_baseline: "central",
                                                    fill: "{font_color}",
                                                    font_size: "{font_sz * 0.85}",
                                                    font_weight: "bold",
                                                    style: "pointer-events:none;",
                                                    "{temp_text}"
                                                }
                                            }
                                        }
                                    }
                                }

                                // Vertex handles (when selected in edit mode)
                                if is_sel && is_editing && current_tool == CanvasTool::Select {
                                    // Edge midpoint handles (add vertex)
                                    for i in 0..zone.points.len() {
                                        {
                                            let j = (i + 1) % zone.points.len();
                                            let mx = (zone.points[i].0 + zone.points[j].0) / 2.0;
                                            let my = (zone.points[i].1 + zone.points[j].1) / 2.0;
                                            rsx! {
                                                circle {
                                                    key: "{zid}-mid-{i}",
                                                    cx: "{mx}",
                                                    cy: "{my}",
                                                    r: "6",
                                                    fill: "var(--bg-surface)",
                                                    stroke: "var(--accent)",
                                                    stroke_width: "1.5",
                                                    stroke_dasharray: "3,2",
                                                    style: "pointer-events:none; opacity:0.7;",
                                                }
                                            }
                                        }
                                    }
                                    // Vertex handles
                                    for (vi, vpt) in zone.points.iter().enumerate() {
                                        circle {
                                            key: "{zid}-vtx-{vi}",
                                            cx: "{vpt.0}",
                                            cy: "{vpt.1}",
                                            r: "8",
                                            fill: "var(--accent)",
                                            stroke: "white",
                                            stroke_width: "2",
                                            style: "pointer-events:none; cursor:move;",
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Equipment symbols
                    for equip in &page.equipment {
                        {
                            let eid = equip.id.clone();
                            let is_sel = matches!(&current_selection, CanvasSelection::Equipment(id) if id == &equip.id);
                            let eq_x = equip.x;
                            let eq_y = equip.y;
                            let circle_r = if is_sel { "22" } else { "18" };
                            let circle_fill = if is_sel { "var(--accent-subtle)" } else { "var(--bg-surface)" };
                            let circle_stroke = if is_sel { "var(--accent)" } else { "var(--border-light)" };
                            let symbol_path = equip_symbol_path(&equip.symbol);
                            let ecfg = &equip.label_config;
                            let font_color = ecfg.font_color.clone();

                            // Label position based on placement
                            let (lx, ly, anchor, baseline) = match ecfg.placement {
                                LabelPlacement::Bottom => (0.0, 32.0, "middle", "auto"),
                                LabelPlacement::Top => (0.0, -26.0, "middle", "auto"),
                                LabelPlacement::Left => (-28.0, 0.0, "end", "central"),
                                LabelPlacement::Right => (28.0, 0.0, "start", "central"),
                            };

                            // Build tooltip
                            let eq_tooltip = if ecfg.tooltip {
                                equip.label.clone()
                            } else {
                                String::new()
                            };

                            rsx! {
                                g {
                                    key: "{eid}",
                                    transform: "translate({eq_x},{eq_y})",
                                    style: "pointer-events:none;",
                                    circle {
                                        r: circle_r,
                                        fill: circle_fill,
                                        stroke: circle_stroke,
                                        stroke_width: "2",
                                    }
                                    path {
                                        d: symbol_path,
                                        fill: "var(--text-secondary)",
                                    }
                                    if ecfg.show_label {
                                        text {
                                            x: "{lx}",
                                            y: "{ly}",
                                            text_anchor: anchor,
                                            dominant_baseline: baseline,
                                            fill: "{font_color}",
                                            font_size: "16",
                                            style: "pointer-events:none;",
                                            "{equip.label}"
                                        }
                                    }
                                }
                                // Tooltip hover overlay (view mode)
                                if !is_editing && !eq_tooltip.is_empty() {
                                    {
                                        let dev_click = equip.device_id.clone();
                                        let tt = eq_tooltip.clone();
                                        rsx! {
                                            circle {
                                                key: "{eid}-hover",
                                                cx: "{eq_x}",
                                                cy: "{eq_y}",
                                                r: "20",
                                                fill: "rgba(0,0,0,0)",
                                                style: "pointer-events:all; cursor:pointer;",
                                                onmouseenter: move |e: Event<MouseData>| {
                                                    let client = e.data().client_coordinates();
                                                    tooltip.set(Some((tt.clone(), client.x, client.y)));
                                                },
                                                onmousemove: move |e: Event<MouseData>| {
                                                    let cur = tooltip.read().clone();
                                                    if let Some((text, _, _)) = cur {
                                                        let client = e.data().client_coordinates();
                                                        tooltip.set(Some((text, client.x, client.y)));
                                                    }
                                                },
                                                onmouseleave: move |_| { tooltip.set(None); },
                                                onmousedown: move |e: Event<MouseData>| {
                                                    e.stop_propagation();
                                                    tooltip.set(None);
                                                    if e.data().trigger_button() == Some(dioxus::html::input_data::MouseButton::Primary) {
                                                        if let Some(ref dev_id) = dev_click {
                                                            state.selected_device.set(Some(dev_id.clone()));
                                                            state.selected_point.set(None);
                                                            state.detail_open.set(true);
                                                        }
                                                    }
                                                },
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Polygon drawing preview
                    if !draw_pts.is_empty() {
                        polyline {
                            points: "{svg_points(&draw_pts)}",
                            fill: "none",
                            stroke: "var(--accent)",
                            stroke_width: "3",
                            stroke_dasharray: "8,4",
                            style: "pointer-events:none;",
                        }
                        line {
                            x1: "{draw_pts.last().unwrap().0}",
                            y1: "{draw_pts.last().unwrap().1}",
                            x2: "{mouse_cx}",
                            y2: "{mouse_cy}",
                            stroke: "var(--accent)",
                            stroke_width: "2",
                            stroke_dasharray: "6,3",
                            style: "pointer-events:none;",
                        }
                        for (i, pt) in draw_pts.iter().enumerate() {
                            circle {
                                key: "draw-pt-{i}",
                                cx: "{pt.0}",
                                cy: "{pt.1}",
                                r: if i == 0 { "8" } else { "5" },
                                fill: if i == 0 { "var(--accent)" } else { "var(--accent-dim)" },
                                stroke: "white",
                                stroke_width: "2",
                                style: "pointer-events:none;",
                            }
                        }
                    }

                    // Empty state text
                    if page.background.is_none() && page.zones.is_empty() && page.equipment.is_empty() && draw_pts.is_empty() {
                        text {
                            x: "{CANVAS_W / 2.0}",
                            y: "{CANVAS_H / 2.0 - 20.0}",
                            text_anchor: "middle",
                            fill: "var(--text-muted)",
                            font_size: "32",
                            "Set a background floor plan image to get started."
                        }
                        text {
                            x: "{CANVAS_W / 2.0}",
                            y: "{CANVAS_H / 2.0 + 20.0}",
                            text_anchor: "middle",
                            fill: "var(--text-muted)",
                            font_size: "22",
                            "Click Edit, then use the image button to load a floor plan."
                        }
                    }
                }

                // Properties panel
                if has_props {
                    div {
                        class: "canvas-properties",
                        // Stop keyboard events from bubbling to the canvas handler
                        // so typing in inputs doesn't trigger delete/escape actions
                        onkeydown: move |e: Event<KeyboardData>| {
                            e.stop_propagation();
                        },
                        onmounted: move |_| {
                            spawn(async move {
                                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                                measure_svg_rect(svg_rect);
                            });
                        },
                        if let Some(zone) = selected_zone {
                            ZoneProperties {
                                key: "{zone.id}",
                                page_id: pid_props.clone(),
                                zone_id: zone.id.clone(),
                                device_list: device_list.clone(),
                                point_names: point_names.clone(),
                            }
                        }
                        if let Some(equip) = selected_equip {
                            EquipmentProperties {
                                key: "{equip.id}",
                                page_id: pid_props.clone(),
                                equip_id: equip.id.clone(),
                                device_list: device_list.clone(),
                            }
                        }
                    }
                }

                // Floating tooltip
                if let Some((ref tt_text, tt_x, tt_y)) = *tooltip.read() {
                    div {
                        class: "canvas-tooltip",
                        style: "left: {tt_x + 12.0}px; top: {tt_y + 12.0}px;",
                        "{tt_text}"
                    }
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Zone fill color computation
// ----------------------------------------------------------------

fn compute_zone_fill(zone: &Zone, state: &AppState) -> String {
    let device_id = match &zone.device_id {
        Some(d) => d,
        None => return zone.color.clone(),
    };
    let temp_pid = match &zone.temp_point_id {
        Some(p) => p,
        None => return zone.color.clone(),
    };
    let setpoint_source = match &zone.setpoint_source {
        Some(s) => s,
        None => return zone.color.clone(),
    };

    let temp_key = PointKey {
        device_instance_id: device_id.clone(),
        point_id: temp_pid.clone(),
    };
    let temp_val = state
        .store
        .get(&temp_key)
        .and_then(|tv| point_value_to_f64(&tv.value));

    let sp_val = match setpoint_source {
        SetpointSource::Static(v) => Some(*v),
        SetpointSource::Point(pid) => {
            let sp_key = PointKey {
                device_instance_id: device_id.clone(),
                point_id: pid.clone(),
            };
            state
                .store
                .get(&sp_key)
                .and_then(|sv| point_value_to_f64(&sv.value))
        }
    };

    match (temp_val, sp_val) {
        (Some(t), Some(s)) => deviation_color(t - s),
        _ => zone.color.clone(),
    }
}

/// Get the current temperature reading for a zone, if configured.
fn zone_temp_reading(zone: &Zone, state: &AppState) -> Option<f64> {
    let device_id = zone.device_id.as_ref()?;
    let temp_pid = zone.temp_point_id.as_ref()?;
    let key = PointKey {
        device_instance_id: device_id.clone(),
        point_id: temp_pid.clone(),
    };
    state
        .store
        .get(&key)
        .and_then(|tv| point_value_to_f64(&tv.value))
}

fn point_value_to_f64(v: &PointValue) -> Option<f64> {
    match v {
        PointValue::Float(f) => Some(*f),
        PointValue::Integer(i) => Some(*i as f64),
        PointValue::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
    }
}

// ----------------------------------------------------------------
// Properties panels
// ----------------------------------------------------------------

#[component]
fn ZoneProperties(
    page_id: String,
    zone_id: String,
    device_list: Vec<(String, String)>,
    point_names: HashMap<String, String>,
) -> Element {
    let mut state = use_context::<AppState>();

    // Read zone directly from the pages signal for freshest data
    let zone = {
        let pages = state.pages.read();
        pages
            .get(&page_id)
            .and_then(|p| p.zones.iter().find(|z| z.id == zone_id).cloned())
    };
    let zone = match zone {
        Some(z) => z,
        None => return rsx! {},
    };

    let zid = zone_id.clone();
    let pid = page_id.clone();
    let device_list_for_nav = device_list.to_vec();
    let ns_for_update = state.node_store.clone();

    let update_zone = move |f: Box<dyn FnOnce(&mut Zone)>| {
        let mut pages = state.pages.write();
        if let Some(data) = pages.get_mut(&pid) {
            if let Some(z) = data.zones.iter_mut().find(|z| z.id == zid) {
                f(z);
                // Sync nav node
                if let Some(ref nav_id) = z.nav_node_id {
                    let (label, kind) = zone_nav_info(z, &device_list_for_nav);
                    let nid = nav_id.clone();
                    // Also sync the NodeStore space node display name
                    let space_id = format!("space-{}-{}", pid, z.id);
                    let new_label = z.label.clone();
                    let ns = ns_for_update.clone();
                    spawn(async move {
                        // Update space node dis if it exists
                        if ns.get_node(&space_id).await.is_ok() {
                            let _ = ns.update_dis(&space_id, &new_label).await;
                        }
                    });
                    drop(pages);
                    let mut tree = state.nav_tree.write();
                    update_nav_node(&mut tree, &nid, label, kind);
                    return;
                }
            }
        }
    };

    // Get point IDs for the selected device (from its profile)
    // Build point list from PointStore with display names from point_names map
    let point_list: Vec<(String, String)> = if let Some(ref dev_id) = zone.device_id {
        let mut pts: Vec<(String, String)> = state
            .store
            .all_keys()
            .into_iter()
            .filter(|k| k.device_instance_id == *dev_id)
            .map(|k| {
                let node_id = format!("{}/{}", dev_id, k.point_id);
                let label = point_names
                    .get(&node_id)
                    .cloned()
                    .unwrap_or_else(|| k.point_id.clone());
                (k.point_id, label)
            })
            .collect();
        pts.sort_by(|a, b| a.1.cmp(&b.1));
        pts
    } else {
        Vec::new()
    };

    let pid_del = page_id.clone();
    let zid_del = zone.id.clone();

    rsx! {
        div { class: "props-section",
            h3 { "Zone" }

            label { "Name" }
            input {
                r#type: "text",
                value: "{zone.label}",
                oninput: {
                    let mut update_zone = update_zone.clone();
                    move |e: Event<FormData>| {
                        let val = e.value();
                        (update_zone)(Box::new(move |z| z.label = val));
                    }
                },
            }

            label { "Room #" }
            input {
                r#type: "text",
                value: "{zone.room_number}",
                oninput: {
                    let mut update_zone = update_zone.clone();
                    move |e: Event<FormData>| {
                        let val = e.value();
                        (update_zone)(Box::new(move |z| z.room_number = val));
                    }
                },
            }

            label { "Device" }
            {
                let cur_dev = zone.device_id.clone().unwrap_or_default();
                rsx! {
                    select {
                        onchange: {
                            let mut update_zone = update_zone.clone();
                            let ns = state.node_store.clone();
                            let zone_id_for_ref = zone_id.clone();
                            let page_id_for_ref = page_id.clone();
                            let zone_label_for_ref = zone.label.clone();
                            let old_dev = zone.device_id.clone();
                            let zone_nav_id = zone.nav_node_id.clone();
                            move |e: Event<FormData>| {
                                let val = e.value();
                                let new_dev = if val.is_empty() { None } else { Some(val) };
                                let dev_clone = new_dev.clone();
                                (update_zone)(Box::new(move |z| {
                                    z.device_id = dev_clone;
                                    z.temp_point_id = None;
                                    z.setpoint_source = None;
                                }));
                                let ns = ns.clone();
                                let space_id = format!("space-{}-{}", page_id_for_ref, zone_id_for_ref);
                                let label = zone_label_for_ref.clone();
                                let old_dev = old_dev.clone();
                                let zone_nav_id = zone_nav_id.clone();
                                spawn(async move {
                                    // Remove all spatial refs from old device
                                    if let Some(ref old_id) = old_dev {
                                        let _ = ns.remove_ref(old_id, "spaceRef").await;
                                        let _ = ns.remove_ref(old_id, "floorRef").await;
                                        let _ = ns.remove_ref(old_id, "buildingRef").await;
                                        let _ = ns.remove_ref(old_id, "siteRef").await;
                                    }
                                    // Clean up: remove spaceRef from any other
                                    // equipment that references this same space node
                                    let stale = ns.find_by_ref("spaceRef", &space_id).await;
                                    for stale_node in &stale {
                                        let _ = ns.remove_ref(&stale_node.id, "spaceRef").await;
                                    }
                                    // Set spatial refs on new device
                                    if let Some(ref dev_id) = new_dev {
                                        // Ensure space node exists for this zone
                                        if ns.get_node(&space_id).await.is_err() {
                                            use crate::node::{Node, NodeType};
                                            let space_node = Node::new(
                                                space_id.clone(),
                                                NodeType::Space,
                                                &label,
                                            );
                                            let _ = ns.create_node(space_node).await;
                                        } else {
                                            let _ = ns.update_dis(&space_id, &label).await;
                                        }
                                        let _ = ns.set_ref(dev_id, "spaceRef", &space_id).await;
                                        // Walk up nav tree to set floor/building/site refs
                                        if let Some(ref nav_id) = zone_nav_id {
                                            let st = use_context::<AppState>();
                                            let tree = st.nav_tree.read();
                                            let ancestors = collect_spatial_ancestors(&tree, nav_id);
                                            drop(tree);
                                            if let Some(ref floor_id) = ancestors.floor_ref {
                                                let _ = ns.set_ref(dev_id, "floorRef", floor_id).await;
                                            }
                                            if let Some(ref bldg_id) = ancestors.building_ref {
                                                let _ = ns.set_ref(dev_id, "buildingRef", bldg_id).await;
                                            }
                                            if let Some(ref site_id) = ancestors.site_ref {
                                                let _ = ns.set_ref(dev_id, "siteRef", site_id).await;
                                            }
                                        }
                                    }
                                });
                            }
                        },
                        option { value: "", selected: cur_dev.is_empty(), "None" }
                        for (did, dlabel) in &device_list {
                            option { value: "{did}", selected: *did == cur_dev, "{dlabel}" }
                        }
                    }
                }
            }

            if zone.device_id.is_some() {
                label { "Temp Point" }
                {
                    let cur_temp = zone.temp_point_id.clone().unwrap_or_default();
                    rsx! {
                        select {
                            onchange: {
                                let mut update_zone = update_zone.clone();
                                move |e: Event<FormData>| {
                                    let val = e.value();
                                    let pt = if val.is_empty() { None } else { Some(val) };
                                    (update_zone)(Box::new(move |z| z.temp_point_id = pt));
                                }
                            },
                            option { value: "", selected: cur_temp.is_empty(), "None" }
                            for (pid, plabel) in &point_list {
                                option { value: "{pid}", selected: *pid == cur_temp, "{plabel}" }
                            }
                        }
                    }
                }

                label { "Setpoint Source" }
                {
                    let sp_mode = match &zone.setpoint_source {
                        None => "none",
                        Some(SetpointSource::Point(_)) => "point",
                        Some(SetpointSource::Static(_)) => "static",
                    };
                    let sp_point_val = match &zone.setpoint_source {
                        Some(SetpointSource::Point(p)) => p.clone(),
                        _ => String::new(),
                    };
                    let sp_static_val = match &zone.setpoint_source {
                        Some(SetpointSource::Static(v)) => format!("{v}"),
                        _ => "72".into(),
                    };
                    rsx! {
                        select {
                            onchange: {
                                let mut update_zone = update_zone.clone();
                                let saved_static = sp_static_val.clone();
                                move |e: Event<FormData>| {
                                    let val = e.value();
                                    let saved = saved_static.clone();
                                    let src = match val.as_str() {
                                        "point" => Some(SetpointSource::Point(String::new())),
                                        "static" => Some(SetpointSource::Static(
                                            saved.parse::<f64>().unwrap_or(72.0),
                                        )),
                                        _ => None,
                                    };
                                    (update_zone)(Box::new(move |z| z.setpoint_source = src));
                                }
                            },
                            option { value: "none", selected: sp_mode == "none", "None" }
                            option { value: "point", selected: sp_mode == "point", "Device Point" }
                            option { value: "static", selected: sp_mode == "static", "Static Value" }
                        }

                        if sp_mode == "point" {
                            select {
                                onchange: {
                                    let mut update_zone = update_zone.clone();
                                    move |e: Event<FormData>| {
                                        let val = e.value();
                                        let src = if val.is_empty() {
                                            None
                                        } else {
                                            Some(SetpointSource::Point(val))
                                        };
                                        (update_zone)(Box::new(move |z| z.setpoint_source = src));
                                    }
                                },
                                option { value: "", selected: sp_point_val.is_empty(), "Select point..." }
                                for (pid, plabel) in &point_list {
                                    option { value: "{pid}", selected: *pid == sp_point_val, "{plabel}" }
                                }
                            }
                        }

                        if sp_mode == "static" {
                            input {
                                r#type: "number",
                                value: "{sp_static_val}",
                                oninput: {
                                    let mut update_zone = update_zone.clone();
                                    move |e: Event<FormData>| {
                                        let val = e.value();
                                        if let Ok(v) = val.parse::<f64>() {
                                            (update_zone)(Box::new(move |z| {
                                                z.setpoint_source = Some(SetpointSource::Static(v));
                                            }));
                                        }
                                    }
                                },
                            }
                        }
                    }
                }
            }

            label { "Color (fallback)" }
            input {
                r#type: "color",
                value: "{zone_color_hex(&zone.color)}",
                oninput: {
                    let mut update_zone = update_zone.clone();
                    move |e: Event<FormData>| {
                        let hex = e.value();
                        let rgba = format!("{hex}33");
                        (update_zone)(Box::new(move |z| z.color = rgba));
                    }
                },
            }

            // Label display options
            h4 { class: "props-subhead", "Label Options" }

            div { class: "props-label-row",
                span { class: "props-label-name", "Name" }
                div { class: "props-checkbox",
                    input {
                        r#type: "checkbox",
                        checked: zone.label_config.show_label,
                        onchange: {
                            let mut update_zone = update_zone.clone();
                            move |e: Event<FormData>| {
                                let checked = e.value() == "true";
                                (update_zone)(Box::new(move |z| z.label_config.show_label = checked));
                            }
                        },
                    }
                    span { "Show" }
                }
                div { class: "props-checkbox",
                    input {
                        r#type: "checkbox",
                        checked: zone.label_config.tooltip_label,
                        onchange: {
                            let mut update_zone = update_zone.clone();
                            move |e: Event<FormData>| {
                                let checked = e.value() == "true";
                                (update_zone)(Box::new(move |z| z.label_config.tooltip_label = checked));
                            }
                        },
                    }
                    span { "Tooltip" }
                }
            }
            div { class: "props-label-row",
                span { class: "props-label-name", "Room #" }
                div { class: "props-checkbox",
                    input {
                        r#type: "checkbox",
                        checked: zone.label_config.show_room_number,
                        onchange: {
                            let mut update_zone = update_zone.clone();
                            move |e: Event<FormData>| {
                                let checked = e.value() == "true";
                                (update_zone)(Box::new(move |z| z.label_config.show_room_number = checked));
                            }
                        },
                    }
                    span { "Show" }
                }
                div { class: "props-checkbox",
                    input {
                        r#type: "checkbox",
                        checked: zone.label_config.tooltip_room_number,
                        onchange: {
                            let mut update_zone = update_zone.clone();
                            move |e: Event<FormData>| {
                                let checked = e.value() == "true";
                                (update_zone)(Box::new(move |z| z.label_config.tooltip_room_number = checked));
                            }
                        },
                    }
                    span { "Tooltip" }
                }
            }
            div { class: "props-label-row",
                span { class: "props-label-name", "Temp" }
                div { class: "props-checkbox",
                    input {
                        r#type: "checkbox",
                        checked: zone.label_config.show_temp,
                        onchange: {
                            let mut update_zone = update_zone.clone();
                            move |e: Event<FormData>| {
                                let checked = e.value() == "true";
                                (update_zone)(Box::new(move |z| z.label_config.show_temp = checked));
                            }
                        },
                    }
                    span { "Show" }
                }
                div { class: "props-checkbox",
                    input {
                        r#type: "checkbox",
                        checked: zone.label_config.tooltip_temp,
                        onchange: {
                            let mut update_zone = update_zone.clone();
                            move |e: Event<FormData>| {
                                let checked = e.value() == "true";
                                (update_zone)(Box::new(move |z| z.label_config.tooltip_temp = checked));
                            }
                        },
                    }
                    span { "Tooltip" }
                }
            }

            label { "Font Size" }
            input {
                r#type: "range",
                min: "12",
                max: "48",
                step: "2",
                value: "{zone.label_config.font_size}",
                oninput: {
                    let mut update_zone = update_zone.clone();
                    move |e: Event<FormData>| {
                        if let Ok(v) = e.value().parse::<f64>() {
                            (update_zone)(Box::new(move |z| z.label_config.font_size = v));
                        }
                    }
                },
            }

            label { "Font Color" }
            input {
                r#type: "color",
                value: "{zone.label_config.font_color}",
                oninput: {
                    let mut update_zone = update_zone.clone();
                    move |e: Event<FormData>| {
                        let val = e.value();
                        (update_zone)(Box::new(move |z| z.label_config.font_color = val));
                    }
                },
            }

            label { "Vertices: {zone.points.len()}" }

            button {
                class: "props-delete-btn",
                onclick: {
                    let nav_node_id = zone.nav_node_id.clone();
                    let device_id = zone.device_id.clone();
                    let ns = state.node_store.clone();
                    let space_id = format!("space-{}-{}", pid_del, zid_del);
                    move |_| {
                        let mut pages = state.pages.write();
                        if let Some(data) = pages.get_mut(&pid_del) {
                            data.zones.retain(|z| z.id != zid_del);
                        }
                        drop(pages);
                        if let Some(ref nid) = nav_node_id {
                            let mut tree = state.nav_tree.write();
                            remove_nav_node(&mut tree, nid);
                        }
                        // Clean up spatial refs and space node
                        let ns = ns.clone();
                        let device_id = device_id.clone();
                        let space_id = space_id.clone();
                        spawn(async move {
                            // Clear all spatial refs from bound device
                            if let Some(ref dev_id) = device_id {
                                let _ = ns.remove_ref(dev_id, "spaceRef").await;
                                let _ = ns.remove_ref(dev_id, "floorRef").await;
                                let _ = ns.remove_ref(dev_id, "buildingRef").await;
                                let _ = ns.remove_ref(dev_id, "siteRef").await;
                            }
                            // Clear stale refs from any other equipment
                            let stale = ns.find_by_ref("spaceRef", &space_id).await;
                            for node in &stale {
                                let _ = ns.remove_ref(&node.id, "spaceRef").await;
                            }
                            // Delete the synthetic space node
                            let _ = ns.delete_node(&space_id).await;
                        });
                    }
                },
                "Delete Zone"
            }
        }
    }
}

#[component]
fn EquipmentProperties(
    page_id: String,
    equip_id: String,
    device_list: Vec<(String, String)>,
) -> Element {
    let mut state = use_context::<AppState>();

    let equipment = {
        let pages = state.pages.read();
        pages
            .get(&page_id)
            .and_then(|p| p.equipment.iter().find(|e| e.id == equip_id).cloned())
    };
    let equipment = match equipment {
        Some(e) => e,
        None => return rsx! {},
    };

    let eid = equip_id.clone();
    let pid = page_id.clone();

    let update_equip = move |f: Box<dyn FnOnce(&mut Equipment)>| {
        let mut pages = state.pages.write();
        if let Some(data) = pages.get_mut(&pid) {
            if let Some(eq) = data.equipment.iter_mut().find(|e| e.id == eid) {
                f(eq);
            }
        }
    };

    let pid_del = page_id.clone();
    let eid_del = equipment.id.clone();

    let cur_placement = match equipment.label_config.placement {
        LabelPlacement::Top => "top",
        LabelPlacement::Bottom => "bottom",
        LabelPlacement::Left => "left",
        LabelPlacement::Right => "right",
    };
    let cur_symbol = equipment.symbol.id().to_string();

    rsx! {
        div { class: "props-section",
            h3 { "Equipment" }

            label { "Label" }
            input {
                r#type: "text",
                value: "{equipment.label}",
                oninput: {
                    let mut update_equip = update_equip.clone();
                    move |e: Event<FormData>| {
                        let val = e.value();
                        (update_equip)(Box::new(move |eq| eq.label = val));
                    }
                },
            }

            label { "Device" }
            {
                let cur_dev = equipment.device_id.clone().unwrap_or_default();
                rsx! {
                    select {
                        onchange: {
                            let mut update_equip = update_equip.clone();
                            move |e: Event<FormData>| {
                                let val = e.value();
                                let dev = if val.is_empty() { None } else { Some(val) };
                                (update_equip)(Box::new(move |eq| eq.device_id = dev));
                            }
                        },
                        option { value: "", selected: cur_dev.is_empty(), "None" }
                        for (did, dlabel) in &device_list {
                            option { value: "{did}", selected: *did == cur_dev, "{dlabel}" }
                        }
                    }
                }
            }

            label { "Symbol" }
            select {
                onchange: {
                    let mut update_equip = update_equip.clone();
                    move |e: Event<FormData>| {
                        let val = e.value();
                        (update_equip)(Box::new(move |eq| eq.symbol = EquipSymbol::from_id(&val)));
                    }
                },
                for sym in EquipSymbol::all() {
                    option {
                        value: "{sym.id()}",
                        selected: sym.id() == cur_symbol,
                        "{sym.label()}"
                    }
                }
            }

            h4 { class: "props-subhead", "Label Options" }

            div { class: "props-label-row",
                span { class: "props-label-name", "Label" }
                div { class: "props-checkbox",
                    input {
                        r#type: "checkbox",
                        checked: equipment.label_config.show_label,
                        onchange: {
                            let mut update_equip = update_equip.clone();
                            move |e: Event<FormData>| {
                                let checked = e.value() == "true";
                                (update_equip)(Box::new(move |eq| eq.label_config.show_label = checked));
                            }
                        },
                    }
                    span { "Show" }
                }
                div { class: "props-checkbox",
                    input {
                        r#type: "checkbox",
                        checked: equipment.label_config.tooltip,
                        onchange: {
                            let mut update_equip = update_equip.clone();
                            move |e: Event<FormData>| {
                                let checked = e.value() == "true";
                                (update_equip)(Box::new(move |eq| eq.label_config.tooltip = checked));
                            }
                        },
                    }
                    span { "Tooltip" }
                }
            }

            label { "Placement" }
            select {
                onchange: {
                    let mut update_equip = update_equip.clone();
                    move |e: Event<FormData>| {
                        let val = e.value();
                        let p = match val.as_str() {
                            "top" => LabelPlacement::Top,
                            "left" => LabelPlacement::Left,
                            "right" => LabelPlacement::Right,
                            _ => LabelPlacement::Bottom,
                        };
                        (update_equip)(Box::new(move |eq| eq.label_config.placement = p));
                    }
                },
                option { value: "bottom", selected: cur_placement == "bottom", "Bottom" }
                option { value: "top", selected: cur_placement == "top", "Top" }
                option { value: "left", selected: cur_placement == "left", "Left" }
                option { value: "right", selected: cur_placement == "right", "Right" }
            }

            label { "Font Color" }
            input {
                r#type: "color",
                value: "{equipment.label_config.font_color}",
                oninput: {
                    let mut update_equip = update_equip.clone();
                    move |e: Event<FormData>| {
                        let val = e.value();
                        (update_equip)(Box::new(move |eq| eq.label_config.font_color = val));
                    }
                },
            }

            button {
                class: "props-delete-btn",
                onclick: move |_| {
                    let mut pages = state.pages.write();
                    if let Some(data) = pages.get_mut(&pid_del) {
                        data.equipment.retain(|e| e.id != eid_del);
                    }
                },
                "Delete Equipment"
            }
        }
    }
}

/// SVG path for each equipment symbol, centered at origin.
pub fn equip_symbol_path(sym: &EquipSymbol) -> &'static str {
    match sym {
        EquipSymbol::Gear => "M-1.5-8.6h3l.3 2.1c.5.2.9.5 1.3.8l2-.8 1.5 2.6-1.7 1.3c.04.3.06.5.06.8s-.02.5-.06.8l1.7 1.3-1.5 2.6-2-.8c-.4.3-.8.6-1.3.8l-.3 2.1h-3l-.3-2.1c-.5-.2-.9-.5-1.3-.8l-2 .8-1.5-2.6 1.7-1.3a4 4 0 010-1.6l-1.7-1.3 1.5-2.6 2 .8c.4-.3.8-.6 1.3-.8l.3-2.1zM0 3a3 3 0 100-6 3 3 0 000 6z",
        EquipSymbol::Fan => "M0-2a2 2 0 010 4 2 2 0 01-2-2 2 2 0 012-2zm0-7c1.5 0 3 2 3 5a3 3 0 01-3 2v-7zm0 14c-1.5 0-3-2-3-5a3 3 0 013-2v7zm7-7c0 1.5-2 3-5 3a3 3 0 01-2-3h7zm-14 0c0-1.5 2-3 5-3a3 3 0 012 3h-7z",
        EquipSymbol::Thermometer => "M-2-9h4a2 2 0 012 2v10a4 4 0 11-8 0V-7a2 2 0 012-2zm1 3v8h2v-8h-2z",
        EquipSymbol::Valve => "M-8 0l8-8 8 8-8 8-8-8zm5 0a3 3 0 106 0 3 3 0 00-6 0z",
        EquipSymbol::Pump => "M0-8a8 8 0 110 16 8 8 0 010-16zm0 3a5 5 0 100 10 5 5 0 000-10zm-6 5h12M0-8v16",
        EquipSymbol::AHU => "M-10-6h20v12h-20zM-10-6v12M10-6v12M-6-2h2v4h-2zM4-2h2v4h-2z",
        EquipSymbol::Coil => "M-8 0c0-4 4-4 4 0s4 4 4 0 4-4 4 0 4 4 4 0",
        EquipSymbol::Damper => "M-8-4h16v8h-16zM-6 0h12M-8-4l16 8M-8 4l16-8",
        EquipSymbol::Filter => "M-6-8l6 8 6-8M-6 8l6-8 6 8M-4-4h8M-4 4h8",
        EquipSymbol::Compressor => "M0-8a8 8 0 110 16 8 8 0 010-16zM-4-3l8 6M-4 3l8-6",
        EquipSymbol::HeatExchanger => "M-8-6h16v12h-16zM0-6v12M-4-3v6M4-3v6",
        EquipSymbol::Sensor => "M0-8a8 8 0 110 16 8 8 0 010-16zM0-4v8M-4 0h8",
    }
}

fn zone_color_hex(color: &str) -> String {
    if color.starts_with('#') && color.len() >= 7 {
        color[..7].to_string()
    } else {
        "#d4714e".to_string()
    }
}
