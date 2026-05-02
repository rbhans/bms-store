//! Bridge-config CRUD endpoints. Reads/writes the SQLite-backed
//! [`BridgeStore`] so the GUI can register new BACnet networks and Modbus
//! buses at runtime instead of hand-editing `scenario.json` and restarting.
//!
//! Mutations require `Permission::ManageDiscovery` (same gate as the
//! discovery-scan endpoints) and emit a Toast event recommending a
//! restart for changes to take effect.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};

use bms_core::{Event, ToastLevel};
use bms_store_storage::auth::Permission;
use bms_store_storage::store::audit_store::{AuditAction, AuditEntryBuilder};
use bms_store_storage::store::bridge_store::{BridgeStoreError, StoredBacnetNetwork, StoredModbusBus};

use crate::api::auth::{require_permission, AuthUser};
use crate::api::error::ApiError;
use crate::api::ApiState;

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct BacnetNetworkRow {
    pub id: i64,
    pub name: String,
    pub config: serde_json::Value,
    pub enabled: bool,
    pub created_ms: i64,
    pub updated_ms: i64,
}

impl From<StoredBacnetNetwork> for BacnetNetworkRow {
    fn from(s: StoredBacnetNetwork) -> Self {
        let config = serde_json::from_str(&s.config_json).unwrap_or(serde_json::Value::Null);
        BacnetNetworkRow {
            id: s.id,
            name: s.name,
            config,
            enabled: s.enabled,
            created_ms: s.created_ms,
            updated_ms: s.updated_ms,
        }
    }
}

#[derive(Serialize)]
pub struct ModbusBusRow {
    pub id: i64,
    pub name: String,
    pub config: serde_json::Value,
    pub enabled: bool,
    pub created_ms: i64,
    pub updated_ms: i64,
}

impl From<StoredModbusBus> for ModbusBusRow {
    fn from(s: StoredModbusBus) -> Self {
        let config = serde_json::from_str(&s.config_json).unwrap_or(serde_json::Value::Null);
        ModbusBusRow {
            id: s.id,
            name: s.name,
            config,
            enabled: s.enabled,
            created_ms: s.created_ms,
            updated_ms: s.updated_ms,
        }
    }
}

#[derive(Deserialize)]
pub struct CreateBridgeRequest {
    pub name: String,
    pub config: serde_json::Value,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Deserialize)]
pub struct UpdateBridgeRequest {
    pub config: serde_json::Value,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

fn map_err(e: BridgeStoreError) -> ApiError {
    match e {
        BridgeStoreError::NotFound => ApiError::NotFound("bridge config not found".into()),
        BridgeStoreError::Duplicate => {
            ApiError::BadRequest("bridge with that name already exists".into())
        }
        other => ApiError::Internal(other.to_string()),
    }
}

fn restart_toast(state: &ApiState, source: &str, what: &str) {
    state.event_bus.publish(Event::toast(
        ToastLevel::Warn,
        source,
        format!("{what} — restart bms-store to activate"),
    ));
}

// ---------------------------------------------------------------------------
// BACnet networks
// ---------------------------------------------------------------------------

/// `GET /api/bridges/bacnet` — list registered BACnet networks.
pub async fn list_bacnet(
    State(state): State<ApiState>,
    _auth: AuthUser,
) -> Json<Vec<BacnetNetworkRow>> {
    let nets = state.bridge_store.list_bacnet_networks().await;
    Json(nets.into_iter().map(Into::into).collect())
}

/// `GET /api/bridges/bacnet/:id` — fetch one.
pub async fn get_bacnet(
    State(state): State<ApiState>,
    _auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<BacnetNetworkRow>, ApiError> {
    state
        .bridge_store
        .get_bacnet_network(id)
        .await
        .map(|s| Json(s.into()))
        .ok_or_else(|| ApiError::NotFound("bridge config not found".into()))
}

/// `POST /api/bridges/bacnet` — register a new BACnet network.
pub async fn create_bacnet(
    State(state): State<ApiState>,
    auth: AuthUser,
    Json(req): Json<CreateBridgeRequest>,
) -> Result<(StatusCode, Json<BacnetNetworkRow>), ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageDiscovery, &perms)?;

