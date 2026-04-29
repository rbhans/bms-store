use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::api::auth::{require_permission, AuthUser};
use crate::api::error::ApiError;
use crate::api::ApiState;
use crate::auth::Permission;
use crate::backup::{BackupConfig, BackupInfo};

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub components: Vec<ComponentHealth>,
}

#[derive(Serialize)]
pub struct ComponentHealth {
    pub name: String,
    pub status: String,
}

/// GET /api/health — no auth required
pub async fn health(State(state): State<ApiState>) -> Json<HealthResponse> {
    let mut components = Vec::new();

    // Check HealthRegistry supervised tasks
    let snapshot = state.health.snapshot();
    let registry_healthy = state.health.is_healthy();
    for (name, status) in snapshot {
        components.push(ComponentHealth {
            name,
            status: status.to_string(),
        });
    }

    // Check node store (SQLite)
    let node_ok = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        state.node_store.list_nodes(None, None),
    )
    .await
    .is_ok();
    components.push(ComponentHealth {
        name: "node_store".into(),
        status: if node_ok { "healthy" } else { "down" }.into(),
    });

    // Check alarm store (SQLite)
    let alarm_ok = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        state.alarm_store.list_configs(),
    )
    .await
    .is_ok();
    components.push(ComponentHealth {
        name: "alarm_store".into(),
        status: if alarm_ok { "healthy" } else { "down" }.into(),
    });

    // Point store (in-memory, always available)
    components.push(ComponentHealth {
        name: "point_store".into(),
        status: "healthy".into(),
    });

    let all_healthy = registry_healthy && node_ok && alarm_ok;
    Json(HealthResponse {
        status: if all_healthy { "healthy" } else { "degraded" }.to_string(),
        components,
    })
}

#[derive(Serialize)]
pub struct SystemInfoResponse {
    pub version: String,
    pub point_count: usize,
    pub device_count: usize,
    pub scenario_name: String,
}

#[derive(Serialize)]
pub struct CapabilitiesResponse {
    pub version: String,
    pub bridges: Vec<String>,
    pub features: Vec<String>,
}

/// GET /api/system/info
pub async fn system_info(
    State(state): State<ApiState>,
    _auth: AuthUser,
) -> Json<SystemInfoResponse> {
    let point_count = state.point_store.point_count();
    let device_ids = state.point_store.device_ids();

    Json(SystemInfoResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        point_count,
        device_count: device_ids.len(),
        scenario_name: state.scenario_name.clone(),
    })
}

/// GET /api/system/capabilities
pub async fn capabilities(State(state): State<ApiState>) -> Json<CapabilitiesResponse> {
    let mut bridges = state.bridge_registry.bridge_protocol_ids();
    bridges.sort();

    #[cfg(not(feature = "cloud"))]
    let features = vec![
        "jwt-auth".to_string(),
        "api-keys".to_string(),
        "websocket-events".to_string(),
    ];
    #[cfg(feature = "cloud")]
    let features = vec![
        "jwt-auth".to_string(),
        "api-keys".to_string(),
        "websocket-events".to_string(),
        "cloud".to_string(),
    ];

    Json(CapabilitiesResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        bridges,
        features,
    })
}

/// POST /api/system/backup — trigger immediate backup
pub async fn trigger_backup(
    State(state): State<ApiState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageUsers, &perms)?; // admin-only

    let scheduler = state.backup_scheduler.lock().await;
    let path = scheduler.backup_now().map_err(ApiError::Internal)?;

    Ok(Json(serde_json::json!({
        "ok": true,
        "path": path.display().to_string()
    })))
}

/// GET /api/system/backups — list available backups
pub async fn list_backups(
    State(state): State<ApiState>,
    auth: AuthUser,
) -> Result<Json<Vec<BackupInfo>>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageUsers, &perms)?;

    let scheduler = state.backup_scheduler.lock().await;
    Ok(Json(scheduler.list_backups()))
}

/// GET /api/system/backup-config
pub async fn get_backup_config(
    State(state): State<ApiState>,
    auth: AuthUser,
) -> Result<Json<BackupConfig>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageUsers, &perms)?;

    let scheduler = state.backup_scheduler.lock().await;
    Ok(Json(scheduler.config().clone()))
}

/// PUT /api/system/backup-config
pub async fn set_backup_config(
    State(state): State<ApiState>,
    auth: AuthUser,
    Json(config): Json<BackupConfig>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageUsers, &perms)?;

    let mut scheduler = state.backup_scheduler.lock().await;
    scheduler.set_config(config);

    Ok(Json(serde_json::json!({"ok": true})))
}
