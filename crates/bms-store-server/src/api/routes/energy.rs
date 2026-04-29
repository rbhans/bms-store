use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::api::auth::{require_permission, AuthUser};
use crate::api::error::ApiError;
use crate::api::ApiState;
use crate::auth::Permission;
use crate::store::audit_store::{AuditAction, AuditEntryBuilder};

// ----------------------------------------------------------------
// Meters
// ----------------------------------------------------------------

/// GET /api/energy/meters
pub async fn list_meters(
    State(state): State<ApiState>,
    auth: AuthUser,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageEnergy, &perms)?;
    let meters = state.energy_store.list_meters().await;
    let values: Vec<serde_json::Value> = meters
        .into_iter()
        .filter_map(|m| serde_json::to_value(m).ok())
        .collect();
    Ok(Json(values))
}

#[derive(Deserialize)]
pub struct CreateMeterRequest {
    pub name: String,
    pub node_id: String,
    #[serde(default)]
    pub energy_node_id: Option<String>,
    #[serde(default)]
    pub utility_rate_id: Option<i64>,
    #[serde(default = "default_electric")]
    pub meter_type: String,
    #[serde(default = "default_kw")]
    pub unit: String,
}

fn default_electric() -> String {
    "electric".into()
}
fn default_kw() -> String {
    "kW".into()
}

/// POST /api/energy/meters
pub async fn create_meter(
    State(state): State<ApiState>,
    auth: AuthUser,
    Json(req): Json<CreateMeterRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageEnergy, &perms)?;

    let id = state
        .energy_store
        .create_meter(
            &req.name,
            &req.node_id,
            req.energy_node_id.as_deref(),
            req.utility_rate_id,
            &req.meter_type,
            &req.unit,
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder = AuditEntryBuilder::new(AuditAction::CreateEnergyMeter, "energy_meter")
        .resource_id(&id.to_string());
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true, "id": id})))
}

/// GET /api/energy/meters/:id
pub async fn get_meter(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageEnergy, &perms)?;
    let meter = state
        .energy_store
        .get_meter(id)
        .await
        .ok_or(ApiError::NotFound("Meter not found".into()))?;
    Ok(Json(serde_json::to_value(meter).unwrap()))
}

#[derive(Deserialize)]
pub struct UpdateMeterRequest {
    pub name: String,
    pub node_id: String,
    #[serde(default)]
    pub energy_node_id: Option<String>,
    #[serde(default)]
    pub utility_rate_id: Option<i64>,
    #[serde(default = "default_electric")]
    pub meter_type: String,
    #[serde(default = "default_kw")]
    pub unit: String,
}

/// PUT /api/energy/meters/:id
pub async fn update_meter(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<UpdateMeterRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageEnergy, &perms)?;

    state
        .energy_store
        .update_meter(
            id,
            &req.name,
            &req.node_id,
            req.energy_node_id.as_deref(),
            req.utility_rate_id,
            &req.meter_type,
            &req.unit,
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder = AuditEntryBuilder::new(AuditAction::UpdateEnergyMeter, "energy_meter")
        .resource_id(&id.to_string());
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

/// DELETE /api/energy/meters/:id
pub async fn delete_meter(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageEnergy, &perms)?;

    state
        .energy_store
        .delete_meter(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder = AuditEntryBuilder::new(AuditAction::DeleteEnergyMeter, "energy_meter")
        .resource_id(&id.to_string());
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

// ----------------------------------------------------------------
// Rates
// ----------------------------------------------------------------

/// GET /api/energy/rates
pub async fn list_rates(
    State(state): State<ApiState>,
    auth: AuthUser,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageEnergy, &perms)?;
    let rates = state.energy_store.list_rates().await;
    let values: Vec<serde_json::Value> = rates
        .into_iter()
        .filter_map(|r| serde_json::to_value(r).ok())
        .collect();
    Ok(Json(values))
}

#[derive(Deserialize)]
pub struct CreateRateRequest {
    pub name: String,
    pub rate_type: String,
    pub config: serde_json::Value,
    #[serde(default = "default_usd")]
    pub currency: String,
}

fn default_usd() -> String {
    "USD".into()
}

/// POST /api/energy/rates
pub async fn create_rate(
    State(state): State<ApiState>,
    auth: AuthUser,
    Json(req): Json<CreateRateRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageEnergy, &perms)?;

    let config_str = serde_json::to_string(&req.config).unwrap_or_default();
    let id = state
        .energy_store
        .create_rate(&req.name, &req.rate_type, &config_str, &req.currency)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder = AuditEntryBuilder::new(AuditAction::CreateUtilityRate, "utility_rate")
        .resource_id(&id.to_string());
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true, "id": id})))
}

/// PUT /api/energy/rates/:id
pub async fn update_rate(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<CreateRateRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageEnergy, &perms)?;

    let config_str = serde_json::to_string(&req.config).unwrap_or_default();
    state
        .energy_store
        .update_rate(id, &req.name, &req.rate_type, &config_str, &req.currency)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder = AuditEntryBuilder::new(AuditAction::UpdateUtilityRate, "utility_rate")
        .resource_id(&id.to_string());
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

/// DELETE /api/energy/rates/:id
pub async fn delete_rate(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageEnergy, &perms)?;

    state
        .energy_store
        .delete_rate(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder = AuditEntryBuilder::new(AuditAction::DeleteUtilityRate, "utility_rate")
        .resource_id(&id.to_string());
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

// ----------------------------------------------------------------
// Baselines
// ----------------------------------------------------------------

/// GET /api/energy/baselines?meter_id=
pub async fn list_baselines(
    State(state): State<ApiState>,
    auth: AuthUser,
    Query(q): Query<BaselineMeterQuery>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageEnergy, &perms)?;
    let baselines = state.energy_store.list_baselines(q.meter_id).await;
    let values: Vec<serde_json::Value> = baselines
        .into_iter()
        .filter_map(|b| serde_json::to_value(b).ok())
        .collect();
    Ok(Json(values))
}

