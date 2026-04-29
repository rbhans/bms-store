use dioxus::prelude::*;

use crate::gui::state::{
    insert_nav_child, remove_nav_node, ActiveView, AppState, NavNode, NavNodeKind,
};
use crate::node::{Node, NodeType};
use crate::store::node_store::NodeStore;

#[component]
pub fn NavTree() -> Element {
    let state = use_context::<AppState>();
    let tree = state.nav_tree.read().clone();

    rsx! {
        div { class: "nav-tree",
            if tree.is_empty() {
                div { class: "nav-empty",
                    p { "No items yet." }
                    p { class: "nav-empty-hint", "Click + to add a node." }
                }
            }
            ul { class: "tree-list",
                for node in &tree {
                    NavNodeView { node: node.clone(), depth: 0 }
                }
            }
            // Root-level add button
            AddNodeButton { parent_id: None, depth: 0, parent_spatial: "root".to_string() }
        }
    }
}

/// Determine the nearest spatial ancestor kind for context-sensitive dropdown.
/// Returns "root", "site", "building", "floor", "room", or "folder".
fn nearest_spatial_context(tree: &[NavNode], parent_nav_id: Option<&str>) -> &'static str {
    let Some(pid) = parent_nav_id else {
        return "root";
    };
    if let Some(node) = find_nav_node(tree, pid) {
        match &node.kind {
            NavNodeKind::Site { .. } => "site",
            NavNodeKind::Building { .. } => "building",
            NavNodeKind::Floor { .. } => "floor",
            NavNodeKind::Room { .. } => "room",
            NavNodeKind::Folder => {
                if let Some(parent) = find_nav_parent(tree, pid) {
                    return nearest_spatial_context(tree, Some(&parent.id));
                }
                "root"
            }
            _ => "root",
        }
    } else {
        "root"
    }
}

/// Find a nav node by ID in the tree.
fn find_nav_node<'a>(tree: &'a [NavNode], id: &str) -> Option<&'a NavNode> {
    for node in tree {
        if node.id == id {
            return Some(node);
        }
        if let Some(found) = find_nav_node(&node.children, id) {
            return Some(found);
        }
    }
    None
}

/// Find the parent of a nav node by ID.
pub fn find_nav_parent<'a>(tree: &'a [NavNode], child_id: &str) -> Option<&'a NavNode> {
    for node in tree {
        for child in &node.children {
            if child.id == child_id {
                return Some(node);
            }
        }
        if let Some(found) = find_nav_parent(&node.children, child_id) {
            return Some(found);
        }
    }
    None
}

/// Walk up nav tree collecting spatial ancestor node_store_ids.
pub fn collect_spatial_ancestors(tree: &[NavNode], nav_id: &str) -> SpatialRefs {
    let mut refs = SpatialRefs::default();
    collect_spatial_up(tree, nav_id, &mut refs);
    refs
}

#[derive(Default)]
pub struct SpatialRefs {
    pub site_ref: Option<String>,
    pub building_ref: Option<String>,
    pub floor_ref: Option<String>,
    pub space_ref: Option<String>,
}

fn collect_spatial_up(tree: &[NavNode], nav_id: &str, refs: &mut SpatialRefs) {
    if let Some(node) = find_nav_node(tree, nav_id) {
        match &node.kind {
            NavNodeKind::Room { node_id } => {
                if refs.space_ref.is_none() {
                    refs.space_ref = Some(node_id.clone());
                }
            }
            NavNodeKind::Floor { node_id } => {
                if refs.floor_ref.is_none() {
                    refs.floor_ref = Some(node_id.clone());
                }
            }
            NavNodeKind::Building { node_id } => {
                if refs.building_ref.is_none() {
                    refs.building_ref = Some(node_id.clone());
                }
            }
            NavNodeKind::Site { node_id } => {
                if refs.site_ref.is_none() {
                    refs.site_ref = Some(node_id.clone());
                }
            }
            _ => {}
        }
    }
    if let Some(parent) = find_nav_parent(tree, nav_id) {
        collect_spatial_up(tree, &parent.id.clone(), refs);
    }
}

