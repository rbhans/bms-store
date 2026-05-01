use std::collections::BTreeMap;
use std::sync::Arc;

use dioxus::prelude::*;

use bms_store_bridges::normalize::value_map::{BoolMap, ValueMap};
use bms_store_storage::auth::Permission;
use bms_store_storage::config::profile::{PointAccess, PointValue};
use bms_store_storage::store::point_store::PointStatusFlags;
use bms_core::event::Event;
use crate::gui::state::AppState;

use super::preview_modal::{ChangeKind, PreviewModal, PreviewRow};
use super::write_dialog::WriteDialog;

// ---------------------------------------------------------------------------
// Value-mapping presets
// ---------------------------------------------------------------------------

/// Built-in ValueMap presets the user can apply with one click.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ValueMapPreset {
    OnOff,
    OpenClosed,
    OccupiedUnoccupied,
    AutoManual,
    Clear,
}

impl ValueMapPreset {
    const ALL: &'static [ValueMapPreset] = &[
        Self::OnOff,
        Self::OpenClosed,
        Self::OccupiedUnoccupied,
        Self::AutoManual,
        Self::Clear,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::OnOff => "Apply ON/OFF",
            Self::OpenClosed => "Apply OPEN/CLOSED",
            Self::OccupiedUnoccupied => "Apply OCCUPIED/UNOCCUPIED",
            Self::AutoManual => "Apply AUTO/MANUAL",
            Self::Clear => "Clear mapping",
        }
    }

    /// Returns the ValueMap entries for the preset, or None if Clear.
    fn to_value_map(self) -> Option<ValueMap> {
        match self {
            Self::OnOff => Some(BoolMap::on_off()),
            Self::OpenClosed => Some(BoolMap::open_closed()),
            Self::OccupiedUnoccupied => Some(BoolMap::occupied_unoccupied()),
            Self::AutoManual => Some(BoolMap::auto_manual()),
            Self::Clear => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Point status flag tooltips
// ---------------------------------------------------------------------------

fn flag_tooltip(flag: &str) -> &'static str {
    match flag {
        "down" => "Point is unreachable — the device or bridge is offline.",
        "fault" => "Point has a fault condition reported by the device.",
        "alarm" => "Point value is in an alarm state.",
        "overridden" => "Point value has been manually overridden.",
        "stale" => "Point has not been updated within the expected poll interval.",
        "disabled" => "Point is disabled and not being polled.",
        _ => "Unknown status flag.",
    }
}

// ---------------------------------------------------------------------------
// PointDetail
// ---------------------------------------------------------------------------

#[component]
pub fn PointDetail() -> Element {
    let state = use_context::<AppState>();

    let _version = state.store_version.read();
    let selected_device = state.selected_device.read().clone();
    let selected_point = state.selected_point.read().clone();

    let (Some(device_id), Some(point_id)) = (selected_device, selected_point) else {
        return rsx! {
            div { class: "point-detail-body",
                p { class: "placeholder", "Select a point to view details." }
            }
        };
    };

    // Compose the full entity id as "device_id/point_id"
    let entity_id = format!("{device_id}/{point_id}");

    let profile_point = state
        .loaded
        .devices
        .iter()
        .find(|d| d.instance_id == device_id)
        .and_then(|d| d.profile.points.iter().find(|p| p.id == point_id));

    let live_value = state.store.get(&bms_store_storage::store::point_store::PointKey {
        device_instance_id: device_id.clone(),
        point_id: point_id.clone(),
    });

    let is_writable = profile_point
        .map(|p| !matches!(p.access, PointAccess::Input))
        .unwrap_or(false);

    // -------------------------------------------------------------------
    // Deliverable 4: Live quality-flag badges via event subscription.
    // Top-level use_signal — never nested inside other hooks.
    // -------------------------------------------------------------------
    let initial_flags = live_value
        .as_ref()
        .map(|tv| tv.status)
        .unwrap_or_default();
    let mut live_flags: Signal<PointStatusFlags> = use_signal(move || initial_flags);

    // Subscribe to QualityChanged events for this specific point.
    // Spawns a background task; result stored top-level to avoid BorrowMutError.
    // The event_bus is cloned into the future; subscribe() is called inside the async block.
    let node_id_for_sub = entity_id.clone();
    let event_bus_for_sub = state.event_bus.clone();
    use_future(move || {
        let bus = event_bus_for_sub.clone();
        let nid = node_id_for_sub.clone();
        async move {
            let mut bus_rx = bus.subscribe();
            loop {
                let ev = bus_rx.recv().await;
                match ev {
                    Ok(ev_arc) => {
                        match ev_arc.as_ref() {
                            Event::QualityChanged { node_id, flags, .. } if node_id == &nid => {
                                live_flags.set(PointStatusFlags(*flags));
                            }
                            Event::StatusChanged { node_id, flags } if node_id == &nid => {
                                live_flags.set(PointStatusFlags(*flags));
                            }
                            _ => {}
                        }
                    }
                    Err(_) => break,
                }
            }
        }
    });

    let current_flags = *live_flags.read();

    // -------------------------------------------------------------------
    // Deliverable 1: Value-mapping editor state.
    // -------------------------------------------------------------------
    let es = state.entity_store.clone();
    let eid_for_entity = entity_id.clone();
    let entity_res = use_resource(move || {
        let store = es.clone();
        let id = eid_for_entity.clone();
        let _v = state.store_version.read();
        async move { store.get_entity(&id).await.ok() }
    });

    // Current enum tag (value map JSON blob)
    let current_enum_json: Option<String> = entity_res
        .read()
        .as_ref()
        .and_then(|e| e.as_ref()?.tags.get("enum")?.clone());

    // Parse existing map
    let existing_map: Option<ValueMap> = current_enum_json
        .as_deref()
        .and_then(ValueMap::from_json);

    // Editing state: entries as BTreeMap for display/editing
    let mut vm_entries: Signal<BTreeMap<String, String>> = use_signal(|| {
        existing_map
            .as_ref()
            .map(|vm| vm.to_json())
            .and_then(|j| serde_json::from_str::<BTreeMap<String, String>>(&j).ok())
            .unwrap_or_default()
    });

    // New row input fields
    let mut new_raw = use_signal(String::new);
    let mut new_canonical = use_signal(String::new);

    // Preset dropdown
    let mut selected_preset: Signal<Option<ValueMapPreset>> = use_signal(|| None);
    let mut show_vm_preview: Signal<bool> = use_signal(|| false);

    // -------------------------------------------------------------------
    // Deliverable 2: Supply chain breadcrumb (read from entity refs).
    // -------------------------------------------------------------------
    let es_chain = state.entity_store.clone();
    let eid_for_chain = entity_id.clone();
    let supply_chain_res = use_resource(move || {
        let store = es_chain.clone();
        let eid = eid_for_chain.clone();
        async move {
            // Get the equip that owns this point (entity id prefix before '/')
            let equip_id = eid.split('/').next().unwrap_or("").to_string();
            if equip_id.is_empty() {
                return Vec::new();
            }
            bms_store_storage::store::relationships::walk_supply_chain(
                &store, &equip_id, 8,
            ).await
        }
    });

    let supply_chain_read = supply_chain_res.read();
    let supply_chain: &[bms_store_storage::store::entity_store::Entity] =
        supply_chain_read.as_deref().unwrap_or(&[]);

    // -------------------------------------------------------------------
    // Build value-map preview rows for preset confirmation
    // -------------------------------------------------------------------
    let entity_id_for_save = entity_id.clone();
    let es_save = state.entity_store.clone();
    let current_enum_for_preview = current_enum_json.clone();

    rsx! {
        div { class: "point-detail-body",
            h4 { class: "detail-point-name", "{point_id}" }

            // ── Quality flag badges (Deliverable 4) ─────────────────────────
            if !current_flags.is_normal() {
                div { class: "status-badges",
                    for flag in current_flags.active_flags() {
                        span {
                            class: "status-badge status-{flag}",
                            title: "{flag_tooltip(flag)}",
                            "{flag}"
                        }
                    }
                    // Force re-poll: best-effort — no dedicated API yet.
                    // Bumping store_version causes bridges to reschedule the poll.
                    span {
                        class: "status-badge-action",
                        title: "Force re-poll (best-effort: bridges will reschedule next poll cycle)",
                        onclick: {
                            let mut sv = state.store_version;
                            move |_| {
                                let cur = *sv.read();
                                sv.set(cur.wrapping_add(1));
                            }
                        },
                        "Re-poll"
                    }
                }
            } else {
                div { class: "status-badges-ok",
                    span { class: "status-badge-ok", title: "All quality flags normal", "OK" }
                }
            }

            if let Some(pt) = profile_point {
                dl { class: "detail-grid",
                    dt { "Name" }
                    dd { "{pt.name}" }

                    if let Some(desc) = &pt.description {
                        dt { "Description" }
                        dd { "{desc}" }
                    }

                    dt { "Kind" }
                    dd { "{pt.kind:?}" }

                    dt { "Access" }
                    dd { "{pt.access:?}" }

                    if let Some(units) = &pt.units {
                        dt { "Units" }
                        dd { "{units}" }
                    }

                    if let Some(constraints) = &pt.constraints {
                        if let Some(min) = constraints.min {
                            dt { "Min" }
                            dd { "{min}" }
                        }
                        if let Some(max) = constraints.max {
                            dt { "Max" }
                            dd { "{max}" }
                        }
                        if let Some(states) = &constraints.states {
                            dt { "States" }
                            dd {
                                for (k, v) in states.iter() {
                                    span { class: "state-label", "{k}: {v}" }
                                }
                            }
                        }
                    }

                    if let Some(tv) = &live_value {
                        dt { "Current Value" }
                        dd { class: "live-value", "{tv.value:?}" }
                    }
                }
            } else {
                if let Some(tv) = &live_value {
                    dl { class: "detail-grid",
                        dt { "Current Value" }
                        dd { class: "live-value", "{tv.value:?}" }
                    }
                }
            }

            // ── Supply chain breadcrumb (Deliverable 2) ──────────────────────
            if !supply_chain.is_empty() {
                div { class: "supply-chain-section",
                    h5 { class: "detail-section-title", "Supply Chain (read-only)" }
                    div { class: "supply-chain-breadcrumb",
                        for (i, equip) in supply_chain.iter().enumerate() {
                            if i > 0 {
                                span { class: "supply-chain-arrow", " \u{2190} supplied by \u{2190} " }
                            }
                            span { class: "supply-chain-node",
                                if equip.dis.is_empty() { "{equip.id}" } else { "{equip.dis}" }
                            }
                        }
                    }
                    p { class: "config-hint",
                        "Relationship editing is done on the equipment node."
                    }
                }
            }

            // ── Value Mapping section (Deliverable 1) ─────────────────────────
            div { class: "value-mapping-section",
                h5 { class: "detail-section-title", "Value Mapping" }

                // Current mapping table
                {
                    let entries = vm_entries.read().clone();
                    if entries.is_empty() {
                        rsx! {
                            p { class: "config-hint", "No mapping configured. Values pass through raw." }
                        }
                    } else {
                        rsx! {
                            table { class: "vm-table",
                                thead {
                                    tr {
                                        th { "Raw" }
                                        th { "Canonical" }
                                        th {}
                                    }
                                }
                                tbody {
                                    for (raw_key, canonical_val) in &entries {
                                        {
                                            let rk = raw_key.clone();
                                            let cv = canonical_val.clone();
                                            let rk_remove = rk.clone();
                                            rsx! {
                                                tr { class: "vm-row",
                                                    td { class: "vm-raw", "{rk}" }
                                                    td { class: "vm-canonical", "{cv}" }
                                                    td {
                                                        button {
                                                            class: "config-tag-remove",
                                                            title: "Remove entry",
                                                            onclick: move |_| {
                                                                let mut map = vm_entries.write();
                                                                map.remove(&rk_remove);
                                                            },
                                                            "x"
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

                // Live preview: show current value normalized by proposed map
                if let Some(tv) = &live_value {
                    {
                        let entries = vm_entries.read().clone();
                        let mut vm = ValueMap::new();
                        for (k, v) in &entries {
                            vm.insert(k.clone(), v.clone());
                        }
                        let normalized = vm.normalize_value(&tv.value);
                        rsx! {
                            div { class: "vm-live-preview",
                                span { class: "vm-preview-label", "Preview: " }
                                span { class: "vm-preview-raw", "{tv.value:?}" }
                                span { class: "vm-preview-arrow", " \u{2192} " }
                                if let Some(canonical) = normalized.canonical() {
                                    span { class: "vm-preview-canonical", "\"{canonical}\"" }
                                } else {
                                    span { class: "vm-preview-raw", "(pass-through)" }
                                }
                            }
                        }
                    }
                }

                // Add row
                div { class: "vm-add-row",
                    input {
                        class: "config-input vm-input-raw",
                        r#type: "text",
                        placeholder: "Raw (e.g. 0)",
                        value: "{new_raw}",
                        oninput: move |e| new_raw.set(e.value()),
                    }
                    span { class: "vm-arrow-label", "\u{2192}" }
                    input {
                        class: "config-input vm-input-canonical",
                        r#type: "text",
                        placeholder: "Canonical (e.g. OFF)",
                        value: "{new_canonical}",
                        oninput: move |e| new_canonical.set(e.value()),
                    }
                    button {
                        class: "config-btn",
                        onclick: move |_| {
                            let raw = new_raw.read().trim().to_string();
                            let canonical = new_canonical.read().trim().to_string();
                            if !raw.is_empty() && !canonical.is_empty() {
                                vm_entries.write().insert(raw, canonical);
                                new_raw.set(String::new());
                                new_canonical.set(String::new());
                            }
                        },
                        "+ Add"
                    }
                }

                // Presets
                div { class: "vm-presets",
                    select {
                        class: "config-input vm-preset-select",
                        value: "",
                        onchange: move |evt| {
                            let val = evt.value();
                            let preset = match val.as_str() {
                                "on_off" => Some(ValueMapPreset::OnOff),
                                "open_closed" => Some(ValueMapPreset::OpenClosed),
                                "occ_unocc" => Some(ValueMapPreset::OccupiedUnoccupied),
                                "auto_manual" => Some(ValueMapPreset::AutoManual),
                                "clear" => Some(ValueMapPreset::Clear),
                                _ => None,
                            };
                            selected_preset.set(preset);
                            if preset.is_some() {
                                show_vm_preview.set(true);
                            }
                        },
                        option { value: "", "Quick preset..." }
                        option { value: "on_off", "Apply ON/OFF" }
                        option { value: "open_closed", "Apply OPEN/CLOSED" }
                        option { value: "occ_unocc", "Apply OCCUPIED/UNOCCUPIED" }
                        option { value: "auto_manual", "Apply AUTO/MANUAL" }
                        option { value: "clear", "Clear mapping" }
                    }
                }

                // Preset confirmation modal (DESTRUCTIVE — replaces existing map)
                if *show_vm_preview.read() {
                    {
                        if let Some(preset) = *selected_preset.read() {
                            let proposed = preset.to_value_map();
                            let existing_for_preview = current_enum_for_preview.clone();
                            let has_existing = existing_for_preview.is_some();

                            let before_str = if has_existing {
                                existing_for_preview.as_deref().unwrap_or("{}").to_string()
                            } else {
                                "(none)".to_string()
                            };
                            let after_str = proposed
                                .as_ref()
                                .map(|vm| vm.to_json())
                                .unwrap_or_else(|| "(cleared)".to_string());

                            let change_kind = if !has_existing {
                                if proposed.is_some() { ChangeKind::Add } else { ChangeKind::NoOp }
                            } else if proposed.is_none() {
                                ChangeKind::Remove
                            } else {
                                ChangeKind::Modify
                            };

                            let preview_rows = vec![PreviewRow {
                                id: entity_id.clone(),
                                label: entity_id.clone(),
                                before: before_str,
                                after: after_str,
                                change_kind,
                            }];

                            let es_preset = es_save.clone();
                            let eid_preset = entity_id_for_save.clone();
                            rsx! {
                                PreviewModal {
                                    title: format!("Apply preset: {}", preset.label()),
                                    rows: preview_rows,
                                    on_confirm: move |_| {
                                        let store = es_preset.clone();
                                        let eid = eid_preset.clone();
                                        let proposed_inner = preset.to_value_map();
                                        let json_val = proposed_inner.as_ref().map(|vm| vm.to_json());
                                        // Update local state immediately
                                        if let Some(ref new_map) = proposed_inner {
                                            let entries: BTreeMap<String, String> = serde_json::from_str(&new_map.to_json()).unwrap_or_default();
                                            vm_entries.set(entries);
                                        } else {
                                            vm_entries.set(BTreeMap::new());
                                        }
                                        spawn(async move {
                                            if let Some(json) = json_val {
                                                let _ = store.set_tag(&eid, "enum", Some(&json)).await;
                                            } else {
                                                let _ = store.remove_tag(&eid, "enum").await;
                                            }
                                        });
                                        show_vm_preview.set(false);
                                        selected_preset.set(None);
                                    },
                                    on_cancel: move |_| {
                                        show_vm_preview.set(false);
                                        selected_preset.set(None);
                                    },
                                }
                            }
                        } else {
                            rsx! {}
                        }
                    }
                }

                // Save button
                div { class: "vm-save-row",
                    {
                        let es_btn = state.entity_store.clone();
                        let eid_btn = entity_id_for_save.clone();
                        rsx! {
                            button {
                                class: "config-btn config-btn-primary",
                                onclick: move |_| {
                                    let store = es_btn.clone();
                                    let eid = eid_btn.clone();
                                    let entries = vm_entries.read().clone();
                                    spawn(async move {
                                        if entries.is_empty() {
                                            let _ = store.remove_tag(&eid, "enum").await;
                                        } else {
                                            let json = serde_json::to_string(&entries)
                                                .unwrap_or_else(|_| "{}".to_string());
                                            let _ = store.set_tag(&eid, "enum", Some(&json)).await;
                                        }
                                    });
                                },
                                "Save Mapping"
                            }
                        }
                    }
                }
            }

            // ── Write dialog ────────────────────────────────────────────────
            if is_writable && state.has_permission(Permission::WritePoints) {
                WriteDialog {
                    device_id: device_id.clone(),
                    point_id: point_id.clone(),
                }
            }
        }
    }
}
