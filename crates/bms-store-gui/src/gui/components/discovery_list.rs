use dioxus::prelude::*;

use bms_store_storage::auth::Permission;
use bms_store_storage::discovery::model::{ConnStatus, DeviceState, DiscoveredDevice};
use crate::gui::state::AppState;

use super::discovery_utils::{
    bump, network_badge_class, protocol_badge, protocol_badge_class, ConnBadge, DeviceDetailTab,
};

// ── Helper render functions to reduce nesting ──

pub(crate) fn render_pending_device(
    dev: &DiscoveredDevice,
    sel: &Option<String>,
    state: &AppState,
    mut selected_device_id: Signal<Option<String>>,
    mut detail_tab: Signal<DeviceDetailTab>,
    mut refresh_counter: Signal<u64>,
    mut preview_device_id: Signal<Option<String>>,
) -> Element {
    let dev_id = dev.id.clone();
    let dev_id2 = dev.id.clone();
    let dev_id3 = dev.id.clone();
    let dev_id_preview = dev.id.clone();
    let is_selected = sel.as_deref() == Some(&dev.id);
    let svc = state.discovery_service.clone();
    let svc2 = state.discovery_service.clone();
    let badge_class = protocol_badge_class(&dev.protocol);
    rsx! {
        div {
            class: if is_selected { "discovery-device-row selected" } else { "discovery-device-row" },
            onclick: move |_| {
                selected_device_id.set(Some(dev_id.clone()));
                detail_tab.set(DeviceDetailTab::Overview);
                bump(&mut refresh_counter);
            },
            span { class: "discovery-protocol-badge {badge_class}", "{protocol_badge(&dev.protocol)}" }
            span { class: "discovery-device-name", "{dev.display_name}" }
            span { class: "discovery-device-addr", "{dev.address}" }
            if dev.point_count > 0 {
                span { class: "discovery-point-count", "{dev.point_count} pts" }
            }
            div { class: "discovery-actions",
                if state.has_permission(Permission::ManageDiscovery) {
                    button {
                        class: "discovery-action-btn preview",
                        title: "Preview the tags accept_device would apply",
                        onclick: {
                            let did = dev_id_preview.clone();
                            move |evt: Event<MouseData>| {
                                evt.stop_propagation();
                                preview_device_id.set(Some(did.clone()));
                            }
                        },
                        "Preview"
                    }
                    button {
                        class: "discovery-action-btn accept",
                        onclick: {
                            let list_audit = state.clone();
                            move |evt: Event<MouseData>| {
                                evt.stop_propagation();
                                let svc = svc.clone();
                                let id = dev_id2.clone();
                                let audit_state = list_audit.clone();
                                spawn(async move {
                                    if let Err(e) = svc.accept_device(&id).await {
                                        eprintln!("Accept failed: {e}");
                                        audit_state.audit(
                                            bms_store_storage::store::audit_store::AuditEntryBuilder::new(
                                                bms_store_storage::store::audit_store::AuditAction::AcceptDevice, "device",
                                            ).resource_id(&id).failure(&format!("{e}")),
                                        );
                                    } else {
                                        audit_state.audit(
                                            bms_store_storage::store::audit_store::AuditEntryBuilder::new(
                                                bms_store_storage::store::audit_store::AuditAction::AcceptDevice, "device",
                                            ).resource_id(&id),
                                        );
                                    }
                                    bump(&mut refresh_counter);
                                });
                            }
                        },
                        "Accept"
                    }
                }
                button {
                    class: "discovery-action-btn ignore",
                    onclick: move |evt| {
                        evt.stop_propagation();
                        let svc2 = svc2.clone();
                        let id = dev_id3.clone();
                        spawn(async move {
                            let _ = svc2.ignore_device(&id).await;
                            bump(&mut refresh_counter);
                        });
                    },
                    "Ignore"
                }
            }
        }
    }
}

pub(crate) fn render_accepted_device(
    dev: &DiscoveredDevice,
    sel: &Option<String>,
    mut selected_device_id: Signal<Option<String>>,
    mut detail_tab: Signal<DeviceDetailTab>,
    mut refresh_counter: Signal<u64>,
) -> Element {
    let dev_id = dev.id.clone();
    let is_selected = sel.as_deref() == Some(&dev.id);
    let badge_class = protocol_badge_class(&dev.protocol);
    rsx! {
        div {
            class: if is_selected { "discovery-device-row selected" } else { "discovery-device-row" },
            onclick: move |_| {
                selected_device_id.set(Some(dev_id.clone()));
                detail_tab.set(DeviceDetailTab::Overview);
                bump(&mut refresh_counter);
            },
            span { class: "discovery-protocol-badge {badge_class}", "{protocol_badge(&dev.protocol)}" }
            if !dev.network_id.is_empty() {
                span { class: "discovery-network-badge {network_badge_class(&dev.network_id)}", "{dev.network_id}" }
            }
            span { class: "discovery-device-name", "{dev.display_name}" }
            span { class: "discovery-device-addr", "{dev.address}" }
            ConnBadge { status: dev.conn_status }
            if dev.point_count > 0 {
                span { class: "discovery-point-count", "{dev.point_count} pts" }
            }
        }
    }
}