/// Sync location refs on a device's equip node in NodeStore.
async fn sync_device_location_refs(
    node_store: &NodeStore,
    device_id: &str,
    nav_tree: &[NavNode],
    parent_nav_id: &str,
) {
    let refs = collect_spatial_ancestors(nav_tree, parent_nav_id);
    if let Some(site_id) = &refs.site_ref {
        let _ = node_store.set_ref(device_id, "siteRef", site_id).await;
    }
    if let Some(bldg_id) = &refs.building_ref {
        let _ = node_store.set_ref(device_id, "buildingRef", bldg_id).await;
    }
    if let Some(floor_id) = &refs.floor_ref {
        let _ = node_store.set_ref(device_id, "floorRef", floor_id).await;
    }
    if let Some(space_id) = &refs.space_ref {
        let _ = node_store.set_ref(device_id, "spaceRef", space_id).await;
    }
}

/// Clear all location refs from a device's equip node.
async fn clear_device_location_refs(node_store: &NodeStore, device_id: &str) {
    let _ = node_store.remove_ref(device_id, "siteRef").await;
    let _ = node_store.remove_ref(device_id, "buildingRef").await;
    let _ = node_store.remove_ref(device_id, "floorRef").await;
    let _ = node_store.remove_ref(device_id, "spaceRef").await;
}

/// Recursively collect all device IDs from a nav subtree.
fn collect_device_ids(node: &NavNode, out: &mut Vec<String>) {
    if let NavNodeKind::Device { device_id } = &node.kind {
        out.push(device_id.clone());
    }
    for child in &node.children {
        collect_device_ids(child, out);
    }
}

/// Collect all nav node IDs in a subtree (including the root node itself).
fn collect_nav_ids(node: &NavNode, out: &mut Vec<String>) {
    out.push(node.id.clone());
    for child in &node.children {
        collect_nav_ids(child, out);
    }
}

