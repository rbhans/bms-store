use std::collections::HashSet;

use dioxus::prelude::*;

use crate::auth::Permission;
use crate::bridge::bacnet::BacnetNetworks;
use crate::config::profile::PointKind;
use crate::gui::state::AppState;
use crate::store::alarm_store::{
    ActiveAlarm, AlarmConfig, AlarmEvent, AlarmHistoryQuery, AlarmParams, AlarmSeverity,
    AlarmState, AlarmType,
};
use crate::store::audit_store::{AuditAction, AuditEntryBuilder};
use crate::store::notification_store::AlarmShelving;

// ----------------------------------------------------------------
// Tab state
// ----------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum AlarmTab {
    Active,
    History,
    Config,
}

// ----------------------------------------------------------------
// AlarmView — 3-pane layout (browser | tabs | template)
// ----------------------------------------------------------------

#[component]
pub fn AlarmView() -> Element {
    let mut tab = use_signal(|| AlarmTab::Active);
    let selected_points: Signal<HashSet<(String, String)>> = use_signal(HashSet::new);
    let search = use_signal(String::new);
    let bulk_status: Signal<Option<String>> = use_signal(|| None);
    let current_tab = *tab.read();

    rsx! {
        AlarmDeviceBrowser { selected_points, search }
        div { class: "main-content",
            div { class: "alarm-tabs",
                button {
                    class: if current_tab == AlarmTab::Active { "alarm-tab active" } else { "alarm-tab" },
                    onclick: move |_| tab.set(AlarmTab::Active),
                    "Active Alarms"
                }
                button {
                    class: if current_tab == AlarmTab::History { "alarm-tab active" } else { "alarm-tab" },
                    onclick: move |_| tab.set(AlarmTab::History),
                    "History"
                }
                button {
                    class: if current_tab == AlarmTab::Config { "alarm-tab active" } else { "alarm-tab" },
                    onclick: move |_| tab.set(AlarmTab::Config),
                    "Config"
                }
            }

            div { class: "alarm-tab-content",
                match current_tab {
                    AlarmTab::Active => rsx! { ActiveAlarmsTab {} },
                    AlarmTab::History => rsx! { AlarmHistoryTab {} },
                    AlarmTab::Config => rsx! { AlarmConfigTab {} },
                }
            }
        }
        AlarmTemplatePanel { selected_points, bulk_status }
    }
}

// ----------------------------------------------------------------
// Left pane: Device/Point Browser with checkboxes
// ----------------------------------------------------------------

