use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;

use bms_store_domain::points::{PointResponse, WriteRequest, WriteResponse};

use crate::api::auth::{require_permission, AuthUser};
use crate::api::error::ApiError;
use crate::api::pagination::{PaginatedResponse, PaginationParams};
use crate::api::write;
use crate::api::ApiState;
use crate::auth::Permission;
use crate::config::profile::PointValue;
use crate::store::audit_store::{AuditAction, AuditEntryBuilder};
use crate::store::point_store::{PointKey, TimestampedValue};

fn point_value_json(v: &PointValue) -> serde_json::Value {
    match v {
        PointValue::Bool(b) => serde_json::json!(b),
        PointValue::Integer(i) => serde_json::json!(i),
        PointValue::Float(f) => serde_json::json!(f),
    }
}

/// Build a PointResponse from a TimestampedValue, applying canonical mapping
/// unless `raw` is set.
fn tv_to_response(device_id: String, point_id: String, tv: &TimestampedValue, raw: bool) -> PointResponse {
    let raw_value = point_value_json(&tv.value);
    let (value, value_mapped) = if raw {
        (raw_value.clone(), false)
    } else if let Some(ref canonical) = tv.canonical_value {
        (serde_json::Value::String(canonical.clone()), true)
    } else {
        (raw_value.clone(), false)
    };
    PointResponse {
        device_id,
        point_id,
        value,
        raw_value,
        value_mapped,
        status: tv
            .status
            .active_flags()
            .into_iter()
            .map(String::from)
            .collect(),
        ingest_ts_ms: tv.ingest_ts_ms,
        source_ts_ms: tv.source_ts_ms,
    }
}

#[derive(Deserialize)]
pub struct ListPointsQuery {
    /// Return raw protocol values instead of canonical mapped values.
    #[serde(default)]
    pub raw: bool,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

/// GET /api/points — list all points
pub async fn list_points(
    State(state): State<ApiState>,
    _auth: AuthUser,
    Query(q): Query<ListPointsQuery>,
) -> Json<PaginatedResponse<PointResponse>> {
    let keys = state.point_store.all_keys();
    let mut points = Vec::with_capacity(keys.len());
    for key in keys {
        if let Some(tv) = state.point_store.get(&key) {
            points.push(tv_to_response(
                key.device_instance_id,
                key.point_id,
                &tv,
                q.raw,
            ));
        }
    }
    Json(PaginatedResponse::from_vec(points, &q.pagination))
}

#[derive(Deserialize)]
pub struct DevicePointsQuery {
    /// Return raw protocol values instead of canonical mapped values.
    #[serde(default)]
    pub raw: bool,
}

/// GET /api/points/:device_id — list points for a device
pub async fn device_points(
    State(state): State<ApiState>,
    _auth: AuthUser,
    Path(device_id): Path<String>,
    Query(q): Query<DevicePointsQuery>,
) -> Json<Vec<PointResponse>> {
    let device_points = state.point_store.get_all_for_device(&device_id);
    let points = device_points
        .into_iter()
        .map(|(key, tv)| {
            tv_to_response(key.device_instance_id, key.point_id, &tv, q.raw)
        })
        .collect();
    Json(points)
}

#[derive(Deserialize)]
pub struct GetPointQuery {
    /// Return raw protocol value instead of canonical mapped value.
    #[serde(default)]
    pub raw: bool,
}

/// GET /api/points/:device_id/:point_id — get a single point
pub async fn get_point(
    State(state): State<ApiState>,
    _auth: AuthUser,
    Path((device_id, point_id)): Path<(String, String)>,
    Query(q): Query<GetPointQuery>,
) -> Result<Json<PointResponse>, ApiError> {
    let key = PointKey {
        device_instance_id: device_id.clone(),
        point_id: point_id.clone(),
    };
    let tv = state
        .point_store
        .get(&key)
        .ok_or_else(|| ApiError::NotFound(format!("point {device_id}/{point_id} not found")))?;
    Ok(Json(tv_to_response(device_id, point_id, &tv, q.raw)))
}

/// POST /api/points/:device_id/:point_id/write
pub async fn write_point(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path((device_id, point_id)): Path<(String, String)>,
    Json(req): Json<WriteRequest>,
) -> Result<Json<WriteResponse>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::WritePoints, &perms)?;

    let value = json_to_point_value(&req.value).ok_or_else(|| {
        ApiError::BadRequest("invalid value: expected bool, int, or float".into())
    })?;

    let resource_id = format!("{device_id}/{point_id}");
    let details = format!("value={value:?} priority={:?}", req.priority);

    let result = write::route_write(
        &device_id,
        &point_id,
        value.clone(),
        req.priority,
        &state.bridge_registry,
    )
    .await;

    // Audit log
    let builder = match &result {
        Ok(()) => AuditEntryBuilder::new(AuditAction::WritePoint, "point")
            .resource_id(&resource_id)
            .details(&details),
        Err(e) => AuditEntryBuilder::new(AuditAction::WritePoint, "point")
            .resource_id(&resource_id)
            .details(&details)
            .failure(&e.to_string()),
    };
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    match result {
        Ok(()) => {
            // Capture original value before overwriting
            let key = PointKey {
                device_instance_id: device_id.clone(),
                point_id: point_id.clone(),
            };
            let original = state
                .point_store
                .get(&key)
                .map(|tv| point_value_json(&tv.value));

            // Update local store
            state.point_store.set(key.clone(), value.clone());
            state.point_store.set_status(
                &key,
                crate::store::point_store::PointStatusFlags::OVERRIDDEN,
            );

            // Record override for lifecycle tracking
            let _ = state
                .override_store
                .record(
                    &device_id,
                    &point_id,
                    original,
                    point_value_json(&value),
                    req.priority,
                    req.expires_ms,
                    &auth.username,
                )
                .await;

            Ok(Json(WriteResponse { ok: true }))
        }
        Err(e) => Err(ApiError::Internal(e.to_string())),
    }
}

fn json_to_point_value(v: &serde_json::Value) -> Option<PointValue> {
    match v {
        serde_json::Value::Bool(b) => Some(PointValue::Bool(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(PointValue::Integer(i))
            } else {
                n.as_f64().map(PointValue::Float)
            }
        }
        _ => None,
    }
}
