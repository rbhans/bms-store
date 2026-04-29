use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::auth::{require_permission, AuthUser};
use crate::api::error::ApiError;
use crate::api::ApiState;
use crate::auth::Permission;
use crate::cloud::{CloudBridgeConfig, CloudBridgeStatus};
use crate::store::audit_store::{AuditAction, AuditEntryBuilder};

// ----------------------------------------------------------------
// Request / response types
// ----------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateBridgeRequest {
    pub name: String,
    pub provider: String,
    pub config: serde_json::Value,
    #[serde(default = "default_true")]
    pub on_values: bool,
    #[serde(default = "default_true")]
    pub on_alarms: bool,
    #[serde(default = "default_true")]
    pub on_fdd: bool,
    #[serde(default)]
    pub on_device_status: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Deserialize)]
pub struct UpdateBridgeRequest {
    pub name: String,
    pub provider: String,
    pub config: serde_json::Value,
    pub enabled: bool,
    pub on_values: bool,
    pub on_alarms: bool,
    pub on_fdd: bool,
    pub on_device_status: bool,
}

#[derive(Serialize)]
pub struct BridgeResponse {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub config: serde_json::Value,
    pub enabled: bool,
    pub on_values: bool,
    pub on_alarms: bool,
    pub on_fdd: bool,
    pub on_device_status: bool,
    pub created_ms: i64,
    pub updated_ms: i64,
}

/// Fields in provider configs that contain secrets and must be redacted in API responses.
const REDACTED_FIELDS: &[&str] = &[
    "key",                   // Azure SAS key
    "key_pem_path",          // AWS private key path
    "key_path",              // Azure X.509 key path
    "cert_pem_path",         // AWS cert path
    "cert_path",             // Azure cert path
    "credentials_json_path", // GCP service account key
    "private_key",           // raw private key (if ever embedded)
];

const REDACTED: &str = "***REDACTED***";

/// Redact sensitive fields from a provider config JSON object.
fn redact_config(mut config: serde_json::Value) -> serde_json::Value {
    if let Some(obj) = config.as_object_mut() {
        for &field in REDACTED_FIELDS {
            if obj.contains_key(field) {
                obj.insert(field.to_string(), serde_json::json!(REDACTED));
            }
        }
        // Handle nested auth_method (Azure)
        if let Some(auth) = obj.get_mut("auth_method") {
            if let Some(auth_obj) = auth.as_object_mut() {
                for &field in REDACTED_FIELDS {
                    if auth_obj.contains_key(field) {
                        auth_obj.insert(field.to_string(), serde_json::json!(REDACTED));
                    }
                }
            }
        }
    }
    config
}

impl From<CloudBridgeConfig> for BridgeResponse {
    fn from(c: CloudBridgeConfig) -> Self {
        let config = serde_json::from_str(&c.config).unwrap_or(serde_json::json!({}));
        let config = redact_config(config);
        Self {
            id: c.id,
            name: c.name,
            provider: c.provider,
            config,
            enabled: c.enabled,
            on_values: c.on_values,
            on_alarms: c.on_alarms,
            on_fdd: c.on_fdd,
            on_device_status: c.on_device_status,
            created_ms: c.created_ms,
            updated_ms: c.updated_ms,
        }
    }
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub bridge_id: String,
    pub last_publish_ms: i64,
    pub messages_published: i64,
    pub last_error: Option<String>,
    pub state: String,
}

impl From<CloudBridgeStatus> for StatusResponse {
    fn from(s: CloudBridgeStatus) -> Self {
        Self {
            bridge_id: s.bridge_id,
            last_publish_ms: s.last_publish_ms,
            messages_published: s.messages_published,
            last_error: s.last_error,
            state: s.state,
        }
    }
}

// ----------------------------------------------------------------
// Handlers
// ----------------------------------------------------------------

/// GET /api/cloud/bridges
pub async fn list_bridges(
    State(state): State<ApiState>,
    user: AuthUser,
) -> Result<Json<Vec<BridgeResponse>>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&user, Permission::ManageCloud, &perms)?;
    let bridges = state.cloud_store.list_bridges().await;
    Ok(Json(bridges.into_iter().map(Into::into).collect()))
}