#[component]
fn AlarmDeviceBrowser(
    selected_points: Signal<HashSet<(String, String)>>,
    search: Signal<String>,
) -> Element {
    let state = use_context::<AppState>();
    let query = search.read().clone();

    rsx! {
        div { class: "sidebar dash-device-browser",
            div { class: "details-header", span { "Devices / Points" } }
            // Search bar
            div { class: "sidebar-search",
                input {
                    class: "sidebar-search-input",
                    r#type: "text",
                    placeholder: "Search points...",
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
            // Bulk action buttons
            div { class: "alarm-browser-actions",
                button {
                    class: "alarm-browser-action-btn",
                    onclick: {
                        let devices = state.loaded.devices.clone();
                        let q = query.clone();
                        move |_| {
                            selected_points.set(collect_visible_points(&devices, &q, None));
                        }
                    },
                    "All"
                }
                button {
                    class: "alarm-browser-action-btn",
                    onclick: {
                        let devices = state.loaded.devices.clone();
                        let q = query.clone();
                        move |_| {
                            selected_points.set(collect_visible_points(&devices, &q, Some(PointKind::Analog)));
                        }
                    },
                    "Analog"
                }
                button {
                    class: "alarm-browser-action-btn",
                    onclick: {
                        let devices = state.loaded.devices.clone();
                        let q = query.clone();
                        move |_| {
                            selected_points.set(collect_visible_points(&devices, &q, Some(PointKind::Binary)));
                        }
                    },
                    "Binary"
                }
                button {
                    class: "alarm-browser-action-btn",
                    onclick: {
                        let devices = state.loaded.devices.clone();
                        let q = query.clone();
                        move |_| {
                            selected_points.set(collect_visible_points(&devices, &q, Some(PointKind::Multistate)));
                        }
                    },
                    "Multi"
                }
                button {
                    class: "alarm-browser-action-btn",
                    onclick: move |_| {
                        selected_points.set(HashSet::new());
                    },
                    "Clear"
                }
            }
            div { class: "sidebar-content",
                {
                    let filtered: Vec<_> = state.loaded.devices.iter()
                        .filter(|d| device_matches_alarm(d, &query))
                        .collect();
                    if filtered.is_empty() && !query.is_empty() {
                        rsx! { div { class: "tree-empty-search", "No matches" } }
                    } else {
                        rsx! {
                            for dev in filtered {
                                AlarmDeviceNode {
                                    device_id: dev.instance_id.clone(),
                                    filter: query.clone(),
                                    selected_points,
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn device_matches_alarm(dev: &crate::config::loader::LoadedDevice, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let q = query.to_lowercase();
    if dev.instance_id.to_lowercase().contains(&q) {
        return true;
    }
    if dev.profile.profile.name.to_lowercase().contains(&q) {
        return true;
    }
    dev.profile
        .points
        .iter()
        .any(|p| p.id.to_lowercase().contains(&q) || p.name.to_lowercase().contains(&q))
}

/// Collect all points that match the current search filter and optional kind filter.
fn collect_visible_points(
    devices: &[crate::config::loader::LoadedDevice],
    query: &str,
    kind_filter: Option<PointKind>,
) -> HashSet<(String, String)> {
    let mut set = HashSet::new();
    let has_filter = !query.is_empty();
    let q = query.to_lowercase();
    for dev in devices {
        if !device_matches_alarm(dev, query) {
            continue;
        }
        for pt in &dev.profile.points {
            if let Some(ref kind) = kind_filter {
                if pt.kind != *kind {
                    continue;
                }
            }
            if has_filter
                && !pt.id.to_lowercase().contains(&q)
                && !pt.name.to_lowercase().contains(&q)
                && !dev.instance_id.to_lowercase().contains(&q)
                && !dev.profile.profile.name.to_lowercase().contains(&q)
            {
                continue;
            }
            set.insert((dev.instance_id.clone(), pt.id.clone()));
        }
    }
    set
}

#[component]
fn AlarmDeviceNode(
    device_id: String,
    filter: String,
    selected_points: Signal<HashSet<(String, String)>>,
) -> Element {
    let has_filter = !filter.is_empty();
    let state = use_context::<AppState>();
    let mut expanded = use_signal(|| false);
    let is_open = *expanded.read() || has_filter;

    let device = state
        .loaded
        .devices
        .iter()
        .find(|d| d.instance_id == device_id);
    let Some(dev) = device else { return rsx! {} };

    let profile_name = dev.profile.profile.name.clone();
    let q = filter.to_lowercase();
    let visible_points: Vec<_> = dev
        .profile
        .points
        .iter()
        .filter(|p| {
            !has_filter || p.id.to_lowercase().contains(&q) || p.name.to_lowercase().contains(&q)
        })
        .map(|p| (p.id.clone(), p.name.clone(), p.kind.clone()))
        .collect();

    rsx! {
        div { class: "dash-device-node",
            div {
                class: "tree-node-row",
                onclick: move |_| expanded.set(!is_open),
                span { class: if is_open { "tree-arrow open" } else { "tree-arrow" }, ">" }
                span { class: "tree-label", "{device_id}" }
                span { class: "tree-badge", "{profile_name}" }
            }
            if is_open {
                div { class: "dash-point-list",
                    for (pt_id, pt_name, pt_kind) in &visible_points {
                        AlarmPointItem {
                            device_id: device_id.clone(),
                            point_id: pt_id.clone(),
                            point_name: pt_name.clone(),
                            point_kind: pt_kind.clone(),
                            selected_points,
                        }
                    }
                    if visible_points.is_empty() {
                        div { class: "dash-point-item muted", "No points" }
                    }
                }
            }
        }
    }
}

#[component]
fn AlarmPointItem(
    device_id: String,
    point_id: String,
    point_name: String,
    point_kind: PointKind,
    selected_points: Signal<HashSet<(String, String)>>,
) -> Element {
    let key = (device_id.clone(), point_id.clone());
    let is_checked = selected_points.read().contains(&key);
    let kind_label = match point_kind {
        PointKind::Analog => "Ana",
        PointKind::Binary => "Bin",
        PointKind::Multistate => "Ms",
    };
    let kind_class = match point_kind {
        PointKind::Analog => "alarm-point-kind ana",
        PointKind::Binary => "alarm-point-kind bin",
        PointKind::Multistate => "alarm-point-kind ms",
    };

    rsx! {
        label { class: "alarm-browser-point",
            input {
                r#type: "checkbox",
                checked: is_checked,
                onchange: {
                    let key = key.clone();
                    move |_| {
                        let mut set = selected_points.read().clone();
                        if set.contains(&key) {
                            set.remove(&key);
                        } else {
                            set.insert(key.clone());
                        }
                        selected_points.set(set);
                    }
                },
            }
            span { class: "dash-point-name", "{point_name}" }
            span { class: "{kind_class}", "{kind_label}" }
        }
    }
}

// ----------------------------------------------------------------
// Right pane: Alarm Template Panel
// ----------------------------------------------------------------

#[component]
fn AlarmTemplatePanel(
    selected_points: Signal<HashSet<(String, String)>>,
    bulk_status: Signal<Option<String>>,
) -> Element {
    let state = use_context::<AppState>();
    let count = selected_points.read().len();

    let mut alarm_type = use_signal(|| "high_limit".to_string());
    let mut severity = use_signal(|| "warning".to_string());
    let limit = use_signal(|| "80.0".to_string());
    let deadband = use_signal(|| "2.0".to_string());
    let delay = use_signal(|| "0".to_string());
    let fault_value = use_signal(|| "1.0".to_string());
    let timeout = use_signal(|| "300".to_string());
    let alarm_value = use_signal(|| "true".to_string());
    let alarm_states = use_signal(|| "1".to_string());
    let fb_device = use_signal(String::new);
    let fb_point = use_signal(String::new);

    let current_type = alarm_type.read().clone();

    // Device/point lists for command mismatch feedback picker
    let device_ids: Vec<String> = state
        .loaded
        .devices
        .iter()
        .map(|d| d.instance_id.clone())
        .collect();
    let selected_fb_dev = fb_device.read().clone();
    let fb_point_ids: Vec<String> = state
        .loaded
        .devices
        .iter()
        .find(|d| d.instance_id == selected_fb_dev)
        .map(|d| d.profile.points.iter().map(|p| p.id.clone()).collect())
        .unwrap_or_default();

    rsx! {
        div { class: "details-pane",
            div { class: "details-header", span { "Alarm Template" } }
            div { class: "alarm-template-body",
                if count == 0 {
                    div { class: "alarm-template-empty",
                        p { "Select points from the device browser to configure alarms in bulk." }
                    }
                } else {
                    div { class: "alarm-template-count", "{count} point(s) selected" }

                    div { class: "alarm-form-row",
                        label { "Type" }
                        select {
                            onchange: move |evt| alarm_type.set(evt.value()),
                            option { value: "high_limit", "High Limit" }
                            option { value: "low_limit", "Low Limit" }
                            option { value: "state_change", "State Change" }
                            option { value: "multi_state_alarm", "Multi-State" }
                            option { value: "command_mismatch", "Cmd Mismatch" }
                            option { value: "state_fault", "State Fault" }
                            option { value: "stale", "Stale" }
                        }
                    }
                    div { class: "alarm-form-row",
                        label { "Severity" }
                        select {
                            onchange: move |evt| severity.set(evt.value()),
                            option { value: "warning", "Warning" }
                            option { value: "critical", "Critical" }
                            option { value: "info", "Info" }
                            option { value: "life_safety", "Life Safety" }
                        }
                    }

                    // Type-specific params
                    {alarm_type_fields(
                        &current_type, limit, deadband, delay, fault_value, timeout,
                        alarm_value, alarm_states, fb_device, fb_point,
                        &device_ids, &fb_point_ids,
                    )}

                    if let Some(ref msg) = *bulk_status.read() {
                        div { class: "alarm-bulk-status", "{msg}" }
                    }

                    button {
                        class: "alarm-apply-btn",
                        onclick: move |_| {
                            let entries: Vec<(String, String)> = selected_points.read().iter().cloned().collect();
                            if entries.is_empty() {
                                return;
                            }
                            let typ = alarm_type.read().clone();
                            let sev_str = severity.read().clone();
                            let sev = AlarmSeverity::from_str(&sev_str).unwrap_or(AlarmSeverity::Warning);

                            let params = match build_alarm_params(
                                &typ, &limit, &deadband, &delay, &fault_value, &timeout,
                                &alarm_value, &alarm_states, &fb_device, &fb_point,
                            ) {
                                Ok(p) => p,
                                Err(e) => { bulk_status.set(Some(e)); return; }
                            };

                            let store = state.alarm_store.clone();
                            spawn(async move {
                                match store.create_configs_batch(&entries, sev, params).await {
                                    Ok(ids) => {
                                        let msg = if ids.is_empty() {
                                            "All alarms already exist (skipped duplicates)".to_string()
                                        } else {
                                            format!("Created {} alarm(s)", ids.len())
                                        };
                                        bulk_status.set(Some(msg));
                                        selected_points.set(HashSet::new());
                                    }
                                    Err(e) => {
                                        bulk_status.set(Some(format!("Error: {e}")));
                                    }
                                }
                            });
                        },
                        "Apply to {count} point(s)"
                    }
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Active Alarms Tab
// ----------------------------------------------------------------

#[component]
fn ActiveAlarmsTab() -> Element {
    let state = use_context::<AppState>();
    let mut alarms = use_signal(Vec::<ActiveAlarm>::new);
    let mut loading = use_signal(|| true);
    let mut shelving_list: Signal<Vec<AlarmShelving>> = use_signal(Vec::new);
    let shelve_target: Signal<Option<(i64, String)>> = use_signal(|| None);
    let shelve_reason = use_signal(String::new);
    let shelve_duration = use_signal(|| "4h".to_string());

    // Load active alarms
    let alarm_store = state.alarm_store.clone();
    use_effect(move || {
        let store = alarm_store.clone();
        spawn(async move {
            let result = store.get_active_alarms().await;
            alarms.set(result);
            loading.set(false);
        });
    });

    // Load active shelving on mount
    let notif_store_init = state.notification_store.clone();
    use_effect(move || {
        let store = notif_store_init.clone();
        spawn(async move {
            let result = store.list_active_shelving().await;
            shelving_list.set(result);
        });
    });

    // Refresh on store version changes
    let _version = state.store_version.read();
    let alarm_store_refresh = state.alarm_store.clone();
    let notif_store_refresh = state.notification_store.clone();
    use_future(move || {
        let store = alarm_store_refresh.clone();
        let notif_store = notif_store_refresh.clone();
        async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                let result = store.get_active_alarms().await;
                alarms.set(result);
                let shelves = notif_store.list_active_shelving().await;
                shelving_list.set(shelves);
            }
        }
    });

    let alarm_list = alarms.read();
    let shelves = shelving_list.read();
    let is_loading = *loading.read();

    rsx! {
        div { class: "alarm-active-tab",
            if is_loading {
                div { class: "alarm-loading", "Loading alarms..." }
            } else if alarm_list.is_empty() && shelves.is_empty() {
                div { class: "alarm-empty",
                    h3 { "No Active Alarms" }
                    p { "All systems normal." }
                }
            } else {
                // Active Shelving section (only shown when shelves exist)
                if !shelves.is_empty() {
                    ActiveShelvingSection { shelving_list }
                }

                div { class: "alarm-active-header",
                    span { class: "alarm-count", "{alarm_list.len()} active alarm(s)" }
                    AckAllButton {}
                }
                table { class: "alarm-table",
                    thead {
                        tr {
                            th { class: "col-severity", "" }
                            th { "Device" }
                            th { "Point" }
                            th { "Type" }
                            th { "Value" }
                            th { "Time" }
                            th { "State" }
                            th { "" }
                        }
                    }
                    tbody {
                        for alarm in alarm_list.iter() {
                            ActiveAlarmRow {
                                alarm: alarm.clone(),
                                shelve_target,
                            }
                        }
                    }
                }
            }

            // Shelve dialog
            if shelve_target.read().is_some() {
                ShelveDialog {
                    shelve_target,
                    shelve_reason,
                    shelve_duration,
                    shelving_list,
                }
            }
        }
    }
}

#[component]
fn AckAllButton() -> Element {
    let state = use_context::<AppState>();
    let can_ack = state.has_permission(Permission::AcknowledgeAlarms);
    let mut ack_result = use_signal(|| Option::<String>::None);

    if !can_ack {
        return rsx! {};
    }

    rsx! {
        button {
            class: "alarm-ack-all-btn",
            onclick: move |_| {
                let store = state.alarm_store.clone();
                let audit = state.audit_store.clone();
                let user = state.current_user.read().clone();
                spawn(async move {
                    match store.acknowledge_all().await {
                        Ok(count) => {
                            ack_result.set(Some(format!("Acknowledged {count} alarm(s)")));
                            let (uid, uname) = match user.as_ref() {
                                Some(u) => (u.id.as_str(), u.username.as_str()),
                                None => ("system", "system"),
                            };
                            let _ = audit.log_action(uid, uname,
                                AuditEntryBuilder::new(
                                    AuditAction::AcknowledgeAllAlarms, "alarm",
                                ).details(&format!("{count} alarms")),
                            ).await;
                        }
                        Err(e) => ack_result.set(Some(format!("Error: {e}"))),
                    }
                });
            },
            "Ack All"
        }
        if let Some(ref msg) = *ack_result.read() {
            span { class: "alarm-ack-msg", "{msg}" }
        }
    }
}

#[component]
fn ActiveShelvingSection(shelving_list: Signal<Vec<AlarmShelving>>) -> Element {
    let state = use_context::<AppState>();
    let notif_store = state.notification_store.clone();
    let audit_store = state.audit_store.clone();
    let mut collapsed = use_signal(|| false);
    let is_collapsed = *collapsed.read();
    let shelves = shelving_list.read();
    let count = shelves.len();

    rsx! {
        div { class: "alarm-shelving-section",
            div {
                class: "alarm-shelving-header",
                onclick: move |_| collapsed.set(!is_collapsed),
                span { class: if is_collapsed { "tree-arrow" } else { "tree-arrow open" }, ">" }
                span { "Active Shelving ({count})" }
            }
            if !is_collapsed {
                table { class: "alarm-table alarm-shelving-table",
                    thead {
                        tr {
                            th { "Alarm / Device" }
                            th { "Shelved By" }
                            th { "Reason" }
                            th { "Time Remaining" }
                            th { "" }
                        }
                    }
                    tbody {
                        for sh in shelves.iter() {
                            {
                                let sh_id = sh.id;
                                let target_label = match (&sh.alarm_config_id, &sh.device_id) {
                                    (Some(cfg_id), _) => format!("Config #{cfg_id}"),
                                    (_, Some(dev)) => format!("Device: {dev}"),
                                    _ => "All".to_string(),
                                };
                                let reason = sh.reason.clone();
                                let shelved_by = sh.shelved_by.clone();
                                let remaining = match sh.expires_ms {
                                    Some(exp) => {
                                        let now = std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .unwrap_or_default()
                                            .as_millis() as i64;
                                        let remaining_ms = exp - now;
                                        if remaining_ms <= 0 {
                                            "Expiring...".to_string()
                                        } else {
                                            let hours = remaining_ms / 3_600_000;
                                            let mins = (remaining_ms % 3_600_000) / 60_000;
                                            if hours > 0 {
                                                format!("{hours}h {mins}m")
                                            } else {
                                                format!("{mins}m")
                                            }
                                        }
                                    }
                                    None => "Indefinite".to_string(),
                                };
                                let ns = notif_store.clone();
                                let aud = audit_store.clone();

                                rsx! {
                                    tr { class: "alarm-row alarm-shelving-row",
                                        td { "{target_label}" }
                                        td { "{shelved_by}" }
                                        td { "{reason}" }
                                        td { "{remaining}" }
                                        td {
                                            button {
                                                class: "alarm-unshelve-btn",
                                                onclick: move |_| {
                                                    let ns = ns.clone();
                                                    let aud = aud.clone();
                                                    let user = state.current_user.read().clone();
                                                    spawn(async move {
                                                        let _ = ns.delete_shelving(sh_id).await;
                                                        let shelves = ns.list_active_shelving().await;
                                                        shelving_list.set(shelves);
                                                        let (uid, uname) = match user.as_ref() {
                                                            Some(u) => (u.id.as_str(), u.username.as_str()),
                                                            None => ("system", "system"),
                                                        };
                                                        let _ = aud.log_action(uid, uname,
                                                            AuditEntryBuilder::new(
                                                                AuditAction::UnshelveAlarm, "alarm",
                                                            ).resource_id(&format!("shelving-{sh_id}")),
                                                        ).await;
                                                    });
                                                },
                                                "Unshelve"
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

#[component]
fn ShelveDialog(
    shelve_target: Signal<Option<(i64, String)>>,
    mut shelve_reason: Signal<String>,
    mut shelve_duration: Signal<String>,
    shelving_list: Signal<Vec<AlarmShelving>>,
) -> Element {
    let state = use_context::<AppState>();
    let target = shelve_target.read().clone();
    let Some((config_id, device_id)) = target else {
        return rsx! {};
    };

    rsx! {
        div { class: "modal-overlay",
            onclick: move |_| shelve_target.set(None),
            div {
                class: "modal-dialog shelve-dialog",
                onclick: move |evt| evt.stop_propagation(),
                h3 { "Shelve Alarm" }
                p { class: "shelve-target-info",
                    "Config #{config_id} on {device_id}"
                }

                div { class: "alarm-form-row",
                    label { "Reason" }
                    input {
                        class: "shelve-reason-input",
                        r#type: "text",
                        placeholder: "Reason for shelving...",
                        value: "{shelve_reason}",
                        oninput: move |evt| shelve_reason.set(evt.value()),
                    }
                }

                div { class: "alarm-form-row",
                    label { "Duration" }
                    select {
                        class: "shelve-duration-select",
                        value: "{shelve_duration}",
                        onchange: move |evt| shelve_duration.set(evt.value()),
                        option { value: "1h", "1 hour" }
                        option { value: "4h", "4 hours" }
                        option { value: "8h", "8 hours" }
                        option { value: "24h", "24 hours" }
                        option { value: "indefinite", "Indefinite" }
                    }
                }

                div { class: "alarm-form-actions",
                    button {
                        class: "alarm-save-btn",
                        onclick: move |_| {
                            let notif_store = state.notification_store.clone();
                            let audit = state.audit_store.clone();
                            let user = state.current_user.read().clone();
                            let reason = shelve_reason.read().clone();
                            let duration_str = shelve_duration.read().clone();

                            let expires_ms = match duration_str.as_str() {
                                "1h" => {
                                    let now = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_millis() as i64;
                                    Some(now + 3_600_000)
                                }
                                "4h" => {
                                    let now = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_millis() as i64;
                                    Some(now + 4 * 3_600_000)
                                }
                                "8h" => {
                                    let now = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_millis() as i64;
                                    Some(now + 8 * 3_600_000)
                                }
                                "24h" => {
                                    let now = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_millis() as i64;
                                    Some(now + 24 * 3_600_000)
                                }
                                _ => None, // indefinite
                            };

                            let (shelved_by, uid, uname) = match user.as_ref() {
                                Some(u) => (u.username.clone(), u.id.clone(), u.username.clone()),
                                None => ("system".to_string(), "system".to_string(), "system".to_string()),
                            };

                            let dev_id = device_id.clone();
                            spawn(async move {
                                let _ = notif_store.create_shelving(
                                    Some(config_id),
                                    Some(dev_id.clone()),
                                    &shelved_by,
                                    &reason,
                                    expires_ms,
                                ).await;
                                // Refresh shelving list
                                let shelves = notif_store.list_active_shelving().await;
                                shelving_list.set(shelves);
                                // Audit
                                let _ = audit.log_action(&uid, &uname,
                                    AuditEntryBuilder::new(
                                        AuditAction::ShelveAlarm, "alarm",
                                    ).resource_id(&format!("{dev_id}/config-{config_id}"))
                                     .details(&format!("reason={reason}, duration={duration_str}")),
                                ).await;
                                // Close dialog and reset
                                shelve_target.set(None);
                                shelve_reason.set(String::new());
                                shelve_duration.set("4h".to_string());
                            });
                        },
                        "Shelve"
                    }
                    button {
                        class: "alarm-cancel-btn",
                        onclick: move |_| {
                            shelve_target.set(None);
                            shelve_reason.set(String::new());
                            shelve_duration.set("4h".to_string());
                        },
                        "Cancel"
                    }
                }
            }
        }
    }
}

#[component]
fn ActiveAlarmRow(alarm: ActiveAlarm, shelve_target: Signal<Option<(i64, String)>>) -> Element {
    let state = use_context::<AppState>();
    let can_ack = state.has_permission(Permission::AcknowledgeAlarms);
    let sev_class = severity_class(alarm.severity);
    let time_str = format_time_ms(alarm.trigger_time_ms);
    let state_str = alarm.state.as_str();
    let is_offnormal = alarm.state == AlarmState::Offnormal;
    let config_id = alarm.config_id;
    let shelve_device_id = alarm.device_id.clone();

    rsx! {
        tr { class: "alarm-row {sev_class}",
            td { class: "col-severity",
                span { class: "severity-dot {sev_class}" }
            }
            td { "{alarm.device_id}" }
            td { "{alarm.point_id}" }
            td { "{alarm.alarm_type.label()}" }
            td { class: "col-value", "{alarm.trigger_value:.1}" }
            td { "{time_str}" }
            td { "{state_str}" }
            td {
                if is_offnormal && can_ack {
                    button {
                        class: "alarm-ack-btn",
                        onclick: move |_| {
                            let store = state.alarm_store.clone();
                            let bridge_handle = state.bacnet_handle();
                            let audit = state.audit_store.clone();
                            let ack_user = state.current_user.read().clone();
                            let dev_id = alarm.device_id.clone();
                            let point_id = alarm.point_id.clone();
                            let alarm_type = alarm.alarm_type.clone();
                            spawn(async move {
                                let _ = store.acknowledge(config_id).await;
                                // Audit log
                                {
                                    let (uid, uname) = match ack_user.as_ref() {
                                        Some(u) => (u.id.as_str(), u.username.as_str()),
                                        None => ("system", "system"),
                                    };
                                    let _ = audit.log_action(uid, uname,
                                        AuditEntryBuilder::new(
                                            AuditAction::AcknowledgeAlarm, "alarm",
                                        ).resource_id(&format!("{dev_id}/{point_id}"))
                                         .details(&format!("config_id={config_id}")),
                                    ).await;
                                }
                                // Also acknowledge on the remote BACnet device
                                if let Some(instance) = dev_id.strip_prefix("bacnet-").and_then(|s| s.parse::<u32>().ok()) {
                                    // Parse point_id to extract BACnet ObjectId.
                                    // Point IDs are formatted as "{ObjectType}-{instance}" where
                                    // ObjectType is kebab-case (e.g. "analog-input-1"). Find the
                                    // last '-' that separates type name from numeric instance.
                                    let object_id = parse_bacnet_object_id(&point_id);
                                    if let Some(obj_id) = object_id {
                                        // Map our AlarmType to the BACnet EventState
                                        let event_state = alarm_type_to_event_state(&alarm_type);
                                        let guard = bridge_handle.lock().await;
                                        let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                                        if let Some(b) = nets.bridge_for_device(instance) {
                                            // TimeStamp::SequenceNumber(0) is used as a fallback
                                            // because we don't persist the original event timestamp
                                            // from the BACnet notification. Per BACnet spec, devices
                                            // should accept this for acknowledgement.
                                            if let Err(e) = b.acknowledge_alarm(
                                                instance,
                                                obj_id,
                                                event_state,
                                                rustbac_client::TimeStamp::SequenceNumber(0),
                                                "operator",
                                            ).await {
                                                eprintln!("BACnet alarm ack failed: {e}");
                                            }
                                        }
                                    } else {
                                        eprintln!("Could not parse BACnet object from point_id: {point_id}");
                                    }
                                }
                            });
                        },
                        "Ack"
                    }
                    button {
                        class: "alarm-shelve-btn",
                        onclick: {
                            let dev_id = shelve_device_id.clone();
                            move |_| {
                                shelve_target.set(Some((config_id, dev_id.clone())));
                            }
                        },
                        "Shelve"
                    }
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// BACnet alarm acknowledge helpers
// ----------------------------------------------------------------

/// Parse a point_id like "analog-input-1" into a BACnet ObjectId.
///
/// Point IDs are formatted as `"{ObjectType}-{instance}"` where ObjectType uses
/// kebab-case display (e.g. "analog-input", "binary-value", "multi-state-input").
/// We find the last '-' followed by only digits to split the type name from the
/// instance number.
fn parse_bacnet_object_id(point_id: &str) -> Option<rustbac_core::types::ObjectId> {
    // Find the last '-' where everything after it is a valid u32
    let mut split_pos = None;
    for (i, ch) in point_id.char_indices().rev() {
        if ch == '-' {
            let suffix = &point_id[i + 1..];
            if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) {
                split_pos = Some(i);
                break;
            }
        }
    }
    let pos = split_pos?;
    let type_name = &point_id[..pos];
    let instance_str = &point_id[pos + 1..];
    let obj_type = rustbac_core::types::ObjectType::from_name(type_name)?;
    let instance = instance_str.parse::<u32>().ok()?;
    Some(rustbac_core::types::ObjectId::new(obj_type, instance))
}

/// Map an OpenCrate AlarmType to the corresponding BACnet EventState.
fn alarm_type_to_event_state(alarm_type: &AlarmType) -> rustbac_client::EventState {
    match alarm_type {
        AlarmType::HighLimit => rustbac_client::EventState::HighLimit,
        AlarmType::LowLimit => rustbac_client::EventState::LowLimit,
        AlarmType::StateFault => rustbac_client::EventState::Fault,
        // All other alarm types map to Offnormal (the generic non-normal state)
        _ => rustbac_client::EventState::Offnormal,
    }
}

// ----------------------------------------------------------------
// History Tab
// ----------------------------------------------------------------

/// Snapshot of filter values, only updated on Apply/Clear.
#[derive(Clone, Default)]
struct HistoryFilterSnapshot {
    severity: Option<AlarmSeverity>,
    search: String,
    device: String,
    point: String,
    transition: String,
    start: String,
    end: String,
    limit: i64,
}

impl HistoryFilterSnapshot {
    fn to_query(&self) -> AlarmHistoryQuery {
        let (from_state, to_state) = match self.transition.as_str() {
            "raised" => (Some("normal".to_string()), Some("offnormal".to_string())),
            "acknowledged" => (None, Some("acknowledged".to_string())),
            "cleared" => (None, Some("normal".to_string())),
            _ => (None, None),
        };
        AlarmHistoryQuery {
            device_id: if self.device.is_empty() {
                None
            } else {
                Some(self.device.clone())
            },
            point_id: if self.point.is_empty() {
                None
            } else {
                Some(self.point.clone())
            },
            severity: self.severity,
            from_state,
            to_state,
            search: if self.search.is_empty() {
                None
            } else {
                Some(self.search.clone())
            },
            start_ms: parse_datetime_to_ms(&self.start),
            end_ms: parse_datetime_to_ms(&self.end),
            limit: Some(self.limit),
        }
    }
}

#[component]
fn AlarmHistoryTab() -> Element {
    let state = use_context::<AppState>();
    let mut events = use_signal(Vec::<AlarmEvent>::new);
    let mut total_count = use_signal(|| 0i64);
    let mut loading = use_signal(|| true);

    // Draft filter signals — these drive the UI controls only
    let mut filter_severity = use_signal(|| "".to_string());
    let mut filter_search = use_signal(String::new);
    let mut filter_device = use_signal(String::new);
    let mut filter_point = use_signal(String::new);
    let mut filter_transition = use_signal(|| "".to_string());
    let mut filter_start = use_signal(String::new);
    let mut filter_end = use_signal(String::new);
    let mut filter_limit = use_signal(|| "200".to_string());

    // Applied snapshot — only this drives queries
    let mut applied = use_signal(|| HistoryFilterSnapshot {
        limit: 200,
        ..Default::default()
    });

    // Snapshot current draft filter signals into `applied`.
    let mut apply_filters = move || {
        let sev_str = filter_severity.read().clone();
        applied.set(HistoryFilterSnapshot {
            severity: AlarmSeverity::from_str(&sev_str),
            search: filter_search.read().clone(),
            device: filter_device.read().clone(),
            point: filter_point.read().clone(),
            transition: filter_transition.read().clone(),
            start: filter_start.read().clone(),
            end: filter_end.read().clone(),
            limit: filter_limit.read().parse().unwrap_or(200),
        });
    };

    let alarm_store = state.alarm_store.clone();
    let alarm_store2 = state.alarm_store.clone();

    // Only re-runs when `applied` changes (not on every keystroke)
    use_effect(move || {
        let snap = applied.read().clone();
        let store = alarm_store.clone();
        let store2 = alarm_store2.clone();

        let query = snap.to_query();
        let count_query = AlarmHistoryQuery {
            limit: None,
            ..query.clone()
        };

        spawn(async move {
            let result = store.query_history(query).await.unwrap_or_default();
            events.set(result);
            loading.set(false);

            let count = store2.count_history(count_query).await.unwrap_or(0);
            total_count.set(count);
        });
    });

    let event_list = events.read();
    let is_loading = *loading.read();
    let shown = event_list.len();
    let total = *total_count.read();

    rsx! {
        div { class: "alarm-history-tab",
            div { class: "alarm-history-filters",
                input {
                    class: "alarm-filter-input",
                    r#type: "text",
                    placeholder: "Search device/point...",
                    value: "{filter_search}",
                    oninput: move |evt| filter_search.set(evt.value()),
                }
                input {
                    class: "alarm-filter-input",
                    r#type: "text",
                    placeholder: "Device ID",
                    value: "{filter_device}",
                    oninput: move |evt| filter_device.set(evt.value()),
                }
                input {
                    class: "alarm-filter-input",
                    r#type: "text",
                    placeholder: "Point ID",
                    value: "{filter_point}",
                    oninput: move |evt| filter_point.set(evt.value()),
                }
                select {
                    class: "alarm-filter-select",
                    value: "{filter_severity}",
                    onchange: move |evt| filter_severity.set(evt.value()),
                    option { value: "", "All Severities" }
                    option { value: "info", "Info" }
                    option { value: "warning", "Warning" }
                    option { value: "critical", "Critical" }
                    option { value: "life_safety", "Life Safety" }
                }
                select {
                    class: "alarm-filter-select",
                    value: "{filter_transition}",
                    onchange: move |evt| filter_transition.set(evt.value()),
                    option { value: "", "All Transitions" }
                    option { value: "raised", "Alarm Raised" }
                    option { value: "acknowledged", "Acknowledged" }
                    option { value: "cleared", "Cleared" }
                }
                input {
                    class: "alarm-filter-date",
                    r#type: "datetime-local",
                    value: "{filter_start}",
                    oninput: move |evt| filter_start.set(evt.value()),
                }
                input {
                    class: "alarm-filter-date",
                    r#type: "datetime-local",
                    value: "{filter_end}",
                    oninput: move |evt| filter_end.set(evt.value()),
                }
                select {
                    class: "alarm-filter-select alarm-filter-limit",
                    value: "{filter_limit}",
                    onchange: move |evt| filter_limit.set(evt.value()),
                    option { value: "100", "100" }
                    option { value: "200", "200" }
                    option { value: "500", "500" }
                    option { value: "1000", "1000" }
                }
                button {
                    class: "alarm-filter-btn",
                    onclick: move |_| apply_filters(),
                    "Apply"
                }
                button {
                    class: "alarm-filter-btn alarm-filter-clear",
                    onclick: move |_| {
                        filter_severity.set("".to_string());
                        filter_search.set(String::new());
                        filter_device.set(String::new());
                        filter_point.set(String::new());
                        filter_transition.set("".to_string());
                        filter_start.set(String::new());
                        filter_end.set(String::new());
                        filter_limit.set("200".to_string());
                        applied.set(HistoryFilterSnapshot { limit: 200, ..Default::default() });
                    },
                    "Clear"
                }
                AlarmExportButton { applied }
            }
            div { class: "alarm-result-count",
                if total > 0 {
                    "Showing {shown} of {total} events"
                } else if !is_loading {
                    "No matching events"
                }
            }
            if is_loading {
                div { class: "alarm-loading", "Loading history..." }
            } else if event_list.is_empty() {
                div { class: "alarm-empty",
                    p { "No alarm history." }
                }
            } else {
                table { class: "alarm-table",
                    thead {
                        tr {
                            th { "Time" }
                            th { "Device" }
                            th { "Point" }
                            th { class: "col-severity", "" }
                            th { "Transition" }
                            th { "Value" }
                        }
                    }
                    tbody {
                        for event in event_list.iter() {
                            AlarmEventRow { event: event.clone() }
                        }
                    }
                }
            }
        }
    }
}

/// Parse a `datetime-local` input value (e.g. "2026-03-24T14:30") to epoch ms.
fn parse_datetime_to_ms(s: &str) -> Option<i64> {
    if s.is_empty() {
        return None;
    }
    // datetime-local format: "YYYY-MM-DDTHH:MM" or "YYYY-MM-DDTHH:MM:SS"
    let parts: Vec<&str> = s.split('T').collect();
    if parts.len() != 2 {
        return None;
    }
    let date_parts: Vec<i64> = parts[0].split('-').filter_map(|p| p.parse().ok()).collect();
    let time_str = parts[1];
    let time_parts: Vec<i64> = time_str.split(':').filter_map(|p| p.parse().ok()).collect();
    if date_parts.len() != 3 || time_parts.len() < 2 {
        return None;
    }
    // Use libc to convert local time to epoch
    #[repr(C)]
    #[derive(Default)]
    struct Tm {
        tm_sec: i32,
        tm_min: i32,
        tm_hour: i32,
        tm_mday: i32,
        tm_mon: i32,
        tm_year: i32,
        tm_wday: i32,
        tm_yday: i32,
        tm_isdst: i32,
        tm_gmtoff: i64,
        tm_zone: *const i8,
    }
    extern "C" {
        fn mktime(tm: *mut Tm) -> i64;
    }
    let mut tm = Tm::default();
    tm.tm_year = (date_parts[0] - 1900) as i32;
    tm.tm_mon = (date_parts[1] - 1) as i32;
    tm.tm_mday = date_parts[2] as i32;
    tm.tm_hour = time_parts[0] as i32;
    tm.tm_min = time_parts[1] as i32;
    tm.tm_sec = if time_parts.len() > 2 {
        time_parts[2] as i32
    } else {
        0
    };
    tm.tm_isdst = -1; // let mktime determine DST
    let epoch = unsafe { mktime(&mut tm) };
    if epoch < 0 {
        None
    } else {
        Some(epoch * 1000)
    }
}

#[cfg(feature = "desktop")]
#[component]
fn AlarmExportButton(applied: Signal<HistoryFilterSnapshot>) -> Element {
    let state = use_context::<AppState>();
    let mut exporting = use_signal(|| false);

    rsx! {
        button {
            class: "alarm-export-btn",
            disabled: *exporting.read(),
            onclick: move |_| {
                exporting.set(true);
                let store = state.alarm_store.clone();
                let query = AlarmHistoryQuery {
                    limit: None, // uncapped for export
                    ..applied.read().to_query()
                };

                spawn(async move {
                    let events = store.query_history(query).await.unwrap_or_default();
                    let csv = format_alarm_csv(&events);
                    save_alarm_csv(csv, "alarm-journal.csv");
                    exporting.set(false);
                });
            },
            if *exporting.read() { "Exporting..." } else { "Export CSV" }
        }
    }
}

#[cfg(not(feature = "desktop"))]
#[component]
fn AlarmExportButton(applied: Signal<HistoryFilterSnapshot>) -> Element {
    rsx! {}
}

/// Escape a field for CSV output. Wraps in quotes if it contains comma, quote, or newline.
fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Format epoch millis to ISO 8601 local datetime string.
fn format_datetime_iso(ms: i64) -> String {
    #[repr(C)]
    #[derive(Default)]
    struct Tm {
        tm_sec: i32,
        tm_min: i32,
        tm_hour: i32,
        tm_mday: i32,
        tm_mon: i32,
        tm_year: i32,
        tm_wday: i32,
        tm_yday: i32,
        tm_isdst: i32,
        tm_gmtoff: i64,
        tm_zone: *const i8,
    }
    extern "C" {
        fn localtime_r(time: *const i64, result: *mut Tm) -> *mut Tm;
    }
    let epoch_secs = ms / 1000;
    let mut tm = Tm::default();
    unsafe { localtime_r(&epoch_secs, &mut tm) };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec,
    )
}

fn format_alarm_csv(events: &[AlarmEvent]) -> String {
    let mut csv = String::from(
        "timestamp,datetime,device_id,point_id,severity,from_state,to_state,value,note\n",
    );
    for e in events {
        let datetime = format_datetime_iso(e.timestamp_ms);
        let note = e.note.as_deref().unwrap_or("");
        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{:.2},{}\n",
            e.timestamp_ms,
            datetime,
            csv_escape(&e.device_id),
            csv_escape(&e.point_id),
            csv_escape(e.severity.as_str()),
            csv_escape(&e.from_state),
            csv_escape(&e.to_state),
            e.value,
            csv_escape(note),
        ));
    }
    csv
}

#[cfg(feature = "desktop")]
fn save_alarm_csv(csv_content: String, default_name: &str) {
    let name = default_name.to_string();
    spawn(async move {
        let path = tokio::task::spawn_blocking(move || {
            rfd::FileDialog::new()
                .add_filter("CSV", &["csv"])
                .set_file_name(&name)
                .save_file()
        })
        .await
        .ok()
        .flatten();

        if let Some(p) = path {
            let _ = tokio::fs::write(p, csv_content).await;
        }
    });
}

#[component]
fn AlarmEventRow(event: AlarmEvent) -> Element {
    let sev_class = severity_class(event.severity);
    let time_str = format_time_ms(event.timestamp_ms);
    let transition = format!("{} -> {}", event.from_state, event.to_state);

    rsx! {
        tr { class: "alarm-row",
            td { "{time_str}" }
            td { "{event.device_id}" }
            td { "{event.point_id}" }
            td { class: "col-severity",
                span { class: "severity-dot {sev_class}" }
            }
            td { "{transition}" }
            td { class: "col-value", "{event.value:.1}" }
        }
    }
}

// ----------------------------------------------------------------
// Config Tab
// ----------------------------------------------------------------

#[component]
fn AlarmConfigTab() -> Element {
    let state = use_context::<AppState>();
    let mut configs = use_signal(Vec::<AlarmConfig>::new);
    let mut loading = use_signal(|| true);
    let mut show_add_form = use_signal(|| false);

    let alarm_store = state.alarm_store.clone();
    use_effect(move || {
        let store = alarm_store.clone();
        spawn(async move {
            let result = store.list_configs().await;
            configs.set(result);
            loading.set(false);
        });
    });

    // Refresh when config changes
    let alarm_store_refresh = state.alarm_store.clone();
    let mut config_watch = use_signal(|| 0u64);
    use_future(move || {
        let store = alarm_store_refresh.clone();
        async move {
            let mut rx = store.subscribe_config_changes();
            loop {
                if rx.changed().await.is_err() {
                    break;
                }
                let result = store.list_configs().await;
                configs.set(result);
                config_watch.set(*rx.borrow());
            }
        }
    });

    let config_list = configs.read();
    let is_loading = *loading.read();
    let adding = *show_add_form.read();

    rsx! {
        div { class: "alarm-config-tab",
            div { class: "alarm-config-header",
                button {
                    class: "alarm-add-btn",
                    onclick: move |_| show_add_form.set(!adding),
                    if adding { "Cancel" } else { "+ Add Alarm" }
                }
            }

            if adding {
                AddAlarmForm {
                    on_done: move || show_add_form.set(false),
                }
            }

            if is_loading {
                div { class: "alarm-loading", "Loading configs..." }
            } else if config_list.is_empty() {
                div { class: "alarm-empty",
                    p { "No alarms configured." }
                    p { class: "alarm-empty-hint", "Click \"+ Add Alarm\" or use the bulk template panel." }
                }
            } else {
                table { class: "alarm-table",
                    thead {
                        tr {
                            th { "Device" }
                            th { "Point" }
                            th { "Type" }
                            th { "Params" }
                            th { class: "col-severity", "Severity" }
                            th { "Enabled" }
                            th { "" }
                        }
                    }
                    tbody {
                        for cfg in config_list.iter() {
                            AlarmConfigRow { config: cfg.clone() }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn AlarmConfigRow(config: AlarmConfig) -> Element {
    let state = use_context::<AppState>();
    let mut confirming = use_signal(|| false);
    let sev_class = severity_class(config.severity);
    let params_str = format_params(&config.params);
    let config_id = config.id;
    let enabled = config.enabled;
    let is_confirming = *confirming.read();

    rsx! {
        tr { class: "alarm-row",
            td { "{config.device_id}" }
            td { "{config.point_id}" }
            td { "{config.alarm_type.label()}" }
            td { class: "alarm-params-cell", "{params_str}" }
            td { class: "col-severity",
                span { class: "severity-dot {sev_class}" }
                " {config.severity.label()}"
            }
            td {
                span {
                    class: if enabled { "alarm-enabled-badge on" } else { "alarm-enabled-badge off" },
                    if enabled { "On" } else { "Off" }
                }
            }
            td {
                if is_confirming {
                    button {
                        class: "alarm-delete-btn confirm",
                        title: "Confirm delete",
                        onclick: move |_| {
                            let store = state.alarm_store.clone();
                            spawn(async move {
                                let _ = store.delete_config(config_id).await;
                            });
                        },
                        "Delete"
                    }
                    button {
                        class: "alarm-cancel-btn",
                        onclick: move |_| confirming.set(false),
                        "Cancel"
                    }
                } else {
                    button {
                        class: "alarm-delete-btn",
                        title: "Delete alarm",
                        onclick: move |_| confirming.set(true),
                        "x"
                    }
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Add Alarm Form (single-point, kept as fallback in Config tab)
// ----------------------------------------------------------------

#[component]
fn AddAlarmForm(on_done: EventHandler<()>) -> Element {
    let state = use_context::<AppState>();

    let mut device_id = use_signal(String::new);
    let mut point_id = use_signal(String::new);
    let mut alarm_type = use_signal(|| "high_limit".to_string());
    let mut severity = use_signal(|| "warning".to_string());
    let limit = use_signal(|| "80.0".to_string());
    let deadband = use_signal(|| "2.0".to_string());
    let delay = use_signal(|| "0".to_string());
    let fault_value = use_signal(|| "1.0".to_string());
    let timeout = use_signal(|| "300".to_string());
    let alarm_value = use_signal(|| "true".to_string());
    let alarm_states = use_signal(|| "1".to_string());
    let fb_device = use_signal(String::new);
    let fb_point = use_signal(String::new);
    let mut error_msg = use_signal(|| Option::<String>::None);

    let device_ids: Vec<String> = state
        .loaded
        .devices
        .iter()
        .map(|d| d.instance_id.clone())
        .collect();

    let selected_dev = device_id.read().clone();
    let point_ids: Vec<String> = state
        .loaded
        .devices
        .iter()
        .find(|d| d.instance_id == selected_dev)
        .map(|d| d.profile.points.iter().map(|p| p.id.clone()).collect())
        .unwrap_or_default();

    // Feedback device/point lists for command mismatch
    let selected_fb_dev = fb_device.read().clone();
    let fb_point_ids: Vec<String> = state
        .loaded
        .devices
        .iter()
        .find(|d| d.instance_id == selected_fb_dev)
        .map(|d| d.profile.points.iter().map(|p| p.id.clone()).collect())
        .unwrap_or_default();

    let current_type = alarm_type.read().clone();

    rsx! {
        div { class: "alarm-add-form",
            div { class: "alarm-form-row",
                label { "Device" }
                select {
                    onchange: move |evt| device_id.set(evt.value()),
                    option { value: "", "Select device..." }
                    for dev in &device_ids {
                        option { value: "{dev}", "{dev}" }
                    }
                }
            }
            div { class: "alarm-form-row",
                label { "Point" }
                select {
                    onchange: move |evt| point_id.set(evt.value()),
                    option { value: "", "Select point..." }
                    for pt in &point_ids {
                        option { value: "{pt}", "{pt}" }
                    }
                }
            }
            div { class: "alarm-form-row",
                label { "Type" }
                select {
                    onchange: move |evt| alarm_type.set(evt.value()),
                    option { value: "high_limit", "High Limit" }
                    option { value: "low_limit", "Low Limit" }
                    option { value: "state_change", "State Change" }
                    option { value: "multi_state_alarm", "Multi-State" }
                    option { value: "command_mismatch", "Cmd Mismatch" }
                    option { value: "state_fault", "State Fault" }
                    option { value: "stale", "Stale" }
                }
            }
            div { class: "alarm-form-row",
                label { "Severity" }
                select {
                    onchange: move |evt| severity.set(evt.value()),
                    option { value: "info", "Info" }
                    option { value: "warning", selected: true, "Warning" }
                    option { value: "critical", "Critical" }
                    option { value: "life_safety", "Life Safety" }
                }
            }

            {alarm_type_fields(
                &current_type, limit, deadband, delay, fault_value, timeout,
                alarm_value, alarm_states, fb_device, fb_point,
                &device_ids, &fb_point_ids,
            )}

            if let Some(ref err) = *error_msg.read() {
                div { class: "alarm-form-error", "{err}" }
            }

            div { class: "alarm-form-actions",
                button {
                    class: "alarm-save-btn",
                    onclick: move |_| {
                        let dev = device_id.read().clone();
                        let pt = point_id.read().clone();
                        let typ = alarm_type.read().clone();
                        let sev_str = severity.read().clone();

                        if dev.is_empty() || pt.is_empty() {
                            error_msg.set(Some("Select a device and point.".into()));
                            return;
                        }

                        let sev = AlarmSeverity::from_str(&sev_str).unwrap_or(AlarmSeverity::Warning);
                        let params = match build_alarm_params(
                            &typ, &limit, &deadband, &delay, &fault_value, &timeout,
                            &alarm_value, &alarm_states, &fb_device, &fb_point,
                        ) {
                            Ok(p) => p,
                            Err(e) => { error_msg.set(Some(e)); return; }
                        };

                        let store = state.alarm_store.clone();
                        let done = on_done.clone();
                        spawn(async move {
                            match store.create_config(&dev, &pt, sev, params).await {
                                Ok(_) => done.call(()),
                                Err(e) => error_msg.set(Some(format!("Error: {e}"))),
                            }
                        });
                    },
                    "Create"
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Point Detail: Alarm section (exported for use in point_detail.rs)
// ----------------------------------------------------------------

#[component]
pub fn PointAlarmSection(device_id: String, point_id: String) -> Element {
    let state = use_context::<AppState>();
    let mut configs = use_signal(Vec::<AlarmConfig>::new);
    let mut show_add = use_signal(|| false);

    let alarm_store = state.alarm_store.clone();
    let dev = device_id.clone();
    let pt = point_id.clone();
    use_effect(move || {
        let store = alarm_store.clone();
        let d = dev.clone();
        let p = pt.clone();
        spawn(async move {
            let result = store.get_configs_for_point(&d, &p).await;
            configs.set(result);
        });
    });

    // Refresh on config version changes
    let alarm_store_watch = state.alarm_store.clone();
    let dev2 = device_id.clone();
    let pt2 = point_id.clone();
    use_future(move || {
        let store = alarm_store_watch.clone();
        let d = dev2.clone();
        let p = pt2.clone();
        async move {
            let mut rx = store.subscribe_config_changes();
            loop {
                if rx.changed().await.is_err() {
                    break;
                }
                let result = store.get_configs_for_point(&d, &p).await;
                configs.set(result);
            }
        }
    });

    let config_list = configs.read();
    let adding = *show_add.read();

    rsx! {
        div { class: "point-alarm-section",
            h4 { class: "point-alarm-title", "Alarms" }

            if config_list.is_empty() && !adding {
                p { class: "point-alarm-empty", "No alarms configured." }
            }

            for cfg in config_list.iter() {
                PointAlarmItem { config: cfg.clone() }
            }

            if adding {
                PointAddAlarmForm {
                    device_id: device_id.clone(),
                    point_id: point_id.clone(),
                    on_done: move || show_add.set(false),
                }
            } else {
                button {
                    class: "point-alarm-add-btn",
                    onclick: move |_| show_add.set(true),
                    "+ Add Alarm"
                }
            }
        }
    }
}

#[component]
fn PointAlarmItem(config: AlarmConfig) -> Element {
    let state = use_context::<AppState>();
    let mut confirming = use_signal(|| false);
    let is_confirming = *confirming.read();
    let config_id = config.id;

    rsx! {
        div { class: "point-alarm-item",
            span { class: "severity-dot {severity_class(config.severity)}" }
            span { "{config.alarm_type.label()}" }
            span { class: "point-alarm-params", "{format_params(&config.params)}" }
            if is_confirming {
                button {
                    class: "alarm-delete-btn confirm",
                    title: "Confirm delete",
                    onclick: move |_| {
                        let store = state.alarm_store.clone();
                        spawn(async move {
                            let _ = store.delete_config(config_id).await;
                        });
                    },
                    "Delete"
                }
                button {
                    class: "alarm-cancel-btn",
                    onclick: move |_| confirming.set(false),
                    "Cancel"
                }
            } else {
                button {
                    class: "alarm-delete-btn",
                    title: "Delete alarm",
                    onclick: move |_| confirming.set(true),
                    "x"
                }
            }
        }
    }
}

/// Simplified add form for point detail panel.
#[component]
fn PointAddAlarmForm(device_id: String, point_id: String, on_done: EventHandler<()>) -> Element {
    let state = use_context::<AppState>();
    let mut error_msg = use_signal(|| Option::<String>::None);

    let mut alarm_type = use_signal(|| "high_limit".to_string());
    let mut severity = use_signal(|| "warning".to_string());
    let limit = use_signal(|| "80.0".to_string());
    let deadband = use_signal(|| "2.0".to_string());
    let delay = use_signal(|| "0".to_string());
    let fault_value = use_signal(|| "1.0".to_string());
    let timeout = use_signal(|| "300".to_string());
    let alarm_value = use_signal(|| "true".to_string());
    let alarm_states = use_signal(|| "1".to_string());
    let fb_device = use_signal(String::new);
    let fb_point = use_signal(String::new);

    let current_type = alarm_type.read().clone();

    let device_ids: Vec<String> = state
        .loaded
        .devices
        .iter()
        .map(|d| d.instance_id.clone())
        .collect();
    let selected_fb_dev = fb_device.read().clone();
    let fb_point_ids: Vec<String> = state
        .loaded
        .devices
        .iter()
        .find(|d| d.instance_id == selected_fb_dev)
        .map(|d| d.profile.points.iter().map(|p| p.id.clone()).collect())
        .unwrap_or_default();

    rsx! {
        div { class: "point-alarm-add-form",
            div { class: "alarm-form-row",
                label { "Type" }
                select {
                    onchange: move |evt| alarm_type.set(evt.value()),
                    option { value: "high_limit", "High Limit" }
                    option { value: "low_limit", "Low Limit" }
                    option { value: "state_change", "State Change" }
                    option { value: "multi_state_alarm", "Multi-State" }
                    option { value: "command_mismatch", "Cmd Mismatch" }
                    option { value: "state_fault", "State Fault" }
                    option { value: "stale", "Stale" }
                }
            }
            div { class: "alarm-form-row",
                label { "Severity" }
                select {
                    onchange: move |evt| severity.set(evt.value()),
                    option { value: "warning", "Warning" }
                    option { value: "critical", "Critical" }
                    option { value: "info", "Info" }
                    option { value: "life_safety", "Life Safety" }
                }
            }

            {alarm_type_fields(
                &current_type, limit, deadband, delay, fault_value, timeout,
                alarm_value, alarm_states, fb_device, fb_point,
                &device_ids, &fb_point_ids,
            )}

            if let Some(ref err) = *error_msg.read() {
                div { class: "alarm-form-error", "{err}" }
            }

            div { class: "alarm-form-actions",
                button {
                    class: "alarm-save-btn",
                    onclick: {
                        let device_id = device_id.clone();
                        let point_id = point_id.clone();
                        move |_| {
                            let typ = alarm_type.read().clone();
                            let sev_str = severity.read().clone();
                            let sev = AlarmSeverity::from_str(&sev_str).unwrap_or(AlarmSeverity::Warning);

                            let params = match build_alarm_params(
                                &typ, &limit, &deadband, &delay, &fault_value, &timeout,
                                &alarm_value, &alarm_states, &fb_device, &fb_point,
                            ) {
                                Ok(p) => p,
                                Err(e) => { error_msg.set(Some(e)); return; }
                            };

                            let store = state.alarm_store.clone();
                            let dev = device_id.clone();
                            let pt = point_id.clone();
                            let done = on_done.clone();
                            spawn(async move {
                                match store.create_config(&dev, &pt, sev, params).await {
                                    Ok(_) => done.call(()),
                                    Err(e) => error_msg.set(Some(format!("Error: {e}"))),
                                }
                            });
                        }
                    },
                    "Add"
                }
                button {
                    class: "alarm-cancel-btn",
                    onclick: move |_| on_done.call(()),
                    "Cancel"
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Shared form helpers
// ----------------------------------------------------------------

/// Renders type-specific alarm parameter fields.
#[allow(clippy::too_many_arguments)]
fn alarm_type_fields(
    current_type: &str,
    mut limit: Signal<String>,
    mut deadband: Signal<String>,
    mut delay: Signal<String>,
    mut fault_value: Signal<String>,
    mut timeout: Signal<String>,
    mut alarm_value: Signal<String>,
    mut alarm_states: Signal<String>,
    mut fb_device: Signal<String>,
    mut fb_point: Signal<String>,
    device_ids: &[String],
    fb_point_ids: &[String],
) -> Element {
    match current_type {
        "high_limit" | "low_limit" => rsx! {
            div { class: "alarm-form-row",
                label { "Limit" }
                input {
                    r#type: "number",
                    step: "0.1",
                    value: "{limit}",
                    onchange: move |evt| limit.set(evt.value()),
                }
            }
            div { class: "alarm-form-row",
                label { "Deadband" }
                input {
                    r#type: "number",
                    step: "0.1",
                    value: "{deadband}",
                    onchange: move |evt| deadband.set(evt.value()),
                }
            }
            div { class: "alarm-form-row",
                label { "Delay (s)" }
                input {
                    r#type: "number",
                    step: "1",
                    value: "{delay}",
                    onchange: move |evt| delay.set(evt.value()),
                }
            }
        },
        "state_fault" => rsx! {
            div { class: "alarm-form-row",
                label { "Fault Value" }
                input {
                    r#type: "number",
                    step: "1",
                    value: "{fault_value}",
                    onchange: move |evt| fault_value.set(evt.value()),
                }
            }
            div { class: "alarm-form-row",
                label { "Delay (s)" }
                input {
                    r#type: "number",
                    step: "1",
                    value: "{delay}",
                    onchange: move |evt| delay.set(evt.value()),
                }
            }
        },
        "stale" => rsx! {
            div { class: "alarm-form-row",
                label { "Timeout (s)" }
                input {
                    r#type: "number",
                    step: "1",
                    value: "{timeout}",
                    onchange: move |evt| timeout.set(evt.value()),
                }
            }
        },
        "state_change" => rsx! {
            div { class: "alarm-form-row",
                label { "Alarm When" }
                select {
                    onchange: move |evt| alarm_value.set(evt.value()),
                    option { value: "true", "ON (true)" }
                    option { value: "false", "OFF (false)" }
                }
            }
            div { class: "alarm-form-row",
                label { "Delay (s)" }
                input {
                    r#type: "number",
                    step: "1",
                    value: "{delay}",
                    onchange: move |evt| delay.set(evt.value()),
                }
            }
        },
        "multi_state_alarm" => rsx! {
            div { class: "alarm-form-row",
                label { "States" }
                input {
                    r#type: "text",
                    placeholder: "1, 3, 4",
                    value: "{alarm_states}",
                    onchange: move |evt| alarm_states.set(evt.value()),
                }
            }
            div { class: "alarm-form-hint", "Comma-separated state numbers that trigger the alarm." }
            div { class: "alarm-form-row",
                label { "Delay (s)" }
                input {
                    r#type: "number",
                    step: "1",
                    value: "{delay}",
                    onchange: move |evt| delay.set(evt.value()),
                }
            }
        },
        "command_mismatch" => rsx! {
            div { class: "alarm-form-row",
                label { "Fb Device" }
                select {
                    onchange: move |evt| fb_device.set(evt.value()),
                    option { value: "", "Select device..." }
                    for dev in device_ids {
                        option { value: "{dev}", "{dev}" }
                    }
                }
            }
            div { class: "alarm-form-row",
                label { "Fb Point" }
                select {
                    onchange: move |evt| fb_point.set(evt.value()),
                    option { value: "", "Select point..." }
                    for pt in fb_point_ids {
                        option { value: "{pt}", "{pt}" }
                    }
                }
            }
            div { class: "alarm-form-row",
                label { "Delay (s)" }
                input {
                    r#type: "number",
                    step: "1",
                    value: "{delay}",
                    onchange: move |evt| delay.set(evt.value()),
                }
            }
            div { class: "alarm-form-hint", "Alarms when command and feedback differ for the delay period." }
        },
        _ => rsx! {},
    }
}

/// Build AlarmParams from form signal values.
#[allow(clippy::too_many_arguments)]
fn build_alarm_params(
    typ: &str,
    limit: &Signal<String>,
    deadband: &Signal<String>,
    delay: &Signal<String>,
    fault_value: &Signal<String>,
    timeout: &Signal<String>,
    alarm_value: &Signal<String>,
    alarm_states: &Signal<String>,
    fb_device: &Signal<String>,
    fb_point: &Signal<String>,
) -> Result<AlarmParams, String> {
    let parse_f64 = |s: &Signal<String>, name: &str| -> Result<f64, String> {
        s.read()
            .parse::<f64>()
            .map_err(|_| format!("Invalid {name}: must be a number"))
    };
    let parse_u64 = |s: &Signal<String>, name: &str| -> Result<u64, String> {
        s.read()
            .parse::<u64>()
            .map_err(|_| format!("Invalid {name}: must be a positive integer"))
    };

    match typ {
        "high_limit" => {
            let l = parse_f64(limit, "limit")?;
            let d = parse_f64(deadband, "deadband")?;
            let dl = parse_u64(delay, "delay")?;
            Ok(AlarmParams::HighLimit {
                limit: l,
                deadband: d,
                delay_secs: dl,
            })
        }
        "low_limit" => {
            let l = parse_f64(limit, "limit")?;
            let d = parse_f64(deadband, "deadband")?;
            let dl = parse_u64(delay, "delay")?;
            Ok(AlarmParams::LowLimit {
                limit: l,
                deadband: d,
                delay_secs: dl,
            })
        }
        "state_fault" => {
            let fv = parse_f64(fault_value, "fault value")?;
            let dl = parse_u64(delay, "delay")?;
            Ok(AlarmParams::StateFault {
                fault_value: fv,
                delay_secs: dl,
            })
        }
        "stale" => {
            let t = parse_u64(timeout, "timeout")?;
            Ok(AlarmParams::Stale { timeout_secs: t })
        }
        "state_change" => {
            let av = alarm_value.read().as_str() == "true";
            let dl = parse_u64(delay, "delay")?;
            Ok(AlarmParams::StateChange {
                alarm_value: av,
                delay_secs: dl,
            })
        }
        "multi_state_alarm" => {
            let states: Vec<i64> = alarm_states
                .read()
                .split(',')
                .filter_map(|s| s.trim().parse::<i64>().ok())
                .collect();
            if states.is_empty() {
                return Err("Enter at least one alarm state number.".into());
            }
            let dl = parse_u64(delay, "delay")?;
            Ok(AlarmParams::MultiStateAlarm {
                alarm_states: states,
                delay_secs: dl,
            })
        }
        "command_mismatch" => {
            let fd = fb_device.read().clone();
            let fp = fb_point.read().clone();
            if fd.is_empty() || fp.is_empty() {
                return Err("Select feedback device and point.".into());
            }
            let dl = parse_u64(delay, "delay")?;
            Ok(AlarmParams::CommandMismatch {
                feedback_device_id: fd,
                feedback_point_id: fp,
                delay_secs: dl,
            })
        }
        _ => Err("Unknown alarm type.".into()),
    }
}

// ----------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------

fn severity_class(severity: AlarmSeverity) -> &'static str {
    match severity {
        AlarmSeverity::Info => "sev-info",
        AlarmSeverity::Warning => "sev-warning",
        AlarmSeverity::Critical => "sev-critical",
        AlarmSeverity::LifeSafety => "sev-life-safety",
    }
}

fn format_params(params: &AlarmParams) -> String {
    match params {
        AlarmParams::HighLimit {
            limit,
            deadband,
            delay_secs,
        } => {
            let mut s = format!("limit: {limit}");
            if *deadband > 0.0 {
                s.push_str(&format!(", db: {deadband}"));
            }
            if *delay_secs > 0 {
                s.push_str(&format!(", delay: {delay_secs}s"));
            }
            s
        }
        AlarmParams::LowLimit {
            limit,
            deadband,
            delay_secs,
        } => {
            let mut s = format!("limit: {limit}");
            if *deadband > 0.0 {
                s.push_str(&format!(", db: {deadband}"));
            }
            if *delay_secs > 0 {
                s.push_str(&format!(", delay: {delay_secs}s"));
            }
            s
        }
        AlarmParams::StateFault {
            fault_value,
            delay_secs,
        } => {
            let mut s = format!("fault: {fault_value}");
            if *delay_secs > 0 {
                s.push_str(&format!(", delay: {delay_secs}s"));
            }
            s
        }
        AlarmParams::Stale { timeout_secs } => format!("timeout: {timeout_secs}s"),
        AlarmParams::Deviation {
            threshold,
            deadband,
            ..
        } => {
            let mut s = format!("threshold: {threshold}");
            if *deadband > 0.0 {
                s.push_str(&format!(", db: {deadband}"));
            }
            s
        }
        AlarmParams::StateChange {
            alarm_value,
            delay_secs,
        } => {
            let state_label = if *alarm_value { "ON" } else { "OFF" };
            let mut s = format!("alarm when: {state_label}");
            if *delay_secs > 0 {
                s.push_str(&format!(", delay: {delay_secs}s"));
            }
            s
        }
        AlarmParams::MultiStateAlarm {
            alarm_states,
            delay_secs,
        } => {
            let states_str: Vec<String> = alarm_states.iter().map(|s| s.to_string()).collect();
            let mut s = format!("states: [{}]", states_str.join(", "));
            if *delay_secs > 0 {
                s.push_str(&format!(", delay: {delay_secs}s"));
            }
            s
        }
        AlarmParams::CommandMismatch {
            feedback_device_id,
            feedback_point_id,
            delay_secs,
        } => {
            format!("fb: {feedback_device_id}/{feedback_point_id}, delay: {delay_secs}s")
        }
    }
}

fn format_time_ms(ms: i64) -> String {
    #[repr(C)]
    #[derive(Default)]
    struct Tm {
        tm_sec: i32,
        tm_min: i32,
        tm_hour: i32,
        tm_mday: i32,
        tm_mon: i32,
        tm_year: i32,
        tm_wday: i32,
        tm_yday: i32,
        tm_isdst: i32,
        tm_gmtoff: i64,
        tm_zone: *const i8,
    }
    extern "C" {
        fn localtime_r(time: *const i64, result: *mut Tm) -> *mut Tm;
    }

    let epoch_secs = ms / 1000;
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let mut tm = Tm::default();
    unsafe { localtime_r(&epoch_secs, &mut tm) };

    let mut now_tm = Tm::default();
    unsafe { localtime_r(&now_secs, &mut now_tm) };

    let hour = tm.tm_hour;
    let min = tm.tm_min;
    let sec = tm.tm_sec;

    if tm.tm_year == now_tm.tm_year && tm.tm_yday == now_tm.tm_yday {
        format!("{hour:02}:{min:02}:{sec:02}")
    } else {
        const MONTHS: [&str; 12] = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        let mon = MONTHS.get(tm.tm_mon as usize).unwrap_or(&"???");
        let day = tm.tm_mday;
        format!("{mon} {day} {hour:02}:{min:02}:{sec:02}")
    }
}