    let cfg_json = serde_json::to_string(&req.config)
        .map_err(|e| ApiError::BadRequest(format!("invalid config json: {e}")))?;
    let n = state
        .bridge_store
        .create_bacnet_network(&req.name, &cfg_json, req.enabled)
        .await
        .map_err(map_err)?;

    let _ = state
        .audit_store
        .log_action(
            &auth.user_id,
            &auth.username,
            AuditEntryBuilder::new(AuditAction::CreateBacnetNetwork, "bridge_bacnet")
                .resource_id(&n.id.to_string())
                .details(&n.name),
        )
        .await;

    restart_toast(&state, "bridges", &format!("BACnet network `{}` added", n.name));
    Ok((StatusCode::CREATED, Json(n.into())))
}

/// `PUT /api/bridges/bacnet/:id` — replace config + enabled flag.
pub async fn update_bacnet(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<UpdateBridgeRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageDiscovery, &perms)?;

    let cfg_json = serde_json::to_string(&req.config)
        .map_err(|e| ApiError::BadRequest(format!("invalid config json: {e}")))?;
    state
        .bridge_store
        .update_bacnet_network(id, &cfg_json, req.enabled)
        .await
        .map_err(map_err)?;

    restart_toast(&state, "bridges", "BACnet network updated");
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/bridges/bacnet/:id`.
pub async fn delete_bacnet(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageDiscovery, &perms)?;

    state
        .bridge_store
        .delete_bacnet_network(id)
        .await
        .map_err(map_err)?;

    restart_toast(&state, "bridges", "BACnet network removed");
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Modbus buses
// ---------------------------------------------------------------------------

/// `GET /api/bridges/modbus` — list registered Modbus buses.
pub async fn list_modbus(
    State(state): State<ApiState>,
    _auth: AuthUser,
) -> Json<Vec<ModbusBusRow>> {
    let buses = state.bridge_store.list_modbus_buses().await;
    Json(buses.into_iter().map(Into::into).collect())
}

/// `GET /api/bridges/modbus/:id`.
pub async fn get_modbus(
    State(state): State<ApiState>,
    _auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<ModbusBusRow>, ApiError> {
    state
        .bridge_store
        .get_modbus_bus(id)
        .await
        .map(|s| Json(s.into()))
        .ok_or_else(|| ApiError::NotFound("bridge config not found".into()))
}

/// `POST /api/bridges/modbus` — register a new Modbus bus (TCP host or RTU serial).
pub async fn create_modbus(
    State(state): State<ApiState>,
    auth: AuthUser,
    Json(req): Json<CreateBridgeRequest>,
) -> Result<(StatusCode, Json<ModbusBusRow>), ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageDiscovery, &perms)?;

    let cfg_json = serde_json::to_string(&req.config)
        .map_err(|e| ApiError::BadRequest(format!("invalid config json: {e}")))?;
    let b = state
        .bridge_store
        .create_modbus_bus(&req.name, &cfg_json, req.enabled)
        .await
        .map_err(map_err)?;

    let _ = state
        .audit_store
        .log_action(
            &auth.user_id,
            &auth.username,
            AuditEntryBuilder::new(AuditAction::CreateModbusBus, "bridge_modbus")
                .resource_id(&b.id.to_string())
                .details(&b.name),
        )
        .await;

    restart_toast(&state, "bridges", &format!("Modbus bus `{}` added", b.name));
    Ok((StatusCode::CREATED, Json(b.into())))
}

/// `PUT /api/bridges/modbus/:id`.
pub async fn update_modbus(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<UpdateBridgeRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageDiscovery, &perms)?;

    let cfg_json = serde_json::to_string(&req.config)
        .map_err(|e| ApiError::BadRequest(format!("invalid config json: {e}")))?;
    state
        .bridge_store
        .update_modbus_bus(id, &cfg_json, req.enabled)
        .await
        .map_err(map_err)?;

    restart_toast(&state, "bridges", "Modbus bus updated");
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/bridges/modbus/:id`.
pub async fn delete_modbus(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageDiscovery, &perms)?;

    state
        .bridge_store
        .delete_modbus_bus(id)
        .await
        .map_err(map_err)?;

    restart_toast(&state, "bridges", "Modbus bus removed");
    Ok(StatusCode::NO_CONTENT)
}
