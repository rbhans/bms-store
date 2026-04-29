use std::collections::HashMap;
use std::collections::HashSet;

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use bms_store_storage::config::profile::PointValue;
use crate::gui::state::{AppState, EquipSymbol};
use bms_store_storage::store::point_store::PointStatusFlags;

use super::floor_plan::equip_symbol_path;

// ---------------------------------------------------------------------------
// Data model — persisted as "pointGroups" node property on the equipment node
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct PointGroupConfig {
    pub groups: Vec<PointGroup>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PointGroup {
    pub id: String,
    pub name: String,
    pub symbol: String,
    pub anim_point: Option<String>,
    pub point_ids: Vec<String>,
}

// ---------------------------------------------------------------------------
// Persistence helpers
// ---------------------------------------------------------------------------

const PROP_KEY: &str = "pointGroups";

// ---------------------------------------------------------------------------
// Sorting
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum SortCol {
    Name,
    Kind,
    Access,
    Value,
}

#[derive(Clone, Copy, PartialEq)]
enum SortDir {
    Asc,
    Desc,
}

/// Cached node info for a point.
#[derive(Clone, Default)]
struct PointNodeInfo {
    dis: String,
    kind: String,
    units: Option<String>,
    writable: bool,
}

// ---------------------------------------------------------------------------
// Row data
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct RowData {
    point_id: String,
    name: String,
    kind: String,
    access: String,
    units: Option<String>,
    value: Option<PointValue>,
    value_str: String,
    status: PointStatusFlags,
}

// ---------------------------------------------------------------------------
// PointTable — top-level component
// ---------------------------------------------------------------------------

#[component]
pub fn PointTable() -> Element {
    let mut state = use_context::<AppState>();
    let mut sort_col = use_signal(|| SortCol::Name);
    let mut sort_dir = use_signal(|| SortDir::Asc);
    let mut pinned: Signal<Vec<String>> = use_signal(Vec::new);
    let mut editing = use_signal(|| false);

    // Drag-and-drop state (edit mode only)
    let mut dragging_point: Signal<Option<String>> = use_signal(|| None);
    let mut dragging_name: Signal<Option<String>> = use_signal(|| None);
    let mut drop_target_group: Signal<Option<String>> = use_signal(|| None);
    let mut drag_pos: Signal<(f64, f64)> = use_signal(|| (0.0, 0.0));

    // Re-read when store changes
    let _version = state.store_version.read();
    let node_ver_sig = state.node_version;

    let selected_device = state.selected_device.read().clone();
    let selected_point = state.selected_point.read().clone();

    let Some(device_id) = selected_device else {
        return rsx! {
            div { class: "point-table empty",
                p { class: "placeholder", "Select a device to view its points." }
            }
        };
    };

    // Load point node info from node store (child point nodes of this device)
    let ns = state.node_store.clone();
    let mut point_nodes_info = use_signal(HashMap::<String, PointNodeInfo>::new);
    let selected_dev_sig_pts = state.selected_device;
    let _ = use_resource(move || {
        let ns = ns.clone();
        // Read the signal inside the closure so Dioxus tracks it as a dependency
        let did = selected_dev_sig_pts.read().clone();
        let _nv = *node_ver_sig.read();
        async move {
            let Some(did) = did else {
                point_nodes_info.set(HashMap::new());
                return;
            };
            let point_nodes = ns.list_nodes(Some("point"), Some(&did)).await;
            let mut info_map = HashMap::new();
            for pn in &point_nodes {
                let point_id = pn
                    .id
                    .strip_prefix(&format!("{}/", did))
                    .unwrap_or(&pn.id)
                    .to_string();
                let kind = pn.properties.get("kind").cloned().unwrap_or_default();
                let units = pn.properties.get("units").cloned();
                let writable = pn.capabilities.writable;
                info_map.insert(
                    point_id,
                    PointNodeInfo {
                        dis: pn.dis.clone(),
                        kind,
                        units,
                        writable,
                    },
                );
            }
            point_nodes_info.set(info_map);
        }
    });

    // Load group config + device display name
    let mut group_cfg: Signal<PointGroupConfig> = use_signal(|| PointGroupConfig::default());
    let mut edit_cfg: Signal<PointGroupConfig> = use_signal(|| PointGroupConfig::default());
    let mut device_dis: Signal<String> = use_signal(|| String::new());
    let ns2 = state.node_store.clone();
    let selected_dev_sig = state.selected_device;
    let _ = use_resource(move || {
        let ns = ns2.clone();
        // Read the signal inside the closure so Dioxus tracks it as a dependency
        let did = selected_dev_sig.read().clone();
        let _nv = *node_ver_sig.read();
        async move {
            let Some(did) = did else {
                group_cfg.set(PointGroupConfig::default());
                device_dis.set(String::new());
                return;
            };
            if let Ok(node) = ns.get_node(&did).await {
                if !node.dis.is_empty() {
                    device_dis.set(node.dis.clone());
                } else {
                    device_dis.set(did.clone());
                }
                if let Some(json) = node.properties.get(PROP_KEY) {
                    if let Ok(cfg) = serde_json::from_str::<PointGroupConfig>(json) {
                        group_cfg.set(cfg);
                        return;
                    }
                }
            } else {
                device_dis.set(did.clone());
            }
            group_cfg.set(PointGroupConfig::default());
        }
    });

    let profile = state
        .loaded
        .devices
        .iter()
        .find(|d| d.instance_id == device_id)
        .map(|d| &d.profile);

    let live_points = state.store.get_all_for_device(&device_id);
    let node_info = point_nodes_info.read();

    let mut rows: Vec<RowData> = if let Some(profile) = profile {
        profile
            .points
            .iter()
            .map(|pt| {
                let live = live_points.iter().find(|(k, _)| k.point_id == pt.id);
                let value = live.map(|(_, v)| v.value.clone());
                let status = live.map(|(_, v)| v.status).unwrap_or_default();
                let prec = pt.ui.as_ref().and_then(|u| u.precision).unwrap_or(1) as usize;
                let value_str = match &value {
                    Some(PointValue::Bool(b)) => {
                        if *b {
                            "ON".into()
                        } else {
                            "OFF".into()
                        }
                    }
                    Some(PointValue::Integer(i)) => i.to_string(),
                    Some(PointValue::Float(f)) => format!("{f:.prec$}"),
                    None => "\u{2014}".into(),
                };

                RowData {
                    point_id: pt.id.clone(),
                    name: pt.name.clone(),
                    kind: format!("{:?}", pt.kind).to_lowercase(),
                    access: format!("{:?}", pt.access).to_lowercase(),
                    units: pt.units.clone(),
                    value: value.clone(),
                    value_str,
                    status,
                }
            })
            .collect()
    } else {
        live_points
            .iter()
            .map(|(k, v)| {
                let ni = node_info.get(&k.point_id);
                let value_str = match &v.value {
                    PointValue::Bool(b) => {
                        if *b {
                            "ON".into()
                        } else {
                            "OFF".into()
                        }
                    }
                    PointValue::Integer(i) => i.to_string(),
                    PointValue::Float(f) => format!("{f:.1}"),
                };
                RowData {
                    point_id: k.point_id.clone(),
                    name: ni
                        .map(|n| n.dis.clone())
                        .unwrap_or_else(|| k.point_id.clone()),
                    kind: ni.map(|n| n.kind.clone()).unwrap_or_default(),
                    access: if ni.map(|n| n.writable).unwrap_or(false) {
                        "readwrite".into()
                    } else {
                        "readonly".into()
                    },
                    units: ni.and_then(|n| n.units.clone()),
                    value: Some(v.value.clone()),
                    value_str,
                    status: v.status,
                }
            })
            .collect()
    };

    // Sort
    let col = *sort_col.read();
    let dir = *sort_dir.read();
    let cmp = |a: &RowData, b: &RowData| -> std::cmp::Ordering {
        let ord = match col {
            SortCol::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortCol::Kind => a.kind.cmp(&b.kind),
            SortCol::Access => a.access.cmp(&b.access),
            SortCol::Value => a.value_str.cmp(&b.value_str),
        };
        match dir {
            SortDir::Asc => ord,
            SortDir::Desc => ord.reverse(),
        }
    };

    // Build row lookup
    let row_map: HashMap<String, RowData> = rows
        .iter()
        .map(|r| (r.point_id.clone(), r.clone()))
        .collect();

    // Determine grouped vs ungrouped
    let cfg = if *editing.read() {
        edit_cfg.read().clone()
    } else {
        group_cfg.read().clone()
    };
    let grouped_ids: HashSet<String> = cfg
        .groups
        .iter()
        .flat_map(|g| g.point_ids.iter().cloned())
        .collect();

    // Ungrouped rows: partition into pinned and unpinned
    let pinned_set = pinned.read().clone();
    let mut pinned_rows: Vec<RowData> = Vec::new();
    let mut ungrouped_rows: Vec<RowData> = Vec::new();
    for row in rows.drain(..) {
        if grouped_ids.contains(&row.point_id) {
            continue;
        }
        if pinned_set.contains(&row.point_id) {
            pinned_rows.push(row);
        } else {
            ungrouped_rows.push(row);
        }
    }
    pinned_rows.sort_by(&cmp);
    ungrouped_rows.sort_by(&cmp);

    // Sort header helpers
    let mut on_header_click = move |clicked: SortCol| {
        let cur_col = *sort_col.read();
        if cur_col == clicked {
            let cur_dir = *sort_dir.read();
            sort_dir.set(if cur_dir == SortDir::Asc {
                SortDir::Desc
            } else {
                SortDir::Asc
            });
        } else {
            sort_col.set(clicked);
            sort_dir.set(SortDir::Asc);
        }
    };

    let indicator = |c: SortCol| -> &'static str {
        if *sort_col.read() == c {
            if *sort_dir.read() == SortDir::Asc {
                " \u{25B2}"
            } else {
                " \u{25BC}"
            }
        } else {
            ""
        }
    };

    let has_pinned = !pinned_rows.is_empty();
    let has_groups = !cfg.groups.is_empty();
    let is_editing = *editing.read();

    // Group names for assign dropdown
    let group_names: Vec<(String, String)> = cfg
        .groups
        .iter()
        .map(|g| (g.id.clone(), g.name.clone()))
        .collect();

    let did_save = device_id.clone();

    let is_dragging = dragging_point.peek().is_some();
    let table_class = if is_dragging {
        "point-table pt-dragging"
    } else {
        "point-table"
    };
    let current_drop_target = drop_target_group.read().clone();

    // Total columns for colspans (sym + pin + status + name + kind + access + value [+ assign])
    let total_cols = if is_editing { "8" } else { "7" };

    rsx! {
        div {
            class: "{table_class}",
            // Track cursor position during drag
            onmousemove: move |evt: MouseEvent| {
                if dragging_point.peek().is_some() {
                    let coords = evt.page_coordinates();
                    drag_pos.set((coords.x, coords.y));
                }
            },
            // Cancel drag if mouseup happens outside a group card
            onmouseup: move |_| {
                if dragging_point.peek().is_some() {
                    dragging_point.set(None);
                    dragging_name.set(None);
                    drop_target_group.set(None);
                }
            },

            // Floating drag indicator
            if is_dragging {
                {
                    let (px, py) = *drag_pos.read();
                    let name = dragging_name.read().clone().unwrap_or_default();
                    rsx! {
                        div {
                            class: "pt-drag-indicator",
                            style: "left: {px + 12.0}px; top: {py - 10.0}px;",
                            "{name}"
                        }
                    }
                }
            }

            // Header with edit toggle
            div { class: "pt-header",
                span { class: "pt-title", "{device_dis}" }
                if is_editing {
                    button {
                        class: "btn-accent btn-sm",
                        onclick: move |_| {
                            let ns = state.node_store.clone();
                            let did = did_save.clone();
                            let cfg = edit_cfg.read().clone();
                            spawn(async move {
                                if let Ok(json) = serde_json::to_string(&cfg) {
                                    let _ = ns.set_property(&did, PROP_KEY, &json).await;
                                }
                                group_cfg.set(cfg);
                                editing.set(false);
                            });
                        },
                        "Done"
                    }
                    button {
                        class: "btn-secondary btn-sm",
                        onclick: move |_| editing.set(false),
                        "Cancel"
                    }
                    button {
                        class: "btn-secondary btn-sm",
                        onclick: move |_| {
                            let mut cfg = edit_cfg.write();
                            let next_id = cfg.groups.len() + 1;
                            cfg.groups.push(PointGroup {
                                id: format!("grp-{next_id}"),
                                name: "New Group".to_string(),
                                symbol: "fan".to_string(),
                                anim_point: None,
                                point_ids: Vec::new(),
                            });
                        },
                        "+ Add Group"
                    }
                } else {
                    button {
                        class: "pt-edit-btn",
                        title: "Edit point groups",
                        onclick: move |_| {
                            edit_cfg.set(group_cfg.read().clone());
                            editing.set(true);
                        },
                        "\u{270E}"
                    }
                }
            }

            // Single unified table for groups + ungrouped
            table {
                thead {
                    tr {
                        th { class: "col-sym-header" }
                        th { class: "col-pin-header" }
                        th { class: "col-status-header", "Status" }
                        th { class: "sortable", onclick: move |_| on_header_click(SortCol::Name),
                            "Point{indicator(SortCol::Name)}"
                        }
                        th { class: "sortable", onclick: move |_| on_header_click(SortCol::Kind),
                            "Kind{indicator(SortCol::Kind)}"
                        }
                        th { class: "sortable", onclick: move |_| on_header_click(SortCol::Access),
                            "Access{indicator(SortCol::Access)}"
                        }
                        th { class: "sortable", onclick: move |_| on_header_click(SortCol::Value),
                            "Value{indicator(SortCol::Value)}"
                        }
                        if is_editing {
                            th { class: "col-assign-header" }
                        }
                    }
                }
                tbody {
                    // ---- Group sections ----
                    for (gi, group) in cfg.groups.iter().enumerate() {
                        {
                            let group_rows: Vec<RowData> = group.point_ids.iter()
                                .filter_map(|pid| row_map.get(pid).cloned())
                                .collect();

                            let anim_value = group.anim_point.as_ref().and_then(|ap| {
                                row_map.get(ap).and_then(|r| r.value.clone())
                            });

                            let group_id = group.id.clone();
                            let group_name = group.name.clone();
                            let group_symbol = group.symbol.clone();
                            let group_anim_point = group.anim_point.clone();
                            let group_point_ids = group.point_ids.clone();
                            let num_group_rows = group_rows.len().max(1); // at least 1 for empty placeholder

                            let is_drop_target = is_editing && current_drop_target.as_deref() == Some(&group_id);
                            let header_class = if is_drop_target { "group-header-row drop-target" } else { "group-header-row" };

                            let gid_enter_h = group.id.clone();
                            let gid_leave_h = group.id.clone();

                            rsx! {
                                // Group header row — spans all columns, just the name + edit controls
                                tr {
                                    key: "gh-{group_id}",
                                    class: "{header_class}",
                                    onmouseenter: move |_| {
                                        if dragging_point.peek().is_some() {
                                            drop_target_group.set(Some(gid_enter_h.clone()));
                                        }
                                    },
                                    onmouseleave: move |_| {
                                        if drop_target_group.peek().as_deref() == Some(&gid_leave_h) {
                                            drop_target_group.set(None);
                                        }
                                    },
                                    onmouseup: move |evt: MouseEvent| {
                                        let dp = dragging_point.peek().clone();
                                        if let Some(pid) = dp {
                                            evt.stop_propagation();
                                            let mut cfg = edit_cfg.write();
                                            for g in cfg.groups.iter_mut() { g.point_ids.retain(|p| p != &pid); }
                                            if let Some(g) = cfg.groups.get_mut(gi) { g.point_ids.push(pid); }
                                            drop(cfg);
                                            dragging_point.set(None);
                                            dragging_name.set(None);
                                            drop_target_group.set(None);
                                        }
                                    },
                                    td { colspan: total_cols, class: "group-title-cell",
                                        if is_editing {
                                            input {
                                                class: "group-name-input",
                                                r#type: "text",
                                                value: "{group_name}",
                                                oninput: move |evt: FormEvent| {
                                                    let v = evt.value().to_string();
                                                    let mut cfg = edit_cfg.write();
                                                    if let Some(g) = cfg.groups.get_mut(gi) { g.name = v; }
                                                },
                                            }
                                            select {
                                                class: "group-symbol-select",
                                                value: "{group_symbol}",
                                                onchange: move |evt: FormEvent| {
                                                    let v = evt.value().to_string();
                                                    let mut cfg = edit_cfg.write();
                                                    if let Some(g) = cfg.groups.get_mut(gi) { g.symbol = v; }
                                                },
                                                for es in EquipSymbol::all().iter() {
                                                    option { value: "{es.id()}", selected: es.id() == group_symbol, "{es.label()}" }
                                                }
                                            }
                                            select {
                                                class: "group-anim-select",
                                                value: "{group_anim_point.as_deref().unwrap_or(\"\")}",
                                                onchange: move |evt: FormEvent| {
                                                    let v = evt.value().to_string();
                                                    let mut cfg = edit_cfg.write();
                                                    if let Some(g) = cfg.groups.get_mut(gi) {
                                                        g.anim_point = if v.is_empty() { None } else { Some(v) };
                                                    }
                                                },
                                                option { value: "", "Anim: None" }
                                                for pid in group_point_ids.iter() {
                                                    {
                                                        let label = row_map.get(pid).map(|r| r.name.clone()).unwrap_or_else(|| pid.clone());
                                                        let is_sel = group_anim_point.as_deref() == Some(pid.as_str());
                                                        rsx! { option { value: "{pid}", selected: is_sel, "{label}" } }
                                                    }
                                                }
                                            }
                                            button {
                                                class: "group-delete-btn",
                                                title: "Delete group",
                                                onclick: move |_| {
                                                    let mut cfg = edit_cfg.write();
                                                    cfg.groups.retain(|g| g.id != group_id);
                                                },
                                                "\u{2715}"
                                            }
                                        } else {
                                            span { class: "group-card-title", "{group_name}" }
                                        }
                                    }
                                }

                                // Group point rows — symbol cell with rowspan on first row
                                for (ri, row) in group_rows.iter().enumerate() {
                                    {
                                        let pid = row.point_id.clone();
                                        let pid_click = row.point_id.clone();
                                        let is_selected = selected_point.as_deref() == Some(row.point_id.as_str());
                                        let status_class = row.status.worst_status()
                                            .map(|s| format!("status-dot status-{s}"))
                                            .unwrap_or_default();
                                        let status_title = row.status.active_flags().join(", ");
                                        let row_class = if is_selected { "point-row grouped selected" } else { "point-row grouped" };

                                        let gid_enter_r = group.id.clone();
                                        let gid_leave_r = group.id.clone();
                                        let sym_for_row = group_symbol.clone();

                                        rsx! {
                                            tr {
                                                key: "g-{pid}",
                                                class: "{row_class}",
                                                onclick: move |_| {
                                                    state.selected_point.set(Some(pid_click.clone()));
                                                    state.detail_open.set(true);
                                                },
                                                onmouseenter: move |_| {
                                                    if dragging_point.peek().is_some() {
                                                        drop_target_group.set(Some(gid_enter_r.clone()));
                                                    }
                                                },
                                                onmouseleave: move |_| {
                                                    if drop_target_group.peek().as_deref() == Some(&gid_leave_r) {
                                                        drop_target_group.set(None);
                                                    }
                                                },
                                                onmouseup: move |evt: MouseEvent| {
                                                    let dp = dragging_point.peek().clone();
                                                    if let Some(pid) = dp {
                                                        evt.stop_propagation();
                                                        let mut cfg = edit_cfg.write();
                                                        for g in cfg.groups.iter_mut() { g.point_ids.retain(|p| p != &pid); }
                                                        if let Some(g) = cfg.groups.get_mut(gi) { g.point_ids.push(pid); }
                                                        drop(cfg);
                                                        dragging_point.set(None);
                                                        dragging_name.set(None);
                                                        drop_target_group.set(None);
                                                    }
                                                },
                                                // Symbol cell — only on first row, spans all rows
                                                if ri == 0 {
                                                    td {
                                                        class: "col-sym",
                                                        rowspan: "{num_group_rows}",
                                                        AnimatedSymbol {
                                                            symbol: sym_for_row,
                                                            value: anim_value.clone(),
                                                            size: 48,
                                                        }
                                                    }
                                                }
                                                td { class: "col-pin" }
                                                td { class: "col-status",
                                                    if !row.status.is_normal() {
                                                        span {
                                                            class: "{status_class}",
                                                            title: "{status_title}",
                                                        }
                                                    }
                                                }
                                                td { class: "col-name", "{row.name}" }
                                                td { class: "col-kind", "{row.kind}" }
                                                td { class: "col-access", "{row.access}" }
                                                td { class: "col-value",
                                                    "{row.value_str}"
                                                    if let Some(u) = row.units.as_deref() {
                                                        span { class: "value-units", " {u}" }
                                                    }
                                                }
                                                if is_editing {
                                                    td { class: "col-assign",
                                                        {
                                                            let pid_remove = pid.clone();
                                                            rsx! {
                                                                button {
                                                                    class: "gpr-remove-btn",
                                                                    title: "Remove from group",
                                                                    onclick: move |e: Event<MouseData>| {
                                                                        e.stop_propagation();
                                                                        let mut cfg = edit_cfg.write();
                                                                        if let Some(g) = cfg.groups.get_mut(gi) {
                                                                            g.point_ids.retain(|p| p != &pid_remove);
                                                                        }
                                                                    },
                                                                    "\u{2715}"
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // Empty placeholder if no points in group
                                if group_rows.is_empty() {
                                    {
                                        let gid_enter_e = group.id.clone();
                                        let gid_leave_e = group.id.clone();
                                        let sym_empty = group_symbol.clone();
                                        rsx! {
                                            tr {
                                                class: if is_drop_target { "point-row grouped empty drop-target" } else { "point-row grouped empty" },
                                                onmouseenter: move |_| {
                                                    if dragging_point.peek().is_some() {
                                                        drop_target_group.set(Some(gid_enter_e.clone()));
                                                    }
                                                },
                                                onmouseleave: move |_| {
                                                    if drop_target_group.peek().as_deref() == Some(&gid_leave_e) {
                                                        drop_target_group.set(None);
                                                    }
                                                },
                                                onmouseup: move |evt: MouseEvent| {
                                                    let dp = dragging_point.peek().clone();
                                                    if let Some(pid) = dp {
                                                        evt.stop_propagation();
                                                        let mut cfg = edit_cfg.write();
                                                        for g in cfg.groups.iter_mut() { g.point_ids.retain(|p| p != &pid); }
                                                        if let Some(g) = cfg.groups.get_mut(gi) { g.point_ids.push(pid); }
                                                        drop(cfg);
                                                        dragging_point.set(None);
                                                        dragging_name.set(None);
                                                        drop_target_group.set(None);
                                                    }
                                                },
                                                td { class: "col-sym",
                                                    AnimatedSymbol {
                                                        symbol: sym_empty,
                                                        value: anim_value.clone(),
                                                        size: 48,
                                                    }
                                                }
                                                td { class: "col-pin" }
                                                td { colspan: if is_editing { "6" } else { "5" },
                                                    class: "col-name text-muted",
                                                    "No points assigned \u{2014} drag points here"
                                                }
                                            }
                                        }
                                    }
                                }

                                // Spacer row between groups
                                tr { class: "group-spacer",
                                    td { colspan: total_cols }
                                }
                            }
                        }
                    }

                    // ---- Ungrouped section ----
                    if has_groups || is_editing {
                        tr { class: "ungrouped-label-row",
                            td { colspan: total_cols,
                                span { class: "pt-ungrouped-label", "Ungrouped Points" }
                            }
                        }
                    }

                    // Pinned rows
                    for row in &pinned_rows {
                        {
                            let is_selected = selected_point.as_deref() == Some(row.point_id.as_str());
                            let pid = row.point_id.clone();
                            let pid_unpin = row.point_id.clone();
                            let pid_assign = row.point_id.clone();
                            let pid_drag = row.point_id.clone();
                            let drag_label = row.name.clone();
                            let status_class = row.status.worst_status()
                                .map(|s| format!("status-dot status-{s}"))
                                .unwrap_or_default();
                            let status_title = row.status.active_flags().join(", ");
                            let gnames = group_names.clone();
                            let row_class = if is_selected { "point-row pinned selected" } else { "point-row pinned" };
                            let edit_row_class = if is_editing { format!("{row_class} draggable") } else { row_class.to_string() };

                            rsx! {
                                tr {
                                    key: "pin-{pid}",
                                    class: "{edit_row_class}",

                                    onclick: move |_| {
                                        if dragging_point.peek().is_none() {
                                            state.selected_point.set(Some(pid.clone()));
                                            state.detail_open.set(true);
                                        }
                                    },
                                    onmousedown: move |evt: MouseEvent| {
                                        if is_editing {
                                            evt.prevent_default();
                                            let coords = evt.page_coordinates();
                                            drag_pos.set((coords.x, coords.y));
                                            dragging_point.set(Some(pid_drag.clone()));
                                            dragging_name.set(Some(drag_label.clone()));
                                        }
                                    },
                                    td { class: "col-sym" }
                                    td { class: "col-pin",
                                        button {
                                            class: "pin-btn pinned",
                                            title: "Unpin",
                                            onclick: move |e: Event<MouseData>| {
                                                e.stop_propagation();
                                                pinned.write().retain(|p| p != &pid_unpin);
                                            },
                                            "\u{1F4CC}"
                                        }
                                    }
                                    td { class: "col-status",
                                        if !row.status.is_normal() {
                                            span {
                                                class: "{status_class}",
                                                title: "{status_title}",
                                            }
                                        }
                                    }
                                    td { class: "col-name", "{row.name}" }
                                    td { class: "col-kind", "{row.kind}" }
                                    td { class: "col-access", "{row.access}" }
                                    td { class: "col-value",
                                        "{row.value_str}"
                                        if let Some(u) = row.units.as_deref() {
                                            span { class: "value-units", " {u}" }
                                        }
                                    }
                                    if is_editing {
                                        td { class: "col-assign",
                                            select {
                                                onclick: move |e: Event<MouseData>| e.stop_propagation(),
                                                onchange: move |evt: FormEvent| {
                                                    let gid = evt.value().to_string();
                                                    if !gid.is_empty() {
                                                        let mut cfg = edit_cfg.write();
                                                        for g in cfg.groups.iter_mut() {
                                                            g.point_ids.retain(|p| p != &pid_assign);
                                                        }
                                                        if let Some(g) = cfg.groups.iter_mut().find(|g| g.id == gid) {
                                                            g.point_ids.push(pid_assign.clone());
                                                        }
                                                    }
                                                },
                                                option { value: "", "\u{2014}" }
                                                for (gid, gname) in gnames.iter() {
                                                    option { value: "{gid}", "{gname}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Divider between pinned and unpinned
                    if has_pinned {
                        tr { class: "pin-divider",
                            td { colspan: total_cols }
                        }
                    }

                    // Unpinned rows
                    for row in &ungrouped_rows {
                        {
                            let is_selected = selected_point.as_deref() == Some(row.point_id.as_str());
                            let pid = row.point_id.clone();
                            let pid_pin = row.point_id.clone();
                            let pid_assign = row.point_id.clone();
                            let pid_drag = row.point_id.clone();
                            let drag_label = row.name.clone();
                            let status_class = row.status.worst_status()
                                .map(|s| format!("status-dot status-{s}"))
                                .unwrap_or_default();
                            let status_title = row.status.active_flags().join(", ");
                            let gnames = group_names.clone();
                            let row_class = if is_selected { "point-row selected" } else { "point-row" };
                            let edit_row_class = if is_editing { format!("{row_class} draggable") } else { row_class.to_string() };

                            rsx! {
                                tr {
                                    key: "{pid}",
                                    class: "{edit_row_class}",

                                    onclick: move |_| {
                                        if dragging_point.peek().is_none() {
                                            state.selected_point.set(Some(pid.clone()));
                                            state.detail_open.set(true);
                                        }
                                    },
                                    onmousedown: move |evt: MouseEvent| {
                                        if is_editing {
                                            evt.prevent_default();
                                            let coords = evt.page_coordinates();
                                            drag_pos.set((coords.x, coords.y));
                                            dragging_point.set(Some(pid_drag.clone()));
                                            dragging_name.set(Some(drag_label.clone()));
                                        }
                                    },
                                    td { class: "col-sym" }
                                    td { class: "col-pin",
                                        button {
                                            class: "pin-btn",
                                            title: "Pin to top",
                                            onclick: move |e: Event<MouseData>| {
                                                e.stop_propagation();
                                                pinned.write().push(pid_pin.clone());
                                            },
                                            "\u{1F4CC}"
                                        }
                                    }
                                    td { class: "col-status",
                                        if !row.status.is_normal() {
                                            span {
                                                class: "{status_class}",
                                                title: "{status_title}",
                                            }
                                        }
                                    }
                                    td { class: "col-name", "{row.name}" }
                                    td { class: "col-kind", "{row.kind}" }
                                    td { class: "col-access", "{row.access}" }
                                    td { class: "col-value",
                                        "{row.value_str}"
                                        if let Some(u) = row.units.as_deref() {
                                            span { class: "value-units", " {u}" }
                                        }
                                    }
                                    if is_editing {
                                        td { class: "col-assign",
                                            select {
                                                onclick: move |e: Event<MouseData>| e.stop_propagation(),
                                                onchange: move |evt: FormEvent| {
                                                    let gid = evt.value().to_string();
                                                    if !gid.is_empty() {
                                                        let mut cfg = edit_cfg.write();
                                                        for g in cfg.groups.iter_mut() {
                                                            g.point_ids.retain(|p| p != &pid_assign);
                                                        }
                                                        if let Some(g) = cfg.groups.iter_mut().find(|g| g.id == gid) {
                                                            g.point_ids.push(pid_assign.clone());
                                                        }
                                                    }
                                                },
                                                option { value: "", "\u{2014}" }
                                                for (gid, gname) in gnames.iter() {
                                                    option { value: "{gid}", "{gname}" }
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
}

// ---------------------------------------------------------------------------
// AnimatedSymbol — renders an SVG symbol with CSS animation
// ---------------------------------------------------------------------------

#[component]
fn AnimatedSymbol(
    symbol: String,
    value: Option<PointValue>,
    #[props(default = 64)] size: u32,
) -> Element {
    let equip_sym = EquipSymbol::from_id(&symbol);
    let path_d = equip_symbol_path(&equip_sym);

    // Determine active state and proportional value
    let (is_active, pct) = match &value {
        Some(PointValue::Bool(b)) => (*b, if *b { 100.0 } else { 0.0 }),
        Some(PointValue::Integer(i)) => (*i > 0, *i as f64),
        Some(PointValue::Float(f)) => (*f > 0.0, *f),
        None => (false, 0.0),
    };

    let sym_class = format!("sym-{}", symbol);

    // Symbol-specific inline style for proportional animations (valve/damper)
    let is_proportional = matches!(equip_sym, EquipSymbol::Valve | EquipSymbol::Damper);
    let inner_style = if is_proportional {
        let angle = (pct.clamp(0.0, 100.0) / 100.0) * 90.0;
        format!("transform-box: fill-box; transform-origin: center; transform: rotate({angle:.1}deg); transition: transform 0.5s ease;")
    } else {
        "transform-box: fill-box; transform-origin: center;".to_string()
    };

    // Thermometer fill level
    let is_thermo = matches!(equip_sym, EquipSymbol::Thermometer);
    let thermo_fill_pct = if is_thermo {
        // Map value to 0-100 range (assume 0-100 scale for simplicity)
        pct.clamp(0.0, 100.0)
    } else {
        0.0
    };

    // Coil / HeatExchanger: stroke color changes when active
    let is_heat = matches!(equip_sym, EquipSymbol::Coil | EquipSymbol::HeatExchanger);
    let stroke_color = if is_heat && is_active {
        "var(--accent)"
    } else {
        "var(--text-primary)"
    };

    let active_class = if is_active { "sym-active" } else { "" };

    rsx! {
        svg {
            class: "sym-card-icon {sym_class} {active_class}",
            view_box: "-12 -12 24 24",
            width: "{size}",
            height: "{size}",
            g {
                class: "sym-inner",
                style: "{inner_style}",
                path {
                    d: "{path_d}",
                    fill: "none",
                    stroke: "{stroke_color}",
                    stroke_width: "1.2",
                }
            }
            if is_thermo && is_active {
                // Fill indicator for thermometer
                rect {
                    x: "-1",
                    y: "{9.0 - thermo_fill_pct * 0.16}",
                    width: "2",
                    height: "{thermo_fill_pct * 0.16}",
                    fill: "var(--accent)",
                    opacity: "0.6",
                    rx: "1",
                }
            }
        }
    }
}
