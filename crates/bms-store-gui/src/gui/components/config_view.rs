use std::collections::{HashMap, HashSet};

use dioxus::prelude::*;

use crate::gui::state::AppState;
use bms_store_bridges::haystack::prototypes::{EQUIP_PROTOTYPES, POINT_PROTOTYPES};
use bms_store_bridges::haystack::tags::{self, TagKind};
use bms_store_bridges::haystack::validation::{validate_tags, Severity, ValidationIssue};
use bms_store_storage::store::entity_store::Entity;

use bms_store_storage::auth::Permission;
use bms_store_storage::store::node_store::NodeRecord;

use super::audit_log_view::AuditLogView;
use super::discovery_view::DiscoveryView;
use super::preview_modal::{ChangeKind, PreviewModal, PreviewRow};
use super::programming_view::ProgrammingView;
use super::theme_settings::ThemeSettingsView;
use super::user_management::UserManagementView;
use super::virtual_points_view::VirtualPointsView;
use super::web_server_settings::WebServerSettingsView;

// ----------------------------------------------------------------
// Config sub-tabs
// ----------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConfigSection {
    Haystack,
    Discovery,
    Programming,
    VirtualPoints,
    Plugins,
    Appearance,
    Mqtt,
    Webhooks,
    Commissioning,
    WebServer,
    Users,
    AuditLog,
    DataExport,
    Atlas,
}

impl ConfigSection {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Haystack => "Haystack",
            Self::Discovery => "Discovery",
            Self::Programming => "Programming",
            Self::VirtualPoints => "Virtual Points",
            Self::Plugins => "Plugins",
            Self::Appearance => "Appearance",
            Self::Mqtt => "MQTT",
            Self::Webhooks => "Webhooks",
            Self::Commissioning => "Commissioning",
            Self::DataExport => "Data Export",
            Self::WebServer => "Web Server",
            Self::Users => "Users",
            Self::AuditLog => "Audit Log",
            Self::Atlas => "Atlas",
        }
    }

    /// Returns sections visible to the current user.
    pub fn visible_sections(
        can_manage_users: bool,
        can_manage_mqtt: bool,
        can_manage_webhooks: bool,
        can_manage_commissioning: bool,
        can_manage_export: bool,
        can_view_audit: bool,
    ) -> Vec<ConfigSection> {
        let mut sections = vec![
            Self::Haystack,
            Self::Discovery,
            Self::Programming,
            Self::VirtualPoints,
            Self::Plugins,
            Self::Appearance,
        ];
        if can_manage_mqtt {
            sections.push(Self::Mqtt);
        }
        if can_manage_webhooks {
            sections.push(Self::Webhooks);
        }
        if can_manage_commissioning {
            sections.push(Self::Commissioning);
        }
        if can_manage_export {
            sections.push(Self::DataExport);
        }
        sections.push(Self::Atlas);
        if can_manage_users {
            sections.push(Self::WebServer);
            sections.push(Self::Users);
        }
        if can_view_audit {
            sections.push(Self::AuditLog);
        }
        sections
    }
}

// ----------------------------------------------------------------
// ConfigView — sub-tabbed config mode
// ----------------------------------------------------------------

