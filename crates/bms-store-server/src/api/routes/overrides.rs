use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;

use crate::api::auth::{require_permission, AuthUser};
use crate::api::error::ApiError;
use crate::api::ApiState;
use crate::auth::Permission;

/// GET /api/overrides/active
pub async fn list_active(
    State(state): State<ApiState>,
    _auth: AuthUser,
) -> Json<serde_json::Value> {
    let overrides = state.override_store.list_active().await;
    Json(serde_json::to_value(overrides).unwrap_or_default())
}

#[derive(Deserialize)]
pub struct ListAllQuery {
    pub limit: Option<i64>,
}

/// GET /api/overrides
pub async fn list_all(
    State(state): State<ApiState>,
    _auth: AuthUser,
    Query(q): Query<ListAllQuery>,
) -> Json<serde_json::Value> {
    let overrides = state.override_store.list_all(q.limit.unwrap_or(100)).await;
    Json(serde_json::to_value(overrides).unwrap_or_default())
}

#[derive(Deserialize)]
pub struct UpdateExpiryRequest {
    pub expires_ms: Option<i64>,
}

/// PUT /api/overrides/:id
pub async fn update_override(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<UpdateExpiryRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::WritePoints, &perms)?;

    state
        .override_store
        .update_expiry(id, req.expires_ms)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(serde_json::json!({"ok": true})))
}

/// POST /api/points/:device_id/:point_id/relinquish
pub async fn relinquish_point(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path((device_id, point_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::WritePoints, &perms)?;

    let overrides = state
        .override_store
        .relinquish_by_point(&device_id, &point_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // For each relinquished override with a priority, send a NULL write to release
    // the priority array slot on the BACnet device.
    // For Modbus, write back the original value if available.
    let mut relinquished = 0;
    for ov in &overrides {
        if let Some(priority) = ov.priority {
            // BACnet relinquish: write NULL at the priority level
            // This is handled by writing the original value back or letting the
            // relinquish-default take over.
            if let Some(ref orig) = ov.original_value {
                if let Some(pv) = json_to_point_value(orig) {
                    let _ = crate::api::write::route_write(
                        &device_id,
                        &point_id,
                        pv,
                        Some(priority),
                        &state.bridge_registry,
                    )
                    .await;
                }
            }
        }
        relinquished += 1;
    }

    // Clear the OVERRIDDEN flag if no more active overrides remain
    let remaining = state.override_store.list_active().await;
    let still_overridden = remaining
        .iter()
        .any(|o| o.device_id == device_id && o.point_id == point_id);
    if !still_overridden {
        let key = crate::store::point_store::PointKey {
            device_instance_id: device_id,
            point_id,
        };
        state.point_store.clear_status(
            &key,
            crate::store::point_store::PointStatusFlags::OVERRIDDEN,
        );
    }

    Ok(Json(
        serde_json::json!({"ok": true, "relinquished": relinquished}),
    ))
}

fn json_to_point_value(v: &serde_json::Value) -> Option<crate::config::profile::PointValue> {
    match v {
        serde_json::Value::Bool(b) => Some(crate::config::profile::PointValue::Bool(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(crate::config::profile::PointValue::Integer(i))
            } else {
                n.as_f64().map(crate::config::profile::PointValue::Float)
            }
        }
        _ => None,
    }
}
