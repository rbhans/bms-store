use std::collections::HashMap;

use dioxus::prelude::*;

use bms_store_bridges::discovery::grouping::{
    find_related_groups, point_set_from_json, suggest_group_name, RelatedGroup,
};
use crate::gui::state::{ActiveView, AppState};

/// A group of devices sharing the same point set.
#[derive(Clone, Default)]
struct DeviceGroup {
    id: String,
    name: String,
    device_ids: Vec<String>,
    point_set_json: String,
    related: Vec<RelatedGroup>,
}

/// Grouping info loaded from node store.
#[derive(Clone, Default)]
struct GroupInfo {
    groups: Vec<DeviceGroup>,
    /// device_id → group_id (for devices that belong to a group)
    device_to_group: HashMap<String, String>,
    /// All accepted device IDs (equip nodes in node store)
    accepted_device_ids: Vec<String>,
    /// device_id → display name from node store
    device_names: HashMap<String, String>,
    /// device_id → point count from node store (child point nodes)
    device_point_counts: HashMap<String, usize>,
}

#[component]
pub fn DeviceTree(filter: String) -> Element {
    let mut state = use_context::<AppState>();
    let mut collapsed_groups = use_signal::<Vec<String>>(Vec::new);
    let mut group_info = use_signal(GroupInfo::default);

    let version = *state.store_version.read();
    let node_ver = *state.node_version.read();

    // Load group info from node store (async)
    let ns = state.node_store.clone();
    let _ = use_resource(move || {
        let ns = ns.clone();
        let _v = version;
        let _nv = node_ver;
        async move {
            // Get all group (Space) nodes
            let space_nodes = ns.list_nodes(Some("space"), None).await;
            let mut gmap: HashMap<String, DeviceGroup> = HashMap::new();

            // Collect all group data including point sets for similarity
            let mut all_group_data: Vec<(String, String, String)> = Vec::new();

            for sn in &space_nodes {
                if sn.id.starts_with("group-") {
                    let ps_json = sn.properties.get("pointSet").cloned().unwrap_or_default();
                    all_group_data.push((sn.id.clone(), sn.dis.clone(), ps_json.clone()));
                    gmap.insert(
                        sn.id.clone(),
                        DeviceGroup {
                            id: sn.id.clone(),
                            name: sn.dis.clone(),
                            device_ids: Vec::new(),
                            point_set_json: ps_json,
                            related: Vec::new(),
                        },
                    );
                }
            }

            // Compute related groups for each group (similarity >= 0.5)
            for group in gmap.values_mut() {
                if group.point_set_json.is_empty() {
                    continue;
                }
                let target = point_set_from_json(&group.point_set_json);
                group.related = find_related_groups(&group.id, &target, &all_group_data, 0.5);
            }

            // Get all equip nodes and assign to groups by parent_id
            let equip_nodes = ns.list_nodes(Some("equip"), None).await;
            let mut d2g: HashMap<String, String> = HashMap::new();
            for en in &equip_nodes {
                if let Some(ref pid) = en.parent_id {
                    if let Some(group) = gmap.get_mut(pid) {
                        group.device_ids.push(en.id.clone());
                        d2g.insert(en.id.clone(), pid.clone());
                    }
                }
            }

            // Collect all accepted device IDs and display names
            let accepted_ids: Vec<String> = equip_nodes.iter().map(|en| en.id.clone()).collect();
            let device_names: HashMap<String, String> = equip_nodes
                .iter()
                .map(|en| (en.id.clone(), en.dis.clone()))
                .collect();

            // Get point counts per device from node store (child point nodes)
            let all_point_nodes = ns.list_nodes(Some("point"), None).await;
            let mut device_point_counts: HashMap<String, usize> = HashMap::new();
            for pn in &all_point_nodes {
                if let Some(ref parent) = pn.parent_id {
                    *device_point_counts.entry(parent.clone()).or_default() += 1;
                }
            }

            // Recompute group names from actual device display names
            for group in gmap.values_mut() {
                if let Some(first_dev_id) = group.device_ids.first() {
                    if let Some(en) = equip_nodes.iter().find(|e| &e.id == first_dev_id) {
                        group.name = suggest_group_name(&en.dis);
                    }
                }
            }

            let mut groups: Vec<DeviceGroup> = gmap
                .into_values()
                .filter(|g| !g.device_ids.is_empty())
                .collect();
            groups.sort_by(|a, b| a.name.cmp(&b.name));

            group_info.set(GroupInfo {
                groups,
                device_to_group: d2g,
                accepted_device_ids: accepted_ids,
                device_names,
                device_point_counts,
            });
        }
    });

    let gi = group_info.read();

    // Show all accepted devices (equip nodes in node store), filtered by search
    let q_lower = filter.to_lowercase();
    let device_ids: Vec<String> = gi
        .accepted_device_ids
        .iter()
        .filter(|id| {
            if filter.is_empty() {
                return true;
            }
            let name = gi.device_names.get(*id).map(|s| s.as_str()).unwrap_or("");
            id.to_lowercase().contains(&q_lower) || name.to_lowercase().contains(&q_lower)
        })
        .cloned()
        .collect();
    let selected = state.selected_device.read().clone();

    // Filter groups to only include devices in device_ids (respects search filter)
    let filtered_groups: Vec<DeviceGroup> = gi
        .groups
        .iter()
        .map(|g| DeviceGroup {
            id: g.id.clone(),
            name: g.name.clone(),
            device_ids: g
                .device_ids
                .iter()
                .filter(|did| device_ids.contains(did))
                .cloned()
                .collect(),
            point_set_json: g.point_set_json.clone(),
            related: g.related.clone(),
        })
        .filter(|g| !g.device_ids.is_empty())
        .collect();

    // Ungrouped devices
    let ungrouped: Vec<String> = device_ids
        .iter()
        .filter(|id| !gi.device_to_group.contains_key(*id))
        .cloned()
        .collect();

    rsx! {
        div { class: "device-tree",
            if device_ids.is_empty() && !filter.is_empty() {
                div { class: "tree-empty-search",
                    "No matches for \"{filter}\""
                }
            }
            ul { class: "tree-list",
                // Grouped devices
                for group in filtered_groups.iter() {
                    {
                        let gid = group.id.clone();
                        let gname = group.name.clone();
                        let member_count = group.device_ids.len();
                        let is_collapsed = collapsed_groups.read().contains(&gid);
                        let gid_toggle = gid.clone();

                        rsx! {
                            li { class: "tree-node tree-group",
                                div {
                                    class: "tree-node-row tree-group-row",
                                    onclick: move |_| {
                                        let mut cg = collapsed_groups.write();
                                        if let Some(pos) = cg.iter().position(|x| x == &gid_toggle) {
                                            cg.remove(pos);
                                        } else {
                                            cg.push(gid_toggle.clone());
                                        }
                                    },
                                    span {
                                        class: if is_collapsed { "tree-arrow" } else { "tree-arrow open" },
                                        "▶"
                                    }
                                    span { class: "tree-group-icon", "\u{25A0}" }
                                    span { class: "tree-label tree-group-label", "{gname}" }
                                    span { class: "tree-badge tree-group-badge", "{member_count}" }
                                }
                                if !is_collapsed {
                                    ul { class: "tree-list",
                                        for device_id in group.device_ids.iter() {
                                            {
                                                let dname = gi.device_names.get(device_id)
                                                    .map(|s| s.as_str())
                                                    .unwrap_or(device_id);
                                                let pc = gi.device_point_counts.get(device_id).copied().unwrap_or(0);
                                                render_device_leaf(&mut state, &selected, device_id, dname, pc)
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                // Ungrouped devices (configured or not yet grouped)
                for device_id in ungrouped.iter() {
                    {
                        let dname = gi.device_names.get(device_id)
                            .map(|s| s.as_str())
                            .unwrap_or(device_id);
                        let pc = gi.device_point_counts.get(device_id).copied().unwrap_or(0);
                        render_device_leaf(&mut state, &selected, device_id, dname, pc)
                    }
                }
            }
        }
    }
}

fn render_device_leaf(
    state: &mut AppState,
    selected: &Option<String>,
    device_id: &str,
    display_name: &str,
    point_count: usize,
) -> Element {
    let is_selected = selected.as_deref() == Some(device_id);
    let did = device_id.to_string();
    let label = display_name.to_string();
    let mut state = state.clone();
    rsx! {
        li {
            class: if is_selected { "tree-node leaf selected" } else { "tree-node leaf" },
            onclick: move |_| {
                state.selected_device.set(Some(did.clone()));
                state.selected_point.set(None);
                state.detail_open.set(false);
                state.active_view.set(ActiveView::Home);
            },
            span { class: "tree-label", "{label}" }
            span { class: "tree-badge", "{point_count}" }
        }
    }
}