pub(crate) fn render_ignored_device(
    dev: &DiscoveredDevice,
    sel: &Option<String>,
    state: &AppState,
    mut selected_device_id: Signal<Option<String>>,
    mut detail_tab: Signal<DeviceDetailTab>,
    mut refresh_counter: Signal<u64>,
) -> Element {
    let dev_id = dev.id.clone();
    let dev_id2 = dev.id.clone();
    let is_selected = sel.as_deref() == Some(&dev.id);
    let svc = state.discovery_service.clone();
    let badge_class = protocol_badge_class(&dev.protocol);
    rsx! {
        div {
            class: if is_selected { "discovery-device-row selected" } else { "discovery-device-row" },
            onclick: move |_| {
                selected_device_id.set(Some(dev_id.clone()));
                detail_tab.set(DeviceDetailTab::Overview);
                bump(&mut refresh_counter);
            },
            span { class: "discovery-protocol-badge {badge_class}", "{protocol_badge(&dev.protocol)}" }
            span { class: "discovery-device-name dimmed", "{dev.display_name}" }
            button {
                class: "discovery-action-btn",
                onclick: move |evt| {
                    evt.stop_propagation();
                    let svc = svc.clone();
                    let id = dev_id2.clone();
                    spawn(async move {
                        let _ = svc.unignore_device(&id).await;
                        bump(&mut refresh_counter);
                    });
                },
                "Un-ignore"
            }
        }
    }
}

/// Universal device row for grouped display — shows state badge + accept/ignore buttons.
pub(crate) fn render_device_row(
    dev: &DiscoveredDevice,
    sel: &Option<String>,
    state: &AppState,
    mut selected_device_id: Signal<Option<String>>,
    mut selected_group: Signal<Option<u64>>,
    mut detail_tab: Signal<DeviceDetailTab>,
    mut refresh_counter: Signal<u64>,
    user_is_admin: bool,
) -> Element {
    let dev_id = dev.id.clone();
    let dev_id2 = dev.id.clone();
    let is_selected = sel.as_deref() == Some(&dev.id);
    let badge_class = protocol_badge_class(&dev.protocol);
    let state_class = match dev.state {
        DeviceState::Accepted => "discovery-state-badge accepted",
        DeviceState::Ignored => "discovery-state-badge ignored",
        _ => "discovery-state-badge pending",
    };
    let svc = state.discovery_service.clone();
    let audit_state = state.clone();
    rsx! {
        div {
            class: if is_selected { "discovery-device-row selected" } else { "discovery-device-row" },
            onclick: move |_| {
                selected_device_id.set(Some(dev_id.clone()));
                selected_group.set(None);
                detail_tab.set(DeviceDetailTab::Overview);
                bump(&mut refresh_counter);
            },
            span { class: "discovery-protocol-badge {badge_class}", "{protocol_badge(&dev.protocol)}" }
            if !dev.network_id.is_empty() {
                span { class: "discovery-network-badge {network_badge_class(&dev.network_id)}", "{dev.network_id}" }
            }
            span { class: "discovery-device-name", "{dev.display_name}" }
            span { class: "discovery-device-addr", "{dev.address}" }
            span { class: state_class, "{dev.state.as_str()}" }
            if dev.conn_status == ConnStatus::Online {
                ConnBadge { status: dev.conn_status }
            }
            if dev.state == DeviceState::Discovered && user_is_admin {
                button {
                    class: "discovery-action-btn accept",
                    onclick: {
                        let svc = svc.clone();
                        let audit = audit_state.clone();
                        move |evt: Event<MouseData>| {
                            evt.stop_propagation();
                            let svc = svc.clone();
                            let id = dev_id2.clone();
                            let audit = audit.clone();
                            spawn(async move {
                                if let Err(e) = svc.accept_device(&id).await {
                                    eprintln!("Accept failed: {e}");
                                } else {
                                    audit.audit(
                                        bms_store_storage::store::audit_store::AuditEntryBuilder::new(
                                            bms_store_storage::store::audit_store::AuditAction::AcceptDevice, "device",
                                        ).resource_id(&id),
                                    );
                                }
                                bump(&mut refresh_counter);
                            });
                        }
                    },
                    "Accept"
                }
            }
        }
    }
}
