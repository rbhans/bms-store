use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::auth::{require_permission, AuthUser};
use crate::api::error::ApiError;
use crate::api::pagination::{validate_string, PaginatedResponse, PaginationParams};
use crate::api::ApiState;
use crate::auth::Permission;
use crate::config::profile::PointValue;
use crate::store::audit_store::{AuditAction, AuditEntryBuilder};
use crate::store::schedule_store::{Schedule, ScheduleAssignment, ScheduleValueType};

#[derive(Serialize)]
pub struct ScheduleResponse {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub value_type: String,
    pub default_value: serde_json::Value,
    pub enabled: bool,
    pub created_ms: i64,
    pub updated_ms: i64,
}

fn pv_json(v: &PointValue) -> serde_json::Value {
    match v {
        PointValue::Bool(b) => serde_json::json!(b),
        PointValue::Integer(i) => serde_json::json!(i),
        PointValue::Float(f) => serde_json::json!(f),
    }
}

fn schedule_to_response(s: Schedule) -> ScheduleResponse {
    ScheduleResponse {
        id: s.id,
        name: s.name,
        description: s.description,
        value_type: s.value_type.as_str().to_string(),
        default_value: pv_json(&s.default_value),
        enabled: s.enabled,
        created_ms: s.created_ms,
        updated_ms: s.updated_ms,
    }
}

#[derive(Deserialize)]
pub struct ListSchedulesQuery {
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

/// GET /api/schedules
pub async fn list_schedules(
    State(state): State<ApiState>,
    _auth: AuthUser,
    Query(q): Query<ListSchedulesQuery>,
) -> Json<PaginatedResponse<ScheduleResponse>> {
    let schedules = state.schedule_store.list_schedules().await;
    let all: Vec<ScheduleResponse> = schedules.into_iter().map(schedule_to_response).collect();
    Json(PaginatedResponse::from_vec(all, &q.pagination))
}

/// GET /api/schedules/:id
pub async fn get_schedule(
    State(state): State<ApiState>,
    _auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<ScheduleResponse>, ApiError> {
    let schedule = state
        .schedule_store
        .get_schedule(id)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("schedule {id} not found")))?;
    Ok(Json(schedule_to_response(schedule)))
}

#[derive(Deserialize)]
pub struct CreateScheduleRequest {
    pub name: String,
    pub description: Option<String>,
    pub value_type: String,
    pub default_value: serde_json::Value,
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

/// POST /api/schedules
pub async fn create_schedule(
    State(state): State<ApiState>,
    auth: AuthUser,
    Json(req): Json<CreateScheduleRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_string("name", &req.name, 256)?;
    if let Some(ref desc) = req.description {
        if desc.len() > 1024 {
            return Err(ApiError::BadRequest(
                "description exceeds maximum length of 1024".into(),
            ));
        }
    }

    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageSchedules, &perms)?;

    let value_type = ScheduleValueType::from_str(&req.value_type)
        .ok_or_else(|| ApiError::BadRequest(format!("invalid value_type: {}", req.value_type)))?;

    let default_value = json_to_point_value(&req.default_value)
        .ok_or_else(|| ApiError::BadRequest("invalid default_value".into()))?;

    let weekly = crate::store::schedule_store::empty_weekly();

    let id = state
        .schedule_store
        .create_schedule(
            &req.name,
            req.description.as_deref().unwrap_or(""),
            value_type,
            default_value,
            weekly,
        )
        .await?;

    let builder = AuditEntryBuilder::new(AuditAction::CreateSchedule, "schedule")
        .resource_id(&id.to_string())
        .details(&req.name);
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true, "id": id})))
}

#[derive(Deserialize)]
pub struct UpdateScheduleRequest {
    pub name: String,
    pub description: Option<String>,
    pub default_value: serde_json::Value,
    pub enabled: bool,
}

/// PUT /api/schedules/:id
pub async fn update_schedule(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<UpdateScheduleRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageSchedules, &perms)?;

    validate_string("name", &req.name, 256)?;
    if let Some(ref desc) = req.description {
        if desc.len() > 1024 {
            return Err(ApiError::BadRequest(
                "description exceeds maximum length of 1024".into(),
            ));
        }
    }

    let default_value = json_to_point_value(&req.default_value)
        .ok_or_else(|| ApiError::BadRequest("invalid default_value".into()))?;

    // Fetch existing schedule to get its weekly schedule (preserve it)
    let existing = state
        .schedule_store
        .get_schedule(id)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("schedule {id} not found")))?;

    state
        .schedule_store
        .update_schedule(
            id,
            &req.name,
            req.description.as_deref().unwrap_or(""),
            default_value,
            req.enabled,
            existing.weekly,
        )
        .await?;

    let builder = AuditEntryBuilder::new(AuditAction::UpdateSchedule, "schedule")
        .resource_id(&id.to_string())
        .details(&req.name);
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

/// DELETE /api/schedules/:id
pub async fn delete_schedule(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageSchedules, &perms)?;

    state.schedule_store.delete_schedule(id).await?;

    let builder = AuditEntryBuilder::new(AuditAction::DeleteSchedule, "schedule")
        .resource_id(&id.to_string());
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

#[derive(Serialize)]
pub struct AssignmentResponse {
    pub id: i64,
    pub schedule_id: i64,
    pub device_id: String,
    pub point_id: String,
    pub priority: i32,
    pub enabled: bool,
    pub created_ms: i64,
}

fn assignment_to_response(a: ScheduleAssignment) -> AssignmentResponse {
    AssignmentResponse {
        id: a.id,
        schedule_id: a.schedule_id,
        device_id: a.device_id,
        point_id: a.point_id,
        priority: a.priority,
        enabled: a.enabled,
        created_ms: a.created_ms,
    }
}

/// GET /api/schedules/:id/assignments
pub async fn list_assignments(
    State(state): State<ApiState>,
    _auth: AuthUser,
    Path(schedule_id): Path<i64>,
    Query(q): Query<ListSchedulesQuery>,
) -> Json<PaginatedResponse<AssignmentResponse>> {
    let assignments = state
        .schedule_store
        .list_assignments_for_schedule(schedule_id)
        .await;
    let all: Vec<AssignmentResponse> = assignments
        .into_iter()
        .map(assignment_to_response)
        .collect();
    Json(PaginatedResponse::from_vec(all, &q.pagination))
}

#[derive(Deserialize)]
pub struct CreateAssignmentRequest {
    pub device_id: String,
    pub point_id: String,
    pub priority: Option<i32>,
}

/// POST /api/schedules/:id/assignments
pub async fn create_assignment(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(schedule_id): Path<i64>,
    Json(req): Json<CreateAssignmentRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageSchedules, &perms)?;

    let id = state
        .schedule_store
        .create_assignment(
            schedule_id,
            &req.device_id,
            &req.point_id,
            req.priority.unwrap_or(16),
        )
        .await?;

    let builder = AuditEntryBuilder::new(AuditAction::CreateAssignment, "schedule_assignment")
        .resource_id(&id.to_string())
        .details(&format!(
            "schedule={schedule_id} {}/{}",
            req.device_id, req.point_id
        ));
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true, "id": id})))
}

/// DELETE /api/schedules/assignments/:id
pub async fn delete_assignment(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageSchedules, &perms)?;

    state.schedule_store.delete_assignment(id).await?;

    let builder = AuditEntryBuilder::new(AuditAction::DeleteAssignment, "schedule_assignment")
        .resource_id(&id.to_string());
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}
