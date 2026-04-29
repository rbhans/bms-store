use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::auth::{require_permission, AuthUser};
use crate::api::error::ApiError;
use crate::api::ApiState;
use crate::auth::Permission;
use crate::export::{ExportConnectorConfig, ExportStatus};
use crate::store::audit_store::{AuditAction, AuditEntryBuilder};

// ----------------------------------------------------------------
// Request / response types
// ----------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateConnectorRequest {
    pub name: String,
    pub connector_type: String,
    pub config: serde_json::Value,
    #[serde(default = "default_true")]
    pub on_values: bool,
    #[serde(default = "default_true")]
    pub on_alarms: bool,
    #[serde(default = "default_true")]
    pub on_fdd: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Deserialize)]
pub struct UpdateConnectorRequest {
    pub name: String,
    pub connector_type: String,
    pub config: serde_json::Value,
    pub enabled: bool,
    pub on_values: bool,
    pub on_alarms: bool,
    pub on_fdd: bool,
}

#[derive(Deserialize)]
pub struct BackfillRequest {
    pub start_ms: i64,
    pub end_ms: i64,
}

#[derive(Serialize)]
pub struct ConnectorResponse {
    pub id: String,
    pub name: String,
    pub connector_type: String,
    pub config: serde_json::Value,
    pub enabled: bool,
    pub on_values: bool,
    pub on_alarms: bool,
    pub on_fdd: bool,
    pub created_ms: i64,
    pub updated_ms: i64,
}

impl From<ExportConnectorConfig> for ConnectorResponse {
    fn from(c: ExportConnectorConfig) -> Self {
        let config = serde_json::from_str(&c.config).unwrap_or(serde_json::json!({}));
        Self {
            id: c.id,
            name: c.name,
            connector_type: c.connector_type,
            config,
            enabled: c.enabled,
            on_values: c.on_values,
            on_alarms: c.on_alarms,
            on_fdd: c.on_fdd,
            created_ms: c.created_ms,
            updated_ms: c.updated_ms,
        }
    }
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub connector_id: String,
    pub last_sync_ms: i64,
    pub rows_exported: i64,
    pub last_error: Option<String>,
    pub state: String,
}

impl From<ExportStatus> for StatusResponse {
    fn from(s: ExportStatus) -> Self {
        Self {
            connector_id: s.connector_id,
            last_sync_ms: s.last_sync_ms,
            rows_exported: s.rows_exported,
            last_error: s.last_error,
            state: s.state,
        }
    }
}

// ----------------------------------------------------------------
// Handlers
// ----------------------------------------------------------------

/// GET /api/export/connectors
pub async fn list_connectors(
    State(state): State<ApiState>,
    user: AuthUser,
) -> Result<Json<Vec<ConnectorResponse>>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&user, Permission::ManageExport, &perms)?;
    let connectors = state.export_store.list_connectors().await;
    Ok(Json(connectors.into_iter().map(Into::into).collect()))
}

/// POST /api/export/connectors
pub async fn create_connector(
    State(state): State<ApiState>,
    user: AuthUser,
    Json(body): Json<CreateConnectorRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&user, Permission::ManageExport, &perms)?;

    let id = format!(
        "exp-{}",
        uuid::Uuid::new_v4()
            .to_string()
            .split('-')
            .next()
            .unwrap_or("0")
    );
    let config_str =
        serde_json::to_string(&body.config).map_err(|e| ApiError::BadRequest(e.to_string()))?;

    state
        .export_store
        .create_connector(
            &id,
            &body.name,
            &body.connector_type,
            &config_str,
            body.on_values,
            body.on_alarms,
            body.on_fdd,
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder =
        AuditEntryBuilder::new(AuditAction::CreateExportConnector, "export_connector").details(
            &format!("Created {} connector '{}'", body.connector_type, body.name),
        );
    let _ = state
        .audit_store
        .log_action(&user.user_id, &user.username, builder)
        .await;

    Ok(Json(serde_json::json!({ "id": id })))
}

/// GET /api/export/connectors/:id
pub async fn get_connector(
    State(state): State<ApiState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<ConnectorResponse>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&user, Permission::ManageExport, &perms)?;
    let connector = state
        .export_store
        .get_connector(&id)
        .await
        .ok_or_else(|| ApiError::NotFound("connector not found".into()))?;
    Ok(Json(connector.into()))
}

