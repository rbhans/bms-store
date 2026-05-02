use dioxus::prelude::*;

use crate::gui::state::{
    insert_nav_child, remove_nav_node, ActiveView, AppState, NavNode, NavNodeKind,
};
use bms_core::node::{Node, NodeType};
use bms_store_storage::store::node_store::NodeStore;

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
            // FloorArea + Room both populate space_ref — they're both
            // sub-floor spaces in Project Haystack terms.
            NavNodeKind::Room { node_id } | NavNodeKind::FloorArea { node_id } => {
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
        }
    }
    if let Some(parent) = find_nav_parent(tree, nav_id) {
        collect_spatial_up(tree, &parent.id.clone(), refs);
    }
}

/// Devices are linked to spatial nodes via `siteRef` / `buildingRef` /
/// `floorRef` / `spaceRef` on the device's equip node in `NodeStore`.
/// They no longer appear as nav-tree leaves; this helper is kept so
/// future "Move device to space" UI can call it directly.
#[allow(dead_code)]
async fn clear_device_location_refs(node_store: &NodeStore, device_id: &str) {
    let _ = node_store.remove_ref(device_id, "siteRef").await;
    let _ = node_store.remove_ref(device_id, "buildingRef").await;
    let _ = node_store.remove_ref(device_id, "floorRef").await;
    let _ = node_store.remove_ref(device_id, "spaceRef").await;
}

/// Inline add-node button + form.
#[component]
fn AddNodeButton(parent_id: Option<String>, depth: u32, parent_spatial: String) -> Element {
    let mut state = use_context::<AppState>();
    let mut adding = use_signal(|| false);
    let mut name_input = use_signal(|| String::new());
    let mut kind_choice = use_signal(|| String::new());

    let is_adding = *adding.read();
    let is_child = parent_id.is_some();

    // Context-sensitive kind options. The Nav tab is the building / campus
    // hierarchy only — Site → Building → Floor → (FloorArea) → Room.
    let kind_options: Vec<(&str, &str)> = match parent_spatial.as_str() {
        "root" => vec![("site", "Site")],
        "site" => vec![("building", "Building")],
        "building" => vec![("floor", "Floor")],
        "floor" => vec![("floorArea", "Floor Area"), ("room", "Room")],
        "floorArea" => vec![("room", "Room")],
        // Room is a leaf — no further children. Empty options collapses
        // the Add button below.
        _ => vec![],
    };

    let default_kind = kind_options
        .first()
        .map(|(v, _)| v.to_string())
        .unwrap_or_default();

    if kind_options.is_empty() {
        // Leaf — nothing to add here.
        return rsx! {};
    }

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
                },
                "{label}"
            }
        };
    }

    let pid = parent_id.clone();

    let confirm = move |_| {
        let label = name_input.read().trim().to_string();
        if label.is_empty() {
            return;
        }

        let nav_node_id = state.alloc_node_id();
        let kind_val = kind_choice.read().clone();

        let kind = match kind_val.as_str() {
            "site" => NavNodeKind::Site {
                node_id: format!("site-{}", uuid::Uuid::new_v4()),
            },
            "building" => NavNodeKind::Building {
                node_id: format!("bldg-{}", uuid::Uuid::new_v4()),
            },
            "floor" => NavNodeKind::Floor {
                node_id: format!("floor-{}", uuid::Uuid::new_v4()),
            },
            "floorArea" => NavNodeKind::FloorArea {
                node_id: format!("flarea-{}", uuid::Uuid::new_v4()),
            },
            "room" => NavNodeKind::Room {
                node_id: format!("room-{}", uuid::Uuid::new_v4()),
            },
            // Unknown — bail out to avoid creating a malformed node.
            _ => return,
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
            NavNodeKind::FloorArea { node_id } => {
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
                    let _ = ns2.set_tag(&nid, "floorArea", None).await;
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
        }

        // Every nav node is spatial — open its canvas page.
        state.active_view.set(ActiveView::Page(nav_node_id));

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

    let is_active = match &active_view {
        ActiveView::Page(id) => id == &node.id,
        _ => false,
    };

    let icon = match &node.kind {
        NavNodeKind::Site { .. } => "\u{1F3D8}\u{FE0F}",
        NavNodeKind::Building { .. } => "\u{1F3E2}",
        NavNodeKind::Floor { .. } => "\u{25A6}",
        NavNodeKind::FloorArea { .. } => "\u{25A3}",
        NavNodeKind::Room { .. } => "\u{1F6AA}",
    };

    // Every kind except Room can have children. Rooms are leaves.
    let is_container = !matches!(node.kind, NavNodeKind::Room { .. });

    // Determine spatial context for child add button
    let child_spatial_str = node.kind.kind_str().to_string();

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
                    move |_| {
                        // Every nav node opens its canvas page; non-leaf
                        // nodes also auto-expand on click.
                        state.active_view.set(ActiveView::Page(nid.clone()));
                        if is_container && !is_open {
                            expanded.set(true);
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
                        move |e: Event<MouseData>| {
                            e.stop_propagation();
                            let is_active = matches!(
                                &*state.active_view.read(),
                                ActiveView::Page(id) if id == &did
                            );

                            // Clean up the NodeStore node. Devices that
                            // referenced this space keep their stale ref
                            // for now; a future "Move device to space"
                            // modal can clean them up.
                            let ns = state.node_store.clone();
                            if let Some(sid) = dkind.node_store_id().map(|s| s.to_string()) {
                                spawn(async move {
                                    let _ = ns.delete_node(&sid).await;
                                });
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
