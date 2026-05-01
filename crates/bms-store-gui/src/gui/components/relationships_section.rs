use std::collections::HashMap;

use dioxus::prelude::*;

use crate::gui::state::{ActiveView, AppState};
use bms_store_storage::store::entity_store::Entity;
use bms_store_storage::store::node_store::NodeRecord;
use bms_store_storage::store::relationships::find_referrers;

/// Spatial ref tag names shown as location hierarchy.
const SPATIAL_REF_TAGS: &[&str] = &["siteRef", "buildingRef", "floorRef", "spaceRef"];

/// Functional relationship ref tags handled in dedicated sections.
const FUNCTIONAL_REF_TAGS: &[&str] = &["supplyRef", "returnRef", "connectedTo"];

/// Read-only display of spatial hierarchy and equipment relationships on the Home/device page.
#[component]
pub fn RelationshipsSection() -> Element {
    let mut state = use_context::<AppState>();
    let ns = state.node_store.clone();
    let es = state.entity_store.clone();
    let mut node_sig: Signal<Option<NodeRecord>> = use_signal(|| None);
    let mut incoming_sig: Signal<Vec<(String, NodeRecord)>> = use_signal(Vec::new);
    let mut entity_sig: Signal<Option<Entity>> = use_signal(|| None);
    {
        let ns = ns.clone();
        let es = es.clone();
        let _ = use_resource(move || {
            let ns = ns.clone();
            let es = es.clone();
            let did = state.selected_device.read().clone();
            let _nv = *state.node_version.read();
            async move {
                let Some(did) = did else {
                    node_sig.set(None);
                    incoming_sig.set(Vec::new());
                    entity_sig.set(None);
                    return;
                };
                if let Ok(node) = ns.get_node(&did).await {
                    node_sig.set(Some(node));
                } else {
                    node_sig.set(None);
                }
                let incoming = ns.find_all_referencing(&did).await;
                incoming_sig.set(incoming);

                // Load entity for functional ref info
                entity_sig.set(es.get_entity(&did).await.ok());
            }
        });
    }

    let node = node_sig.read();
    let incoming = incoming_sig.read();
    let entity = entity_sig.read();

    // Group incoming refs by tag
    let mut incoming_grouped: HashMap<String, Vec<&NodeRecord>> = HashMap::new();
    for (ref_tag, rec) in incoming.iter() {
        incoming_grouped
            .entry(ref_tag.clone())
            .or_default()
            .push(rec);
    }

    // Outgoing equipment refs — split spatial vs non-spatial
    let outgoing_refs: Vec<(&str, &str)> = node
        .as_ref()
        .map(|n| {
            n.refs
                .iter()
                .filter(|(k, _)| {
                    !SPATIAL_REF_TAGS.contains(&k.as_str())
                        && !FUNCTIONAL_REF_TAGS.contains(&k.as_str())
                })
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect()
        })
        .unwrap_or_default();

    // Spatial refs for location display
    let spatial_refs: Vec<(&str, &str)> = node
        .as_ref()
        .map(|n| {
            SPATIAL_REF_TAGS
                .iter()
                .filter_map(|tag| n.refs.get(*tag).map(|v| (*tag, v.as_str())))
                .collect()
        })
        .unwrap_or_default();

    // Functional refs from entity
    let supply_ref: Option<String> = entity
        .as_ref()
        .and_then(|e| e.refs.get("supplyRef").cloned());
    let return_ref: Option<String> = entity
        .as_ref()
        .and_then(|e| e.refs.get("returnRef").cloned());
    let connected_to: Option<String> = entity
        .as_ref()
        .and_then(|e| e.refs.get("connectedTo").cloned());

    // Entities that have supplyRef pointing to this device
    let supply_referrers_sig: Signal<Vec<Entity>> = use_signal(Vec::new);
    {
        let es2 = state.entity_store.clone();
        let mut supply_referrers_sig = supply_referrers_sig.clone();
        let _ = use_resource(move || {
            let es = es2.clone();
            let did = state.selected_device.read().clone();
            let _nv = *state.node_version.read();
            async move {
                if let Some(did) = did {
                    let referrers = find_referrers(&es, &did, "supplyRef").await;
                    supply_referrers_sig.set(referrers);
                } else {
                    supply_referrers_sig.set(Vec::new());
                }
            }
        });
    }

    let has_spatial = !spatial_refs.is_empty();
    let has_equip = !outgoing_refs.is_empty() || !incoming_grouped.is_empty();
    let has_functional = supply_ref.is_some()
        || return_ref.is_some()
        || connected_to.is_some()
        || !supply_referrers_sig.read().is_empty();

    if !has_spatial && !has_equip && !has_functional {
        return rsx! {};
    }

    let device_id_str = state.selected_device.read().clone().unwrap_or_default();

    rsx! {
        div { class: "relationships-section",
            // Location hierarchy
            if has_spatial {
                LocationChain { refs: spatial_refs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect() }
            }

            // ── Functional relationships ──────────────────────────────────
            FunctionalRelationships {
                device_id: device_id_str.clone(),
                supply_ref: supply_ref.clone(),
                return_ref: return_ref.clone(),
                connected_to: connected_to.clone(),
                supply_referrers: supply_referrers_sig.read().clone(),
            }

            // Equipment relationships (non-functional non-spatial)
            if has_equip {
                div { class: "relationships-block",
                    h3 { "Equipment Relationships" }

                    // Incoming: other equipment referencing this one
                    for (ref_tag, nodes) in incoming_grouped.iter() {
                        div { class: "rel-tag-group",
                            span { class: "rel-tag-label", "{ref_tag}" }
                            span { class: "rel-tag-count", "({nodes.len()})" }
                            ul { class: "rel-list",
                                for rec in nodes {
                                    {
                                        let rid = rec.id.clone();
                                        let rname = if rec.dis.is_empty() { rec.id.clone() } else { rec.dis.clone() };
                                        rsx! {
                                            li { class: "rel-item",
                                                span {
                                                    class: "rel-item-name clickable",
                                                    onclick: {
                                                        let rid = rid.clone();
                                                        move |_| {
                                                            state.selected_device.set(Some(rid.clone()));
                                                            state.active_view.set(ActiveView::Home);
                                                        }
                                                    },
                                                    "{rname}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Outgoing: this equipment's refs to other equipment
                    if !outgoing_refs.is_empty() {
                        ul { class: "rel-list",
                            for (ref_tag, target_id) in &outgoing_refs {
                                {
                                    let tid = target_id.to_string();
                                    let rtag = ref_tag.to_string();
                                    rsx! {
                                        li { class: "rel-item",
                                            span { class: "rel-tag-label", "{rtag}" }
                                            span { class: "rel-arrow", "\u{2192}" }
                                            RefTargetName { target_id: tid.clone() }
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
// FunctionalRelationships — supplyRef / returnRef / connectedTo
// ---------------------------------------------------------------------------

#[component]
fn FunctionalRelationships(
    device_id: String,
    supply_ref: Option<String>,
    return_ref: Option<String>,
    connected_to: Option<String>,
    supply_referrers: Vec<Entity>,
) -> Element {
    let mut state = use_context::<AppState>();
    let has_any = supply_ref.is_some()
        || return_ref.is_some()
        || connected_to.is_some()
        || !supply_referrers.is_empty();

    // Add-relationship modal state
    let mut show_add_modal: Signal<bool> = use_signal(|| false);
    let mut modal_ref_type: Signal<String> = use_signal(|| "supplyRef".to_string());
    let mut modal_target: Signal<String> = use_signal(String::new);
    let mut modal_status: Signal<Option<String>> = use_signal(|| None);

    // Fetch all equip entities once for target dropdown
    let es_list = state.entity_store.clone();
    let equip_list_res = use_resource(move || {
        let store = es_list.clone();
        async move { store.list_entities(Some("equip"), None).await }
    });
    let equip_read = equip_list_res.read();
    let equip_list = equip_read.as_deref().unwrap_or(&[]);

    // Dropdown filter
    let mut target_filter = use_signal(String::new);
    let filter_q = target_filter.read().to_lowercase();
    let filtered_equip: Vec<&Entity> = equip_list
        .iter()
        .filter(|e| e.id != device_id)
        .filter(|e| {
            filter_q.is_empty()
                || e.id.to_lowercase().contains(&filter_q)
                || e.dis.to_lowercase().contains(&filter_q)
        })
        .collect();

    // Delete confirmation
    let mut pending_delete: Signal<Option<String>> = use_signal(|| None); // tag name

    if !has_any && !*show_add_modal.read() {
        return rsx! {
            div { class: "relationships-block",
                h3 { "Functional Relationships" }
                p { class: "config-hint", "No supply/return/connection refs configured." }
                button {
                    class: "config-btn btn-sm",
                    onclick: move |_| {
                        modal_status.set(None);
                        show_add_modal.set(true);
                    },
                    "+ Add Relationship"
                }
            }
        };
    }

    rsx! {
        div { class: "relationships-block functional-relationships",
            h3 { "Functional Relationships" }

            // Supplies this equip (entities with supplyRef → this equip)
            if !supply_referrers.is_empty() {
                div { class: "rel-functional-group",
                    span { class: "rel-tag-label", "Supplies" }
                    span { class: "rel-tag-hint", " (entities receiving supply from this equipment)" }
                    ul { class: "rel-list",
                        for e in &supply_referrers {
                            {
                                let eid = e.id.clone();
                                let ename = if e.dis.is_empty() { e.id.clone() } else { e.dis.clone() };
                                rsx! {
                                    li { class: "rel-item",
                                        span {
                                            class: "rel-item-name clickable",
                                            onclick: move |_| {
                                                state.selected_device.set(Some(eid.clone()));
                                                state.active_view.set(ActiveView::Home);
                                            },
                                            "{ename}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Receives from (supplyRef target)
            if let Some(ref target) = supply_ref {
                div { class: "rel-functional-group",
                    span { class: "rel-tag-label", "Receives From (supplyRef)" }
                    {
                        let t = target.clone();
                        let t_del = target.clone();
                        let es_del = state.entity_store.clone();
                        let did_del = device_id.clone();
                        rsx! {
                            div { class: "rel-functional-item",
                                RefTargetName { target_id: t.clone() }
                                button {
                                    class: "config-tag-remove",
                                    title: "Remove supplyRef",
                                    onclick: move |_| {
                                        pending_delete.set(Some("supplyRef".to_string()));
                                    },
                                    "x"
                                }
                            }
                        }
                    }
                }
            }

            // returnRef
            if let Some(ref target) = return_ref {
                div { class: "rel-functional-group",
                    span { class: "rel-tag-label", "Return To (returnRef)" }
                    {
                        let t = target.clone();
                        rsx! {
                            div { class: "rel-functional-item",
                                RefTargetName { target_id: t.clone() }
                                button {
                                    class: "config-tag-remove",
                                    title: "Remove returnRef",
                                    onclick: move |_| {
                                        pending_delete.set(Some("returnRef".to_string()));
                                    },
                                    "x"
                                }
                            }
                        }
                    }
                }
            }

            // connectedTo
            if let Some(ref target) = connected_to {
                div { class: "rel-functional-group",
                    span { class: "rel-tag-label", "Connected To (connectedTo)" }
                    {
                        let t = target.clone();
                        rsx! {
                            div { class: "rel-functional-item",
                                RefTargetName { target_id: t.clone() }
                                button {
                                    class: "config-tag-remove",
                                    title: "Remove connectedTo",
                                    onclick: move |_| {
                                        pending_delete.set(Some("connectedTo".to_string()));
                                    },
                                    "x"
                                }
                            }
                        }
                    }
                }
            }

            // Add button
            button {
                class: "config-btn btn-sm",
                onclick: move |_| {
                    modal_status.set(None);
                    target_filter.set(String::new());
                    modal_target.set(String::new());
                    show_add_modal.set(true);
                },
                "+ Add Relationship"
            }

            // Delete confirmation inline
            if let Some(ref tag_to_delete) = *pending_delete.read() {
                {
                    let tag = tag_to_delete.clone();
                    let es_del = state.entity_store.clone();
                    let did_del = device_id.clone();
                    let tag_label = tag.clone();
                    rsx! {
                        div { class: "rel-delete-confirm",
                            span { "Remove {tag_label} relationship?" }
                            button {
                                class: "config-btn-danger btn-sm",
                                onclick: move |_| {
                                    let store = es_del.clone();
                                    let did = did_del.clone();
                                    let t = tag.clone();
                                    spawn(async move {
                                        let _ = store.remove_ref(&did, &t).await;
                                    });
                                    pending_delete.set(None);
                                },
                                "Confirm Delete"
                            }
                            button {
                                class: "config-btn btn-sm",
                                onclick: move |_| pending_delete.set(None),
                                "Cancel"
                            }
                        }
                    }
                }
            }

            // Add relationship modal
            if *show_add_modal.read() {
                div { class: "rel-add-modal-overlay",
                    onclick: move |_| show_add_modal.set(false),
                    div { class: "rel-add-modal",
                        onclick: move |e| e.stop_propagation(),
                        h4 { "Add Relationship" }

                        // Ref type
                        div { class: "rel-add-row",
                            label { "Type" }
                            select {
                                class: "config-input",
                                value: "{modal_ref_type}",
                                onchange: move |e| modal_ref_type.set(e.value()),
                                option { value: "supplyRef", "supplyRef — receives supply from" }
                                option { value: "returnRef", "returnRef — sends return to" }
                                option { value: "connectedTo", "connectedTo — connected to" }
                            }
                        }

                        // Target search
                        div { class: "rel-add-row",
                            label { "Target" }
                            input {
                                class: "config-input",
                                r#type: "text",
                                placeholder: "Search equipment...",
                                value: "{target_filter}",
                                oninput: move |e| target_filter.set(e.value()),
                            }
                        }

                        // Filtered list
                        div { class: "rel-target-list",
                            for eq in filtered_equip.iter().take(20) {
                                {
                                    let eid = eq.id.clone();
                                    let edis = if eq.dis.is_empty() { eq.id.clone() } else { eq.dis.clone() };
                                    let cur = modal_target.read().clone();
                                    let is_selected = cur == eid;
                                    rsx! {
                                        div {
                                            class: if is_selected { "rel-target-item selected" } else { "rel-target-item" },
                                            onclick: move |_| modal_target.set(eid.clone()),
                                            "{edis}"
                                        }
                                    }
                                }
                            }
                            if filtered_equip.is_empty() {
                                p { class: "config-hint", "No equipment found." }
                            }
                        }

                        if let Some(ref msg) = *modal_status.read() {
                            p { class: "config-hint", "{msg}" }
                        }

                        div { class: "rel-add-footer",
                            button {
                                class: "config-btn",
                                onclick: move |_| show_add_modal.set(false),
                                "Cancel"
                            }
                            button {
                                class: "config-btn config-btn-primary",
                                disabled: modal_target.read().is_empty(),
                                onclick: {
                                    let es_apply = state.entity_store.clone();
                                    let did_apply = device_id.clone();
                                    move |_| {
                                        let target = modal_target.read().clone();
                                        let ref_type = modal_ref_type.read().clone();
                                        if target.is_empty() { return; }
                                        let store = es_apply.clone();
                                        let did = did_apply.clone();
                                        let target_spawn = target.clone();
                                        let ref_type_spawn = ref_type.clone();
                                        let status_msg = format!("Applied {ref_type} \u{2192} {target}");
                                        spawn(async move {
                                            let _ = store.set_ref(&did, &ref_type_spawn, &target_spawn).await;
                                        });
                                        modal_status.set(Some(status_msg));
                                        show_add_modal.set(false);
                                    }
                                },
                                "Apply"
                            }
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// LocationChain
// ---------------------------------------------------------------------------

/// Resolves a spatial ref chain into a readable location line.
#[component]
fn LocationChain(refs: Vec<(String, String)>) -> Element {
    let state = use_context::<AppState>();
    let ns = state.node_store.clone();
    let nv = *state.node_version.read();

    let refs_clone = refs.clone();
    let mut names: Signal<Vec<(String, String)>> = use_signal(Vec::new);
    {
        let ns = ns.clone();
        let _ = use_resource(move || {
            let ns = ns.clone();
            let refs = refs_clone.clone();
            let _nv = nv;
            async move {
                let mut result = Vec::new();
                for (tag, id) in &refs {
                    let label = tag.replace("Ref", "");
                    let name = match ns.get_node(id).await {
                        Ok(n) => {
                            if n.dis.is_empty() {
                                id.clone()
                            } else {
                                n.dis.clone()
                            }
                        }
                        Err(_) => id.clone(),
                    };
                    result.push((label, name));
                }
                names.set(result);
            }
        });
    }

    let resolved = names.read();
    if resolved.is_empty() {
        return rsx! {};
    }

    rsx! {
        div { class: "relationships-block",
            h3 { "Location" }
            div { class: "location-chain",
                for (i, (label, name)) in resolved.iter().enumerate() {
                    if i > 0 {
                        span { class: "location-sep", "\u{203A}" }
                    }
                    span { class: "location-item",
                        span { class: "location-type", "{label}" }
                        span { class: "location-name", "{name}" }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// RefTargetName
// ---------------------------------------------------------------------------

/// Resolves and renders a target node's display name as a clickable link.
#[component]
fn RefTargetName(target_id: String) -> Element {
    let mut state = use_context::<AppState>();
    let ns = state.node_store.clone();
    let tid = target_id.clone();
    let nv = *state.node_version.read();

    let mut name: Signal<String> = use_signal(|| target_id.clone());
    {
        let ns = ns.clone();
        let tid = tid.clone();
        let _ = use_resource(move || {
            let ns = ns.clone();
            let tid = tid.clone();
            let _nv = nv;
            async move {
                if let Ok(n) = ns.get_node(&tid).await {
                    let label = if n.dis.is_empty() { tid } else { n.dis };
                    name.set(label);
                }
            }
        });
    }

    let display = name.read();
    rsx! {
        span {
            class: "rel-item-name clickable",
            onclick: {
                let tid = tid.clone();
                move |_| {
                    state.selected_device.set(Some(tid.clone()));
                    state.active_view.set(ActiveView::Home);
                }
            },
            "{display}"
        }
    }
}
