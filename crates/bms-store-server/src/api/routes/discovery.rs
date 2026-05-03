use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;

use crate::api::auth::{require_permission, AuthUser};
use crate::api::error::ApiError;
use crate::api::pagination::{PaginatedResponse, PaginationParams};
use crate::api::ApiState;
use crate::auth::Permission;
use crate::bridge::bacnet::BacnetNetworks;
use crate::bridge::modbus::ModbusBridge;
use crate::store::audit_store::{AuditAction, AuditEntryBuilder};

#[derive(Deserialize)]
pub struct ListDevicesQuery {
    pub state: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

/// GET /api/discovery/devices
pub async fn list_devices(
    State(state): State<ApiState>,
    _auth: AuthUser,
    Query(q): Query<ListDevicesQuery>,
) -> Json<PaginatedResponse<serde_json::Value>> {
    let state_filter = q.state.as_deref().and_then(|s| match s {
        "discovered" => Some(crate::discovery::model::DeviceState::Discovered),
        "accepted" => Some(crate::discovery::model::DeviceState::Accepted),
        "ignored" => Some(crate::discovery::model::DeviceState::Ignored),
        _ => None,
    });
    let devices = state.discovery_store.list_devices(state_filter).await;
    let all: Vec<serde_json::Value> = devices
        .into_iter()
        .map(|d| serde_json::to_value(d).unwrap_or_default())
        .collect();
    Json(PaginatedResponse::from_vec(all, &q.pagination))
}

/// GET /api/discovery/devices/:id
pub async fn get_device(
    State(state): State<ApiState>,
    _auth: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let device = state.discovery_store.get_device(&id).await?;
    Ok(Json(serde_json::to_value(device).unwrap_or_default()))
}

/// GET /api/discovery/devices/:id/points
pub async fn get_device_points(
    State(state): State<ApiState>,
    _auth: AuthUser,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let points = state.discovery_store.get_points(&id).await;
    Json(serde_json::to_value(points).unwrap_or_default())
}

#[derive(Default, serde::Deserialize)]
pub struct AcceptDeviceBody {
    /// Optional NodeStore id of a Site/Building/Floor/FloorArea/Room.
    /// When set, the new equip entity gets siteRef/buildingRef/floorRef/
    /// spaceRef populated from the ancestor chain.
    pub target_space_id: Option<String>,
}

/// POST /api/discovery/devices/:id/accept — body is optional;
/// `{ "target_space_id": "<node-id>" }` places the device into a
/// spatial parent in one round trip.
pub async fn accept_device(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<String>,
    body: Option<Json<AcceptDeviceBody>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageDiscovery, &perms)?;

    let opts = bms_store_bridges::discovery::service::AcceptOptions {
        skip_auto_tag: false,
        target_space_id: body.and_then(|b| b.0.target_space_id),
    };

    state
        .discovery_service
        .accept_device_with_options(&id, opts)
        .await
        .map_err(ApiError::Internal)?;

    let builder = AuditEntryBuilder::new(AuditAction::AcceptDevice, "device").resource_id(&id);
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

/// GET /api/discovery/devices/:id/preview-tags — dry-run the tags
/// `accept_device` would apply, with source + confidence per row.
/// Lets the GUI show a "Preview Tags" modal before commit so wrong
/// suggestions can be overridden up front.
pub async fn preview_device_tags(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageDiscovery, &perms)?;

    let preview = state
        .discovery_service
        .preview_device_tags(&id)
        .await
        .map_err(ApiError::Internal)?;

    Ok(Json(serde_json::json!({
        "device_id": preview.device_id,
        "device_dis": preview.device_dis,
        "equip": {
            "tags": tags_to_json(&preview.equip_tags),
            "source": tag_source_str(preview.equip_source),
            "confidence": preview.equip_confidence,
        },
        "points": preview.points.iter().map(|p| serde_json::json!({
            "point_id": p.point_id,
            "point_dis": p.point_dis,
            "units": p.units,
            "tags": tags_to_json(&p.tags),
            "source": tag_source_str(p.source),
            "confidence": p.confidence,
        })).collect::<Vec<_>>(),
    })))
}

