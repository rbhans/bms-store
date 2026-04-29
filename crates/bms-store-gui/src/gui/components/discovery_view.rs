use std::collections::HashMap;

use dioxus::prelude::*;

use bms_store_storage::auth::Permission;
use bms_store_bridges::bridge::bacnet::BacnetNetworks;
use bms_store_bridges::bridge::modbus::ModbusBridge;
use bms_store_storage::config::profile::{load_profile_library, DeviceProfile};
use bms_store_bridges::discovery::grouping::{
    canonical_point_set, find_related_groups, kind_signature, point_kind_fingerprint,
    point_set_to_json, suggest_group_name,
};
use bms_store_storage::discovery::model::{
    DeviceState, DiscoveredDevice, DiscoveredPoint, PointKindHint, PROTOCOL_BACNET, PROTOCOL_MODBUS,
};
use crate::gui::state::AppState;

use super::bacnet_network_tools::BacnetNetworkTools;
use super::discovery_detail::render_device_detail;
use super::discovery_group_editor::render_group_editor;
use super::discovery_list::{
    render_accepted_device, render_device_row, render_ignored_device, render_pending_device,
};
use super::discovery_utils::{bump, DeviceDetailTab, DeviceGroup, DiscoveryTab};

#[component]
pub fn DiscoveryView() -> Element {
    let state = use_context::<AppState>();
    let user_is_admin = state.has_permission(Permission::ManageDiscovery);
    let mut devices = use_signal(Vec::<DiscoveredDevice>::new);
    let mut selected_device_id = use_signal(|| Option::<String>::None);
    let mut selected_points = use_signal(Vec::<DiscoveredPoint>::new);
    let mut scanning_bacnet = use_signal(|| false);
    let mut scanning_modbus = use_signal(|| false);
    let mut refresh_counter = use_signal(|| 0u64);
    let event_infos: Signal<Vec<bms_store_bridges::bridge::bacnet::BacnetEventInfo>> = use_signal(Vec::new);
    let trend_logs: Signal<Vec<(u32, String)>> = use_signal(Vec::new);
    let create_object_type: Signal<String> = use_signal(|| "AnalogValue".to_string());
    let delete_object_input: Signal<String> = use_signal(String::new);
    let commission_status: Signal<Option<String>> = use_signal(|| None);

    let mut detail_tab = use_signal(|| DeviceDetailTab::Overview);
    let mut discovery_tab = use_signal(|| DiscoveryTab::AllDevices);

    // Editing state
    let editing_device_name = use_signal(|| false);
    let device_name_draft = use_signal(String::new);
    let selected_point_ids = use_signal(std::collections::HashSet::<String>::new);
    let editing_point_id = use_signal(|| Option::<String>::None);
    let point_name_draft = use_signal(String::new);
    let point_units_draft = use_signal(String::new);
    let point_desc_draft = use_signal(String::new);
    let point_state_labels_draft = use_signal(HashMap::<String, String>::new);
    let point_kind_editing = use_signal(|| PointKindHint::Analog);
    let bulk_units_draft = use_signal(String::new);
    let bulk_status = use_signal(|| Option::<String>::None);

    // Group editing state
    let mut device_groups = use_signal(Vec::<DeviceGroup>::new);
    let mut selected_group = use_signal(|| Option::<u64>::None);
    let mut expanded_groups = use_signal(std::collections::HashSet::<u64>::new);
    let group_name_drafts = use_signal(HashMap::<String, String>::new);
    let group_find_text = use_signal(String::new);
    let group_replace_text = use_signal(String::new);
    let group_template_text = use_signal(|| "{device} {point}".to_string());
    let group_status = use_signal(|| Option::<String>::None);
    let mut group_point_name_drafts = use_signal(HashMap::<String, String>::new);
    let mut group_point_units_drafts = use_signal(HashMap::<String, String>::new);
    let mut group_point_state_labels_drafts =
        use_signal(HashMap::<String, Option<HashMap<String, String>>>::new);
    let mut group_shared_points = use_signal(Vec::<DiscoveredPoint>::new);

    // B4: Network selector for BACnet tab (None = All Networks)
    let mut bacnet_network_filter: Signal<Option<String>> = use_signal(|| None);

    // D2: Modbus profile library
    let modbus_profiles: Signal<Vec<DeviceProfile>> = use_signal(|| {
        let lib_dir = state.project_paths.profiles_dir.join("modbus-library");
        load_profile_library(&lib_dir)
    });
    let selected_profile_idx: Signal<Option<usize>> = use_signal(|| None);
    let profile_apply_status: Signal<Option<String>> = use_signal(|| None);

    // Filter state
    let mut filter_text = use_signal(String::new);

    // Modbus network scan state
    let mut modbus_scan_host = use_signal(|| "192.168.1.1".to_string());
    let mut modbus_scan_port = use_signal(|| "502".to_string());
    let mut modbus_scan_start = use_signal(|| "1".to_string());
    let mut modbus_scan_end = use_signal(|| "10".to_string());
    let mut scanning_modbus_network = use_signal(|| false);
    let mut modbus_scan_result: Signal<Option<String>> = use_signal(|| None);
    let mut modbus_is_rtu = use_signal(|| false);

    // Detect RTU mode on mount
    {
        let bridge_handle = state.modbus_handle();
        use_effect(move || {
            let bridge_handle = bridge_handle.clone();
            if let Some(bridge_handle) = bridge_handle {
                spawn(async move {
                    let guard = bridge_handle.lock().await;
                    let bridge = guard.as_any().downcast_ref::<ModbusBridge>().unwrap();
                    modbus_is_rtu.set(bridge.is_rtu());
                });
            }
        });
    }

    // Auto-refresh when discovery store changes (e.g. conn_status updates from EventBus)
    {
        let ds_watch = state.discovery_store.clone();
        use_hook(move || {
            spawn(async move {
                let mut rx = ds_watch.subscribe();
                loop {
                    if rx.changed().await.is_err() {
                        break;
                    }
                    refresh_counter += 1;
                }
            });
        });
    }

    // Load devices + compute groups when refresh_counter changes
    let ds = state.discovery_store.clone();
    let _ = use_resource(move || {
        let ds = ds.clone();
        let _counter = *refresh_counter.read();
        async move {
            let all = ds.list_devices(None).await;

            // Compute kind-based groups from all device points
            let all_points = ds.get_all_device_points().await;
            let mut kind_groups: HashMap<u64, Vec<String>> = HashMap::new();
            let mut group_points: HashMap<u64, Vec<bms_store_storage::discovery::model::DiscoveredPoint>> =
                HashMap::new();
            let mut device_names: HashMap<String, String> = HashMap::new();
            for dev in &all {
                device_names.insert(dev.id.clone(), dev.display_name.clone());
            }
            for (device_id, pts) in &all_points {
                if pts.is_empty() {
                    continue;
                }
                let fp = point_kind_fingerprint(pts);
                kind_groups.entry(fp).or_default().push(device_id.clone());
                group_points.entry(fp).or_insert_with(|| pts.clone());
            }

            // Build group data with point sets for similarity comparison
            let mut groups: Vec<DeviceGroup> = kind_groups
                .into_iter()
                .filter(|(_, ids)| ids.len() >= 2) // Only group 2+ devices
                .map(|(fp, ids)| {
                    let first_name = ids
                        .first()
                        .and_then(|id| device_names.get(id))
                        .map(|n| n.as_str())
                        .unwrap_or("Equipment");
                    let kind_sig = group_points
                        .get(&fp)
                        .map(|pts| kind_signature(pts))
                        .unwrap_or_default();
                    DeviceGroup {
                        fingerprint: fp,
                        name: suggest_group_name(first_name),
                        kind_sig,
                        device_ids: ids,
                        related: Vec::new(),
                    }
                })
                .collect();
            groups.sort_by(|a, b| b.device_ids.len().cmp(&a.device_ids.len()));

            // Compute related groups (similarity between groups)
            let all_group_data: Vec<(String, String, String)> = groups
                .iter()
                .filter_map(|g| {
                    group_points.get(&g.fingerprint).map(|pts| {
                        let ps = canonical_point_set(pts);
                        (
                            format!("{}", g.fingerprint),
                            g.name.clone(),
                            point_set_to_json(&ps),
                        )
                    })
                })
                .collect();

            for group in &mut groups {
                if let Some(pts) = group_points.get(&group.fingerprint) {
                    let target = canonical_point_set(pts);
                    group.related = find_related_groups(
                        &format!("{}", group.fingerprint),
                        &target,
                        &all_group_data,
                        0.5,
                    );
                }
            }
            device_groups.set(groups);

            devices.set(all);
        }
    });

    // Load points when selected device changes
    let ds2 = state.discovery_store.clone();
    let _ = use_resource(move || {
        let ds2 = ds2.clone();
        let sel_id = selected_device_id.read().clone();
        let _counter = *refresh_counter.read();
        async move {
            if let Some(ref id) = sel_id {
                let pts = ds2.get_points(id).await;
                selected_points.set(pts);
            } else {
                selected_points.set(vec![]);
            }
        }
    });

    // Load shared points when a group is selected (uses first device in group)
    let ds3 = state.discovery_store.clone();
    let _ = use_resource(move || {
        let ds3 = ds3.clone();
        let sel_fp = *selected_group.read();
        let groups = device_groups.read().clone();
        let _counter = *refresh_counter.read();
        async move {
            if let Some(fp) = sel_fp {
                if let Some(group) = groups.iter().find(|g| g.fingerprint == fp) {
                    if let Some(first_id) = group.device_ids.first() {
                        let pts = ds3.get_points(first_id).await;
                        group_shared_points.set(pts);
                        group_point_name_drafts.write().clear();
                        group_point_units_drafts.write().clear();
                        group_point_state_labels_drafts.write().clear();
                        return;
                    }
                }
            }
            group_shared_points.set(vec![]);
        }
    });

    let all_devices = devices.read();

    // Apply filters: text + protocol (protocol filtered by active tab)
    let active_tab = *discovery_tab.read();
    let filter_text_val = filter_text.read().clone();
    let protocol_filter: Option<&str> = match active_tab {
        DiscoveryTab::AllDevices => None,
        DiscoveryTab::Bacnet => Some(PROTOCOL_BACNET),
        DiscoveryTab::Modbus => Some(PROTOCOL_MODBUS),
    };
    // B4: Network filter for BACnet tab
    let bacnet_net_filter_val = bacnet_network_filter.read().clone();
    let filtered_devices: Vec<&DiscoveredDevice> = all_devices
        .iter()
        .filter(|d| {
            if let Some(proto) = protocol_filter {
                if d.protocol != proto {
                    return false;
                }
            }
            // B4: Filter by selected BACnet network
            if let Some(ref net_id) = bacnet_net_filter_val {
                if d.protocol == PROTOCOL_BACNET && d.network_id != *net_id {
                    return false;
                }
            }
            if !filter_text_val.is_empty() {
                let needle = filter_text_val.to_lowercase();
                let name_match = d.display_name.to_lowercase().contains(&needle);
                let addr_match = d.address.to_lowercase().contains(&needle);
                if !name_match && !addr_match {
                    return false;
                }
            }
            true
        })
        .collect();

    let pending: Vec<&&DiscoveredDevice> = filtered_devices
        .iter()
        .filter(|d| d.state == DeviceState::Discovered)
        .collect();
    let accepted: Vec<&&DiscoveredDevice> = filtered_devices
        .iter()
        .filter(|d| d.state == DeviceState::Accepted)
        .collect();
    let ignored: Vec<&&DiscoveredDevice> = filtered_devices
        .iter()
        .filter(|d| d.state == DeviceState::Ignored)
        .collect();

    let sel = selected_device_id.read().clone();
    let selected_dev = sel
        .as_ref()
        .and_then(|id| all_devices.iter().find(|d| d.id == *id));
    let points = selected_points.read();

    // Pre-clone handles used across multiple RSX match arms
    let scan_svc = state.discovery_service.clone();
    let scan_svc_modbus = state.discovery_service.clone();
    let scan_svc_network = state.discovery_service.clone();
    let scan_svc_rtu = state.discovery_service.clone();
    let scan_bridge = state.bacnet_handle();
    let scan_modbus_bridge = state.modbus_handle();
    let scan_modbus_network_bridge = state.modbus_handle();
    let scan_modbus_rtu_bridge = state.modbus_handle();

    // Count devices per protocol for tab badges
    let bacnet_count = all_devices
        .iter()
        .filter(|d| d.protocol == PROTOCOL_BACNET)
        .count();
    let modbus_count = all_devices
        .iter()
        .filter(|d| d.protocol == PROTOCOL_MODBUS)
        .count();

    rsx! {
        div { class: "discovery-view",
            // ── Left sidebar ──
            div { class: "discovery-device-list",
                // ── Discovery sub-tab bar ──
                div { class: "discovery-tab-bar",
                    button {
                        class: if active_tab == DiscoveryTab::AllDevices { "discovery-tab active" } else { "discovery-tab" },
                        onclick: move |_| discovery_tab.set(DiscoveryTab::AllDevices),
                        "All"
                        if !all_devices.is_empty() {
                            span { class: "discovery-tab-count", "{all_devices.len()}" }
                        }
                    }
                    button {
                        class: if active_tab == DiscoveryTab::Bacnet { "discovery-tab active bacnet" } else { "discovery-tab" },
                        onclick: move |_| discovery_tab.set(DiscoveryTab::Bacnet),
                        "BACnet"
                        if bacnet_count > 0 {
                            span { class: "discovery-tab-count", "{bacnet_count}" }
                        }
                    }
                    button {
                        class: if active_tab == DiscoveryTab::Modbus { "discovery-tab active modbus" } else { "discovery-tab" },
                        onclick: move |_| discovery_tab.set(DiscoveryTab::Modbus),
                        "Modbus"
                        if modbus_count > 0 {
                            span { class: "discovery-tab-count", "{modbus_count}" }
                        }
                    }
                }

                // ── Protocol-specific toolbar (only on protocol tabs) ──
                match active_tab {
                    DiscoveryTab::Bacnet => {
                        // B4: Get available network IDs for selector
                        let scan_bridge_ids = state.bacnet_handle();
                        let scan_bridge_per = state.bacnet_handle();
                        let scan_svc_per = state.discovery_service.clone();
                        rsx! {
                        div { class: "discovery-scan-toolbar",
                            // B4: Network selector (hidden when only 1 network)
                            {
                                let ids_handle = scan_bridge_ids.clone();
                                let mut network_ids = use_signal(Vec::<String>::new);
                                use_effect(move || {
                                    let ids_handle = ids_handle.clone();
                                    spawn(async move {
                                        let guard = ids_handle.lock().await;
                                        let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                                        network_ids.set(nets.network_ids());
                                    });
                                });
                                let ids = network_ids.read();
                                if ids.len() > 1 {
                                    rsx! {
                                        select {
                                            class: "discovery-input discovery-network-select",
                                            value: bacnet_network_filter.read().as_deref().unwrap_or("__all__"),
                                            onchange: move |e| {
                                                let val = e.value();
                                                if val == "__all__" {
                                                    bacnet_network_filter.set(None);
                                                } else {
                                                    bacnet_network_filter.set(Some(val));
                                                }
                                            },
                                            option { value: "__all__", "All Networks" }
                                            for nid in ids.iter() {
                                                option { value: "{nid}", "{nid}" }
                                            }
                                        }
                                    }
                                } else {
                                    rsx! {}
                                }
                            }
                            // B4: Scan button — scans selected network or all
                            button {
                                class: "discovery-scan-btn bacnet",
                                disabled: *scanning_bacnet.read(),
                                onclick: {
                                    let net_filter = bacnet_network_filter.clone();
                                    move |_| {
                                        scanning_bacnet.set(true);
                                        let svc = scan_svc.clone();
                                        let svc_per = scan_svc_per.clone();
                                        let bridge_handle = scan_bridge.clone();
                                        let bridge_per = scan_bridge_per.clone();
                                        let selected_net = net_filter.read().clone();
                                        spawn(async move {
                                            if let Some(ref nid) = selected_net {
                                                // Per-network scan
                                                let mut guard = bridge_per.lock().await;
                                                let nets = guard.as_any_mut().downcast_mut::<BacnetNetworks>().unwrap();
                                                if let Some(bridge) = nets.get_mut(nid) {
                                                    svc_per.scan_bacnet(bridge).await;
                                                }
                                                drop(guard);
                                            } else {
                                                // Scan all networks
                                                let mut guard = bridge_handle.lock().await;
                                                let nets = guard.as_any_mut().downcast_mut::<BacnetNetworks>().unwrap();
                                                svc.scan_bacnet_all(nets).await;
                                                drop(guard);
                                            }
                                            scanning_bacnet.set(false);
                                            bump(&mut refresh_counter);
                                        });
                                    }
                                },
                                if *scanning_bacnet.read() { "Scanning..." } else {
                                    if bacnet_network_filter.read().is_some() { "Scan Network" } else { "Scan All" }
                                }
                            }
                        }
                        if *scanning_bacnet.read() {
                            div { class: "discovery-scan-progress", "Scanning BACnet network..." }
                        }
                    }},
                    DiscoveryTab::Modbus => rsx! {
                        div { class: "discovery-scan-toolbar",
                            button {
                                class: "discovery-scan-btn modbus",
                                disabled: *scanning_modbus.read(),
                                onclick: move |_| {
                                    scanning_modbus.set(true);
                                    let svc = scan_svc_modbus.clone();
                                    let bridge_handle = scan_modbus_bridge.clone();
                                    if let Some(bridge_handle) = bridge_handle {
                                        spawn(async move {
                                            let guard = bridge_handle.lock().await;
                                            let bridge = guard.as_any().downcast_ref::<ModbusBridge>().unwrap();
                                            svc.scan_modbus(bridge).await;
                                            drop(guard);
                                            scanning_modbus.set(false);
                                            bump(&mut refresh_counter);
                                        });
                                    } else {
                                        scanning_modbus.set(false);
                                    }
                                },
                                if *scanning_modbus.read() { "Refreshing..." } else { "Refresh Devices" }
                            }
                        }
                        if *scanning_modbus.read() {
                            div { class: "discovery-scan-progress", "Checking configured Modbus devices..." }
                        }
                        // ── Network/RTU scan form ──
                        div { class: "discovery-scan-section",
                            h4 { class: "discovery-scan-section-title",
                                if *modbus_is_rtu.read() { "Scan RTU Bus" } else { "Scan Network" }
                            }
                            div { class: "discovery-scan-form",
                                // TCP mode: host + port row
                                if !*modbus_is_rtu.read() {
                                    div { class: "discovery-scan-row",
                                        label { class: "discovery-scan-label", "Host" }
                                        input {
                                            class: "discovery-input",
                                            r#type: "text",
                                            placeholder: "IP address",
                                            value: "{modbus_scan_host}",
                                            oninput: move |e| modbus_scan_host.set(e.value()),
                                        }
                                        label { class: "discovery-scan-label", "Port" }
                                        input {
                                            class: "discovery-input short",
                                            r#type: "number",
                                            placeholder: "502",
                                            value: "{modbus_scan_port}",
                                            oninput: move |e| modbus_scan_port.set(e.value()),
                                        }
                                    }
                                }
                                // Unit ID range row (both modes)
                                div { class: "discovery-scan-row",
                                    label { class: "discovery-scan-label", "Unit IDs" }
                                    input {
                                        class: "discovery-input short",
                                        r#type: "number",
                                        placeholder: "1",
                                        value: "{modbus_scan_start}",
                                        oninput: move |e| modbus_scan_start.set(e.value()),
                                    }
                                    span { class: "discovery-scan-separator", "to" }
                                    input {
                                        class: "discovery-input short",
                                        r#type: "number",
                                        placeholder: "10",
                                        value: "{modbus_scan_end}",
                                        oninput: move |e| modbus_scan_end.set(e.value()),
                                    }
                                    if *modbus_is_rtu.read() {
                                        button {
                                            class: "discovery-scan-btn modbus",
                                            disabled: *scanning_modbus_network.read(),
                                            onclick: {
                                                let svc = scan_svc_rtu.clone();
                                                let bridge_handle = scan_modbus_rtu_bridge.clone();
                                                move |_| {
                                                    let start: u8 = modbus_scan_start.read().parse().unwrap_or(1);
                                                    let end: u8 = modbus_scan_end.read().parse().unwrap_or(10);
                                                    let svc = svc.clone();
                                                    let bridge_handle = bridge_handle.clone();
                                                    scanning_modbus_network.set(true);
                                                    modbus_scan_result.set(None);
                                                    if let Some(bridge_handle) = bridge_handle {
                                                        spawn(async move {
                                                            let guard = bridge_handle.lock().await;
                                                            let bridge = guard.as_any().downcast_ref::<ModbusBridge>().unwrap();
                                                            let found = svc.scan_modbus_rtu(bridge, start, end).await;
                                                            drop(guard);
                                                            modbus_scan_result.set(Some(
                                                                if found == 0 {
                                                                    "No responding devices found on bus.".to_string()
                                                                } else {
                                                                    format!("Found {found} responding device(s) on bus.")
                                                                }
                                                            ));
                                                            scanning_modbus_network.set(false);
                                                            bump(&mut refresh_counter);
                                                        });
                                                    } else {
                                                        scanning_modbus_network.set(false);
                                                    }
                                                }
                                            },
                                            if *scanning_modbus_network.read() { "Scanning..." } else { "Scan Bus" }
                                        }
                                    } else {
                                        button {
                                            class: "discovery-scan-btn modbus",
                                            disabled: *scanning_modbus_network.read(),
                                            onclick: {
                                                let svc = scan_svc_network.clone();
                                                let bridge_handle = scan_modbus_network_bridge.clone();
                                                move |_| {
                                                    let host = modbus_scan_host.read().clone();
                                                    let port: u16 = modbus_scan_port.read().parse().unwrap_or(502);
                                                    let start: u8 = modbus_scan_start.read().parse().unwrap_or(1);
                                                    let end: u8 = modbus_scan_end.read().parse().unwrap_or(10);
                                                    let svc = svc.clone();
                                                    let bridge_handle = bridge_handle.clone();
                                                    scanning_modbus_network.set(true);
                                                    modbus_scan_result.set(None);
                                                    if let Some(bridge_handle) = bridge_handle {
                                                        spawn(async move {
                                                            let guard = bridge_handle.lock().await;
                                                            let bridge = guard.as_any().downcast_ref::<ModbusBridge>().unwrap();
                                                            let found = svc.scan_modbus_network(bridge, &host, port, start, end).await;
                                                            drop(guard);
                                                            modbus_scan_result.set(Some(
                                                                if found == 0 {
                                                                    "No responding devices found.".to_string()
                                                                } else {
                                                                    format!("Found {found} responding device(s).")
                                                                }
                                                            ));
                                                            scanning_modbus_network.set(false);
                                                            bump(&mut refresh_counter);
                                                        });
                                                    } else {
                                                        scanning_modbus_network.set(false);
                                                    }
                                                }
                                            },
                                            if *scanning_modbus_network.read() { "Scanning..." } else { "Scan" }
                                        }
                                    }
                                }
                                if *scanning_modbus_network.read() {
                                    div { class: "discovery-scan-progress", "Probing unit IDs..." }
                                }
                                if let Some(ref msg) = *modbus_scan_result.read() {
                                    div { class: "discovery-scan-result", "{msg}" }
                                }
                            }
                        }
                    },
                    DiscoveryTab::AllDevices => rsx! {},
                }

                // ── Filter bar ──
                div { class: "discovery-filter-bar",
                    input {
                        class: "discovery-filter-input",
                        r#type: "text",
                        placeholder: "Filter devices...",
                        value: "{filter_text.read()}",
                        oninput: move |evt: Event<FormData>| filter_text.set(evt.value()),
                    }
                }

                // ── Device list ──
                div { class: "discovery-device-list-body",
                    // On All tab with groups: show equipment groups first
                    if active_tab == DiscoveryTab::AllDevices && !device_groups.read().is_empty() {
                        {
                            let groups = device_groups.read();
                            // Collect IDs of devices that belong to a group
                            let grouped_ids: std::collections::HashSet<String> = groups
                                .iter()
                                .flat_map(|g| g.device_ids.iter().cloned())
                                .collect();
                            let ungrouped: Vec<&DiscoveredDevice> = filtered_devices
                                .iter()
                                .filter(|d| !grouped_ids.contains(&d.id))
                                .copied()
                                .collect();
                            rsx! {
                                // Equipment type groups
                                for group in groups.iter() {
                                    {
                                        let fp = group.fingerprint;
                                        let fp_toggle = fp;
                                        let fp_select = fp;
                                        let is_expanded = expanded_groups.read().contains(&fp);
                                        let group_devs: Vec<&DiscoveredDevice> = filtered_devices
                                            .iter()
                                            .filter(|d| group.device_ids.contains(&d.id))
                                            .copied()
                                            .collect();
                                        let dev_count = group_devs.len();
                                        if dev_count == 0 {
                                            // All devices filtered out
                                            rsx! {}
                                        } else {
                                            let group_name = group.name.clone();
                                            let kind_sig = group.kind_sig.clone();
                                            let related = group.related.clone();
                                            let has_related = !related.is_empty();
                                            rsx! {
                                                div { class: "discovery-group",
                                                    div {
                                                        class: "discovery-group-header clickable",
                                                        onclick: move |_| {
                                                            let mut set = expanded_groups.write();
                                                            if set.contains(&fp_toggle) {
                                                                set.remove(&fp_toggle);
                                                            } else {
                                                                set.insert(fp_toggle);
                                                            }
                                                        },
                                                        span { class: "discovery-group-chevron",
                                                            if is_expanded { "▾" } else { "▸" }
                                                        }
                                                        span { class: "discovery-group-title", "{group_name}" }
                                                        span { class: "discovery-group-badge", "{dev_count}" }
                                                        span { class: "discovery-kind-sig", "{kind_sig}" }
                                                        if user_is_admin {
                                                            button {
                                                                class: "discovery-group-edit-btn",
                                                                onclick: move |evt: Event<MouseData>| {
                                                                    evt.stop_propagation();
                                                                    selected_group.set(Some(fp_select));
                                                                    selected_device_id.set(None);
                                                                },
                                                                "Edit Group"
                                                            }
                                                        }
                                                    }
                                                    if is_expanded {
                                                        if has_related {
                                                            div { class: "discovery-related-groups",
                                                                span { class: "discovery-related-label", "Similar equipment:" }
                                                                for rg in related.iter() {
                                                                    {
                                                                        let pct = (rg.diff.similarity * 100.0) as u32;
                                                                        let added = rg.diff.only_b.len();
                                                                        let removed = rg.diff.only_a.len();
                                                                        let rg_name = rg.group_name.clone();
                                                                        let detail = if added > 0 && removed > 0 {
                                                                            format!("+{added} / -{removed} points")
                                                                        } else if added > 0 {
                                                                            format!("+{added} points")
                                                                        } else if removed > 0 {
                                                                            format!("-{removed} points")
                                                                        } else {
                                                                            String::new()
                                                                        };
                                                                        rsx! {
                                                                            span {
                                                                                class: "discovery-related-chip",
                                                                                title: "{detail}",
                                                                                "{rg_name} {pct}%"
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                        for dev in group_devs.iter() {
                                                            { render_device_row(dev, &sel, &state, selected_device_id, selected_group, detail_tab, refresh_counter, user_is_admin) }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // Ungrouped devices (unique point sets or no points)
                                if !ungrouped.is_empty() {
                                    div { class: "discovery-group",
                                        div { class: "discovery-group-header", "Other ({ungrouped.len()})" }
                                        for dev in ungrouped.iter() {
                                            { render_device_row(dev, &sel, &state, selected_device_id, selected_group, detail_tab, refresh_counter, user_is_admin) }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Non-All tabs or no groups: show flat list by state
                    if active_tab != DiscoveryTab::AllDevices || device_groups.read().is_empty() {
                        // Pending devices
                        if !pending.is_empty() {
                            div { class: "discovery-group",
                                div { class: "discovery-group-header", "Pending ({pending.len()})" }
                                for dev in pending.iter() {
                                    { render_pending_device(dev, &sel, &state, selected_device_id, detail_tab, refresh_counter) }
                                }
                            }
                        }

                        // Accepted devices
                        if !accepted.is_empty() {
                            div { class: "discovery-group",
                                div { class: "discovery-group-header", "Accepted ({accepted.len()})" }
                                for dev in accepted.iter() {
                                    { render_accepted_device(dev, &sel, selected_device_id, detail_tab, refresh_counter) }
                                }
                            }
                        }

                        // Ignored devices
                        if !ignored.is_empty() {
                            div { class: "discovery-group",
                                div { class: "discovery-group-header", "Ignored ({ignored.len()})" }
                                for dev in ignored.iter() {
                                    { render_ignored_device(dev, &sel, &state, selected_device_id, detail_tab, refresh_counter) }
                                }
                            }
                        }
                    }

                    if filtered_devices.is_empty() {
                        div { class: "discovery-empty",
                            match active_tab {
                                DiscoveryTab::AllDevices => if all_devices.is_empty() {
                                    rsx! {
                                        p { "No devices discovered yet." }
                                        p { class: "discovery-hint", "Switch to the BACnet or Modbus tab to scan for devices." }
                                    }
                                } else {
                                    rsx! { p { "No devices match the current filter." } }
                                },
                                DiscoveryTab::Bacnet => rsx! {
                                    p { "No BACnet devices discovered." }
                                    p { class: "discovery-hint", "Click \"Scan Network\" to send a Who-Is broadcast." }
                                },
                                DiscoveryTab::Modbus => rsx! {
                                    p { "No Modbus devices found." }
                                    p { class: "discovery-hint", "Click \"Refresh Devices\" to check configured devices, or use Scan Network to probe unit IDs." }
                                },
                            }
                        }
                    }
                }

                // ── BACnet Network Tools (only on BACnet tab, below device list) ──
                if active_tab == DiscoveryTab::Bacnet {
                    div { class: "discovery-tools-section",
                        div { class: "discovery-group-header", "Network Tools" }
                        div { class: "discovery-tools-body",
                            BacnetNetworkTools {}
                        }
                    }
                }
            }

            // ── Right pane — device detail or group editor ──
            div { class: "discovery-detail",
                // Group editor (shown when a group is selected)
                if let Some(sel_fp) = *selected_group.read() {
                    {
                        let groups = device_groups.read();
                        if let Some(group) = groups.iter().find(|g| g.fingerprint == sel_fp) {
                            rsx! {
                                { render_group_editor(
                                    &state,
                                    group,
                                    &all_devices,
                                    selected_group,
                                    group_name_drafts,
                                    group_find_text,
                                    group_replace_text,
                                    group_template_text,
                                    group_status,
                                    group_point_name_drafts,
                                    group_point_units_drafts,
                                    group_point_state_labels_drafts,
                                    group_shared_points,
                                    refresh_counter,
                                ) }
                            }
                        } else {
                            rsx! {
                                div { class: "discovery-detail-empty",
                                    p { "Group not found." }
                                }
                            }
                        }
                    }
                }

                if selected_group.read().is_none() && selected_dev.is_some() {
                    { render_device_detail(
                        &state,
                        selected_dev,
                        &points,
                        user_is_admin,
                        detail_tab,
                        refresh_counter,
                        editing_device_name,
                        device_name_draft,
                        selected_point_ids,
                        editing_point_id,
                        point_name_draft,
                        point_units_draft,
                        point_desc_draft,
                        point_state_labels_draft,
                        point_kind_editing,
                        bulk_units_draft,
                        bulk_status,
                        event_infos,
                        trend_logs,
                        create_object_type,
                        delete_object_input,
                        commission_status,
                        modbus_profiles,
                        selected_profile_idx,
                        profile_apply_status,
                        selected_points,
                    ) }
                } else {
                    // No device selected in device detail mode — but only if no group is selected either
                    if selected_group.read().is_none() {
                        div { class: "discovery-detail-empty",
                            p { "Select a device or group to view details." }
                        }
                    }
                }
            }
        }
    }
}
