use dioxus::prelude::*;

use crate::config::profile::PointValue;
use crate::gui::state::AppState;
use crate::node::{Node, NodeCapabilities, NodeType, ProtocolBinding};
use crate::store::node_store::NodeRecord;
use crate::store::point_store::PointKey;

// ----------------------------------------------------------------
// VirtualPointsView — manage virtual/calculated points
// ----------------------------------------------------------------

#[component]
pub fn VirtualPointsView() -> Element {
    let state = use_context::<AppState>();
    let mut version = use_signal(|| 0u64);
    let mut selected_id: Signal<Option<String>> = use_signal(|| None);

    // Form state for creating new virtual points
    let mut new_name = use_signal(|| String::new());
    let mut new_data_type = use_signal(|| "float".to_string());
    let mut new_parent = use_signal(|| String::new());
    let mut new_default = use_signal(|| String::new());
    let mut create_error: Signal<Option<String>> = use_signal(|| None);

    // Load virtual points from NodeStore
    let ns = state.node_store.clone();
    let virtual_points: Signal<Vec<NodeRecord>> = use_signal(Vec::new);
    let equip_nodes: Signal<Vec<NodeRecord>> = use_signal(Vec::new);

    {
        let ns2 = ns.clone();
        let mut vp = virtual_points.clone();
        let mut eq = equip_nodes.clone();
        let _v = *version.read(); // reactive dependency
        let _ = use_resource(move || {
            let ns = ns2.clone();
            async move {
                vp.set(ns.list_nodes(Some("virtual_point"), None).await);
                eq.set(ns.list_nodes(Some("equip"), None).await);
            }
        });
    }

    let points = virtual_points.read();
    let equips = equip_nodes.read();
    let sel = selected_id.read().clone();

    let selected_point = sel
        .as_ref()
        .and_then(|id| points.iter().find(|p| p.id == *id));

    rsx! {
        div { class: "virtual-points-view",
            // Left panel: list of virtual points + create form
            div { class: "vp-list-panel",
                h3 { class: "vp-section-title", "Virtual Points" }

                // Create form
                div { class: "vp-create-form",
                    div { class: "vp-form-row",
                        label { "Name" }
                        input {
                            r#type: "text",
                            placeholder: "e.g. zone-1-sp",
                            value: "{new_name}",
                            oninput: move |e| new_name.set(e.value().clone()),
                        }
                    }
                    div { class: "vp-form-row",
                        label { "Type" }
                        select {
                            value: "{new_data_type}",
                            onchange: move |e| new_data_type.set(e.value().clone()),
                            option { value: "float", "Float" }
                            option { value: "integer", "Integer" }
                            option { value: "bool", "Bool" }
                        }
                    }
                    div { class: "vp-form-row",
                        label { "Parent" }
                        select {
                            value: "{new_parent}",
                            onchange: move |e| new_parent.set(e.value().clone()),
                            option { value: "", "(none)" }
                            for eq in equips.iter() {
                                option {
                                    value: "{eq.id}",
                                    "{eq.dis} ({eq.id})"
                                }
                            }
                        }
                    }
                    div { class: "vp-form-row",
                        label { "Default" }
                        input {
                            r#type: "text",
                            placeholder: "optional initial value",
                            value: "{new_default}",
                            oninput: move |e| new_default.set(e.value().clone()),
                        }
                    }
                    {
                        let ns = ns.clone();
                        let store = state.store.clone();
                        rsx! {
                            button {
                                class: "vp-create-btn",
                                onclick: move |_| {
                                    let name_val = new_name.peek().clone();
                                    if name_val.trim().is_empty() {
                                        create_error.set(Some("Name is required".into()));
                                        return;
                                    }
                                    let parent_val = new_parent.peek().clone();
                                    let data_type_val = new_data_type.peek().clone();
                                    let default_val = new_default.peek().clone();

                                    // Build node ID: parent/name or just name
                                    let node_id = if parent_val.is_empty() {
                                        format!("vp/{}", name_val.trim())
                                    } else {
                                        format!("{}/{}", parent_val, name_val.trim())
                                    };

                                    let ns = ns.clone();
                                    let store = store.clone();
                                    spawn(async move {
                                        let mut node = Node::new(
                                            node_id.clone(),
                                            NodeType::VirtualPoint,
                                            name_val.trim().to_string(),
                                        )
                                        .with_capabilities(NodeCapabilities {
                                            readable: true,
                                            writable: true,
                                            historizable: true,
                                            alarmable: true,
                                            schedulable: false,
                                        })
                                        .with_binding(ProtocolBinding::virtual_binding());

                                        if !parent_val.is_empty() {
                                            node = node.with_parent(parent_val.clone());
                                        }

                                        // Add data type as a property
                                        node.properties.insert("data_type".into(), data_type_val.clone());

                                        // Add virtual point tag
                                        node.tags.insert("virtual".into(), None);
                                        node.tags.insert("point".into(), None);

                                        match ns.create_node(node).await {
                                            Ok(()) => {
                                                // Set initial value in PointStore
                                                let initial = parse_default_value(&data_type_val, &default_val);
                                                let (dev, pt) = if let Some((d, p)) = node_id.split_once('/') {
                                                    (d.to_string(), p.to_string())
                                                } else {
                                                    ("vp".to_string(), node_id.clone())
                                                };
                                                store.set(
                                                    PointKey {
                                                        device_instance_id: dev,
                                                        point_id: pt,
                                                    },
                                                    initial,
                                                );
                                                create_error.set(None);
                                                version += 1;
                                                new_name.set(String::new());
                                                new_default.set(String::new());
                                            }
                                            Err(e) => {
                                                create_error.set(Some(format!("{e}")));
                                            }
                                        }
                                    });
                                },
                                "Create Virtual Point"
                            }
                        }
                    }
                    if let Some(ref err) = *create_error.read() {
                        div { class: "vp-error", "{err}" }
                    }
                }

                // Point list
                div { class: "vp-point-list",
                    if points.is_empty() {
                        p { class: "vp-empty", "No virtual points yet. Create one above." }
                    }
                    for vp in points.iter() {
                        {
                            let id = vp.id.clone();
                            let is_selected = sel.as_ref() == Some(&id);
                            rsx! {
                                div {
                                    class: if is_selected { "vp-item selected" } else { "vp-item" },
                                    onclick: move |_| {
                                        selected_id.set(Some(id.clone()));
                                    },
                                    div { class: "vp-item-name", "{vp.dis}" }
                                    div { class: "vp-item-id", "{vp.id}" }
                                }
                            }
                        }
                    }
                }
            }

            // Right panel: detail/edit for selected virtual point
            div { class: "vp-detail-panel",
                if let Some(vp) = selected_point {
                    VirtualPointDetail {
                        point: vp.clone(),
                        version,
                    }
                } else {
                    div { class: "vp-detail-empty",
                        p { "Select a virtual point to view details and set its value." }
                    }
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Detail panel for a single virtual point
// ----------------------------------------------------------------

#[component]
fn VirtualPointDetail(point: NodeRecord, mut version: Signal<u64>) -> Element {
    let state = use_context::<AppState>();
    let mut write_value = use_signal(|| String::new());
    let mut write_status: Signal<Option<String>> = use_signal(|| None);
    let mut confirm_delete = use_signal(|| false);

    let data_type = point
        .properties
        .get("data_type")
        .cloned()
        .unwrap_or_else(|| "float".into());

    // Get live value from PointStore
    let _sv = state.store_version.read();
    let (dev_id, pt_id) = if let Some((d, p)) = point.id.split_once('/') {
        (d.to_string(), p.to_string())
    } else {
        ("vp".to_string(), point.id.clone())
    };
    let live = state.store.get(&PointKey {
        device_instance_id: dev_id.clone(),
        point_id: pt_id.clone(),
    });

    rsx! {
        div { class: "vp-detail-content",
            h3 { class: "vp-detail-title", "{point.dis}" }

            dl { class: "detail-grid",
                dt { "ID" }
                dd { "{point.id}" }

                dt { "Data Type" }
                dd { "{data_type}" }

                if let Some(ref pid) = point.parent_id {
                    dt { "Parent" }
                    dd { "{pid}" }
                }

                dt { "Current Value" }
                dd { class: "live-value",
                    if let Some(ref tv) = live {
                        "{tv.value:?}"
                    } else {
                        "(no value)"
                    }
                }

                if let Some(ref tv) = live {
                    if !tv.status.is_normal() {
                        dt { "Status" }
                        dd {
                            for flag in tv.status.active_flags() {
                                span { class: "status-badge status-{flag}", "{flag}" }
                            }
                        }
                    }
                }
            }

            // Manual write section
            div { class: "vp-write-section",
                h4 { "Write Value" }
                div { class: "vp-write-form",
                    if data_type == "bool" {
                        select {
                            value: "{write_value}",
                            onchange: move |e| write_value.set(e.value().clone()),
                            option { value: "", "(select)" }
                            option { value: "true", "true" }
                            option { value: "false", "false" }
                        }
                    } else {
                        input {
                            r#type: "text",
                            placeholder: if data_type == "integer" { "e.g. 72" } else { "e.g. 72.5" },
                            value: "{write_value}",
                            oninput: move |e| write_value.set(e.value().clone()),
                        }
                    }
                    {
                        let store = state.store.clone();
                        let dt = data_type.clone();
                        let dev = dev_id.clone();
                        let pt = pt_id.clone();
                        rsx! {
                            button {
                                class: "vp-write-btn",
                                onclick: move |_| {
                                    let val_str = write_value.peek().clone();
                                    if val_str.is_empty() {
                                        write_status.set(Some("Enter a value".into()));
                                        return;
                                    }
                                    match parse_typed_value(&dt, &val_str) {
                                        Some(pv) => {
                                            store.set(
                                                PointKey {
                                                    device_instance_id: dev.clone(),
                                                    point_id: pt.clone(),
                                                },
                                                pv,
                                            );
                                            write_status.set(Some("Written".into()));
                                            write_value.set(String::new());
                                        }
                                        None => {
                                            write_status.set(Some(format!("Invalid {dt} value")));
                                        }
                                    }
                                },
                                "Write"
                            }
                        }
                    }
                }
                if let Some(ref status) = *write_status.read() {
                    div {
                        class: if status == "Written" { "vp-write-ok" } else { "vp-write-err" },
                        "{status}"
                    }
                }
            }

            // Delete section
            div { class: "vp-delete-section",
                if *confirm_delete.read() {
                    span { "Delete this virtual point? " }
                    {
                        let ns = state.node_store.clone();
                        let pid = point.id.clone();
                        rsx! {
                            button {
                                class: "vp-delete-confirm",
                                onclick: move |_| {
                                    let ns = ns.clone();
                                    let pid = pid.clone();
                                    spawn(async move {
                                        let _ = ns.delete_node(&pid).await;
                                        version += 1;
                                    });
                                    confirm_delete.set(false);
                                },
                                "Yes, Delete"
                            }
                        }
                    }
                    button {
                        class: "vp-delete-cancel",
                        onclick: move |_| confirm_delete.set(false),
                        "Cancel"
                    }
                } else {
                    button {
                        class: "vp-delete-btn",
                        onclick: move |_| confirm_delete.set(true),
                        "Delete Point"
                    }
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------

fn parse_default_value(data_type: &str, raw: &str) -> PointValue {
    if raw.trim().is_empty() {
        return match data_type {
            "bool" => PointValue::Bool(false),
            "integer" => PointValue::Integer(0),
            _ => PointValue::Float(0.0),
        };
    }
    parse_typed_value(data_type, raw).unwrap_or_else(|| match data_type {
        "bool" => PointValue::Bool(false),
        "integer" => PointValue::Integer(0),
        _ => PointValue::Float(0.0),
    })
}

fn parse_typed_value(data_type: &str, raw: &str) -> Option<PointValue> {
    match data_type {
        "bool" => match raw.trim().to_lowercase().as_str() {
            "true" | "1" | "on" | "yes" => Some(PointValue::Bool(true)),
            "false" | "0" | "off" | "no" => Some(PointValue::Bool(false)),
            _ => None,
        },
        "integer" => raw.trim().parse::<i64>().ok().map(PointValue::Integer),
        _ => raw.trim().parse::<f64>().ok().map(PointValue::Float),
    }
}
