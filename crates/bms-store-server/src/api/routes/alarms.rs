use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::auth::{require_permission, AuthUser};
use crate::api::error::ApiError;
use crate::api::pagination::{PaginatedResponse, PaginationParams};
use crate::api::ApiState;
use crate::auth::Permission;
use crate::store::alarm_store::{ActiveAlarm, AlarmHistoryQuery, AlarmSeverity};
use crate::store::audit_store::{AuditAction, AuditEntryBuilder};

#[derive(Serialize)]
pub struct ActiveAlarmResponse {
    pub config_id: i64,
    pub device_id: String,
    pub point_id: String,
    pub alarm_type: String,
    pub severity: String,
    pub state: String,
    pub trigger_value: f64,
    pub trigger_time_ms: i64,
    pub ack_time_ms: Option<i64>,
}

fn severity_str(s: &AlarmSeverity) -> &'static str {
    match s {
        AlarmSeverity::Critical => "critical",
        AlarmSeverity::Warning => "warning",
        AlarmSeverity::Info => "info",
        AlarmSeverity::LifeSafety => "life_safety",
    }
}

fn alarm_to_response(a: ActiveAlarm) -> ActiveAlarmResponse {
    ActiveAlarmResponse {
        config_id: a.config_id,
        device_id: a.device_id,
        point_id: a.point_id,
        alarm_type: format!("{:?}", a.alarm_type),
        severity: severity_str(&a.severity).to_string(),
        state: format!("{:?}", a.state),
        trigger_value: a.trigger_value,
        trigger_time_ms: a.trigger_time_ms,
        ack_time_ms: a.ack_time_ms,
    }
}

#[derive(Deserialize)]
pub struct ActiveAlarmsQuery {
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

/// GET /api/alarms/active
pub async fn active_alarms(
    State(state): State<ApiState>,
    _auth: AuthUser,
    Query(q): Query<ActiveAlarmsQuery>,
) -> Json<PaginatedResponse<ActiveAlarmResponse>> {
    let alarms = state.alarm_store.get_active_alarms().await;
    let all: Vec<ActiveAlarmResponse> = alarms.into_iter().map(alarm_to_response).collect();
    Json(PaginatedResponse::from_vec(all, &q.pagination))
}

/// POST /api/alarms/:id/ack
pub async fn acknowledge_alarm(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(config_id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::AcknowledgeAlarms, &perms)?;

    state.alarm_store.acknowledge(config_id).await?;

    let builder = AuditEntryBuilder::new(AuditAction::AcknowledgeAlarm, "alarm")
        .resource_id(&config_id.to_string());
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

/// POST /api/alarms/ack-all
pub async fn acknowledge_all(
    State(state): State<ApiState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::AcknowledgeAlarms, &perms)?;

    let count = state.alarm_store.acknowledge_all().await?;

    let builder = AuditEntryBuilder::new(AuditAction::AcknowledgeAllAlarms, "alarm")
        .details(&format!("acknowledged {count} alarms"));
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true, "count": count})))
}

#[derive(Serialize)]
pub struct AlarmConfigResponse {
    pub id: i64,
    pub device_id: String,
    pub point_id: String,
    pub alarm_type: String,
    pub severity: String,
    pub enabled: bool,
}