/// POST /api/cloud/bridges
pub async fn create_bridge(
    State(state): State<ApiState>,
    user: AuthUser,
    Json(body): Json<CreateBridgeRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&user, Permission::ManageCloud, &perms)?;

    let id = format!(
        "cloud-{}",
        uuid::Uuid::new_v4()
            .to_string()
            .split('-')
            .next()
            .unwrap_or("0")
    );
    let config_str =
        serde_json::to_string(&body.config).map_err(|e| ApiError::BadRequest(e.to_string()))?;

    state
        .cloud_store
        .create_bridge(
            &id,
            &body.name,
            &body.provider,
            &config_str,
            body.on_values,
            body.on_alarms,
            body.on_fdd,
            body.on_device_status,
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder = AuditEntryBuilder::new(AuditAction::CreateCloudBridge, "cloud_bridge")
        .details(&format!("Created {} bridge '{}'", body.provider, body.name));
    let _ = state
        .audit_store
        .log_action(&user.user_id, &user.username, builder)
        .await;

    Ok(Json(serde_json::json!({ "id": id })))
}

/// GET /api/cloud/bridges/:id
pub async fn get_bridge(
    State(state): State<ApiState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<BridgeResponse>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&user, Permission::ManageCloud, &perms)?;
    let bridge = state
        .cloud_store
        .get_bridge(&id)
        .await
        .ok_or_else(|| ApiError::NotFound("bridge not found".into()))?;
    Ok(Json(bridge.into()))
}

/// PUT /api/cloud/bridges/:id
pub async fn update_bridge(
    State(state): State<ApiState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(body): Json<UpdateBridgeRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&user, Permission::ManageCloud, &perms)?;

    let config_str =
        serde_json::to_string(&body.config).map_err(|e| ApiError::BadRequest(e.to_string()))?;

    state
        .cloud_store
        .update_bridge(
            &id,
            &body.name,
            &body.provider,
            &config_str,
            body.enabled,
            body.on_values,
            body.on_alarms,
            body.on_fdd,
            body.on_device_status,
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder = AuditEntryBuilder::new(AuditAction::UpdateCloudBridge, "cloud_bridge")
        .details(&format!("Updated bridge '{}'", body.name));
    let _ = state
        .audit_store
        .log_action(&user.user_id, &user.username, builder)
        .await;

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// DELETE /api/cloud/bridges/:id
pub async fn delete_bridge(
    State(state): State<ApiState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&user, Permission::ManageCloud, &perms)?;

    state
        .cloud_store
        .delete_bridge(&id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder = AuditEntryBuilder::new(AuditAction::DeleteCloudBridge, "cloud_bridge")
        .details(&format!("Deleted bridge '{}'", id));
    let _ = state
        .audit_store
        .log_action(&user.user_id, &user.username, builder)
        .await;

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// POST /api/cloud/bridges/:id/test
pub async fn test_bridge(
    State(state): State<ApiState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&user, Permission::ManageCloud, &perms)?;

    let bridge_config = state
        .cloud_store
        .get_bridge(&id)
        .await
        .ok_or_else(|| ApiError::NotFound("bridge not found".into()))?;

    let mut connector =
        crate::cloud::build_connector(&bridge_config.provider, &bridge_config.config)
            .map_err(|e| ApiError::BadRequest(format!("invalid provider or config: {e}")))?;

    if let Err(e) = connector.connect().await {
        return Ok(Json(
            serde_json::json!({ "ok": false, "message": format!("connect failed: {e}") }),
        ));
    }

    let result = connector.test_connection().await;
    connector.close().await;

    match result {
        Ok(()) => {
            let builder = AuditEntryBuilder::new(AuditAction::TestCloudBridge, "cloud_bridge")
                .details(&format!("Tested bridge '{}' — success", bridge_config.name));
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

/// GET /api/cloud/status
pub async fn list_statuses(
    State(state): State<ApiState>,
    user: AuthUser,
) -> Result<Json<Vec<StatusResponse>>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&user, Permission::ManageCloud, &perms)?;
    let statuses = state.cloud_store.list_statuses().await;
    Ok(Json(statuses.into_iter().map(Into::into).collect()))
}