/// PUT /api/export/connectors/:id
pub async fn update_connector(
    State(state): State<ApiState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(body): Json<UpdateConnectorRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&user, Permission::ManageExport, &perms)?;

    let config_str =
        serde_json::to_string(&body.config).map_err(|e| ApiError::BadRequest(e.to_string()))?;

    state
        .export_store
        .update_connector(
            &id,
            &body.name,
            &body.connector_type,
            &config_str,
            body.enabled,
            body.on_values,
            body.on_alarms,
            body.on_fdd,
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder = AuditEntryBuilder::new(AuditAction::UpdateExportConnector, "export_connector")
        .details(&format!("Updated connector '{}'", body.name));
    let _ = state
        .audit_store
        .log_action(&user.user_id, &user.username, builder)
        .await;

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// DELETE /api/export/connectors/:id
pub async fn delete_connector(
    State(state): State<ApiState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&user, Permission::ManageExport, &perms)?;

    state
        .export_store
        .delete_connector(&id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder = AuditEntryBuilder::new(AuditAction::DeleteExportConnector, "export_connector")
        .details(&format!("Deleted connector '{}'", id));
    let _ = state
        .audit_store
        .log_action(&user.user_id, &user.username, builder)
        .await;

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// POST /api/export/connectors/:id/test
pub async fn test_connector(
    State(state): State<ApiState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&user, Permission::ManageExport, &perms)?;

    let connector_config = state
        .export_store
        .get_connector(&id)
        .await
        .ok_or_else(|| ApiError::NotFound("connector not found".into()))?;

    let connector = crate::export::publisher::build_connector_from_config(&connector_config)
        .ok_or_else(|| {
            ApiError::BadRequest("unsupported connector type or invalid config".into())
        })?;

    match connector.test_connection().await {
        Ok(()) => {
            let builder =
                AuditEntryBuilder::new(AuditAction::TestExportConnector, "export_connector")
                    .details(&format!(
                        "Tested connector '{}' — success",
                        connector_config.name
                    ));
            let _ = state
                .audit_store
                .log_action(&user.user_id, &user.username, builder)
                .await;
            Ok(Json(
                serde_json::json!({ "ok": true, "message": "Connection successful" }),
            ))
        }
        Err(e) => Ok(Json(
            serde_json::json!({ "ok": false, "message": e.to_string() }),
        )),
    }
}

/// GET /api/export/status
pub async fn list_statuses(
    State(state): State<ApiState>,
    user: AuthUser,
) -> Result<Json<Vec<StatusResponse>>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&user, Permission::ManageExport, &perms)?;
    let statuses = state.export_store.list_statuses().await;
    Ok(Json(statuses.into_iter().map(Into::into).collect()))
}

/// POST /api/export/connectors/:id/backfill
pub async fn backfill_connector(
    State(state): State<ApiState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(body): Json<BackfillRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&user, Permission::ManageExport, &perms)?;

    let connector_config = state
        .export_store
        .get_connector(&id)
        .await
        .ok_or_else(|| ApiError::NotFound("connector not found".into()))?;

    // Check if a backfill is already running for this connector
    let current_status = state.export_store.get_status(&id).await;
    if let Some(ref status) = current_status {
        if status.state == "backfilling" {
            return Err(ApiError::BadRequest(format!(
                "Backfill already in progress for connector '{}'",
                connector_config.name
            )));
        }
    }

    let builder =
        AuditEntryBuilder::new(AuditAction::RunExportBackfill, "export_connector").details(
            &format!("Started backfill for connector '{}'", connector_config.name),
        );
    let _ = state
        .audit_store
        .log_action(&user.user_id, &user.username, builder)
        .await;

    // Mark as backfilling before spawning
    let _ = state
        .export_store
        .update_status(&id, now_ms(), 0, None, "backfilling")
        .await;

    // Spawn backfill as background task
    let export_store = state.export_store.clone();
    let history_store = state.history_store.clone();
    let point_store = state.point_store.clone();
    let cancel = tokio_util::sync::CancellationToken::new();

    tokio::spawn(async move {
        match crate::export::backfill::run_backfill(
            &connector_config,
            &export_store,
            &history_store,
            &point_store,
            body.start_ms,
            body.end_ms,
            cancel,
        )
        .await
        {
            Ok(rows) => {
                tracing::info!(connector = %id, rows, "Backfill completed");
            }
            Err(e) => {
                tracing::error!(connector = %id, error = %e, "Backfill failed");
            }
        }
    });

    Ok(Json(
        serde_json::json!({ "ok": true, "message": "Backfill started" }),
    ))
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