#[derive(Deserialize)]
pub struct ListConfigsQuery {
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

/// GET /api/alarms/configs
pub async fn list_configs(
    State(state): State<ApiState>,
    _auth: AuthUser,
    Query(q): Query<ListConfigsQuery>,
) -> Json<PaginatedResponse<AlarmConfigResponse>> {
    let configs = state.alarm_store.list_configs().await;
    let all: Vec<AlarmConfigResponse> = configs
        .into_iter()
        .map(|c| AlarmConfigResponse {
            id: c.id,
            device_id: c.device_id,
            point_id: c.point_id,
            alarm_type: format!("{:?}", c.alarm_type),
            severity: severity_str(&c.severity).to_string(),
            enabled: c.enabled,
        })
        .collect();
    Json(PaginatedResponse::from_vec(all, &q.pagination))
}

#[derive(Deserialize)]
pub struct HistoryQueryParams {
    pub device_id: Option<String>,
    pub point_id: Option<String>,
    pub severity: Option<String>,
    pub from_state: Option<String>,
    pub to_state: Option<String>,
    pub search: Option<String>,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub limit: Option<i64>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

#[derive(Serialize)]
pub struct AlarmEventResponse {
    pub id: i64,
    pub config_id: i64,
    pub device_id: String,
    pub point_id: String,
    pub severity: String,
    pub from_state: String,
    pub to_state: String,
    pub value: f64,
    pub timestamp_ms: i64,
}

/// GET /api/alarms/history
pub async fn alarm_history(
    State(state): State<ApiState>,
    _auth: AuthUser,
    Query(q): Query<HistoryQueryParams>,
) -> Result<Json<PaginatedResponse<AlarmEventResponse>>, ApiError> {
    let severity = q.severity.as_deref().and_then(|s| match s {
        "critical" => Some(AlarmSeverity::Critical),
        "warning" => Some(AlarmSeverity::Warning),
        "info" => Some(AlarmSeverity::Info),
        "life_safety" => Some(AlarmSeverity::LifeSafety),
        _ => None,
    });

    let query = AlarmHistoryQuery {
        device_id: q.device_id,
        point_id: q.point_id,
        severity,
        from_state: q.from_state,
        to_state: q.to_state,
        search: q.search,
        start_ms: q.from.or(q.start_ms),
        end_ms: q.to.or(q.end_ms),
        limit: q.limit,
    };

    let events = state.alarm_store.query_history(query).await?;
    let all: Vec<AlarmEventResponse> = events
        .into_iter()
        .map(|e| AlarmEventResponse {
            id: e.id,
            config_id: e.config_id,
            device_id: e.device_id,
            point_id: e.point_id,
            severity: severity_str(&e.severity).to_string(),
            from_state: e.from_state,
            to_state: e.to_state,
            value: e.value,
            timestamp_ms: e.timestamp_ms,
        })
        .collect();
    Ok(Json(PaginatedResponse::from_vec(all, &q.pagination)))
}

#[derive(Deserialize)]
pub struct HistoryExportParams {
    pub device_id: Option<String>,
    pub point_id: Option<String>,
    pub severity: Option<String>,
    pub from_state: Option<String>,
    pub to_state: Option<String>,
    pub search: Option<String>,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
}

/// Escape a field for CSV: wrap in quotes if it contains comma, quote, or newline.
fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Format epoch millis to ISO 8601 UTC datetime string.
fn format_datetime_utc(ms: i64) -> String {
    let secs = ms / 1000;
    // Simple UTC breakdown without external crate
    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;
    let hour = time_of_day / 3600;
    let min = (time_of_day % 3600) / 60;
    let sec = time_of_day % 60;

    // Civil date from days since 1970-01-01 (algorithm from Howard Hinnant)
    let z = days_since_epoch + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02}T{hour:02}:{min:02}:{sec:02}Z")
}

/// GET /api/alarms/history/export?format=csv
pub async fn alarm_history_export(
    State(state): State<ApiState>,
    _auth: AuthUser,
    Query(q): Query<HistoryExportParams>,
) -> Result<impl IntoResponse, ApiError> {
    let severity = q.severity.as_deref().and_then(|s| match s {
        "critical" => Some(AlarmSeverity::Critical),
        "warning" => Some(AlarmSeverity::Warning),
        "info" => Some(AlarmSeverity::Info),
        "life_safety" => Some(AlarmSeverity::LifeSafety),
        _ => None,
    });

    let query = AlarmHistoryQuery {
        device_id: q.device_id,
        point_id: q.point_id,
        severity,
        from_state: q.from_state,
        to_state: q.to_state,
        search: q.search,
        start_ms: q.start_ms,
        end_ms: q.end_ms,
        limit: None, // uncapped for export
    };

    let events = state.alarm_store.query_history(query).await?;

    let mut csv = String::from(
        "timestamp,datetime,device_id,point_id,severity,from_state,to_state,value,note\n",
    );
    for e in &events {
        let datetime = format_datetime_utc(e.timestamp_ms);
        let note = e.note.as_deref().unwrap_or("");
        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{:.2},{}\n",
            e.timestamp_ms,
            datetime,
            csv_field(&e.device_id),
            csv_field(&e.point_id),
            csv_field(e.severity.as_str()),
            csv_field(&e.from_state),
            csv_field(&e.to_state),
            e.value,
            csv_field(note),
        ));
    }

    let disposition = "attachment; filename=\"alarm-journal.csv\"".to_string();
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv".to_string()),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        csv,
    ))
}