/// Inline add-node button + form.
#[component]
fn AddNodeButton(parent_id: Option<String>, depth: u32, parent_spatial: String) -> Element {
    let mut state = use_context::<AppState>();
    let mut adding = use_signal(|| false);
    let mut name_input = use_signal(|| String::new());
    let mut kind_choice = use_signal(|| String::new());
    let mut device_choice = use_signal(|| String::new());

    let is_adding = *adding.read();
    let is_child = parent_id.is_some();

    // Build device list from NodeStore (includes both scenario and discovered/accepted devices)
    let _node_ver = *state.node_version.read();
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
    let first_device = device_list
        .first()
        .map(|(id, _)| id.clone())
        .unwrap_or_default();

    // Context-sensitive kind options based on parent spatial context
    // Hierarchy: Site > Building > Floor > Room > Device
    let kind_options: Vec<(&str, &str)> = match parent_spatial.as_str() {
        "root" => vec![
            ("site", "Site"),
            ("building", "Building"),
            ("folder", "Folder"),
            ("page", "Page"),
        ],
        "site" => vec![
            ("building", "Building"),
            ("folder", "Folder"),
            ("page", "Page"),
            ("device", "Device"),
        ],
        "building" => vec![
            ("floor", "Floor"),
            ("folder", "Folder"),
            ("page", "Page"),
            ("device", "Device"),
        ],
        "floor" => vec![
            ("room", "Room"),
            ("folder", "Folder"),
            ("page", "Page"),
            ("device", "Device"),
        ],
        "room" => vec![("device", "Device"), ("folder", "Folder"), ("page", "Page")],
        // Under folder — allow all types
        _ => vec![
            ("site", "Site"),
            ("building", "Building"),
            ("floor", "Floor"),
            ("room", "Room"),
            ("folder", "Folder"),
            ("page", "Page"),
            ("device", "Device"),
        ],
    };

    let default_kind = kind_options
        .first()
        .map(|(v, _)| v.to_string())
        .unwrap_or("folder".into());

    if !is_adding {
        let label = if is_child { "+ Child" } else { "+ Add" };
        let btn_class = if is_child {
            "nav-add-btn nav-add-child"
        } else {
            "nav-add-btn"
        };
        return rsx! {
            button {
                class: btn_class,
                onclick: move |_| {
                    adding.set(true);
                    name_input.set(String::new());
                    kind_choice.set(default_kind.clone());
                    device_choice.set(first_device.clone());
                },
                "{label}"
            }
        };
    }

    let kind_str = kind_choice.read().clone();
    let pid = parent_id.clone();

    let confirm = move |_| {
        let label = name_input.read().trim().to_string();
        if label.is_empty() {
            return;
        }

        let nav_node_id = state.alloc_node_id();
        let kind_val = kind_choice.read().clone();
        let device_id_val = device_choice.read().clone();

        let kind = match kind_val.as_str() {
            "site" => {
                let store_id = format!("site-{}", uuid::Uuid::new_v4());
                NavNodeKind::Site { node_id: store_id }
            }
            "building" => {
                let store_id = format!("bldg-{}", uuid::Uuid::new_v4());
                NavNodeKind::Building { node_id: store_id }
            }
            "floor" => {
                let store_id = format!("floor-{}", uuid::Uuid::new_v4());
                NavNodeKind::Floor { node_id: store_id }
            }
            "room" => {
                let store_id = format!("room-{}", uuid::Uuid::new_v4());
                NavNodeKind::Room { node_id: store_id }
            }
            "page" => NavNodeKind::Page,
            "device" => NavNodeKind::Device {
                device_id: device_id_val.clone(),
            },
            _ => NavNodeKind::Folder,
        };

        let new_node = NavNode {
            id: nav_node_id.clone(),
            label: label.clone(),
            kind: kind.clone(),
            children: Vec::new(),
        };

        let mut tree = state.nav_tree.write();
        if let Some(ref parent) = pid {
            insert_nav_child(&mut tree, parent, new_node);
        } else {
            tree.push(new_node);
        }
        drop(tree);

        // Create NodeStore nodes for spatial kinds & sync refs for devices
        let ns = state.node_store.clone();
        let tree_snap = state.nav_tree.read().clone();
        let pid_clone = pid.clone();
        match &kind {
            NavNodeKind::Site { node_id } => {
                let nid = node_id.clone();
                let lbl = label.clone();
                let parent_store_id = pid_clone
                    .as_ref()
                    .and_then(|p| find_spatial_parent_store_id(&tree_snap, p));
                spawn(async move {
                    let mut node = Node::new(nid.clone(), NodeType::Site, lbl);
                    if let Some(psid) = parent_store_id {
                        node = node.with_parent(psid);
                    }
                    let _ = ns.create_node(node).await;
                });
            }
            NavNodeKind::Building { node_id } => {
                let nid = node_id.clone();
                let lbl = label.clone();
                let parent_store_id = pid_clone
                    .as_ref()
                    .and_then(|p| find_spatial_parent_store_id(&tree_snap, p));
                let ns2 = ns.clone();
                spawn(async move {
                    let mut node = Node::new(nid.clone(), NodeType::Space, lbl);
                    if let Some(psid) = parent_store_id {
                        node = node.with_parent(psid);
                    }
                    let _ = ns2.create_node(node).await;
                    let _ = ns2.set_tag(&nid, "building", None).await;
                });
            }
            NavNodeKind::Floor { node_id } => {
                let nid = node_id.clone();
                let lbl = label.clone();
                let parent_store_id = pid_clone
                    .as_ref()
                    .and_then(|p| find_spatial_parent_store_id(&tree_snap, p));
                let ns2 = ns.clone();
                spawn(async move {
                    let mut node = Node::new(nid.clone(), NodeType::Space, lbl);
                    if let Some(psid) = parent_store_id {
                        node = node.with_parent(psid);
                    }
                    let _ = ns2.create_node(node).await;
                    let _ = ns2.set_tag(&nid, "floor", None).await;
                });
            }
            NavNodeKind::Room { node_id } => {
                let nid = node_id.clone();
                let lbl = label.clone();
                let parent_store_id = pid_clone
                    .as_ref()
                    .and_then(|p| find_spatial_parent_store_id(&tree_snap, p));
                let ns2 = ns.clone();
                spawn(async move {
                    let mut node = Node::new(nid.clone(), NodeType::Space, lbl);
                    if let Some(psid) = parent_store_id {
                        node = node.with_parent(psid);
                    }
                    let _ = ns2.create_node(node).await;
                    let _ = ns2.set_tag(&nid, "room", None).await;
                });
            }
            NavNodeKind::Device { device_id } => {
                let did = device_id.clone();
                if let Some(ref parent_nav) = pid_clone {
                    let pnav = parent_nav.clone();
                    spawn(async move {
                        sync_device_location_refs(&ns, &did, &tree_snap, &pnav).await;
                    });
                }
            }
            _ => {}
        }

        // Spatial nodes act as pages (show floor plan canvas) AND navigate to them
        if kind.is_spatial() {
            state.active_view.set(ActiveView::Page(nav_node_id));
        } else {
            match &kind {
                NavNodeKind::Page => {
                    state.active_view.set(ActiveView::Page(nav_node_id));
                }
                NavNodeKind::Device { device_id } => {
                    state.selected_device.set(Some(device_id.clone()));
                    state.active_view.set(ActiveView::Device {
                        node_id: nav_node_id,
                        device_id: device_id.clone(),
                    });
                }
                _ => {}
            }
        }

        state.save_layout();
        adding.set(false);
    };

    rsx! {
        div { class: "nav-add-form",
            input {
                r#type: "text",
                placeholder: "Name",
                value: "{name_input}",
                oninput: move |e| name_input.set(e.value()),
            }
            select {
                value: "{kind_choice}",
                onchange: move |e| kind_choice.set(e.value()),
                for (val, display) in &kind_options {
                    option { value: *val, "{display}" }
                }
            }
            if kind_str == "device" {
                select {
                    value: "{device_choice}",
                    onchange: move |e| device_choice.set(e.value()),
                    for (did, label) in &device_list {
                        option { value: "{did}", "{label}" }
                    }
                }
            }
            div { class: "nav-add-actions",
                button {
                    class: "nav-confirm-btn",
                    onclick: confirm,
                    "Add"
                }
                button {
                    class: "nav-cancel-btn",
                    onclick: move |_| adding.set(false),
                    "Cancel"
                }
            }
        }
    }
}

