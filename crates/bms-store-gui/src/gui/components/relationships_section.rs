use std::collections::HashMap;

use dioxus::prelude::*;

use crate::gui::state::{ActiveView, AppState};
use bms_store_storage::store::node_store::NodeRecord;

/// Spatial ref tag names shown as location hierarchy.
const SPATIAL_REF_TAGS: &[&str] = &["siteRef", "buildingRef", "floorRef", "spaceRef"];

/// Read-only display of spatial hierarchy and equipment relationships on the Home/device page.
#[component]
pub fn RelationshipsSection() -> Element {
    let mut state = use_context::<AppState>();
    let ns = state.node_store.clone();
    let mut node_sig: Signal<Option<NodeRecord>> = use_signal(|| None);
    let mut incoming_sig: Signal<Vec<(String, NodeRecord)>> = use_signal(Vec::new);
    {
        let ns = ns.clone();
        let _ = use_resource(move || {
            let ns = ns.clone();
            let did = state.selected_device.read().clone();
            let _nv = *state.node_version.read();
            async move {
                let Some(did) = did else {
                    node_sig.set(None);
                    incoming_sig.set(Vec::new());
                    return;
                };
                if let Ok(node) = ns.get_node(&did).await {
                    node_sig.set(Some(node));
                } else {
                    node_sig.set(None);
                }
                let incoming = ns.find_all_referencing(&did).await;
                incoming_sig.set(incoming);
            }
        });
    }

    let node = node_sig.read();
    let incoming = incoming_sig.read();

    // Group incoming refs by tag
    let mut incoming_grouped: HashMap<String, Vec<&NodeRecord>> = HashMap::new();
    for (ref_tag, rec) in incoming.iter() {
        incoming_grouped
            .entry(ref_tag.clone())
            .or_default()
            .push(rec);
    }

    // Outgoing equipment refs (exclude spatial — shown separately)
    let outgoing_refs: Vec<(&str, &str)> = node
        .as_ref()
        .map(|n| {
            n.refs
                .iter()
                .filter(|(k, _)| !SPATIAL_REF_TAGS.contains(&k.as_str()))
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

    let has_spatial = !spatial_refs.is_empty();
    let has_equip = !outgoing_refs.is_empty() || !incoming_grouped.is_empty();

    if !has_spatial && !has_equip {
        return rsx! {};
    }

    rsx! {
        div { class: "relationships-section",
            // Location hierarchy
            if has_spatial {
                LocationChain { refs: spatial_refs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect() }
            }

            // Equipment relationships
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