#[derive(Deserialize)]
pub struct BaselineMeterQuery {
    pub meter_id: i64,
}

#[derive(Deserialize)]
pub struct CreateBaselineRequest {
    pub meter_id: i64,
    pub name: String,
    pub baseline_type: String,
    #[serde(default = "default_empty_config")]
    pub config: String,
    pub start_ms: i64,
    pub end_ms: i64,
}

fn default_empty_config() -> String {
    "{}".into()
}

/// POST /api/energy/baselines
pub async fn create_baseline(
    State(state): State<ApiState>,
    auth: AuthUser,
    Json(req): Json<CreateBaselineRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageEnergy, &perms)?;

    let id = state
        .energy_store
        .create_baseline(
            req.meter_id,
            &req.name,
            &req.baseline_type,
            &req.config,
            req.start_ms,
            req.end_ms,
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder = AuditEntryBuilder::new(AuditAction::CreateBaseline, "energy_baseline")
        .resource_id(&id.to_string());
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true, "id": id})))
}

/// DELETE /api/energy/baselines/:id
pub async fn delete_baseline(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageEnergy, &perms)?;

    state
        .energy_store
        .delete_baseline(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder = AuditEntryBuilder::new(AuditAction::DeleteBaseline, "energy_baseline")
        .resource_id(&id.to_string());
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

// ----------------------------------------------------------------
// Summary & Rollups
// ----------------------------------------------------------------

#[derive(Deserialize)]
pub struct SummaryQuery {
    #[serde(default)]
    pub meter_id: Option<i64>,
    #[serde(default = "default_range")]
    pub range: String,
}

fn default_range() -> String {
    "7d".into()
}

/// GET /api/energy/summary
pub async fn get_summary(
    State(state): State<ApiState>,
    auth: AuthUser,
    Query(q): Query<SummaryQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageEnergy, &perms)?;

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let today_start = crate::energy::consumption::day_start_ms(now_ms);

    let days = match q.range.as_str() {
        "30d" => 30,
        "90d" => 90,
        "1y" => 365,
        _ => 7,
    };
    let start_ms = today_start - days * 86_400_000;
    let end_ms = today_start + 86_400_000;

    let meters = state.energy_store.list_meters().await;
    let target_meters: Vec<_> = match q.meter_id {
        Some(mid) => meters.into_iter().filter(|m| m.id == mid).collect(),
        None => meters,
    };

    let mut all_rollups = Vec::new();
    for meter in &target_meters {
        let rollups = state
            .energy_store
            .query_rollups(meter.id, "daily", start_ms, end_ms)
            .await;
        all_rollups.extend(rollups);
    }

    let total_kwh: f64 = all_rollups.iter().map(|r| r.consumption_kwh).sum();
    let total_cost: f64 = all_rollups.iter().map(|r| r.cost).sum();
    let peak_demand = all_rollups
        .iter()
        .map(|r| r.peak_demand_kw)
        .fold(0.0f64, f64::max);
    let total_hours = all_rollups.len() as f64 * 24.0;
    let load_factor = if peak_demand > 0.0 && total_hours > 0.0 {
        (total_kwh / total_hours) / peak_demand
    } else {
        0.0
    };

    Ok(Json(serde_json::json!({
        "range": q.range,
        "meters": target_meters.len(),
        "days": all_rollups.len(),
        "total_kwh": total_kwh,
        "total_cost": total_cost,
        "peak_demand_kw": peak_demand,
        "load_factor": load_factor,
    })))
}

#[derive(Deserialize)]
pub struct RollupQuery {
    pub meter_id: i64,
    pub start_ms: i64,
    pub end_ms: i64,
    #[serde(default = "default_daily")]
    pub period: String,
}

fn default_daily() -> String {
    "daily".into()
}

/// GET /api/energy/consumption
pub async fn get_consumption(
    State(state): State<ApiState>,
    auth: AuthUser,
    Query(q): Query<RollupQuery>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageEnergy, &perms)?;

    let rollups = state
        .energy_store
        .query_rollups(q.meter_id, &q.period, q.start_ms, q.end_ms)
        .await;

    let values: Vec<serde_json::Value> = rollups
        .into_iter()
        .filter_map(|r| serde_json::to_value(r).ok())
        .collect();

    Ok(Json(values))
}

/// GET /api/energy/export
pub async fn export_csv(
    State(state): State<ApiState>,
    auth: AuthUser,
    Query(q): Query<RollupQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageEnergy, &perms)?;

    let rollups = state
        .energy_store
        .query_rollups(q.meter_id, &q.period, q.start_ms, q.end_ms)
        .await;

    let mut csv =
        String::from("period_start_ms,consumption_kwh,peak_demand_kw,avg_kw,cost,hdd,cdd\n");
    for r in &rollups {
        csv.push_str(&format!(
            "{},{:.2},{:.2},{:.3},{:.2},{:.1},{:.1}\n",
            r.period_start_ms, r.consumption_kwh, r.peak_demand_kw, r.avg_kw, r.cost, r.hdd, r.cdd
        ));
    }

    Ok((
        [
            (axum::http::header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                axum::http::header::CONTENT_DISPOSITION,
                "attachment; filename=\"energy_export.csv\"",
            ),
        ],
        csv,
    ))
}