/// Walk up nav tree from a nav node ID to find the nearest spatial ancestor's NodeStore ID.
fn find_spatial_parent_store_id(tree: &[NavNode], nav_id: &str) -> Option<String> {
    // Check the node itself first
    if let Some(node) = find_nav_node(tree, nav_id) {
        if let Some(id) = node.kind.node_store_id() {
            return Some(id.to_string());
        }
    }
    // Walk up
    if let Some(parent) = find_nav_parent(tree, nav_id) {
        if let Some(id) = parent.kind.node_store_id() {
            return Some(id.to_string());
        }
        return find_spatial_parent_store_id(tree, &parent.id);
    }
    None
}

#[component]
fn NavNodeView(node: NavNode, depth: u32) -> Element {
    let mut state = use_context::<AppState>();
    let mut expanded = use_signal(|| true);
    let is_open = *expanded.read();
    let has_children = !node.children.is_empty();
    let active_view = state.active_view.read().clone();
    let node_id = node.id.clone();

    // Spatial nodes use Page view for their canvas, so check page match too
    let is_active = match (&active_view, &node.kind) {
        (ActiveView::Page(id), NavNodeKind::Page) => id == &node.id,
        (ActiveView::Page(id), _) if node.kind.is_spatial() => id == &node.id,
        (ActiveView::Device { node_id: nid, .. }, NavNodeKind::Device { .. }) => nid == &node.id,
        _ => false,
    };

    let icon = match &node.kind {
        NavNodeKind::Folder => {
            if is_open && has_children {
                "\u{1F4C2}"
            } else {
                "\u{1F4C1}"
            }
        }
        NavNodeKind::Page => "\u{1F4C4}",
        NavNodeKind::Device { .. } => "\u{2699}\u{FE0F}",
        NavNodeKind::Site { .. } => "\u{1F3D8}\u{FE0F}",
        NavNodeKind::Building { .. } => "\u{1F3E2}",
        NavNodeKind::Floor { .. } => "\u{25A6}",
        NavNodeKind::Room { .. } => "\u{1F6AA}",
    };

    // Spatial nodes and folders are always expandable containers
    let is_container = matches!(
        node.kind,
        NavNodeKind::Folder
            | NavNodeKind::Site { .. }
            | NavNodeKind::Building { .. }
            | NavNodeKind::Floor { .. }
            | NavNodeKind::Room { .. }
    );

    // Determine spatial context for child add button
    let child_spatial = match &node.kind {
        NavNodeKind::Site { .. } => "site",
        NavNodeKind::Building { .. } => "building",
        NavNodeKind::Floor { .. } => "floor",
        NavNodeKind::Room { .. } => "room",
        NavNodeKind::Folder => {
            let tree = state.nav_tree.read();
            nearest_spatial_context(&tree, Some(&node.id))
        }
        _ => "root",
    };
    let child_spatial_str = child_spatial.to_string();

    let delete_id = node.id.clone();
    let delete_kind = node.kind.clone();
    let delete_node_clone = node.clone();
    let child_depth = depth + 1;

    rsx! {
        li { class: "tree-node",
            div {
                class: if is_active { "tree-node-row active" } else { "tree-node-row" },
                onclick: {
                    let nid = node_id.clone();
                    let kind = node.kind.clone();
                    move |_| {
                        match &kind {
                            NavNodeKind::Folder => {
                                expanded.set(!is_open);
                            }
                            // Spatial nodes: open their canvas page AND toggle expand
                            NavNodeKind::Site { .. }
                            | NavNodeKind::Building { .. }
                            | NavNodeKind::Floor { .. }
                            | NavNodeKind::Room { .. } => {
                                state.active_view.set(ActiveView::Page(nid.clone()));
                                if !is_open {
                                    expanded.set(true);
                                }
                            }
                            NavNodeKind::Page => {
                                state.active_view.set(ActiveView::Page(nid.clone()));
                            }
                            NavNodeKind::Device { device_id } => {
                                state.selected_device.set(Some(device_id.clone()));
                                state.selected_point.set(None);
                                state.detail_open.set(false);
                                state.active_view.set(ActiveView::Device {
                                    node_id: nid.clone(),
                                    device_id: device_id.clone(),
                                });
                            }
                        }
                    }
                },

                if has_children || is_container {
                    span {
                        class: if is_open { "tree-arrow open" } else { "tree-arrow" },
                        onclick: move |e| {
                            e.stop_propagation();
                            expanded.set(!is_open);
                        },
                        "\u{25B6}"
                    }
                } else {
                    span { class: "tree-arrow-spacer" }
                }

                span { class: "nav-icon", "{icon}" }

                span { class: "tree-label", "{node.label}" }

                // Delete button (appears on hover via CSS)
                button {
                    class: "nav-delete-btn",
                    title: "Delete",
                    onclick: {
                        let did = delete_id.clone();
                        let dkind = delete_kind.clone();
                        let dnode = delete_node_clone.clone();
                        move |e: Event<MouseData>| {
                            e.stop_propagation();
                            let is_active = match &*state.active_view.read() {
                                ActiveView::Page(id) => id == &did,
                                ActiveView::Device { node_id, .. } => node_id == &did,
                                _ => false,
                            };

                            // For spatial nodes: clean up NodeStore and device refs
                            let ns = state.node_store.clone();
                            if dkind.is_spatial() {
                                let mut device_ids = Vec::new();
                                collect_device_ids(&dnode, &mut device_ids);
                                let store_id = dkind.node_store_id().map(|s| s.to_string());
                                spawn(async move {
                                    for dev_id in &device_ids {
                                        clear_device_location_refs(&ns, dev_id).await;
                                    }
                                    if let Some(sid) = store_id {
                                        let _ = ns.delete_node(&sid).await;
                                    }
                                });
                            } else if let NavNodeKind::Device { device_id } = &dkind {
                                let dev_id = device_id.clone();
                                spawn(async move {
                                    clear_device_location_refs(&ns, &dev_id).await;
                                });
                            }

                            // Remove zones from page data whose nav_node_id is in the deleted subtree
                            let mut deleted_nav_ids = Vec::new();
                            collect_nav_ids(&dnode, &mut deleted_nav_ids);
                            {
                                let deleted_set: std::collections::HashSet<&str> =
                                    deleted_nav_ids.iter().map(|s| s.as_str()).collect();
                                let mut pages = state.pages.write();
                                for page_data in pages.values_mut() {
                                    page_data.zones.retain(|z| {
                                        z.nav_node_id.as_deref().map_or(true, |nid| !deleted_set.contains(nid))
                                    });
                                }
                            }

                            let mut tree = state.nav_tree.write();
                            remove_nav_node(&mut tree, &did);
                            drop(tree);
                            state.save_layout();
                            if is_active {
                                state.active_view.set(ActiveView::Home);
                            }
                        }
                    },
                    "\u{00D7}"
                }
            }

            if is_open {
                ul { class: "tree-list",
                    for child in &node.children {
                        NavNodeView { node: child.clone(), depth: child_depth }
                    }
                }
                AddNodeButton {
                    parent_id: Some(node_id.clone()),
                    depth: child_depth,
                    parent_spatial: child_spatial_str.clone(),
                }
            }
        }
    }
}

