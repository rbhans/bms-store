use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::api::auth::{require_permission, AuthUser};
use crate::api::error::ApiError;
use crate::api::ApiState;
use crate::auth::Permission;
use crate::reporting::templates::template_for_type;
use crate::store::audit_store::{AuditAction, AuditEntryBuilder};
use crate::store::report_store::{ReportConfig, ReportFrequency, ReportRecipient, ReportType};

// ----------------------------------------------------------------
// Report Definitions
// ----------------------------------------------------------------

/// GET /api/reports
pub async fn list_reports(
    State(state): State<ApiState>,
    auth: AuthUser,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageReports, &perms)?;
    let defs = state.report_store.list_definitions().await;
    let values: Vec<serde_json::Value> = defs
        .into_iter()
        .filter_map(|d| serde_json::to_value(d).ok())
        .collect();
    Ok(Json(values))
}

#[derive(Deserialize)]
pub struct CreateReportRequest {
    pub name: String,
    pub report_type: ReportType,
    #[serde(default)]
    pub config: Option<ReportConfig>,
}

/// POST /api/reports
pub async fn create_report(
    State(state): State<ApiState>,
    auth: AuthUser,
    Json(req): Json<CreateReportRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageReports, &perms)?;

    let config = req
        .config
        .unwrap_or_else(|| template_for_type(&req.report_type));

    let id = state
        .report_store
        .create_definition(&req.name, req.report_type, &config)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder =
        AuditEntryBuilder::new(AuditAction::CreateReport, "report").resource_id(&id.to_string());
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true, "id": id})))
}

/// GET /api/reports/:id
pub async fn get_report(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageReports, &perms)?;
    let def = state
        .report_store
        .get_definition(id)
        .await
        .map_err(|e| ApiError::NotFound(e.to_string()))?;
    Ok(Json(serde_json::to_value(def).unwrap_or_default()))
}

#[derive(Deserialize)]
pub struct UpdateReportRequest {
    pub name: String,
    pub report_type: ReportType,
    pub config: ReportConfig,
}

/// PUT /api/reports/:id
pub async fn update_report(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<UpdateReportRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageReports, &perms)?;

    state
        .report_store
        .update_definition(id, &req.name, req.report_type, &req.config)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder =
        AuditEntryBuilder::new(AuditAction::UpdateReport, "report").resource_id(&id.to_string());
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

/// DELETE /api/reports/:id
pub async fn delete_report(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageReports, &perms)?;

    state
        .report_store
        .delete_definition(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder =
        AuditEntryBuilder::new(AuditAction::DeleteReport, "report").resource_id(&id.to_string());
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

// ----------------------------------------------------------------
// Report Schedules
// ----------------------------------------------------------------

/// GET /api/reports/:id/schedules
pub async fn list_schedules(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(report_id): Path<i64>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageReports, &perms)?;
    let scheds = state.report_store.list_schedules(Some(report_id)).await;
    let values: Vec<serde_json::Value> = scheds
        .into_iter()
        .filter_map(|s| serde_json::to_value(s).ok())
        .collect();
    Ok(Json(values))
}

#[derive(Deserialize)]
pub struct CreateScheduleRequest {
    pub frequency: ReportFrequency,
    #[serde(default)]
    pub day_of_week: Option<u8>,
    #[serde(default)]
    pub day_of_month: Option<u8>,
    #[serde(default = "default_hour")]
    pub hour: u8,
    #[serde(default)]
    pub minute: u8,
    #[serde(default)]
    pub timezone_offset_mins: i32,
    #[serde(default)]
    pub recipients: Vec<ReportRecipient>,
}

fn default_hour() -> u8 {
    6
}

/// POST /api/reports/:id/schedules
pub async fn create_schedule(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(report_id): Path<i64>,
    Json(req): Json<CreateScheduleRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageReports, &perms)?;

    let id = state
        .report_store
        .create_schedule(
            report_id,
            req.frequency,
            req.day_of_week,
            req.day_of_month,
            req.hour,
            req.minute,
            req.timezone_offset_mins,
            &req.recipients,
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder = AuditEntryBuilder::new(AuditAction::CreateReportSchedule, "report_schedule")
        .resource_id(&id.to_string());
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true, "id": id})))
}

#[derive(Deserialize)]
pub struct UpdateScheduleRequest {
    pub frequency: ReportFrequency,
    #[serde(default)]
    pub day_of_week: Option<u8>,
    #[serde(default)]
    pub day_of_month: Option<u8>,
    #[serde(default = "default_hour")]
    pub hour: u8,
    #[serde(default)]
    pub minute: u8,
    #[serde(default)]
    pub timezone_offset_mins: i32,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub recipients: Vec<ReportRecipient>,
}

fn default_enabled() -> bool {
    true
}

/// PUT /api/reports/schedules/:id
pub async fn update_schedule(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<UpdateScheduleRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageReports, &perms)?;

    state
        .report_store
        .update_schedule(
            id,
            req.frequency,
            req.day_of_week,
            req.day_of_month,
            req.hour,
            req.minute,
            req.timezone_offset_mins,
            req.enabled,
            &req.recipients,
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder = AuditEntryBuilder::new(AuditAction::UpdateReportSchedule, "report_schedule")
        .resource_id(&id.to_string());
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

/// DELETE /api/reports/schedules/:id
pub async fn delete_schedule(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageReports, &perms)?;

    state
        .report_store
        .delete_schedule(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder = AuditEntryBuilder::new(AuditAction::DeleteReportSchedule, "report_schedule")
        .resource_id(&id.to_string());
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

// ----------------------------------------------------------------
// Executions
// ----------------------------------------------------------------

/// POST /api/reports/:id/run — trigger immediate report generation
pub async fn run_report(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(report_id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageReports, &perms)?;

    let engine = crate::reporting::engine::ReportEngine::new(
        state.history_store.clone(),
        state.alarm_store.clone(),
        state.point_store.clone(),
        state.node_store.clone(),
    );

    let (exec_id, _status) = engine
        .run_report(&state.report_store, report_id, None, "manual")
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder = AuditEntryBuilder::new(AuditAction::RunReport, "report")
        .resource_id(&report_id.to_string());
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(
        serde_json::json!({"ok": true, "execution_id": exec_id}),
    ))
}

#[derive(Deserialize)]
pub struct ListExecutionsQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    100
}

/// GET /api/reports/:id/executions
pub async fn list_executions(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(report_id): Path<i64>,
    Query(q): Query<ListExecutionsQuery>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageReports, &perms)?;
    let execs = state
        .report_store
        .list_executions(Some(report_id), q.limit)
        .await;
    let values: Vec<serde_json::Value> = execs
        .into_iter()
        .filter_map(|e| serde_json::to_value(e).ok())
        .collect();
    Ok(Json(values))
}

/// GET /api/reports/executions/:id/html — download generated report HTML
pub async fn get_execution_html(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageReports, &perms)?;
    let exec = state
        .report_store
        .get_execution(id)
        .await
        .map_err(|e| ApiError::NotFound(e.to_string()))?;

    let html = exec
        .report_html
        .ok_or_else(|| ApiError::NotFound("Report has no HTML content".to_string()))?;

    Ok((
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "inline; filename=\"report.html\"",
            ),
        ],
        html,
    ))
}
