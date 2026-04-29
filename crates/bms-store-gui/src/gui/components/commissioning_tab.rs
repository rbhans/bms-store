use dioxus::prelude::*;

use crate::auth::Permission;
use crate::config::profile::PointValue;
use crate::gui::state::AppState;
use crate::store::audit_store::{AuditAction, AuditEntryBuilder};
use crate::store::commissioning_store::{
    CommissionItem, CommissionItemSeed, CommissionSession, ItemStatus, ItemType, SessionStatus,
};
use crate::store::point_store::PointKey;

// ----------------------------------------------------------------
// CommissioningTab — per-device commissioning checklist
// ----------------------------------------------------------------

#[component]
pub fn CommissioningTab(device_id: String) -> Element {
    let state = use_context::<AppState>();
    let can_manage = state.has_permission(Permission::ManageCommissioning);

    let mut session: Signal<Option<CommissionSession>> = use_signal(|| None);
    let mut items: Signal<Vec<CommissionItem>> = use_signal(Vec::new);
    let mut filter_status: Signal<Option<ItemStatus>> = use_signal(|| None);
    let mut status_msg: Signal<Option<String>> = use_signal(|| None);
    let mut refresh: Signal<u64> = use_signal(|| 0);
    let mut confirm_verify: Signal<Option<(i64, String, String, ItemType)>> = use_signal(|| None);

    // Auto-create session on mount (depends on device_id + refresh)
    {
        let cs = state.commissioning_store.clone();
        let ds = state.discovery_store.clone();
        let device_id = device_id.clone();
        let state_audit = state.clone();
        let _ = use_resource(move || {
            let cs = cs.clone();
            let ds = ds.clone();
            let device_id = device_id.clone();
            let state_audit = state_audit.clone();
            let _r = *refresh.read();
            async move {
                let existing = cs.get_session(&device_id).await;
                let sess = if let Some(s) = existing {
                    s
                } else if !can_manage {
                    // No permission to create — show read-only message
                    session.set(None);
                    items.set(Vec::new());
                    return;
                } else {
                    // Fetch discovered points and build seeds using actual capabilities
                    let points = ds.get_points(&device_id).await;
                    if points.is_empty() {
                        status_msg.set(Some("No discovered points for this device.".to_string()));
                        session.set(None);
                        items.set(Vec::new());
                        return;
                    }
                    let seeds: Vec<CommissionItemSeed> = points
                        .iter()
                        .map(|p| CommissionItemSeed {
                            point_id: p.id.clone(),
                            writable: p.writable,
                            alarmable: p.writable, // only writable points need alarm verify
                            schedulable: p.writable, // only writable points are schedulable
                        })
                        .collect();

                    match cs.create_session(&device_id, seeds).await {
                        Ok(_sid) => {
                            state_audit.audit(
                                AuditEntryBuilder::new(AuditAction::StartCommissioning, "device")
                                    .resource_id(&device_id),
                            );
                        }
                        Err(e) => {
                            status_msg.set(Some(format!("Failed to create session: {e}")));
                            session.set(None);
                            items.set(Vec::new());
                            return;
                        }
                    }
                    match cs.get_session(&device_id).await {
                        Some(s) => s,
                        None => {
                            status_msg.set(Some("Session created but not found.".to_string()));
                            session.set(None);
                            items.set(Vec::new());
                            return;
                        }
                    }
                };

                let session_id = sess.id;
                session.set(Some(sess));
                let loaded_items = cs.list_items(session_id).await;
                items.set(loaded_items);
                status_msg.set(None);
            }
        });
    }

    // Compute progress
    let all_items = items.read();
    let total = all_items.len();
    let verified = all_items
        .iter()
        .filter(|i| i.status == ItemStatus::Verified)
        .count();
    let failed = all_items
        .iter()
        .filter(|i| i.status == ItemStatus::Failed)
        .count();
    let deferred = all_items
        .iter()
        .filter(|i| i.status == ItemStatus::Deferred)
        .count();

    let pct_verified = if total > 0 {
        (verified * 100) / total
    } else {
        0
    };
    let pct_failed = if total > 0 { (failed * 100) / total } else { 0 };
    let pct_deferred = if total > 0 {
        (deferred * 100) / total
    } else {
        0
    };

    // Filter items
    let current_filter = *filter_status.read();
    let filtered_items: Vec<CommissionItem> = all_items
        .iter()
        .filter(|i| match current_filter {
            Some(f) => i.status == f,
            None => true,
        })
        .cloned()
        .collect();

    let current_session = session.read().clone();
    let session_status = current_session.as_ref().map(|s| s.status);
    let is_signed_off = session_status == Some(SessionStatus::SignedOff);
    let is_completed = session_status == Some(SessionStatus::Completed);

    // Sign-off info
    let signed_off_by = current_session
        .as_ref()
        .and_then(|s| s.signed_off_by.clone())
        .unwrap_or_default();
    let signed_off_date = current_session
        .as_ref()
        .and_then(|s| s.signed_off_ms)
        .map(format_timestamp)
        .unwrap_or_default();

    let confirm_data = confirm_verify.read().clone();

    // No session and no permission — show read-only notice
    if session.read().is_none() && !can_manage {
        return rsx! {
            div { class: "commission-panel",
                div { class: "empty-state",
                    "No commissioning session started for this device. A user with commissioning permissions must start the session."
                }
            }
        };
    }

    rsx! {
        div { class: "commission-panel",
            div { class: "section-header",
                h3 { "Commissioning Checklist" }
                div { class: "commission-actions",
                    // Filter dropdown
                    select {
                        onchange: move |e| {
                            let val = e.value();
                            filter_status.set(match val.as_str() {
                                "not_started" => Some(ItemStatus::NotStarted),
                                "in_progress" => Some(ItemStatus::InProgress),
                                "verified" => Some(ItemStatus::Verified),
                                "failed" => Some(ItemStatus::Failed),
                                "deferred" => Some(ItemStatus::Deferred),
                                _ => None,
                            });
                        },
                        option { value: "all", "All" }
                        option { value: "not_started", "Not Started" }
                        option { value: "verified", "Verified" }
                        option { value: "failed", "Failed" }
                        option { value: "deferred", "Deferred" }
                    }

                    // Sign Off button
                    if can_manage && is_completed {
                        {
                            let cs = state.commissioning_store.clone();
                            let device_id = device_id.clone();
                            let state_audit = state.clone();
                            rsx! {
                                button {
                                    class: "btn btn-small",
                                    onclick: move |_| {
                                        let cs = cs.clone();
                                        let device_id = device_id.clone();
                                        let state_audit = state_audit.clone();
                                        let username = get_username(&state_audit);
                                        spawn(async move {
                                            match cs.sign_off_session(&device_id, &username).await {
                                                Ok(()) => {
                                                    state_audit.audit(
                                                        AuditEntryBuilder::new(
                                                            AuditAction::SignOffCommissioning,
                                                            "device",
                                                        )
                                                        .resource_id(&device_id),
                                                    );
                                                    refresh.set(refresh() + 1);
                                                }
                                                Err(e) => {
                                                    status_msg.set(Some(format!("Sign off failed: {e}")));
                                                }
                                            }
                                        });
                                    },
                                    "Sign Off"
                                }
                            }
                        }
                    }

                    // Export CSV button
                    if can_manage {
                        {
                            let export_items = all_items.clone();
                            rsx! {
                                button {
                                    class: "btn btn-small",
                                    onclick: move |_| {
                                        let csv = build_csv(&export_items);
                                        spawn(async move {
                                            let path = tokio::task::spawn_blocking(move || {
                                                rfd::FileDialog::new()
                                                    .add_filter("CSV", &["csv"])
                                                    .set_file_name("commissioning-checklist.csv")
                                                    .save_file()
                                            })
                                            .await
                                            .ok()
                                            .flatten();

                                            if let Some(p) = path {
                                                let _ = tokio::fs::write(p, csv).await;
                                            }
                                        });
                                    },
                                    "Export CSV"
                                }
                            }
                        }
                    }

                    // Reset button
                    if can_manage && !is_signed_off {
                        {
                            let cs = state.commissioning_store.clone();
                            let device_id = device_id.clone();
                            rsx! {
                                button {
                                    class: "btn btn-small btn-danger",
                                    onclick: move |_| {
                                        let cs = cs.clone();
                                        let device_id = device_id.clone();
                                        spawn(async move {
                                            let _ = cs.delete_session(&device_id).await;
                                            refresh.set(refresh() + 1);
                                        });
                                    },
                                    "Reset"
                                }
                            }
                        }
                    }
                }
            }

            // Status message
            if let Some(msg) = status_msg.read().as_ref() {
                div { class: "status-msg", "{msg}" }
            }

            // Progress bar
            div { class: "commission-progress",
                "Progress: {verified}/{total} verified | {failed} failed | {deferred} deferred"
                div { class: "commission-progress-bar",
                    div {
                        class: "progress-segment verified",
                        style: "width: {pct_verified}%",
                    }
                    div {
                        class: "progress-segment failed",
                        style: "width: {pct_failed}%",
                    }
                    div {
                        class: "progress-segment deferred",
                        style: "width: {pct_deferred}%",
                    }
                }
            }

            // Inline verify confirmation
            if let Some((item_id, ref point_id, ref value, item_type)) = confirm_data {
                {
                    let cs = state.commissioning_store.clone();
                    let state_audit = state.clone();
                    let point_id_confirm = point_id.clone();
                    let value_confirm = value.clone();
                    let point_id_fail = point_id.clone();
                    let value_fail = value.clone();
                    let is_write = item_type == ItemType::WriteVerify;
                    rsx! {
                        div { class: "commission-verify-confirm",
                            if is_write {
                                "Write test for {point_id} — use the Write Dialog to command a value, then verify the current readback: "
                            } else {
                                "Current value for {point_id}: "
                            }
                            strong { "{value}" }
                            div { class: "commission-verify-actions",
                                button {
                                    class: "btn btn-small btn-ok",
                                    onclick: {
                                        let cs = cs.clone();
                                        let state_audit = state_audit.clone();
                                        move |_| {
                                            let cs = cs.clone();
                                            let state_audit = state_audit.clone();
                                            let username = get_username(&state_audit);
                                            let pid = point_id_confirm.clone();
                                            let val = value_confirm.clone();
                                            spawn(async move {
                                                let _ = cs
                                                    .update_item_status(
                                                        item_id,
                                                        ItemStatus::Verified,
                                                        Some(username.clone()),
                                                        Some(val),
                                                        String::new(),
                                                    )
                                                    .await;
                                                state_audit.audit(
                                                    AuditEntryBuilder::new(
                                                        AuditAction::VerifyCommissionItem,
                                                        "commission_item",
                                                    )
                                                    .resource_id(&item_id.to_string())
                                                    .details(&pid),
                                                );
                                                confirm_verify.set(None);
                                                refresh.set(refresh() + 1);
                                            });
                                        }
                                    },
                                    "Confirm Verified"
                                }
                                button {
                                    class: "btn btn-small btn-danger",
                                    onclick: {
                                        let cs = cs.clone();
                                        let state_audit = state_audit.clone();
                                        move |_| {
                                            let cs = cs.clone();
                                            let state_audit = state_audit.clone();
                                            let username = get_username(&state_audit);
                                            let pid = point_id_fail.clone();
                                            let val = value_fail.clone();
                                            spawn(async move {
                                                let _ = cs
                                                    .update_item_status(
                                                        item_id,
                                                        ItemStatus::Failed,
                                                        Some(username),
                                                        Some(val),
                                                        String::new(),
                                                    )
                                                    .await;
                                                state_audit.audit(
                                                    AuditEntryBuilder::new(
                                                        AuditAction::FailCommissionItem,
                                                        "commission_item",
                                                    )
                                                    .resource_id(&item_id.to_string())
                                                    .details(&pid),
                                                );
                                                confirm_verify.set(None);
                                                refresh.set(refresh() + 1);
                                            });
                                        }
                                    },
                                    "Mark Failed"
                                }
                                button {
                                    class: "btn btn-small",
                                    onclick: move |_| {
                                        confirm_verify.set(None);
                                    },
                                    "Cancel"
                                }
                            }
                        }
                    }
                }
            }

            // Data table
            table { class: "data-table",
                thead {
                    tr {
                        th { "Point" }
                        th { "Type" }
                        th { "Status" }
                        th { "Value" }
                        th { "Verified By" }
                        th { "Time" }
                        th { "Actions" }
                    }
                }
                tbody {
                    for item in filtered_items.iter() {
                        {
                            let item_id = item.id;
                            let point_id = item.clone().point_id;
                            let item_type = item.item_type;
                            let item_status = item.status;
                            let actual_value = item.actual_value.clone().unwrap_or("-".to_string());
                            let verified_by = item.verified_by.clone().unwrap_or("-".to_string());
                            let verified_time = item
                                .verified_ms
                                .map(format_timestamp)
                                .unwrap_or("-".to_string());

                            let type_badge_class = match item_type {
                                ItemType::ReadVerify => "badge badge-info",
                                ItemType::WriteVerify => "badge badge-warning",
                                ItemType::AlarmVerify => "badge badge-danger",
                                ItemType::ScheduleVerify => "badge badge-inactive",
                            };

                            let status_badge_class = match item_status {
                                ItemStatus::NotStarted => "badge badge-inactive",
                                ItemStatus::InProgress => "badge badge-info",
                                ItemStatus::Verified => "badge badge-ok",
                                ItemStatus::Failed => "badge badge-danger",
                                ItemStatus::Deferred => "badge badge-warning",
                            };

                            rsx! {
                                tr {
                                    td { "{point_id}" }
                                    td {
                                        span { class: "{type_badge_class}", "{item_type.label()}" }
                                    }
                                    td {
                                        span { class: "{status_badge_class}", "{item_status.label()}" }
                                    }
                                    td { "{actual_value}" }
                                    td { "{verified_by}" }
                                    td { "{verified_time}" }
                                    td {
                                        if can_manage && !is_signed_off {
                                            {render_actions(
                                                item_id,
                                                device_id.clone(),
                                                point_id.clone(),
                                                item_status,
                                                item_type,
                                                &state,
                                                confirm_verify,
                                                refresh,
                                                status_msg,
                                            )}
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Signed-off notice
            if is_signed_off {
                div { class: "commission-signoff-notice",
                    "Signed off by {signed_off_by} on {signed_off_date}"
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Action buttons per row
// ----------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn render_actions(
    item_id: i64,
    device_id: String,
    point_id: String,
    status: ItemStatus,
    item_type: ItemType,
    state: &AppState,
    mut confirm_verify: Signal<Option<(i64, String, String, ItemType)>>,
    mut refresh: Signal<u64>,
    mut status_msg: Signal<Option<String>>,
) -> Element {
    let cs = state.commissioning_store.clone();
    let state_audit = state.clone();

    match status {
        ItemStatus::NotStarted | ItemStatus::InProgress => {
            let cs_verify_as = cs.clone();
            let state_verify_as = state_audit.clone();
            let pid_verify_as = point_id.clone();
            let mut refresh_verify_as = refresh;
            let cs_fail = cs.clone();
            let cs_defer = cs.clone();
            let state_fail = state_audit.clone();
            let state_defer = state_audit.clone();
            let pid_fail = point_id.clone();
            let pid_defer = point_id.clone();
            let mut refresh_fail = refresh;
            let mut refresh_defer = refresh;
            let mut msg_fail = status_msg;
            let mut msg_defer = status_msg;
            rsx! {
                if item_type == ItemType::ReadVerify || item_type == ItemType::WriteVerify {
                    button {
                        class: "btn btn-small btn-ok",
                        onclick: {
                            let point_id = point_id.clone();
                            let store = state.store.clone();
                            let device_id_key = device_id.clone();
                            move |_| {
                                // Read live value from PointStore
                                let key = PointKey {
                                    device_instance_id: device_id_key.clone(),
                                    point_id: point_id.clone(),
                                };
                                let live_value = store.get(&key);
                                let formatted = match live_value {
                                    Some(tv) => format_point_value(&tv.value),
                                    None => "N/A".to_string(),
                                };
                                confirm_verify.set(Some((item_id, point_id.clone(), formatted, item_type)));
                            }
                        },
                        "Verify"
                    }
                }
                if item_type == ItemType::AlarmVerify || item_type == ItemType::ScheduleVerify {
                    button {
                        class: "btn btn-small btn-ok",
                        onclick: move |_| {
                            let cs = cs_verify_as.clone();
                            let state_audit = state_verify_as.clone();
                            let username = get_username(&state_audit);
                            let pid = pid_verify_as.clone();
                            spawn(async move {
                                let _ = cs
                                    .update_item_status(
                                        item_id,
                                        ItemStatus::Verified,
                                        Some(username),
                                        None,
                                        String::new(),
                                    )
                                    .await;
                                state_audit.audit(
                                    AuditEntryBuilder::new(
                                        AuditAction::VerifyCommissionItem,
                                        "commission_item",
                                    )
                                    .resource_id(&item_id.to_string())
                                    .details(&pid),
                                );
                                refresh_verify_as.set(refresh_verify_as() + 1);
                            });
                        },
                        "Verify"
                    }
                }
                button {
                    class: "btn btn-small btn-danger",
                    onclick: move |_| {
                        let cs = cs_fail.clone();
                        let state_audit = state_fail.clone();
                        let username = get_username(&state_audit);
                        let pid = pid_fail.clone();
                        spawn(async move {
                            match cs
                                .update_item_status(
                                    item_id,
                                    ItemStatus::Failed,
                                    Some(username),
                                    None,
                                    String::new(),
                                )
                                .await
                            {
                                Ok(()) => {
                                    state_audit.audit(
                                        AuditEntryBuilder::new(
                                            AuditAction::FailCommissionItem,
                                            "commission_item",
                                        )
                                        .resource_id(&item_id.to_string())
                                        .details(&pid),
                                    );
                                    refresh_fail.set(refresh_fail() + 1);
                                }
                                Err(e) => {
                                    msg_fail.set(Some(format!("Failed: {e}")));
                                }
                            }
                        });
                    },
                    "Fail"
                }
                button {
                    class: "btn btn-small btn-warning",
                    onclick: move |_| {
                        let cs = cs_defer.clone();
                        let state_audit = state_defer.clone();
                        let username = get_username(&state_audit);
                        let pid = pid_defer.clone();
                        spawn(async move {
                            match cs
                                .update_item_status(
                                    item_id,
                                    ItemStatus::Deferred,
                                    Some(username),
                                    None,
                                    String::new(),
                                )
                                .await
                            {
                                Ok(()) => {
                                    state_audit.audit(
                                        AuditEntryBuilder::new(
                                            AuditAction::DeferCommissionItem,
                                            "commission_item",
                                        )
                                        .resource_id(&item_id.to_string())
                                        .details(&pid),
                                    );
                                    refresh_defer.set(refresh_defer() + 1);
                                }
                                Err(e) => {
                                    msg_defer.set(Some(format!("Deferred failed: {e}")));
                                }
                            }
                        });
                    },
                    "Defer"
                }
            }
        }
        ItemStatus::Verified => {
            // No actions for verified items
            rsx! {}
        }
        ItemStatus::Failed => {
            let cs_retry = cs.clone();
            let cs_defer = cs.clone();
            let state_retry = state_audit.clone();
            let state_defer = state_audit.clone();
            let pid_retry = point_id.clone();
            let pid_defer = point_id.clone();
            let mut refresh_retry = refresh;
            let mut refresh_defer = refresh;
            rsx! {
                button {
                    class: "btn btn-small btn-ok",
                    onclick: move |_| {
                        let cs = cs_retry.clone();
                        let state_audit = state_retry.clone();
                        let pid = pid_retry.clone();
                        spawn(async move {
                            let _ = cs
                                .update_item_status(
                                    item_id,
                                    ItemStatus::NotStarted,
                                    None,
                                    None,
                                    String::new(),
                                )
                                .await;
                            state_audit.audit(
                                AuditEntryBuilder::new(
                                    AuditAction::VerifyCommissionItem,
                                    "commission_item",
                                )
                                .resource_id(&item_id.to_string())
                                .details(&format!("retry: {pid}")),
                            );
                            refresh_retry.set(refresh_retry() + 1);
                        });
                    },
                    "Retry"
                }
                button {
                    class: "btn btn-small btn-warning",
                    onclick: move |_| {
                        let cs = cs_defer.clone();
                        let state_audit = state_defer.clone();
                        let pid = pid_defer.clone();
                        spawn(async move {
                            let _ = cs
                                .update_item_status(
                                    item_id,
                                    ItemStatus::Deferred,
                                    None,
                                    None,
                                    String::new(),
                                )
                                .await;
                            state_audit.audit(
                                AuditEntryBuilder::new(
                                    AuditAction::DeferCommissionItem,
                                    "commission_item",
                                )
                                .resource_id(&item_id.to_string())
                                .details(&pid),
                            );
                            refresh_defer.set(refresh_defer() + 1);
                        });
                    },
                    "Defer"
                }
            }
        }
        ItemStatus::Deferred => {
            let cs_retry = cs;
            let state_retry = state_audit;
            let pid_retry = point_id;
            let mut refresh_retry = refresh;
            rsx! {
                button {
                    class: "btn btn-small btn-ok",
                    onclick: move |_| {
                        let cs = cs_retry.clone();
                        let state_audit = state_retry.clone();
                        let pid = pid_retry.clone();
                        spawn(async move {
                            let _ = cs
                                .update_item_status(
                                    item_id,
                                    ItemStatus::NotStarted,
                                    None,
                                    None,
                                    String::new(),
                                )
                                .await;
                            state_audit.audit(
                                AuditEntryBuilder::new(
                                    AuditAction::VerifyCommissionItem,
                                    "commission_item",
                                )
                                .resource_id(&item_id.to_string())
                                .details(&format!("retry: {pid}")),
                            );
                            refresh_retry.set(refresh_retry() + 1);
                        });
                    },
                    "Retry"
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------

fn get_username(state: &AppState) -> String {
    state
        .current_user
        .read()
        .as_ref()
        .map(|u| u.username.clone())
        .unwrap_or_else(|| "system".to_string())
}

fn format_point_value(value: &PointValue) -> String {
    match value {
        PointValue::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        PointValue::Integer(i) => i.to_string(),
        PointValue::Float(f) => format!("{:.2}", f),
    }
}

fn format_timestamp(ms: i64) -> String {
    // Manual UTC conversion without chrono dependency
    const SECS_PER_MIN: i64 = 60;
    const SECS_PER_HOUR: i64 = 3600;
    const SECS_PER_DAY: i64 = 86400;

    let total_secs = ms / 1000;
    let time_of_day = total_secs.rem_euclid(SECS_PER_DAY);
    let hour = time_of_day / SECS_PER_HOUR;
    let minute = (time_of_day % SECS_PER_HOUR) / SECS_PER_MIN;

    // Days since epoch
    let mut days = total_secs / SECS_PER_DAY;
    if total_secs < 0 && total_secs % SECS_PER_DAY != 0 {
        days -= 1;
    }

    // Civil date from day count (algorithm from Howard Hinnant)
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{:04}-{:02}-{:02} {:02}:{:02}", y, m, d, hour, minute)
}

fn build_csv(items: &[CommissionItem]) -> String {
    let mut csv = String::from(
        "point_id,point_name,item_type,status,verified_by,verified_at,actual_value,notes\n",
    );
    for item in items {
        let verified_at = item.verified_ms.map(format_timestamp).unwrap_or_default();
        let actual = item.actual_value.as_deref().unwrap_or("");
        let verified_by = item.verified_by.as_deref().unwrap_or("");
        // Escape CSV fields that might contain commas or quotes
        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{}\n",
            csv_escape(&item.point_id),
            csv_escape(&item.point_id),
            csv_escape(item.item_type.label()),
            csv_escape(item.status.label()),
            csv_escape(verified_by),
            csv_escape(&verified_at),
            csv_escape(actual),
            csv_escape(&item.notes),
        ));
    }
    csv
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