/// Location breadcrumb component for device pages.
#[component]
pub fn LocationBreadcrumb() -> Element {
    let state = use_context::<AppState>();
    let ns = state.node_store.clone();

    let ancestors = use_resource(move || {
        let ns = ns.clone();
        let did = state.selected_device.read().clone();
        let _nv = *state.node_version.read();
        async move {
            let did = did?;
            let node = ns.get_node(&did).await.ok()?;
            let mut parts: Vec<(String, String)> = Vec::new();

            // Read location refs in hierarchy order
            if let Some(site_id) = node.refs.get("siteRef") {
                if let Ok(site) = ns.get_node(site_id).await {
                    parts.push(("site".into(), site.dis));
                }
            }
            if let Some(bldg_id) = node.refs.get("buildingRef") {
                if let Ok(bldg) = ns.get_node(bldg_id).await {
                    parts.push(("building".into(), bldg.dis));
                }
            }
            if let Some(floor_id) = node.refs.get("floorRef") {
                if let Ok(floor) = ns.get_node(floor_id).await {
                    parts.push(("floor".into(), floor.dis));
                }
            }
            if let Some(space_id) = node.refs.get("spaceRef") {
                if let Ok(space) = ns.get_node(space_id).await {
                    parts.push(("room".into(), space.dis));
                }
            }
            if parts.is_empty() {
                None
            } else {
                Some(parts)
            }
        }
    });

    let binding = ancestors.read();
    let Some(Some(parts)) = binding.as_ref() else {
        return rsx! {};
    };

    let parts = parts.clone();
    drop(binding);
    let last_idx = parts.len() - 1;

    rsx! {
        div { class: "location-breadcrumb",
            for (i, (_kind, name)) in parts.iter().enumerate() {
                span { class: "location-breadcrumb-item", "{name}" }
                if i < last_idx {
                    span { class: "location-breadcrumb-sep", "\u{203A}" }
                }
            }
        }
    }
}