#[component]
pub fn ConfigView() -> Element {
    let mut state = use_context::<AppState>();
    let can_manage_users = state.has_permission(Permission::ManageUsers);
    let can_manage_mqtt = state.has_permission(Permission::ManageMqtt);
    let can_manage_webhooks = state.has_permission(Permission::ManageWebhooks);
    let can_manage_commissioning = state.has_permission(Permission::ManageCommissioning);
    let can_manage_export = state.has_permission(Permission::ManageExport);
    let can_view_audit = state.has_permission(Permission::ViewAudit);
    let all_sections = ConfigSection::visible_sections(
        can_manage_users,
        can_manage_mqtt,
        can_manage_webhooks,
        can_manage_commissioning,
        can_manage_export,
        can_view_audit,
    );

    let mut section = use_signal(|| ConfigSection::Haystack);
    let mut tab_filter = use_signal(String::new);

    // Consume pending config section from toolbar/menu navigation
    {
        let pending = state.pending_config_section.read().clone();
        if let Some(ref name) = pending {
            let target = all_sections.iter().find(|s| s.label() == name).copied();
            if let Some(t) = target {
                if *section.read() != t {
                    section.set(t);
                }
            }
            drop(pending);
            state.pending_config_section.set(None);
        }
    }

    let current = *section.read();
    let filter_q = tab_filter.read().to_lowercase();
    let sections: Vec<ConfigSection> = if filter_q.is_empty() {
        all_sections.clone()
    } else {
        all_sections
            .iter()
            .filter(|s| s.label().to_lowercase().contains(&filter_q))
            .copied()
            .collect()
    };

    rsx! {
        div { class: "config-view",
            // Sub-tab bar with search
            div { class: "config-section-bar",
                // Quick filter
                div { class: "config-tab-filter",
                    input {
                        class: "config-tab-filter-input",
                        r#type: "text",
                        placeholder: "Filter...",
                        value: "{tab_filter}",
                        oninput: move |evt| tab_filter.set(evt.value()),
                    }
                }

                // Scrollable tabs
                div { class: "config-tab-scroll",
                    for s in &sections {
                        {
                            let s_val = *s;
                            rsx! {
                                button {
                                    class: if current == s_val { "config-section-btn active" } else { "config-section-btn" },
                                    onclick: move |_| {
                                        section.set(s_val);
                                        tab_filter.set(String::new());
                                    },
                                    "{s_val.label()}"
                                }
                            }
                        }
                    }
                }
            }

            // Section content
            div { class: "config-section-body",
                match current {
                    ConfigSection::Haystack => rsx! { HaystackView {} },
                    ConfigSection::Discovery => rsx! { DiscoveryView {} },
                    ConfigSection::Programming => rsx! { ProgrammingView {} },
                    ConfigSection::VirtualPoints => rsx! { VirtualPointsView {} },
                    ConfigSection::Plugins => rsx! { super::plugin_manager::PluginManagerView {} },
                    ConfigSection::Users => rsx! { UserManagementView {} },
                    ConfigSection::Appearance => rsx! { ThemeSettingsView {} },
                    ConfigSection::Mqtt => rsx! { super::mqtt_settings::MqttSettingsView {} },
                    ConfigSection::Webhooks => rsx! { super::webhook_settings::WebhookSettingsView {} },
                    ConfigSection::Commissioning => rsx! { super::commissioning_overview::CommissioningOverview {} },
                    ConfigSection::DataExport => rsx! { super::export_settings::ExportSettingsView {} },
                    ConfigSection::WebServer => rsx! { WebServerSettingsView {} },
                    ConfigSection::AuditLog => rsx! { AuditLogView {} },
                    ConfigSection::Atlas => rsx! { super::atlas_settings::AtlasSettingsView {} },
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Haystack View — 3-pane (device/point browser | tag editor | properties)
// ----------------------------------------------------------------

#[component]
fn HaystackView() -> Element {
    let selected_entity_id: Signal<Option<String>> = use_signal(|| None);
    let mut entity_version = use_signal(|| 0u64);
    let batch_selected: Signal<HashSet<String>> = use_signal(HashSet::new);
    let batch_mode = use_signal(|| false);

    // Watch entity store version for reactivity
    let state = use_context::<AppState>();
    let es = state.entity_store.clone();
    use_future(move || {
        let store = es.clone();
        async move {
            let mut rx = store.subscribe();
            loop {
                if rx.changed().await.is_err() {
                    break;
                }
                entity_version.set(*rx.borrow());
            }
        }
    });

    rsx! {
        HaystackDeviceBrowser {
            selected_entity_id,
            entity_version,
            batch_selected,
            batch_mode,
        }
        div { class: "main-content",
            if *batch_mode.read() {
                BatchTagEditor {
                    batch_selected,
                    entity_version,
                }
            } else {
                TagEditor {
                    selected_entity_id,
                    entity_version,
                }
            }
        }
        EntityProperties {
            selected_entity_id,
            entity_version,
            batch_mode,
            batch_selected,
        }
    }
}

// ----------------------------------------------------------------
// Left pane: Device/Point Browser (from loaded scenario)
// ----------------------------------------------------------------

#[component]
fn HaystackDeviceBrowser(
    selected_entity_id: Signal<Option<String>>,
    entity_version: Signal<u64>,
    batch_selected: Signal<HashSet<String>>,
    batch_mode: Signal<bool>,
) -> Element {
    let state = use_context::<AppState>();
    let mut search = use_signal(String::new);
    let query = search.read().clone();
    let is_batch = *batch_mode.read();
    let _ver = *entity_version.read();
    let nv = *state.node_version.read();

    // Fetch all equipment nodes from NodeStore
    let ns = state.node_store.clone();
    let equip_res = use_resource(move || {
        let ns = ns.clone();
        let _nv = nv;
        async move { ns.list_nodes(Some("equip"), None).await }
    });

    let equip_nodes = equip_res.read();
    let equip_list = equip_nodes.as_deref().unwrap_or(&[]);

    let q_lower = query.to_lowercase();
    let filtered: Vec<&NodeRecord> = equip_list
        .iter()
        .filter(|n| {
            q_lower.is_empty()
                || n.id.to_lowercase().contains(&q_lower)
                || n.dis.to_lowercase().contains(&q_lower)
        })
        .collect();

    rsx! {
        div { class: "sidebar config-device-browser",
            div { class: "details-header",
                span { "Equipment / Points" }
            }

            // Search bar
            div { class: "sidebar-search",
                input {
                    class: "sidebar-search-input",
                    r#type: "text",
                    placeholder: "Search equipment & points...",
                    value: "{query}",
                    oninput: move |evt| search.set(evt.value()),
                }
                if !query.is_empty() {
                    button {
                        class: "sidebar-search-clear",
                        onclick: move |_| search.set(String::new()),
                        "x"
                    }
                }
            }

            // Batch mode toggle
            div { class: "config-browser-actions",
                button {
                    class: if is_batch { "config-batch-toggle active" } else { "config-batch-toggle" },
                    onclick: move |_| {
                        let new_val = !*batch_mode.read();
                        batch_mode.set(new_val);
                        if !new_val {
                            batch_selected.set(HashSet::new());
                        }
                    },
                    "Batch Edit"
                }
                if is_batch {
                    span { class: "config-batch-count",
                        "{batch_selected.read().len()} selected"
                    }
                    button {
                        class: "config-browser-action-btn",
                        onclick: move |_| batch_selected.set(HashSet::new()),
                        "Clear"
                    }
                }
            }

            // Device/point list
            div { class: "sidebar-content",
                if filtered.is_empty() && !query.is_empty() {
                    div { class: "tree-empty-search", "No matches" }
                }
                if filtered.is_empty() && query.is_empty() {
                    div { class: "tree-empty-search", "No equipment found. Accept devices in Discovery first." }
                }
                for node in &filtered {
                    HaystackDeviceNode {
                        device_id: node.id.clone(),
                        device_dis: if node.dis.is_empty() { node.id.clone() } else { node.dis.clone() },
                        filter: query.clone(),
                        selected_entity_id,
                        entity_version,
                        batch_selected,
                        batch_mode,
                    }
                }
            }
        }
    }
}

#[component]
fn HaystackDeviceNode(
    device_id: String,
    device_dis: String,
    filter: String,
    selected_entity_id: Signal<Option<String>>,
    entity_version: Signal<u64>,
    batch_selected: Signal<HashSet<String>>,
    batch_mode: Signal<bool>,
) -> Element {
    let state = use_context::<AppState>();
    let mut expanded = use_signal(|| false);
    let is_batch = *batch_mode.read();
    let _ver = *entity_version.read();
    let nv = *state.node_version.read();

    let equip_entity_id = device_id.clone();
    let is_selected = selected_entity_id.read().as_deref() == Some(&equip_entity_id);

    // Check if entity exists in entity store
    let es = state.entity_store.clone();
    let eid = equip_entity_id.clone();
    let entity_exists = use_resource(move || {
        let store = es.clone();
        let id = eid.clone();
        let _v = *entity_version.read();
        async move { store.get_entity(&id).await.ok() }
    });

    let has_entity = entity_exists
        .read()
        .as_ref()
        .map(|e| e.is_some())
        .unwrap_or(false);
    let tag_count = entity_exists
        .read()
        .as_ref()
        .and_then(|e| e.as_ref().map(|ent| ent.tags.len()))
        .unwrap_or(0);

    // Fetch child points from NodeStore
    let ns = state.node_store.clone();
    let did_for_pts = device_id.clone();
    let points_res = use_resource(move || {
        let ns = ns.clone();
        let did = did_for_pts.clone();
        let _nv = nv;
        async move { ns.list_nodes(Some("point"), Some(&did)).await }
    });

    let points_read = points_res.read();
    let all_points = points_read.as_deref().unwrap_or(&[]);

    let q_lower = filter.to_lowercase();
    let visible_points: Vec<&NodeRecord> = all_points
        .iter()
        .filter(|pt| {
            filter.is_empty()
                || pt.id.to_lowercase().contains(&q_lower)
                || pt.dis.to_lowercase().contains(&q_lower)
                || device_id.to_lowercase().contains(&q_lower)
        })
        .collect();

    // For batch mode: collect point IDs
    let point_ids: Vec<String> = all_points.iter().map(|pt| pt.id.clone()).collect();

    let click_eid = equip_entity_id.clone();
    let dev_dis_display = device_dis.clone();

    rsx! {
        div {
            class: if is_selected && !is_batch { "config-device-node selected" } else { "config-device-node" },
            onclick: move |_| {
                if !is_batch {
                    selected_entity_id.set(Some(click_eid.clone()));
                }
            },

            if is_batch {
                {
                    let eid_check = equip_entity_id.clone();
                    let sel = batch_selected.read().clone();
                    let device_checked = sel.contains(&eid_check);
                    let pids = point_ids.clone();
                    let all_points_checked = !pids.is_empty() && pids.iter().all(|pid| sel.contains(pid));

                    // Tri-state: ✓ = device + all points, — = device only, unchecked = none
                    let check_state = if device_checked && all_points_checked {
                        "all" // ✓
                    } else if device_checked {
                        "partial" // —
                    } else {
                        "none" // unchecked
                    };

                    rsx! {
                        span {
                            class: "config-tristate-check",
                            onclick: move |evt| {
                                evt.stop_propagation();
                                let mut set = batch_selected.read().clone();
                                match check_state {
                                    "all" => {
                                        for pid in &pids {
                                            set.remove(pid);
                                        }
                                        set.insert(eid_check.clone());
                                    }
                                    "partial" => {
                                        set.remove(&eid_check);
                                        for pid in &pids {
                                            set.remove(pid);
                                        }
                                    }
                                    _ => {
                                        set.insert(eid_check.clone());
                                        for pid in &pids {
                                            set.insert(pid.clone());
                                        }
                                    }
                                }
                                batch_selected.set(set);
                            },
                            match check_state {
                                "all" => "\u{2611}",
                                "partial" => "\u{25A3}",
                                _ => "\u{2610}",
                            }
                        }
                    }
                }
            }

            span {
                class: "config-tree-toggle",
                onclick: move |evt| {
                    evt.stop_propagation();
                    expanded.set(!expanded());
                },
                if *expanded.read() { "\u{25BE}" } else { "\u{25B8}" }
            }

            div {
                class: "config-device-info",
                span { class: "config-type-badge config-type-equip", "E" }
                span { class: "config-device-name", "{dev_dis_display}" }
                if has_entity && tag_count > 0 {
                    span { class: "config-tag-count", "{tag_count}" }
                }
                if !has_entity {
                    span { class: "config-no-entity", "untagged" }
                }
            }
        }

        // Expanded: show points
        if *expanded.read() {
            for pt in &visible_points {
                {
                    let point_entity_id = pt.id.clone();
                    let is_pt_selected = selected_entity_id.read().as_deref() == Some(&point_entity_id);
                    let pt_name = if pt.dis.is_empty() {
                        pt.id.split('/').last().unwrap_or(&pt.id).to_string()
                    } else {
                        pt.dis.clone()
                    };
                    let pt_units = pt.properties.get("units").cloned()
                        .or_else(|| pt.tags.get("unit").cloned().flatten())
                        .unwrap_or_default();

                    // Check entity exists
                    let es2 = state.entity_store.clone();
                    let peid = point_entity_id.clone();
                    let pt_entity = use_resource(move || {
                        let store = es2.clone();
                        let id = peid.clone();
                        let _v = *entity_version.read();
                        async move { store.get_entity(&id).await.ok() }
                    });

                    let pt_has_entity = pt_entity.read().as_ref().map(|e| e.is_some()).unwrap_or(false);
                    let pt_tag_count = pt_entity
                        .read()
                        .as_ref()
                        .and_then(|e| e.as_ref().map(|ent| ent.tags.len()))
                        .unwrap_or(0);

                    let click_peid = point_entity_id.clone();
                    let batch_peid = point_entity_id.clone();
                    let check_peid = point_entity_id.clone();

                    rsx! {
                        div {
                            class: if is_pt_selected && !is_batch { "config-point-node selected" } else { "config-point-node" },
                            onclick: move |_| {
                                if !is_batch {
                                    selected_entity_id.set(Some(click_peid.clone()));
                                }
                            },

                            if is_batch {
                                {
                                    let is_checked = batch_selected.read().contains(&check_peid);
                                    rsx! {
                                        input {
                                            r#type: "checkbox",
                                            checked: is_checked,
                                            onclick: move |evt| {
                                                evt.stop_propagation();
                                                let mut set = batch_selected.read().clone();
                                                if is_checked {
                                                    set.remove(&batch_peid);
                                                } else {
                                                    set.insert(batch_peid.clone());
                                                }
                                                batch_selected.set(set);
                                            },
                                        }
                                    }
                                }
                            }

                            div {
                                class: "config-point-info",
                                span { class: "config-type-badge config-type-point", "P" }
                                span { class: "config-point-name", "{pt_name}" }
                                if !pt_units.is_empty() {
                                    span { class: "config-point-units", "{pt_units}" }
                                }
                                if pt_has_entity && pt_tag_count > 0 {
                                    span { class: "config-tag-count", "{pt_tag_count}" }
                                }
                                if !pt_has_entity {
                                    span { class: "config-no-entity", "untagged" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Center pane: Tag Editor (single entity)
// ----------------------------------------------------------------

#[component]
fn TagEditor(selected_entity_id: Signal<Option<String>>, entity_version: Signal<u64>) -> Element {
    let state = use_context::<AppState>();

    // Read signals inside resource so it re-runs when selection changes
    let es = state.entity_store.clone();
    let entity_res = use_resource(move || {
        let store = es.clone();
        let id = selected_entity_id.read().clone();
        let _v = *entity_version.read();
        async move {
            match id {
                Some(eid) => store.get_entity(&eid).await.ok(),
                None => None,
            }
        }
    });

    let sel_id = selected_entity_id.read().clone();
    let Some(entity_id) = sel_id else {
        return rsx! {
            div { class: "config-tag-editor-empty",
                p { class: "placeholder", "Select an equipment or point to view and edit tags." }
            }
        };
    };

    let is_point = entity_id.contains('/');
    let entity_type = if is_point { "point" } else { "equip" };

    // Extract the entity if found
    let entity_opt: Option<Entity> = entity_res
        .read()
        .as_ref()
        .and_then(|e: &Option<Entity>| e.clone());

    match entity_opt {
        Some(entity) => rsx! {
            div {
                EntityTagEditor {
                    entity,
                    entity_version,
                }
                NodeRefsEditor { node_id: entity_id.clone() }
            }
        },
        None => rsx! {
            div {
                CreateEntityPrompt {
                    entity_id: entity_id.clone(),
                    entity_type: entity_type.to_string(),
                }
                NodeRefsEditor { node_id: entity_id.clone() }
            }
        },
    }
}

/// Shown when a device/point is selected but has no entity in the store yet.
#[component]
fn CreateEntityPrompt(entity_id: String, entity_type: String) -> Element {
    let state = use_context::<AppState>();
    let nv = *state.node_version.read();

    // Resolve display name from NodeStore
    let ns = state.node_store.clone();
    let eid_for_node = entity_id.clone();
    let node_res = use_resource(move || {
        let ns = ns.clone();
        let eid = eid_for_node.clone();
        let _nv = nv;
        async move { ns.get_node(&eid).await.ok() }
    });
    let node_read = node_res.read();
    let node_opt: Option<&NodeRecord> = node_read.as_ref().and_then(|o| o.as_ref());
    let display_name = node_opt
        .and_then(|n| {
            if n.dis.is_empty() {
                None
            } else {
                Some(n.dis.clone())
            }
        })
        .unwrap_or_else(|| {
            if entity_id.contains('/') {
                entity_id
                    .split('/')
                    .last()
                    .unwrap_or(&entity_id)
                    .to_string()
            } else {
                entity_id.clone()
            }
        });

    // For equipment, gather child point nodes from NodeStore
    let ns2 = state.node_store.clone();
    let eid_for_pts = entity_id.clone();
    let etype_for_pts = entity_type.clone();
    let points_res = use_resource(move || {
        let ns = ns2.clone();
        let eid = eid_for_pts.clone();
        let etype = etype_for_pts.clone();
        let _nv = nv;
        async move {
            if etype == "equip" {
                ns.list_nodes(Some("point"), Some(&eid)).await
            } else {
                Vec::new()
            }
        }
    });
    let points_read = points_res.read();
    let child_points = points_read.as_deref().unwrap_or(&[]);

    let device_points: Vec<(String, String, Option<String>)> = child_points
        .iter()
        .map(|pt| {
            let pt_id = pt.id.split('/').last().unwrap_or(&pt.id).to_string();
            let pt_name = if pt.dis.is_empty() {
                pt_id.clone()
            } else {
                pt.dis.clone()
            };
            let pt_units = pt
                .properties
                .get("units")
                .cloned()
                .or_else(|| pt.tags.get("unit").cloned().flatten());
            (pt_id, pt_name, pt_units)
        })
        .collect();
    let profile_name: String = node_opt
        .and_then(|n| n.properties.get("profile").cloned())
        .unwrap_or_default();
    let point_count = device_points.len();

    rsx! {
        div { class: "config-tag-editor config-create-prompt",
            div { class: "config-tag-header",
                span { class: "config-type-badge config-type-{entity_type}",
                    if entity_type == "point" { "P" } else { "E" }
                }
                h3 { "{display_name}" }
            }
            p { class: "config-hint", "This item has no Haystack entity yet." }
            p { class: "config-hint", "Create one to start adding tags." }

            {
                let eid = entity_id.clone();
                let etype = entity_type.clone();
                let dname = display_name.clone();
                let es = state.entity_store.clone();
                let pts = device_points.clone();
                // Derive parent_id for points: device entity ID
                let parent = if eid.contains('/') {
                    Some(eid.split('/').next().unwrap_or("").to_string())
                } else {
                    None
                };

                rsx! {
                    div { class: "config-create-actions",
                        button {
                            class: "config-btn config-btn-primary",
                            onclick: move |_| {
                                let store = es.clone();
                                let id = eid.clone();
                                let et = etype.clone();
                                let dn = dname.clone();
                                let pid = parent.clone();
                                let points = pts.clone();

                                let provider = bms_store_bridges::haystack::provider::Haystack4Provider;
                                let pname = profile_name.clone();

                                // Build equip tags
                                let mut initial_tags = vec![(et.clone(), None)];
                                let equip_tags_map: HashMap<String, Option<String>>;
                                if et == "equip" {
                                    let suggested = bms_store_bridges::haystack::auto_tag::suggest_equip_tags(
                                        &pname,
                                        &provider,
                                    );
                                    equip_tags_map = suggested.iter().cloned().collect();
                                    for (name, val) in &suggested {
                                        if !initial_tags.iter().any(|(n, _)| n == name) {
                                            initial_tags.push((name.clone(), val.clone()));
                                        }
                                    }
                                } else {
                                    equip_tags_map = HashMap::new();
                                    // For single point, auto-tag using both ID and display name
                                    let point_id_part = id.split('/').last().unwrap_or(&id);
                                    let suggested = bms_store_bridges::haystack::auto_tag::suggest_point_tags_multi(
                                        &[point_id_part, &dn],
                                        None,
                                        &equip_tags_map,
                                        &provider,
                                    );
                                    for (name, val) in suggested {
                                        if !initial_tags.iter().any(|(n, _)| n == &name) {
                                            initial_tags.push((name, val));
                                        }
                                    }
                                }

                                spawn(async move {
                                    // Create the main entity
                                    let _ = store.create_entity(
                                        &id,
                                        &et,
                                        &dn,
                                        pid.as_deref(),
                                        initial_tags,
                                    ).await;

                                    // For equipment: also create all point entities with auto-tags
                                    if et == "equip" {
                                        for (pt_id, pt_name, pt_units) in &points {
                                            let point_entity_id = format!("{}/{}", id, pt_id);
                                            // Use both ID and display name for better tag matching
                                            let suggested = bms_store_bridges::haystack::auto_tag::suggest_point_tags_multi(
                                                &[pt_id, pt_name],
                                                pt_units.as_deref(),
                                                &equip_tags_map,
                                                &provider,
                                            );
                                            let _ = store.create_entity(
                                                &point_entity_id,
                                                "point",
                                                pt_name,
                                                Some(&id),
                                                suggested,
                                            ).await;
                                        }
                                    }
                                });
                            },
                            if entity_type == "equip" && point_count > 0 {
                                "Auto-Tag Equipment + {point_count} Points"
                            } else {
                                "Create with Auto-Tags"
                            }
                        }
                        button {
                            class: "config-btn",
                            onclick: {
                                let store = state.entity_store.clone();
                                let id = entity_id.clone();
                                let et = entity_type.clone();
                                let dn = display_name.clone();
                                let pid = if id.contains('/') {
                                    Some(id.split('/').next().unwrap_or("").to_string())
                                } else {
                                    None
                                };
                                move |_| {
                                    let s = store.clone();
                                    let i = id.clone();
                                    let e = et.clone();
                                    let d = dn.clone();
                                    let p = pid.clone();
                                    spawn(async move {
                                        let _ = s.create_entity(
                                            &i,
                                            &e,
                                            &d,
                                            p.as_deref(),
                                            vec![(e.clone(), None)],
                                        ).await;
                                    });
                                }
                            },
                            "Create Empty Entity"
                        }
                    }
                }
            }
        }
    }
}

/// Tag editor for an existing entity.
#[component]
fn EntityTagEditor(entity: Entity, entity_version: Signal<u64>) -> Element {
    let state = use_context::<AppState>();
    let etype = entity.entity_type.clone();

    // Sort current tags
    let mut sorted_tags: Vec<_> = entity.tags.iter().collect();
    sorted_tags.sort_by_key(|(name, _)| (*name).clone());

    // Run validation against current in-memory tag state
    let validation_issues = validate_tags(&etype, &entity.tags);

    rsx! {
        div { class: "config-tag-editor",
            // Header
            div { class: "config-tag-header",
                span { class: "config-type-badge config-type-{etype}",
                    match etype.as_str() {
                        "equip" => "E",
                        "point" => "P",
                        "site" => "S",
                        "space" => "Sp",
                        _ => "?",
                    }
                }
                h3 { "{entity.dis}" }
                span { class: "config-entity-id-label", "{entity.id}" }
            }

            // Inline validation banners
            if !validation_issues.is_empty() {
                ValidationBanner { issues: validation_issues }
            }

            // Current tags
            div { class: "config-tag-list",
                h4 { class: "config-section-title", "Applied Tags ({sorted_tags.len()})" }
                if sorted_tags.is_empty() {
                    p { class: "config-hint", "No tags applied yet." }
                }
                div { class: "config-tag-chips",
                    for (tag_name, tag_value) in &sorted_tags {
                        {
                            let tn = tag_name.to_string();
                            let tv = (*tag_value).clone();
                            let remove_tn = tn.clone();
                            let remove_eid = entity.id.clone();
                            let es_remove = state.entity_store.clone();

                            rsx! {
                                div { class: "config-tag-chip",
                                    span { class: "config-tag-name", "{tn}" }
                                    if let Some(ref val) = tv {
                                        span { class: "config-tag-value", "= {val}" }
                                    }
                                    button {
                                        class: "config-tag-remove",
                                        title: "Remove tag",
                                        onclick: move |_| {
                                            let store = es_remove.clone();
                                            let eid = remove_eid.clone();
                                            let tname = remove_tn.clone();
                                            spawn(async move {
                                                let _ = store.remove_tag(&eid, &tname).await;
                                            });
                                        },
                                        "x"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Add tag dropdown
            AddTagDropdown {
                entity_id: entity.id.clone(),
                entity_type: etype.clone(),
                current_tags: entity.tags.clone(),
            }

            // Apply prototype (with PreviewModal)
            ApplyPrototype {
                entity_id: entity.id.clone(),
                entity_type: etype.clone(),
                current_tags: entity.tags.clone(),
            }
        }
    }
}

// ----------------------------------------------------------------
// Batch Tag Editor
// ----------------------------------------------------------------

#[component]
fn BatchTagEditor(batch_selected: Signal<HashSet<String>>, entity_version: Signal<u64>) -> Element {
    let state = use_context::<AppState>();
    let selected = batch_selected.read().clone();
    let count = selected.len();
    let _ver = *entity_version.read();

    if count == 0 {
        return rsx! {
            div { class: "config-tag-editor-empty",
                p { class: "placeholder", "Select items using checkboxes to batch edit tags." }
            }
        };
    }

    // Load all selected entities to find common tags
    let es = state.entity_store.clone();
    let ids: Vec<String> = selected.iter().cloned().collect();
    let entities_res = use_resource(move || {
        let store = es.clone();
        let entity_ids = ids.clone();
        let _v = *entity_version.read();
        async move {
            let mut entities = Vec::new();
            for id in &entity_ids {
                if let Ok(e) = store.get_entity(id).await {
                    entities.push(e);
                }
            }
            entities
        }
    });

    let entities = entities_res.read();
    let entities = entities.as_ref().map(|v| v.as_slice()).unwrap_or(&[]);

    // Find common tags (tags present in ALL selected entities)
    let common_tags: Vec<(String, Option<String>)> = if entities.is_empty() {
        vec![]
    } else {
        let first_tags: HashSet<&String> = entities[0].tags.keys().collect();
        let common_keys: Vec<&String> = first_tags
            .into_iter()
            .filter(|k| entities.iter().all(|e| e.tags.contains_key(*k)))
            .collect();
        let mut result: Vec<_> = common_keys
            .iter()
            .map(|k| (k.to_string(), entities[0].tags.get(*k).cloned().flatten()))
            .collect();
        result.sort_by(|a, b| a.0.cmp(&b.0));
        result
    };

    // Count how many actually have entities vs untagged
    let entity_count = entities.len();
    let untagged_count = count.saturating_sub(entity_count);

    // Determine dominant entity type for validation
    let equip_count = entities.iter().filter(|e| e.entity_type == "equip").count();
    let point_count = entities.iter().filter(|e| e.entity_type == "point").count();
    let dominant_type = if equip_count >= point_count && equip_count > 0 {
        "equip"
    } else if point_count > 0 {
        "point"
    } else {
        "equip" // fallback
    };
    let mixed_types = equip_count > 0 && point_count > 0;

    // Build merged tag map for validation (union of all tags present in at least one entity)
    let merged_tags: HashMap<String, Option<String>> = {
        let mut m: HashMap<String, Option<String>> = HashMap::new();
        for e in entities {
            for (k, v) in &e.tags {
                m.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
        m
    };

    // Run validation on the merged view
    let validation_issues: Vec<ValidationIssue> = if !entities.is_empty() {
        let mut issues = validate_tags(dominant_type, &merged_tags);
        // If mixed, also validate as point
        if mixed_types {
            let point_issues = validate_tags("point", &merged_tags);
            for pi in point_issues {
                if !issues.iter().any(|i: &ValidationIssue| i.message == pi.message) {
                    issues.push(pi);
                }
            }
        }
        issues
    } else {
        vec![]
    };

    let mut batch_tag_search = use_signal(String::new);
    let mut batch_show_dropdown = use_signal(|| false);

    let query = batch_tag_search.read().to_lowercase();
    // Show all tags (equip + point combined) for batch
    let all_tags = tags::tags_for_entity("equip");
    let point_tags = tags::tags_for_entity("point");
    let mut combined: Vec<_> = all_tags;
    for t in point_tags {
        if !combined.iter().any(|c| c.name == t.name) {
            combined.push(t);
        }
    }
    combined.sort_by_key(|t| t.name);

    let filtered: Vec<_> = combined
        .iter()
        .filter(|t| {
            query.is_empty()
                || t.name.to_lowercase().contains(&query)
                || t.doc.to_lowercase().contains(&query)
        })
        .collect();

    rsx! {
        div { class: "config-tag-editor config-batch-editor",
            div { class: "config-tag-header",
                h3 { "Batch Edit — {count} items" }
                if untagged_count > 0 {
                    span { class: "config-hint", "({untagged_count} untagged)" }
                }
            }

            // Validation banners for the merged batch view
            if !validation_issues.is_empty() {
                ValidationBanner { issues: validation_issues }
            }

            // Auto-tag all selected items
            {
                let es_auto = state.entity_store.clone();
                let sel_auto = selected.clone();
                let ns_auto = state.node_store.clone();
                rsx! {
                    div { class: "config-create-actions",
                        button {
                            class: "config-btn config-btn-primary",
                            onclick: move |_| {
                                let store = es_auto.clone();
                                let ns = ns_auto.clone();
                                let ids: Vec<String> = sel_auto.iter().cloned().collect();
                                spawn(async move {
                                    let provider = bms_store_bridges::haystack::provider::Haystack4Provider;
                                    for id in &ids {
                                        let is_point = id.contains('/');
                                        if is_point {
                                            // Point entity — get info from NodeStore
                                            let parts: Vec<&str> = id.splitn(2, '/').collect();
                                            let device_id = parts[0];
                                            let point_id = parts.get(1).unwrap_or(&"");

                                            let (pt_name, pt_units) = match ns.get_node(id).await {
                                                Ok(pn) => {
                                                    let name = if pn.dis.is_empty() { point_id.to_string() } else { pn.dis.clone() };
                                                    let units = pn.properties.get("units").cloned()
                                                        .or_else(|| pn.tags.get("unit").cloned().flatten());
                                                    (name, units)
                                                }
                                                Err(_) => (point_id.to_string(), None),
                                            };

                                            // Get parent equip tags for context
                                            let equip_tags_map: HashMap<String, Option<String>> = match ns.get_node(device_id).await {
                                                Ok(en) => {
                                                    let pname = en.properties.get("profile").cloned().unwrap_or_default();
                                                    bms_store_bridges::haystack::auto_tag::suggest_equip_tags(&pname, &provider)
                                                        .into_iter().collect()
                                                }
                                                Err(_) => HashMap::new(),
                                            };

                                            let suggested = bms_store_bridges::haystack::auto_tag::suggest_point_tags_multi(
                                                &[point_id, &pt_name],
                                                pt_units.as_deref(),
                                                &equip_tags_map,
                                                &provider,
                                            );

                                            if store.get_entity(id).await.ok().is_some() {
                                                for (name, val) in &suggested {
                                                    let _ = store.set_tag(id, name, val.as_deref()).await;
                                                }
                                            } else {
                                                let _ = store.create_entity(
                                                    id,
                                                    "point",
                                                    &pt_name,
                                                    Some(device_id),
                                                    suggested,
                                                ).await;
                                            }
                                        } else {
                                            // Equipment entity — get info from NodeStore
                                            let (dis, profile_name) = match ns.get_node(id).await {
                                                Ok(en) => {
                                                    let d = if en.dis.is_empty() { id.clone() } else { en.dis.clone() };
                                                    let p = en.properties.get("profile").cloned().unwrap_or_default();
                                                    (d, p)
                                                }
                                                Err(_) => (id.clone(), String::new()),
                                            };
                                            let suggested = bms_store_bridges::haystack::auto_tag::suggest_equip_tags(
                                                &profile_name,
                                                &provider,
                                            );
                                            let mut tags = vec![("equip".to_string(), None)];
                                            for (name, val) in &suggested {
                                                if !tags.iter().any(|(n, _)| n == name) {
                                                    tags.push((name.clone(), val.clone()));
                                                }
                                            }

                                            if store.get_entity(id).await.ok().is_some() {
                                                for (name, val) in &tags {
                                                    let _ = store.set_tag(id, name, val.as_deref()).await;
                                                }
                                            } else {
                                                let _ = store.create_entity(
                                                    id,
                                                    "equip",
                                                    &dis,
                                                    None,
                                                    tags,
                                                ).await;
                                            }
                                        }
                                    }
                                });
                            },
                            "Auto-Tag Selected ({count})"
                        }
                    }
                }
            }

            // Common tags section
            if !common_tags.is_empty() {
                div { class: "config-tag-list",
                    h4 { class: "config-section-title", "Common Tags (all {entity_count} entities)" }
                    div { class: "config-tag-chips",
                        for (tag_name, tag_value) in &common_tags {
                            {
                                let tn = tag_name.clone();
                                let tv = tag_value.clone();
                                let remove_tn = tn.clone();
                                let es = state.entity_store.clone();
                                let sel = selected.clone();

                                rsx! {
                                    div { class: "config-tag-chip",
                                        span { class: "config-tag-name", "{tn}" }
                                        if let Some(ref val) = tv {
                                            span { class: "config-tag-value", "= {val}" }
                                        }
                                        button {
                                            class: "config-tag-remove",
                                            title: "Remove from all",
                                            onclick: move |_| {
                                                let store = es.clone();
                                                let tname = remove_tn.clone();
                                                let ids: Vec<String> = sel.iter().cloned().collect();
                                                spawn(async move {
                                                    for id in &ids {
                                                        let _ = store.remove_tag(id, &tname).await;
                                                    }
                                                });
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

            // Apply Prototype to batch
            BatchApplyPrototype {
                selected_ids: selected.iter().cloned().collect(),
                entities: entities.to_vec(),
                mixed_types,
            }

            // Add tag to all
            div { class: "config-add-tag-section",
                h4 { class: "config-section-title", "Add Tag to All" }
                div { class: "config-tag-search-wrap",
                    input {
                        class: "config-input config-tag-search",
                        r#type: "text",
                        placeholder: "Search tags to add...",
                        value: "{batch_tag_search}",
                        oninput: move |evt| {
                            batch_tag_search.set(evt.value());
                            batch_show_dropdown.set(true);
                        },
                        onfocus: move |_| batch_show_dropdown.set(true),
                    }
                }

                if *batch_show_dropdown.read() && !filtered.is_empty() {
                    div { class: "config-tag-dropdown",
                        for tag_def in filtered.iter().take(20) {
                            {
                                let tname = tag_def.name.to_string();
                                let tkind = tag_def.kind.clone();
                                let tdoc = tag_def.doc;
                                let es = state.entity_store.clone();
                                let sel = selected.clone();

                                rsx! {
                                    div {
                                        class: "config-tag-option",
                                        onclick: move |_| {
                                            batch_show_dropdown.set(false);
                                            batch_tag_search.set(String::new());

                                            if tkind == TagKind::Marker {
                                                let store = es.clone();
                                                let ids: Vec<String> = sel.iter().cloned().collect();
                                                let name = tname.clone();
                                                spawn(async move {
                                                    for id in &ids {
                                                        let _ = store.set_tag(id, &name, None).await;
                                                    }
                                                });
                                            }
                                            // TODO: value tags in batch mode need input
                                        },
                                        span { class: "config-tag-opt-name", "{tname}" }
                                        span { class: "config-tag-opt-doc", "{tdoc}" }
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

// ----------------------------------------------------------------
// Validation Banner
// ----------------------------------------------------------------

/// Displays inline validation issues (errors, warnings, info) for a tag set.
/// Does not block saving — it is advisory only.
#[component]
fn ValidationBanner(issues: Vec<ValidationIssue>) -> Element {
    let errors: Vec<&ValidationIssue> = issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .collect();
    let warnings: Vec<&ValidationIssue> = issues
        .iter()
        .filter(|i| i.severity == Severity::Warning)
        .collect();
    let infos: Vec<&ValidationIssue> = issues
        .iter()
        .filter(|i| i.severity == Severity::Info)
        .collect();

    rsx! {
        div { class: "validation-banner-group",
            for issue in &errors {
                div { class: "validation-banner validation-error",
                    span { class: "validation-icon", "!" }
                    div { class: "validation-content",
                        span { class: "validation-message", "{issue.message}" }
                        if !issue.tags_involved.is_empty() {
                            span { class: "validation-tags",
                                "Tags: {issue.tags_involved.join(\", \")}"
                            }
                        }
                        if let Some(ref fix) = issue.suggested_fix {
                            span { class: "validation-fix", "Fix: {fix}" }
                        }
                    }
                }
            }
            for issue in &warnings {
                div { class: "validation-banner validation-warning",
                    span { class: "validation-icon", "!" }
                    div { class: "validation-content",
                        span { class: "validation-message", "{issue.message}" }
                        if !issue.tags_involved.is_empty() {
                            span { class: "validation-tags",
                                "Tags: {issue.tags_involved.join(\", \")}"
                            }
                        }
                        if let Some(ref fix) = issue.suggested_fix {
                            span { class: "validation-fix", "Fix: {fix}" }
                        }
                    }
                }
            }
            for issue in &infos {
                div { class: "validation-banner validation-info",
                    span { class: "validation-icon", "i" }
                    div { class: "validation-content",
                        span { class: "validation-message", "{issue.message}" }
                        if !issue.tags_involved.is_empty() {
                            span { class: "validation-tags",
                                "Tags: {issue.tags_involved.join(\", \")}"
                            }
                        }
                        if let Some(ref fix) = issue.suggested_fix {
                            span { class: "validation-fix", "Fix: {fix}" }
                        }
                    }
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Add Tag Dropdown (single entity)
// ----------------------------------------------------------------

#[component]
fn AddTagDropdown(
    entity_id: String,
    entity_type: String,
    current_tags: HashMap<String, Option<String>>,
) -> Element {
    let state = use_context::<AppState>();
    let mut search = use_signal(String::new);
    let mut show_dropdown = use_signal(|| false);
    let mut pending_value = use_signal(String::new);
    let mut pending_tag: Signal<Option<String>> = use_signal(|| None);

    let available = tags::tags_for_entity(&entity_type);
    let query = search.read().to_lowercase();
    let filtered: Vec<_> = available
        .iter()
        .filter(|t| !current_tags.contains_key(t.name))
        .filter(|t| {
            query.is_empty()
                || t.name.to_lowercase().contains(&query)
                || t.doc.to_lowercase().contains(&query)
        })
        .collect();

    rsx! {
        div { class: "config-add-tag-section",
            h4 { class: "config-section-title", "Add Tag" }

            if let Some(ref tag_name) = *pending_tag.read() {
                {
                    let tn = tag_name.clone();
                    let eid = entity_id.clone();
                    let es = state.entity_store.clone();
                    rsx! {
                        div { class: "config-value-input",
                            span { class: "config-tag-name", "{tn}" }
                            input {
                                class: "config-input",
                                r#type: "text",
                                placeholder: "Enter value...",
                                value: "{pending_value}",
                                oninput: move |evt| pending_value.set(evt.value()),
                            }
                            button {
                                class: "config-btn config-btn-primary",
                                onclick: move |_| {
                                    let store = es.clone();
                                    let entity = eid.clone();
                                    let name = tn.clone();
                                    let val = pending_value.read().clone();
                                    spawn(async move {
                                        let v = if val.is_empty() { None } else { Some(val.as_str()) };
                                        let _ = store.set_tag(&entity, &name, v).await;
                                    });
                                    pending_tag.set(None);
                                    pending_value.set(String::new());
                                },
                                "Set"
                            }
                            button {
                                class: "config-btn",
                                onclick: move |_| {
                                    pending_tag.set(None);
                                    pending_value.set(String::new());
                                },
                                "Cancel"
                            }
                        }
                    }
                }
            } else {
                div { class: "config-tag-search-wrap",
                    input {
                        class: "config-input config-tag-search",
                        r#type: "text",
                        placeholder: "Search tags...",
                        value: "{search}",
                        oninput: move |evt| {
                            search.set(evt.value());
                            show_dropdown.set(true);
                        },
                        onfocus: move |_| show_dropdown.set(true),
                    }
                }

                if *show_dropdown.read() && !filtered.is_empty() {
                    div { class: "config-tag-dropdown",
                        for tag_def in filtered.iter().take(20) {
                            {
                                let tname = tag_def.name.to_string();
                                let tkind = tag_def.kind.clone();
                                let tdoc = tag_def.doc;
                                let eid = entity_id.clone();
                                let es = state.entity_store.clone();

                                rsx! {
                                    div {
                                        class: "config-tag-option",
                                        onclick: move |_| {
                                            show_dropdown.set(false);
                                            search.set(String::new());

                                            if tkind == TagKind::Marker {
                                                let store = es.clone();
                                                let entity = eid.clone();
                                                let name = tname.clone();
                                                spawn(async move {
                                                    let _ = store.set_tag(&entity, &name, None).await;
                                                });
                                            } else {
                                                pending_tag.set(Some(tname.clone()));
                                                pending_value.set(String::new());
                                            }
                                        },
                                        span { class: "config-tag-opt-name", "{tname}" }
                                        span { class: "config-tag-opt-doc", "{tdoc}" }
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

// ----------------------------------------------------------------
// Apply Prototype (single entity, with PreviewModal dry-run)
// ----------------------------------------------------------------

#[component]
fn ApplyPrototype(
    entity_id: String,
    entity_type: String,
    current_tags: HashMap<String, Option<String>>,
) -> Element {
    let state = use_context::<AppState>();
    let mut show_protos = use_signal(|| false);
    // (proto_name, proto_tags) pending confirmation
    let mut pending_proto: Signal<Option<(String, Vec<(String, Option<String>)>)>> =
        use_signal(|| None);

    let prototypes = match entity_type.as_str() {
        "equip" => EQUIP_PROTOTYPES.iter().collect::<Vec<_>>(),
        "point" => POINT_PROTOTYPES.iter().collect::<Vec<_>>(),
        _ => vec![],
    };

    if prototypes.is_empty() {
        return rsx! {};
    }

    // Build preview rows for the pending prototype
    let preview_rows: Vec<PreviewRow> = if let Some((_, ptags)) = pending_proto.read().clone() {
        ptags
            .iter()
            .map(|(tag_name, tag_value)| {
                let before = current_tags
                    .get(tag_name)
                    .map(|v| v.clone().unwrap_or_else(|| "(marker)".to_string()))
                    .unwrap_or_else(|| "—".to_string());
                let after = tag_value
                    .clone()
                    .unwrap_or_else(|| "(marker)".to_string());
                let change_kind = if current_tags.contains_key(tag_name) {
                    ChangeKind::Modify
                } else {
                    ChangeKind::Add
                };
                PreviewRow {
                    id: format!("{}/{}", entity_id, tag_name),
                    label: tag_name.clone(),
                    before,
                    after,
                    change_kind,
                }
            })
            .collect()
    } else {
        vec![]
    };

    let proto_name_for_title = pending_proto
        .read()
        .as_ref()
        .map(|(n, _)| n.clone())
        .unwrap_or_default();

    rsx! {
        div { class: "config-prototype-section",
            h4 { class: "config-section-title", "Prototypes" }
            button {
                class: "config-btn",
                onclick: move |_| show_protos.set(!show_protos()),
                if *show_protos.read() { "Hide Prototypes" } else { "Apply Prototype..." }
            }

            if *show_protos.read() {
                div { class: "config-proto-list",
                    for proto in &prototypes {
                        {
                            let pname = proto.name;
                            let pdoc = proto.doc;
                            let ptags: Vec<(String, Option<String>)> = proto
                                .tags
                                .iter()
                                .map(|&(n, v)| (n.to_string(), v.map(|s| s.to_string())))
                                .collect();

                            rsx! {
                                div {
                                    class: "config-proto-card",
                                    onclick: move |_| {
                                        // Show preview instead of applying immediately
                                        pending_proto.set(Some((pname.to_string(), ptags.clone())));
                                        show_protos.set(false);
                                    },
                                    div { class: "config-proto-name", "{pname}" }
                                    div { class: "config-proto-doc", "{pdoc}" }
                                }
                            }
                        }
                    }
                }
            }

            // Dry-run preview modal
            if pending_proto.read().is_some() {
                {
                    let eid = entity_id.clone();
                    let es = state.entity_store.clone();
                    let tags_to_apply = pending_proto
                        .read()
                        .as_ref()
                        .map(|(_, t)| t.clone())
                        .unwrap_or_default();
                    rsx! {
                        PreviewModal {
                            title: format!("Apply Prototype: {proto_name_for_title}"),
                            rows: preview_rows,
                            on_confirm: move |_| {
                                let store = es.clone();
                                let entity = eid.clone();
                                let tags = tags_to_apply.clone();
                                spawn(async move {
                                    let _ = store.set_tags(&entity, tags).await;
                                });
                                pending_proto.set(None);
                            },
                            on_cancel: move |_| pending_proto.set(None),
                        }
                    }
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Batch Apply Prototype
// ----------------------------------------------------------------

/// Prototype picker for batch mode. Grouped by entity type if mixed.
#[component]
fn BatchApplyPrototype(
    selected_ids: Vec<String>,
    entities: Vec<Entity>,
    mixed_types: bool,
) -> Element {
    let state = use_context::<AppState>();
    let mut show_protos = use_signal(|| false);
    let mut pending_proto: Signal<Option<(String, Vec<(String, Option<String>)>)>> =
        use_signal(|| None);

    let equip_protos: Vec<_> = EQUIP_PROTOTYPES.iter().collect();
    let point_protos: Vec<_> = POINT_PROTOTYPES.iter().collect();

    let entity_count = selected_ids.len();

    // Build preview rows: one row per (entity, tag) pair
    let preview_rows: Vec<PreviewRow> =
        if let Some((pname, ptags)) = pending_proto.read().clone() {
            let mut rows = Vec::new();
            for e in &entities {
                for (tag_name, tag_value) in &ptags {
                    let before = e
                        .tags
                        .get(tag_name)
                        .map(|v| v.clone().unwrap_or_else(|| "(marker)".to_string()))
                        .unwrap_or_else(|| "—".to_string());
                    let after = tag_value
                        .clone()
                        .unwrap_or_else(|| "(marker)".to_string());
                    let change_kind = if e.tags.contains_key(tag_name) {
                        ChangeKind::Modify
                    } else {
                        ChangeKind::Add
                    };
                    rows.push(PreviewRow {
                        id: format!("{}/{}", e.id, tag_name),
                        label: format!(
                            "{} / {}",
                            if e.dis.is_empty() { &e.id } else { &e.dis },
                            tag_name
                        ),
                        before,
                        after,
                        change_kind,
                    });
                }
            }
            // Include untagged selected IDs
            for id in &selected_ids {
                if !entities.iter().any(|e| &e.id == id) {
                    for (tag_name, tag_value) in &ptags {
                        rows.push(PreviewRow {
                            id: format!("{}/{}", id, tag_name),
                            label: format!("{} / {} (untagged)", id, tag_name),
                            before: "—".to_string(),
                            after: tag_value
                                .clone()
                                .unwrap_or_else(|| "(marker)".to_string()),
                            change_kind: ChangeKind::Add,
                        });
                    }
                }
            }
            let _ = pname;
            rows
        } else {
            vec![]
        };

    let proto_title = pending_proto
        .read()
        .as_ref()
        .map(|(n, _)| format!("Apply Prototype: {} to {} items", n, entity_count))
        .unwrap_or_default();

    rsx! {
        div { class: "config-prototype-section",
            h4 { class: "config-section-title", "Apply Prototype to All" }
            button {
                class: "config-btn",
                onclick: move |_| show_protos.set(!show_protos()),
                if *show_protos.read() { "Hide Prototypes" } else { "Apply Prototype..." }
            }

            if *show_protos.read() {
                div { class: "config-proto-list",
                    div { class: "config-proto-group-header", "Equipment Prototypes" }
                    for proto in &equip_protos {
                        {
                            let pname = proto.name;
                            let pdoc = proto.doc;
                            let ptags: Vec<(String, Option<String>)> = proto
                                .tags
                                .iter()
                                .map(|&(n, v)| (n.to_string(), v.map(|s| s.to_string())))
                                .collect();

                            rsx! {
                                div {
                                    class: "config-proto-card",
                                    onclick: move |_| {
                                        pending_proto.set(Some((pname.to_string(), ptags.clone())));
                                        show_protos.set(false);
                                    },
                                    div { class: "config-proto-name", "{pname}" }
                                    div { class: "config-proto-doc", "{pdoc}" }
                                }
                            }
                        }
                    }

                    if mixed_types {
                        hr {}
                    }

                    div { class: "config-proto-group-header", "Point Prototypes" }
                    for proto in &point_protos {
                        {
                            let pname = proto.name;
                            let pdoc = proto.doc;
                            let ptags: Vec<(String, Option<String>)> = proto
                                .tags
                                .iter()
                                .map(|&(n, v)| (n.to_string(), v.map(|s| s.to_string())))
                                .collect();

                            rsx! {
                                div {
                                    class: "config-proto-card",
                                    onclick: move |_| {
                                        pending_proto.set(Some((pname.to_string(), ptags.clone())));
                                        show_protos.set(false);
                                    },
                                    div { class: "config-proto-name", "{pname}" }
                                    div { class: "config-proto-doc", "{pdoc}" }
                                }
                            }
                        }
                    }
                }
            }

            // Dry-run preview modal
            if pending_proto.read().is_some() {
                {
                    let es = state.entity_store.clone();
                    let ids = selected_ids.clone();
                    let tags_to_apply = pending_proto
                        .read()
                        .as_ref()
                        .map(|(_, t)| t.clone())
                        .unwrap_or_default();
                    rsx! {
                        PreviewModal {
                            title: proto_title,
                            rows: preview_rows,
                            on_confirm: move |_| {
                                let store = es.clone();
                                let entity_ids = ids.clone();
                                let tags = tags_to_apply.clone();
                                spawn(async move {
                                    for id in &entity_ids {
                                        let _ = store.set_tags(id, tags.clone()).await;
                                    }
                                });
                                pending_proto.set(None);
                            },
                            on_cancel: move |_| pending_proto.set(None),
                        }
                    }
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Right pane: Entity Properties + Relationships
// ----------------------------------------------------------------

#[component]
fn EntityProperties(
    selected_entity_id: Signal<Option<String>>,
    entity_version: Signal<u64>,
    batch_mode: Signal<bool>,
    batch_selected: Signal<HashSet<String>>,
) -> Element {
    let state = use_context::<AppState>();

    // Always call hooks unconditionally — read signal inside resource
    let es = state.entity_store.clone();
    let entity_res: Resource<Option<Entity>> = use_resource(move || {
        let store = es.clone();
        let id = selected_entity_id.read().clone();
        let _v = *entity_version.read();
        async move {
            match id {
                Some(eid) => store.get_entity(&eid).await.ok(),
                None => None,
            }
        }
    });

    let mut edit_name = use_signal(String::new);

    if *batch_mode.read() {
        return rsx! {
            div { class: "details-pane config-properties",
                div { class: "details-header", span { "Batch Info" } }
                div { class: "point-detail-body",
                    p { class: "config-hint",
                        "{batch_selected.read().len()} items selected for batch editing."
                    }
                    p { class: "config-hint",
                        "Use the center pane to add or remove tags across all selected items."
                    }
                }
            }
        };
    }

    let sel_id = selected_entity_id.read().clone();

    let Some(entity_id) = sel_id else {
        return rsx! {
            div { class: "details-pane config-properties",
                div { class: "details-header", span { "Properties" } }
                div { class: "point-detail-body",
                    p { class: "placeholder", "Select an item." }
                }
            }
        };
    };

    let entity_opt = entity_res
        .read()
        .as_ref()
        .and_then(|e: &Option<Entity>| e.clone());

    let Some(entity) = entity_opt else {
        return rsx! {
            div { class: "details-pane config-properties",
                div { class: "details-header", span { "Properties" } }
                div { class: "point-detail-body",
                    p { class: "config-hint", "No entity created yet for this item." }
                    p { class: "config-hint", "Select it and create an entity to edit properties." }
                }
            }
        };
    };

    // Sync edit_name when entity changes
    if *edit_name.read() != entity.dis && edit_name.read().is_empty() || entity_id != entity.id {
        edit_name.set(entity.dis.clone());
    }
    let name_changed = *edit_name.read() != entity.dis;

    rsx! {
        div { class: "details-pane config-properties",
            div { class: "details-header", span { "Properties" } }

            div { class: "point-detail-body",
                // Display name
                div { class: "config-prop-group",
                    label { class: "config-prop-label", "Display Name" }
                    div { class: "config-prop-row",
                        input {
                            class: "config-input",
                            r#type: "text",
                            value: "{edit_name}",
                            oninput: move |evt| edit_name.set(evt.value()),
                        }
                        if name_changed {
                            {
                                let eid = entity.id.clone();
                                let es = state.entity_store.clone();
                                rsx! {
                                    button {
                                        class: "config-btn config-btn-primary",
                                        onclick: move |_| {
                                            let store = es.clone();
                                            let id = eid.clone();
                                            let new_name = edit_name.read().clone();
                                            spawn(async move {
                                                let _ = store.update_entity(&id, &new_name).await;
                                            });
                                        },
                                        "Save"
                                    }
                                }
                            }
                        }
                    }
                }

                // Entity type
                div { class: "config-prop-group",
                    label { class: "config-prop-label", "Type" }
                    span { class: "config-prop-value", "{entity.entity_type}" }
                }

                // ID
                div { class: "config-prop-group",
                    label { class: "config-prop-label", "ID" }
                    span { class: "config-prop-value config-prop-id", "{entity.id}" }
                }

                // Parent
                if entity.parent_id.is_some() {
                    div { class: "config-prop-group",
                        label { class: "config-prop-label", "Parent" }
                        span { class: "config-prop-value",
                            {entity.parent_id.as_deref().unwrap_or("—")}
                        }
                    }
                }

                // Refs
                if !entity.refs.is_empty() {
                    div { class: "config-prop-group",
                        label { class: "config-prop-label", "References" }
                        for (ref_tag, target_id) in entity.refs.iter() {
                            div { class: "config-ref-row",
                                span { class: "config-ref-tag", "{ref_tag}" }
                                span { class: "config-ref-target", "{target_id}" }
                            }
                        }
                    }
                }

                // Add ref
                AddRefSection { entity_id: entity.id.clone() }

                // Delete entity
                div { class: "config-prop-group config-danger-zone",
                    {
                        let eid = entity.id.clone();
                        let es = state.entity_store.clone();
                        rsx! {
                            button {
                                class: "config-btn config-btn-danger",
                                onclick: move |_| {
                                    let store = es.clone();
                                    let id = eid.clone();
                                    spawn(async move {
                                        let _ = store.delete_entity(&id).await;
                                    });
                                    selected_entity_id.set(None);
                                },
                                "Delete Entity"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn AddRefSection(entity_id: String) -> Element {
    let state = use_context::<AppState>();
    let mut ref_tag = use_signal(|| "siteRef".to_string());
    let mut target_id = use_signal(String::new);
    let mut show_form = use_signal(|| false);

    let ref_options = vec!["siteRef", "equipRef", "spaceRef"];

    rsx! {
        div { class: "config-prop-group",
            label { class: "config-prop-label", "Add Reference" }
            if !*show_form.read() {
                button {
                    class: "config-btn",
                    onclick: move |_| show_form.set(true),
                    "+ Add Ref"
                }
            } else {
                div { class: "config-ref-form",
                    select {
                        class: "config-select",
                        value: "{ref_tag}",
                        onchange: move |evt| ref_tag.set(evt.value()),
                        for opt in &ref_options {
                            option { value: "{opt}", "{opt}" }
                        }
                    }
                    input {
                        class: "config-input",
                        r#type: "text",
                        placeholder: "Target entity ID...",
                        value: "{target_id}",
                        oninput: move |evt| target_id.set(evt.value()),
                    }
                    div { class: "config-add-actions",
                        {
                            let eid = entity_id.clone();
                            let es = state.entity_store.clone();
                            rsx! {
                                button {
                                    class: "config-btn config-btn-primary",
                                    disabled: target_id.read().trim().is_empty(),
                                    onclick: move |_| {
                                        let store = es.clone();
                                        let src = eid.clone();
                                        let tag = ref_tag.read().clone();
                                        let tgt = target_id.read().trim().to_string();
                                        spawn(async move {
                                            let _ = store.set_ref(&src, &tag, &tgt).await;
                                        });
                                        target_id.set(String::new());
                                        show_form.set(false);
                                    },
                                    "Set"
                                }
                            }
                        }
                        button {
                            class: "config-btn",
                            onclick: move |_| show_form.set(false),
                            "Cancel"
                        }
                    }
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Node Refs Editor (equipment relationships via NodeStore)
// ----------------------------------------------------------------

/// Equipment ref tags available for relationships.
const EQUIP_REF_TAGS: &[&str] = &[
    "equipRef",
    "ahuRef",
    "vavRef",
    "systemRef",
    "elecMeterRef",
    "hotWaterPlantRef",
    "chilledWaterPlantRef",
    "steamPlantRef",
    "meterRef",
    "panelRef",
    "networkRef",
];

const SPATIAL_REF_TAGS: &[&str] = &["siteRef", "buildingRef", "floorRef", "spaceRef"];

#[component]
fn NodeRefsEditor(node_id: String) -> Element {
    let state = use_context::<AppState>();
    let ns = state.node_store.clone();

    let nid = node_id.clone();
    let mut node_sig: Signal<Option<NodeRecord>> = use_signal(|| None);
    let mut incoming_sig: Signal<Vec<(String, NodeRecord)>> = use_signal(Vec::new);
    let mut equip_list_sig: Signal<Vec<(String, String)>> = use_signal(Vec::new);
    {
        let ns = ns.clone();
        let nid = nid.clone();
        let _ = use_resource(move || {
            let ns = ns.clone();
            let nid = nid.clone();
            let _nv = *state.node_version.read();
            async move {
                if let Ok(node) = ns.get_node(&nid).await {
                    node_sig.set(Some(node));
                }
                let incoming = ns.find_all_referencing(&nid).await;
                incoming_sig.set(incoming);
                let equips = ns.list_nodes(Some("equip"), None).await;
                let list: Vec<(String, String)> = equips
                    .into_iter()
                    .filter(|n| n.id != nid)
                    .map(|n| {
                        let label = if n.dis.is_empty() {
                            n.id.clone()
                        } else {
                            n.dis.clone()
                        };
                        (n.id, label)
                    })
                    .collect();
                equip_list_sig.set(list);
            }
        });
    }

    let node = node_sig.read();
    let incoming = incoming_sig.read();

    // Group incoming by ref tag
    let mut incoming_grouped: HashMap<String, Vec<&NodeRecord>> = HashMap::new();
    for (ref_tag, rec) in incoming.iter() {
        incoming_grouped
            .entry(ref_tag.clone())
            .or_default()
            .push(rec);
    }

    // Outgoing refs (exclude spatial — managed via nav tree)
    let outgoing: Vec<(String, String)> = node
        .as_ref()
        .map(|n| {
            n.refs
                .iter()
                .filter(|(k, _)| !SPATIAL_REF_TAGS.contains(&k.as_str()))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        })
        .unwrap_or_default();

    let mut show_add = use_signal(|| false);
    let mut show_bulk = use_signal(|| false);

    rsx! {
        div { class: "config-tag-editor",
            div { class: "config-tag-header",
                h3 { "Equipment Refs" }
            }

            // Outgoing refs
            if !outgoing.is_empty() {
                div { class: "config-tag-list",
                    h4 { class: "config-section-title", "Outgoing ({outgoing.len()})" }
                    div { class: "config-tag-chips",
                        for (ref_tag, target_id) in &outgoing {
                            {
                                let rtag = ref_tag.clone();
                                let tid = target_id.clone();
                                let equips = equip_list_sig.read();
                                let target_name = equips.iter()
                                    .find(|(id, _)| id == &tid)
                                    .map(|(_, l)| l.clone())
                                    .unwrap_or_else(|| tid.clone());
                                rsx! {
                                    div { class: "config-tag-chip",
                                        span { class: "config-tag-name", "{rtag}" }
                                        span { class: "config-tag-value", "= {target_name}" }
                                        button {
                                            class: "config-tag-remove",
                                            title: "Remove ref",
                                            onclick: {
                                                let ns = state.node_store.clone();
                                                let nid = node_id.clone();
                                                let rtag = rtag.clone();
                                                move |_| {
                                                    let ns = ns.clone();
                                                    let nid = nid.clone();
                                                    let rtag = rtag.clone();
                                                    spawn(async move {
                                                        let _ = ns.remove_ref(&nid, &rtag).await;
                                                    });
                                                }
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

            // Incoming refs
            if !incoming_grouped.is_empty() {
                div { class: "config-tag-list",
                    h4 { class: "config-section-title", "Referenced By ({incoming.len()})" }
                    for (ref_tag, nodes) in incoming_grouped.iter() {
                        div { class: "rel-tag-group",
                            span { class: "rel-tag-label", "{ref_tag}" }
                            span { class: "rel-tag-count", "({nodes.len()})" }
                            div { class: "config-tag-chips",
                                for rec in nodes {
                                    {
                                        let rname = if rec.dis.is_empty() { rec.id.clone() } else { rec.dis.clone() };
                                        let rid = rec.id.clone();
                                        let rtag = ref_tag.clone();
                                        rsx! {
                                            div { class: "config-tag-chip",
                                                span { class: "config-tag-name", "{rname}" }
                                                button {
                                                    class: "config-tag-remove",
                                                    title: "Remove ref",
                                                    onclick: {
                                                        let ns = state.node_store.clone();
                                                        let rid = rid.clone();
                                                        let rtag = rtag.clone();
                                                        move |_| {
                                                            let ns = ns.clone();
                                                            let rid = rid.clone();
                                                            let rtag = rtag.clone();
                                                            spawn(async move {
                                                                let _ = ns.remove_ref(&rid, &rtag).await;
                                                            });
                                                        }
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

            // Action buttons
            div { class: "rel-actions",
                button {
                    class: "rel-action-btn",
                    onclick: move |_| show_add.set(true),
                    "+ Add Ref"
                }
                button {
                    class: "rel-action-btn",
                    onclick: move |_| show_bulk.set(true),
                    "+ Bulk Assign"
                }
            }

            if *show_add.read() {
                AddRefForm {
                    node_id: node_id.clone(),
                    equip_list: equip_list_sig.read().clone(),
                    on_close: move |_| show_add.set(false),
                }
            }

            if *show_bulk.read() {
                BulkRefForm {
                    node_id: node_id.clone(),
                    equip_list: equip_list_sig.read().clone(),
                    on_close: move |_| show_bulk.set(false),
                }
            }
        }
    }
}

#[component]
fn AddRefForm(
    node_id: String,
    equip_list: Vec<(String, String)>,
    on_close: EventHandler,
) -> Element {
    let state = use_context::<AppState>();
    let mut ref_tag = use_signal(|| "equipRef".to_string());
    let mut target_id = use_signal(String::new);
    let mut direction = use_signal(|| "outgoing".to_string());
    let mut filter_text = use_signal(String::new);

    let filter = filter_text.read().to_lowercase();
    let filtered: Vec<&(String, String)> = equip_list
        .iter()
        .filter(|(_, label)| filter.is_empty() || label.to_lowercase().contains(&filter))
        .collect();

    rsx! {
        div { class: "rel-form",
            h4 { "Add Ref" }

            div { class: "rel-form-row",
                label { "Direction" }
                select {
                    onchange: move |e| direction.set(e.value()),
                    option { value: "outgoing", selected: *direction.read() == "outgoing",
                        "This \u{2192} target"
                    }
                    option { value: "incoming", selected: *direction.read() == "incoming",
                        "Source \u{2192} this"
                    }
                }
            }

            div { class: "rel-form-row",
                label { "Ref" }
                select {
                    onchange: move |e| ref_tag.set(e.value()),
                    for tag in EQUIP_REF_TAGS {
                        option { value: *tag, selected: *ref_tag.read() == *tag, "{tag}" }
                    }
                }
            }

            div { class: "rel-form-row",
                label { "Equipment" }
                input {
                    r#type: "text",
                    placeholder: "Filter...",
                    value: "{filter_text}",
                    oninput: move |e| filter_text.set(e.value()),
                }
                select {
                    size: "6",
                    onchange: move |e| target_id.set(e.value()),
                    for (id, label) in &filtered {
                        option { value: "{id}", "{label}" }
                    }
                }
            }

            div { class: "rel-form-buttons",
                button {
                    class: "rel-action-btn primary",
                    disabled: target_id.read().is_empty(),
                    onclick: {
                        let ns = state.node_store.clone();
                        let nid = node_id.clone();
                        let on_close = on_close.clone();
                        move |_| {
                            let ns = ns.clone();
                            let nid = nid.clone();
                            let tag = ref_tag.read().clone();
                            let tid = target_id.read().clone();
                            let dir = direction.read().clone();
                            let on_close = on_close.clone();
                            spawn(async move {
                                if dir == "outgoing" {
                                    let _ = ns.set_ref(&nid, &tag, &tid).await;
                                } else {
                                    let _ = ns.set_ref(&tid, &tag, &nid).await;
                                }
                                on_close.call(());
                            });
                        }
                    },
                    "Set"
                }
                button {
                    class: "rel-action-btn",
                    onclick: move |_| on_close.call(()),
                    "Cancel"
                }
            }
        }
    }
}

#[component]
fn BulkRefForm(
    node_id: String,
    equip_list: Vec<(String, String)>,
    on_close: EventHandler,
) -> Element {
    let state = use_context::<AppState>();
    let mut ref_tag = use_signal(|| "ahuRef".to_string());
    let mut selected: Signal<HashMap<String, bool>> = use_signal(HashMap::new);
    let mut filter_text = use_signal(String::new);

    let filter = filter_text.read().to_lowercase();
    let filtered: Vec<&(String, String)> = equip_list
        .iter()
        .filter(|(_, label)| filter.is_empty() || label.to_lowercase().contains(&filter))
        .collect();

    let sel = selected.read();
    let count = sel.values().filter(|v| **v).count();

    rsx! {
        div { class: "rel-form bulk-form",
            h4 { "Bulk Assign" }

            div { class: "rel-form-row",
                label { "Ref" }
                select {
                    onchange: move |e| ref_tag.set(e.value()),
                    for tag in EQUIP_REF_TAGS {
                        option { value: *tag, selected: *ref_tag.read() == *tag, "{tag}" }
                    }
                }
            }

            p { class: "bulk-hint",
                "Set "
                strong { "{ref_tag}" }
                " on selected items, pointing to this equipment."
            }

            div { class: "rel-form-row",
                input {
                    r#type: "text",
                    placeholder: "Filter...",
                    value: "{filter_text}",
                    oninput: move |e| filter_text.set(e.value()),
                }
            }

            div { class: "bulk-select-all",
                input {
                    r#type: "checkbox",
                    checked: count == filtered.len() && !filtered.is_empty(),
                    onchange: {
                        let fids: Vec<String> = filtered.iter().map(|(id, _)| id.clone()).collect();
                        move |e: Event<FormData>| {
                            let check = e.value() == "true";
                            let mut sel = selected.write();
                            for id in &fids {
                                sel.insert(id.clone(), check);
                            }
                        }
                    },
                }
                span { "Select All ({filtered.len()})" }
            }

            div { class: "bulk-list",
                for (id, label) in &filtered {
                    {
                        let id_clone = id.clone();
                        let checked = sel.get(id.as_str()).copied().unwrap_or(false);
                        rsx! {
                            label { class: "bulk-item",
                                input {
                                    r#type: "checkbox",
                                    checked: checked,
                                    onchange: {
                                        let id = id_clone.clone();
                                        move |e: Event<FormData>| {
                                            let v = e.value() == "true";
                                            selected.write().insert(id.clone(), v);
                                        }
                                    },
                                }
                                "{label}"
                            }
                        }
                    }
                }
            }

            div { class: "rel-form-buttons",
                button {
                    class: "rel-action-btn primary",
                    disabled: count == 0,
                    onclick: {
                        let ns = state.node_store.clone();
                        let nid = node_id.clone();
                        let on_close = on_close.clone();
                        move |_| {
                            let ns = ns.clone();
                            let nid = nid.clone();
                            let tag = ref_tag.read().clone();
                            let sel = selected.read();
                            let updates: Vec<(String, String, String)> = sel
                                .iter()
                                .filter(|(_, v)| **v)
                                .map(|(id, _)| (id.clone(), tag.clone(), nid.clone()))
                                .collect();
                            let on_close = on_close.clone();
                            spawn(async move {
                                let _ = ns.set_refs(updates).await;
                                on_close.call(());
                            });
                        }
                    },
                    "Assign {count} Selected"
                }
                button {
                    class: "rel-action-btn",
                    onclick: move |_| on_close.call(()),
                    "Cancel"
                }
            }
        }
    }
}
