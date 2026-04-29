use std::collections::HashMap;

use dioxus::prelude::*;

use crate::discovery::model::{DeviceState, DiscoveredDevice, DiscoveredPoint, PointKindHint};
use crate::discovery::naming;
use crate::gui::state::AppState;

use super::discovery_utils::{bump, kind_label, DeviceGroup};

/// Renders the group editor panel (right pane when a group is selected).
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_group_editor(
    state: &AppState,
    group: &DeviceGroup,
    all_devices: &[DiscoveredDevice],
    mut selected_group: Signal<Option<u64>>,
    mut group_name_drafts: Signal<HashMap<String, String>>,
    mut group_find_text: Signal<String>,
    mut group_replace_text: Signal<String>,
    mut group_template_text: Signal<String>,
    mut group_status: Signal<Option<String>>,
    mut group_point_name_drafts: Signal<HashMap<String, String>>,
    mut group_point_units_drafts: Signal<HashMap<String, String>>,
    mut group_point_state_labels_drafts: Signal<HashMap<String, Option<HashMap<String, String>>>>,
    group_shared_points: Signal<Vec<DiscoveredPoint>>,
    mut refresh_counter: Signal<u64>,
) -> Element {
    let group_name = group.name.clone();
    let kind_sig = group.kind_sig.clone();
    let group_ids = group.device_ids.clone();
    let group_devs: Vec<&DiscoveredDevice> = all_devices
        .iter()
        .filter(|d| group.device_ids.contains(&d.id))
        .collect();
    let dev_count = group_devs.len();
    // Build name lookup for closures (avoids borrowing all_devices)
    let dev_name_map: HashMap<String, String> = group_devs
        .iter()
        .map(|d| (d.id.clone(), d.display_name.clone()))
        .collect();
    let svc_rename = state.discovery_service.clone();
    let svc_accept = state.discovery_service.clone();
    let svc_ignore = state.discovery_service.clone();
    let svc_template = state.discovery_service.clone();

    let audit_state = state.clone();
    rsx! {
        div { class: "discovery-group-editor",
            // Header
            div { class: "discovery-group-editor-header",
                button {
                    class: "discovery-back-btn",
                    onclick: move |_| selected_group.set(None),
                    "← Back"
                }
                h3 { "{group_name}" }
                span { class: "discovery-group-badge", "{dev_count} devices" }
                span { class: "discovery-kind-sig", "{kind_sig}" }
            }

            // Device naming table
            div { class: "discovery-seq-table",
                table { class: "discovery-point-table",
                    thead {
                        tr {
                            th { "#" }
                            th { "Name" }
                            th { "Address" }
                            th { "State" }
                        }
                    }
                    tbody {
                        for (idx, dev) in group_devs.iter().enumerate() {
                            {
                                let dev_id = dev.id.clone();
                                let draft_key = dev.id.clone();
                                let draft = group_name_drafts
                                    .read()
                                    .get(&dev.id)
                                    .cloned()
                                    .unwrap_or_else(|| dev.display_name.clone());
                                rsx! {
                                    tr { key: "{dev_id}",
                                        td { class: "text-muted", "{idx + 1}" }
                                        td {
                                            input {
                                                class: "discovery-seq-name-input",
                                                r#type: "text",
                                                value: "{draft}",
                                                oninput: move |evt: Event<FormData>| {
                                                    group_name_drafts.write().insert(draft_key.clone(), evt.value());
                                                },
                                            }
                                        }
                                        td { class: "text-muted", "{dev.address}" }
                                        td {
                                            span {
                                                class: match dev.state {
                                                    DeviceState::Accepted => "discovery-state-badge accepted",
                                                    DeviceState::Ignored => "discovery-state-badge ignored",
                                                    _ => "discovery-state-badge pending",
                                                },
                                                "{dev.state.as_str()}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Auto-Sequence toolbar
            div { class: "discovery-seq-toolbar",
                button {
                    class: "discovery-action-btn accept primary",
                    onclick: {
                        let group_ids = group_ids.clone();
                        move |_| {
                            // Collect named devices as examples for pattern detection
                            let examples: Vec<String> = {
                                let drafts = group_name_drafts.read();
                                group_ids
                                    .iter()
                                    .filter_map(|id| drafts.get(id).filter(|n| !n.is_empty()))
                                    .cloned()
                                    .collect()
                            };

                            if examples.is_empty() {
                                group_status.set(Some("Name at least one device first.".into()));
                                return;
                            }

                            let example_refs: Vec<&str> = examples.iter().map(|s| s.as_str()).collect();
                            match naming::detect_pattern(&example_refs) {
                                Some(pattern) => {
                                    let already_named = examples.len();
                                    let remaining = group_ids.len().saturating_sub(already_named);
                                    let names = naming::generate_names(&pattern, remaining);
                                    let mut drafts = group_name_drafts.write();
                                    let mut name_idx = 0;
                                    for id in &group_ids {
                                        if drafts.get(id).map(|n| !n.is_empty()).unwrap_or(false) {
                                            continue; // Skip already named
                                        }
                                        if name_idx < names.len() {
                                            drafts.insert(id.clone(), names[name_idx].clone());
                                            name_idx += 1;
                                        }
                                    }
                                    group_status.set(Some(format!("Generated {name_idx} name(s)")));
                                }
                                None => {
                                    group_status.set(Some("Could not detect naming pattern. Include a number in the name.".into()));
                                }
                            }
                        }
                    },
                    "Auto-Sequence"
                }
                button {
                    class: "discovery-action-btn accept primary",
                    onclick: {
                        let group_ids = group_ids.clone();
                        let svc = svc_rename.clone();
                        move |_| {
                            let drafts = group_name_drafts.read().clone();
                            let ids: Vec<String> = group_ids
                                .iter()
                                .filter(|id| drafts.contains_key(*id))
                                .cloned()
                                .collect();
                            let names: Vec<String> = ids
                                .iter()
                                .map(|id| drafts.get(id).cloned().unwrap_or_default())
                                .collect();
                            if ids.is_empty() {
                                return;
                            }
                            let svc = svc.clone();
                            spawn(async move {
                                match svc.bulk_rename_devices(&ids, &names).await {
                                    Ok(n) => group_status.set(Some(format!("Renamed {n} device(s)"))),
                                    Err(e) => group_status.set(Some(format!("Error: {e}"))),
                                }
                                group_name_drafts.write().clear();
                                bump(&mut refresh_counter);
                            });
                        }
                    },
                    "Apply Names"
                }
                button {
                    class: "discovery-action-btn",
                    onclick: move |_| {
                        group_name_drafts.write().clear();
                        group_status.set(None);
                    },
                    "Clear"
                }
                if let Some(ref msg) = *group_status.read() {
                    span { class: "discovery-bulk-status", "{msg}" }
                }
            }

            // Device Name Find & Replace
            div { class: "discovery-find-replace",
                h4 { "Device Name Find & Replace" }
                div { class: "discovery-find-replace-row",
                    input {
                        class: "discovery-input",
                        r#type: "text",
                        placeholder: "Find...",
                        value: "{group_find_text.read()}",
                        oninput: move |evt: Event<FormData>| group_find_text.set(evt.value()),
                    }
                    input {
                        class: "discovery-input",
                        r#type: "text",
                        placeholder: "Replace with...",
                        value: "{group_replace_text.read()}",
                        oninput: move |evt: Event<FormData>| group_replace_text.set(evt.value()),
                    }
                    button {
                        class: "discovery-action-btn accept primary",
                        disabled: group_find_text.read().is_empty(),
                        onclick: {
                            let group_ids = group_ids.clone();
                            let name_map = dev_name_map.clone();
                            move |_| {
                                let find = group_find_text.read().clone();
                                let replace = group_replace_text.read().clone();
                                let mut drafts = group_name_drafts.write();
                                for id in &group_ids {
                                    let current = drafts
                                        .get(id)
                                        .cloned()
                                        .unwrap_or_else(|| {
                                            name_map.get(id).cloned().unwrap_or_default()
                                        });
                                    drafts.insert(id.clone(), current.replace(&find, &replace));
                                }
                            }
                        },
                        "Replace All"
                    }
                }
            }

            // Shared points (editable names + units across all devices in group)
            {
                let shared_pts = group_shared_points.read();
                let svc_pts = state.discovery_service.clone();
                let group_ids_pts = group_ids.clone();
                let svc_template = svc_template.clone();
                let group_ids_tpl = group_ids.clone();
                rsx! {
                    if !shared_pts.is_empty() {
                        div { class: "discovery-template-section",
                            h4 { "Points (shared across all {dev_count} devices)" }
                            div { class: "discovery-seq-table",
                                table { class: "discovery-point-table",
                                    thead {
                                        tr {
                                            th { "Point ID" }
                                            th { "Display Name" }
                                            th { "Units" }
                                            th { "Kind" }
                                            th { "State Labels" }
                                        }
                                    }
                                    tbody {
                                        for pt in shared_pts.iter() {
                                            {
                                                let pid = pt.id.clone();
                                                let name_key = pt.id.clone();
                                                let units_key = pt.id.clone();
                                                let labels_key = pt.id.clone();
                                                let pt_kind = pt.point_kind;
                                                let name_draft = group_point_name_drafts
                                                    .read()
                                                    .get(&pt.id)
                                                    .cloned()
                                                    .unwrap_or_else(|| pt.display_name.clone());
                                                let units_draft = group_point_units_drafts
                                                    .read()
                                                    .get(&pt.id)
                                                    .cloned()
                                                    .unwrap_or_else(|| pt.units.clone().unwrap_or_default());
                                                // Get current state labels from draft or point
                                                let current_labels: HashMap<String, String> = {
                                                    let drafts = group_point_state_labels_drafts.read();
                                                    if let Some(Some(labels)) = drafts.get(&pt.id) {
                                                        labels.clone()
                                                    } else {
                                                        pt.state_labels.clone().unwrap_or_default()
                                                    }
                                                };
                                                rsx! {
                                                    tr { key: "{pid}",
                                                        td { class: "text-muted discovery-pt-id-cell", "{pid}" }
                                                        td {
                                                            input {
                                                                class: "discovery-seq-name-input",
                                                                r#type: "text",
                                                                value: "{name_draft}",
                                                                oninput: move |evt: Event<FormData>| {
                                                                    group_point_name_drafts.write().insert(name_key.clone(), evt.value());
                                                                },
                                                            }
                                                        }
                                                        td {
                                                            input {
                                                                class: "discovery-seq-name-input discovery-units-input",
                                                                r#type: "text",
                                                                placeholder: "units",
                                                                value: "{units_draft}",
                                                                oninput: move |evt: Event<FormData>| {
                                                                    group_point_units_drafts.write().insert(units_key.clone(), evt.value());
                                                                },
                                                            }
                                                        }
                                                        td {
                                                            span { class: "discovery-kind-badge", "{kind_label(pt.point_kind)}" }
                                                        }
                                                        td { class: "discovery-state-labels-cell",
                                                            if pt_kind == PointKindHint::Binary {
                                                                {
                                                                    let true_val = current_labels.get("true").cloned().unwrap_or_default();
                                                                    let false_val = current_labels.get("false").cloned().unwrap_or_default();
                                                                    let lk_t = labels_key.clone();
                                                                    let lk_f = labels_key.clone();
                                                                    rsx! {
                                                                        div { class: "discovery-state-label-row",
                                                                            span { class: "discovery-state-label-key", "T:" }
                                                                            input {
                                                                                class: "discovery-state-label-input",
                                                                                r#type: "text",
                                                                                placeholder: "On",
                                                                                value: "{true_val}",
                                                                                oninput: move |evt: Event<FormData>| {
                                                                                    let mut drafts = group_point_state_labels_drafts.write();
                                                                                    let entry = drafts.entry(lk_t.clone()).or_insert_with(|| Some(HashMap::new()));
                                                                                    if let Some(map) = entry {
                                                                                        map.insert("true".to_string(), evt.value());
                                                                                    }
                                                                                },
                                                                            }
                                                                        }
                                                                        div { class: "discovery-state-label-row",
                                                                            span { class: "discovery-state-label-key", "F:" }
                                                                            input {
                                                                                class: "discovery-state-label-input",
                                                                                r#type: "text",
                                                                                placeholder: "Off",
                                                                                value: "{false_val}",
                                                                                oninput: move |evt: Event<FormData>| {
                                                                                    let mut drafts = group_point_state_labels_drafts.write();
                                                                                    let entry = drafts.entry(lk_f.clone()).or_insert_with(|| Some(HashMap::new()));
                                                                                    if let Some(map) = entry {
                                                                                        map.insert("false".to_string(), evt.value());
                                                                                    }
                                                                                },
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            } else if pt_kind == PointKindHint::Multistate {
                                                                {
                                                                    // Show existing state keys + an add button
                                                                    let labels_clone = current_labels.clone();
                                                                    let mut sorted_keys: Vec<String> = labels_clone.keys().cloned().collect();
                                                                    sorted_keys.sort_by(|a, b| {
                                                                        a.parse::<u32>().unwrap_or(u32::MAX)
                                                                            .cmp(&b.parse::<u32>().unwrap_or(u32::MAX))
                                                                    });
                                                                    // If no keys yet, show states 1-3 as placeholders
                                                                    if sorted_keys.is_empty() {
                                                                        sorted_keys = vec!["1".to_string(), "2".to_string(), "3".to_string()];
                                                                    }
                                                                    let lk_ms = labels_key.clone();
                                                                    rsx! {
                                                                        for state_key in sorted_keys.iter() {
                                                                            {
                                                                                let sk = state_key.clone();
                                                                                let sk2 = state_key.clone();
                                                                                let lk = lk_ms.clone();
                                                                                let val = labels_clone.get(state_key).cloned().unwrap_or_default();
                                                                                rsx! {
                                                                                    div { class: "discovery-state-label-row",
                                                                                        span { class: "discovery-state-label-key", "{sk}:" }
                                                                                        input {
                                                                                            class: "discovery-state-label-input",
                                                                                            r#type: "text",
                                                                                            placeholder: "State {sk2}",
                                                                                            value: "{val}",
                                                                                            oninput: move |evt: Event<FormData>| {
                                                                                                let mut drafts = group_point_state_labels_drafts.write();
                                                                                                let entry = drafts.entry(lk.clone()).or_insert_with(|| Some(HashMap::new()));
                                                                                                if let Some(map) = entry {
                                                                                                    map.insert(sk.clone(), evt.value());
                                                                                                }
                                                                                            },
                                                                                        }
                                                                                    }
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            } else {
                                                                span { class: "text-muted", "—" }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            // Point actions
                            div { class: "discovery-seq-toolbar",
                                button {
                                    class: "discovery-action-btn accept primary",
                                    onclick: {
                                        let svc = svc_pts.clone();
                                        let group_ids = group_ids_pts.clone();
                                        move |_| {
                                            let name_drafts = group_point_name_drafts.read().clone();
                                            let units_drafts = group_point_units_drafts.read().clone();
                                            let labels_check_empty = group_point_state_labels_drafts.read().is_empty();
                                            if name_drafts.is_empty() && units_drafts.is_empty() && labels_check_empty {
                                                return;
                                            }
                                            let svc = svc.clone();
                                            let ids = group_ids.clone();
                                            spawn(async move {
                                                let labels_drafts = group_point_state_labels_drafts.read().clone();
                                                match svc.bulk_update_group_points(&ids, &name_drafts, &units_drafts, &labels_drafts).await {
                                                    Ok(n) => group_status.set(Some(format!("Updated {n} point(s) across all devices"))),
                                                    Err(e) => group_status.set(Some(format!("Error: {e}"))),
                                                }
                                                group_point_name_drafts.write().clear();
                                                group_point_units_drafts.write().clear();
                                                group_point_state_labels_drafts.write().clear();
                                                bump(&mut refresh_counter);
                                            });
                                        }
                                    },
                                    "Apply Points"
                                }
                                button {
                                    class: "discovery-action-btn",
                                    onclick: move |_| {
                                        group_point_name_drafts.write().clear();
                                        group_point_units_drafts.write().clear();
                                        group_point_state_labels_drafts.write().clear();
                                    },
                                    "Reset"
                                }
                            }

                            // Point name template
                            h4 { class: "discovery-pt-fr-heading", "Name Template" }
                            div { class: "discovery-find-replace-row",
                                input {
                                    class: "discovery-input",
                                    r#type: "text",
                                    placeholder: "{{device}} {{point}}",
                                    value: "{group_template_text.read()}",
                                    oninput: move |evt: Event<FormData>| group_template_text.set(evt.value()),
                                }
                                button {
                                    class: "discovery-action-btn accept primary",
                                    onclick: {
                                        let group_ids = group_ids_tpl.clone();
                                        let svc = svc_template.clone();
                                        move |_| {
                                            let template = group_template_text.read().clone();
                                            let ids = group_ids.clone();
                                            let svc = svc.clone();
                                            spawn(async move {
                                                match svc.apply_point_name_template(&ids, &template).await {
                                                    Ok(n) => group_status.set(Some(format!("Templated {n} point(s)"))),
                                                    Err(e) => group_status.set(Some(format!("Error: {e}"))),
                                                }
                                                bump(&mut refresh_counter);
                                            });
                                        }
                                    },
                                    "Apply Template"
                                }
                            }
                            p { class: "discovery-hint", "Placeholders: {{device}}, {{point}}, {{kind}}, {{units}}" }
                        }
                    }
                }
            }

            // Bulk actions
            div { class: "discovery-group-bulk-actions",
                h4 { "Bulk Actions" }
                div { class: "discovery-find-replace-row",
                    button {
                        class: "discovery-action-btn accept primary",
                        onclick: {
                            let group_ids = group_ids.clone();
                            let svc = svc_accept.clone();
                            let audit = audit_state.clone();
                            move |_| {
                                let ids = group_ids.clone();
                                let svc = svc.clone();
                                let audit = audit.clone();
                                spawn(async move {
                                    let mut ok = 0;
                                    for id in &ids {
                                        if let Err(e) = svc.accept_device(id).await {
                                            eprintln!("Accept {id} failed: {e}");
                                        } else {
                                            ok += 1;
                                            audit.audit(
                                                crate::store::audit_store::AuditEntryBuilder::new(
                                                    crate::store::audit_store::AuditAction::AcceptDevice, "device",
                                                ).resource_id(id),
                                            );
                                        }
                                    }
                                    group_status.set(Some(format!("Accepted {ok} device(s)")));
                                    bump(&mut refresh_counter);
                                });
                            }
                        },
                        "Accept All"
                    }
                    button {
                        class: "discovery-action-btn ignore",
                        onclick: {
                            let group_ids = group_ids.clone();
                            let svc = svc_ignore.clone();
                            move |_| {
                                let ids = group_ids.clone();
                                let svc = svc.clone();
                                spawn(async move {
                                    for id in &ids {
                                        let _ = svc.ignore_device(id).await;
                                    }
                                    group_status.set(Some("Ignored all devices in group".into()));
                                    bump(&mut refresh_counter);
                                });
                            }
                        },
                        "Ignore All"
                    }
                }
            }
        }
    }
}
