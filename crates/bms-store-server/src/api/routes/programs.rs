use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;

use crate::api::auth::{require_permission, AuthUser};
use crate::api::error::ApiError;
use crate::api::ApiState;
use crate::auth::Permission;
use crate::logic::model::Program;
use crate::store::audit_store::{AuditAction, AuditEntryBuilder};

#[derive(Deserialize)]
pub struct ListProgramsQuery {
    pub enabled_only: Option<bool>,
}

/// GET /api/programs
pub async fn list_programs(
    State(state): State<ApiState>,
    _auth: AuthUser,
    Query(q): Query<ListProgramsQuery>,
) -> Json<Vec<serde_json::Value>> {
    let programs = state
        .program_store
        .list(q.enabled_only.unwrap_or(false))
        .await;
    let values: Vec<serde_json::Value> = programs
        .into_iter()
        .filter_map(|p| serde_json::to_value(p).ok())
        .collect();
    Json(values)
}

/// GET /api/programs/:id
pub async fn get_program(
    State(state): State<ApiState>,
    _auth: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let program = state
        .program_store
        .get(&id)
        .await
        .map_err(|e| ApiError::NotFound(e.to_string()))?;
    Ok(Json(serde_json::to_value(program).unwrap_or_default()))
}

/// POST /api/programs
pub async fn create_program(
    State(state): State<ApiState>,
    auth: AuthUser,
    Json(program): Json<Program>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManagePrograms, &perms)?;

    state
        .program_store
        .create(program.clone())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder =
        AuditEntryBuilder::new(AuditAction::CreateProgram, "program").resource_id(&program.id);
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true, "id": program.id})))
}

/// PUT /api/programs/:id
pub async fn update_program(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(mut program): Json<Program>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManagePrograms, &perms)?;

    program.id = id.clone();
    state
        .program_store
        .update(program)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder = AuditEntryBuilder::new(AuditAction::UpdateProgram, "program").resource_id(&id);
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

/// DELETE /api/programs/:id
pub async fn delete_program(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManagePrograms, &perms)?;

    state
        .program_store
        .delete(&id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder = AuditEntryBuilder::new(AuditAction::DeleteProgram, "program").resource_id(&id);
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

#[derive(Deserialize)]
pub struct SetEnabledRequest {
    pub enabled: bool,
}

/// PUT /api/programs/:id/enabled
pub async fn set_enabled(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<SetEnabledRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManagePrograms, &perms)?;

    state
        .program_store
        .set_enabled(&id, req.enabled)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let action = if req.enabled {
        AuditAction::EnableProgram
    } else {
        AuditAction::DisableProgram
    };
    let builder = AuditEntryBuilder::new(action, "program").resource_id(&id);
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

#[derive(Deserialize)]
pub struct ExecutionLogQuery {
    pub limit: Option<usize>,
}

/// GET /api/programs/:id/log
pub async fn get_execution_log(
    State(state): State<ApiState>,
    _auth: AuthUser,
    Path(id): Path<String>,
    Query(q): Query<ExecutionLogQuery>,
) -> Json<serde_json::Value> {
    let limit = q.limit.unwrap_or(50).min(200);
    let entries = state.program_store.get_execution_log(&id, limit).await;
    Json(serde_json::to_value(entries).unwrap_or_default())
}