fn tags_to_json(tags: &[(String, Option<String>)]) -> serde_json::Value {
    serde_json::Value::Array(
        tags.iter()
            .map(|(name, val)| {
                serde_json::json!({
                    "name": name,
                    "value": val,
                })
            })
            .collect(),
    )
}

fn tag_source_str(s: bms_store_bridges::discovery::service::TagSource) -> &'static str {
    match s {
        bms_store_bridges::discovery::service::TagSource::Atlas => "atlas",
        bms_store_bridges::discovery::service::TagSource::Heuristic => "heuristic",
    }
}

/// POST /api/discovery/devices/:id/ignore
pub async fn ignore_device(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageDiscovery, &perms)?;

    state
        .discovery_service
        .ignore_device(&id)
        .await
        .map_err(ApiError::Internal)?;

    let builder = AuditEntryBuilder::new(AuditAction::IgnoreDevice, "device").resource_id(&id);
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

/// POST /api/discovery/scan/bacnet
pub async fn scan_bacnet(
    State(state): State<ApiState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageDiscovery, &perms)?;

    if let Some(handle) = state.bridge_registry.get("bacnet") {
        let mut guard = handle.lock().await;
        let nets = guard
            .as_any_mut()
            .downcast_mut::<BacnetNetworks>()
            .expect("bacnet bridge type mismatch");
        state.discovery_service.scan_bacnet_all(nets).await;
    }

    let builder = AuditEntryBuilder::new(AuditAction::ScanNetwork, "bacnet");
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

/// POST /api/discovery/scan/modbus
pub async fn scan_modbus(
    State(state): State<ApiState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageDiscovery, &perms)?;

    if let Some(handle) = state.bridge_registry.get("modbus") {
        let guard = handle.lock().await;
        if let Some(bridge) = guard.as_any().downcast_ref::<ModbusBridge>() {
            state.discovery_service.scan_modbus(bridge).await;
        }
    }

    let builder = AuditEntryBuilder::new(AuditAction::ScanNetwork, "modbus");
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

#[derive(Deserialize)]
pub struct BulkDeviceIds {
    pub device_ids: Vec<String>,
}

/// POST /api/discovery/devices/bulk-accept
pub async fn bulk_accept(
    State(state): State<ApiState>,
    auth: AuthUser,
    Json(req): Json<BulkDeviceIds>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageDiscovery, &perms)?;

    let mut accepted = 0;
    let mut errors = Vec::new();

    for id in &req.device_ids {
        match state.discovery_service.accept_device(id).await {
            Ok(()) => accepted += 1,
            Err(e) => errors.push(format!("{id}: {e}")),
        }
    }

    let builder = AuditEntryBuilder::new(AuditAction::AcceptDevice, "device").details(&format!(
        "bulk accept: {accepted} accepted, {} errors",
        errors.len()
    ));
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({
        "ok": true,
        "accepted": accepted,
        "errors": errors,
    })))
}

/// POST /api/discovery/devices/bulk-ignore
pub async fn bulk_ignore(
    State(state): State<ApiState>,
    auth: AuthUser,
    Json(req): Json<BulkDeviceIds>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageDiscovery, &perms)?;

    let mut ignored = 0;
    let mut errors = Vec::new();

    for id in &req.device_ids {
        match state.discovery_service.ignore_device(id).await {
            Ok(()) => ignored += 1,
            Err(e) => errors.push(format!("{id}: {e}")),
        }
    }

    let builder = AuditEntryBuilder::new(AuditAction::IgnoreDevice, "device").details(&format!(
        "bulk ignore: {ignored} ignored, {} errors",
        errors.len()
    ));
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({
        "ok": true,
        "ignored": ignored,
        "errors": errors,
    })))
}

#[derive(Deserialize)]
pub struct RenameRequest {
    pub name: String,
}

/// POST /api/discovery/devices/:id/rename
pub async fn rename_device(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<RenameRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageDiscovery, &perms)?;

    state
        .discovery_store
        .update_device_name(&id, &req.name)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(serde_json::json!({"ok": true})))
}
