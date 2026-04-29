use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;

use crate::api::auth::{require_permission, AuthUser};
use crate::api::error::ApiError;
use crate::api::ApiState;
use crate::auth::Permission;
use crate::fdd::model::{FddCategory, FddCondition, FddHistoryQuery, FddSeverity};
use crate::store::audit_store::{AuditAction, AuditEntryBuilder};

// ----------------------------------------------------------------
// Request / response types
// ----------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateRuleRequest {
    pub name: String,
    pub description: String,
    pub category: String,
    pub equip_tags: Vec<String>,
    pub severity: String,
    pub condition: serde_json::Value,
    pub guidance: String,
    #[serde(default = "default_confirmation")]
    pub confirmation_count: u16,
}

fn default_confirmation() -> u16 {
    3
}

#[derive(Deserialize)]
pub struct UpdateRuleRequest {
    pub name: String,
    pub description: String,
    pub category: String,
    pub equip_tags: Vec<String>,
    pub severity: String,
    pub condition: serde_json::Value,
    pub guidance: String,
    pub enabled: bool,
    pub confirmation_count: u16,
}

#[derive(Deserialize)]
pub struct CreateBindingRequest {
    pub rule_id: i64,
    pub equip_id: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub config_overrides: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Deserialize)]
pub struct UpdateBindingRequest {
    pub enabled: bool,
    pub config_overrides: Option<String>,
}

#[derive(Deserialize)]
pub struct BindingQuery {
    pub equip_id: Option<String>,
    pub rule_id: Option<i64>,
}

#[derive(Deserialize)]
pub struct HistoryQuery {
    pub equip_id: Option<String>,
    pub rule_id: Option<i64>,
    pub severity: Option<String>,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub cursor: Option<i64>,
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 {
    200
}

// ----------------------------------------------------------------
// Rules
// ----------------------------------------------------------------

/// GET /api/fdd/rules
pub async fn list_rules(
    State(state): State<ApiState>,
    auth: AuthUser,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageFdd, &perms)?;
    let rules = state.fdd_store.list_rules().await;
    let values: Vec<serde_json::Value> = rules
        .into_iter()
        .filter_map(|r| serde_json::to_value(r).ok())
        .collect();
    Ok(Json(values))
}

/// POST /api/fdd/rules
pub async fn create_rule(
    State(state): State<ApiState>,
    auth: AuthUser,
    Json(req): Json<CreateRuleRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageFdd, &perms)?;

    let category = FddCategory::from_key(&req.category)
        .ok_or_else(|| ApiError::BadRequest(format!("invalid category: {}", req.category)))?;
    let severity = FddSeverity::from_key(&req.severity)
        .ok_or_else(|| ApiError::BadRequest(format!("invalid severity: {}", req.severity)))?;
    let condition: FddCondition = serde_json::from_value(req.condition)
        .map_err(|e| ApiError::BadRequest(format!("invalid condition: {e}")))?;

    let id = state
        .fdd_store
        .create_rule(
            &req.name,
            &req.description,
            &category,
            &req.equip_tags,
            &severity,
            &condition,
            &req.guidance,
            false,
            None,
            true,
            req.confirmation_count,
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder = AuditEntryBuilder::new(AuditAction::CreateFddRule, "fdd_rule")
        .resource_id(&id.to_string())
        .details(&format!("name={}", req.name));
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true, "id": id})))
}

/// GET /api/fdd/rules/:id
pub async fn get_rule(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageFdd, &perms)?;
    let rule = state
        .fdd_store
        .get_rule(id)
        .await
        .ok_or(ApiError::NotFound("FDD rule not found".into()))?;
    Ok(Json(serde_json::to_value(rule).unwrap()))
}

/// PUT /api/fdd/rules/:id
pub async fn update_rule(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<UpdateRuleRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageFdd, &perms)?;

    let category = FddCategory::from_key(&req.category)
        .ok_or_else(|| ApiError::BadRequest(format!("invalid category: {}", req.category)))?;
    let severity = FddSeverity::from_key(&req.severity)
        .ok_or_else(|| ApiError::BadRequest(format!("invalid severity: {}", req.severity)))?;
    let condition: FddCondition = serde_json::from_value(req.condition)
        .map_err(|e| ApiError::BadRequest(format!("invalid condition: {e}")))?;

    state
        .fdd_store
        .update_rule(
            id,
            &req.name,
            &req.description,
            &category,
            &req.equip_tags,
            &severity,
            &condition,
            &req.guidance,
            req.enabled,
            req.confirmation_count,
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder =
        AuditEntryBuilder::new(AuditAction::UpdateFddRule, "fdd_rule").resource_id(&id.to_string());
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

/// DELETE /api/fdd/rules/:id
pub async fn delete_rule(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageFdd, &perms)?;

    // Reject deletion of builtin rules
    let rule = state
        .fdd_store
        .get_rule(id)
        .await
        .ok_or(ApiError::NotFound("FDD rule not found".into()))?;
    if rule.builtin {
        return Err(ApiError::BadRequest("cannot delete a built-in rule".into()));
    }

    state
        .fdd_store
        .delete_rule(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder =
        AuditEntryBuilder::new(AuditAction::DeleteFddRule, "fdd_rule").resource_id(&id.to_string());
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

// ----------------------------------------------------------------
// Bindings
// ----------------------------------------------------------------

/// GET /api/fdd/bindings
pub async fn list_bindings(
    State(state): State<ApiState>,
    auth: AuthUser,
    Query(q): Query<BindingQuery>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageFdd, &perms)?;
    let bindings = state
        .fdd_store
        .list_bindings(q.equip_id.as_deref(), q.rule_id)
        .await;
    let values: Vec<serde_json::Value> = bindings
        .into_iter()
        .filter_map(|b| serde_json::to_value(b).ok())
        .collect();
    Ok(Json(values))
}

/// POST /api/fdd/bindings
pub async fn create_binding(
    State(state): State<ApiState>,
    auth: AuthUser,
    Json(req): Json<CreateBindingRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageFdd, &perms)?;

    let id = state
        .fdd_store
        .create_binding(
            req.rule_id,
            &req.equip_id,
            req.enabled,
            req.config_overrides.as_deref(),
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder = AuditEntryBuilder::new(AuditAction::CreateFddBinding, "fdd_binding")
        .resource_id(&id.to_string())
        .details(&format!(
            "rule_id={}, equip_id={}",
            req.rule_id, req.equip_id
        ));
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true, "id": id})))
}

/// PUT /api/fdd/bindings/:id
pub async fn update_binding(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<UpdateBindingRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageFdd, &perms)?;

    state
        .fdd_store
        .update_binding(id, req.enabled, req.config_overrides.as_deref())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(serde_json::json!({"ok": true})))
}

/// DELETE /api/fdd/bindings/:id
pub async fn delete_binding(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageFdd, &perms)?;

    state
        .fdd_store
        .delete_binding(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder = AuditEntryBuilder::new(AuditAction::DeleteFddBinding, "fdd_binding")
        .resource_id(&id.to_string());
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

/// POST /api/fdd/bindings/auto
pub async fn auto_bind(
    State(state): State<ApiState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageFdd, &perms)?;

    // 1. List all enabled rules
    let all_rules = state.fdd_store.list_rules().await;
    let enabled_rules: Vec<_> = all_rules.into_iter().filter(|r| r.enabled).collect();

    // 2. List all equipment nodes
    let equip_nodes = state.node_store.list_nodes(Some("equip"), None).await;

    let mut created = 0u32;

    for equip in &equip_nodes {
        let equip_tag_names: Vec<&str> = equip.tags.keys().map(|k| k.as_str()).collect();

        for rule in &enabled_rules {
            // 3. Check if equipment tags are a superset of rule.equip_tags
            let matches = rule
                .equip_tags
                .iter()
                .all(|rt| equip_tag_names.contains(&rt.as_str()));
            if !matches {
                continue;
            }

            // 4. Check if binding already exists
            let existing = state
                .fdd_store
                .list_bindings(Some(&equip.id), Some(rule.id))
                .await;
            if !existing.is_empty() {
                continue;
            }

            // 5. Create binding
            let _ = state
                .fdd_store
                .create_binding(rule.id, &equip.id, true, None)
                .await;
            created += 1;
        }
    }

    Ok(Json(serde_json::json!({"created": created})))
}

// ----------------------------------------------------------------
// Faults
// ----------------------------------------------------------------

/// GET /api/fdd/faults/active
pub async fn active_faults(
    State(state): State<ApiState>,
    auth: AuthUser,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageFdd, &perms)?;
    let faults = state.fdd_store.get_active_faults().await;
    let values: Vec<serde_json::Value> = faults
        .into_iter()
        .filter_map(|f| serde_json::to_value(f).ok())
        .collect();
    Ok(Json(values))
}

/// POST /api/fdd/faults/:id/ack
pub async fn acknowledge_fault(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageFdd, &perms)?;

    state
        .fdd_store
        .acknowledge_fault(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder = AuditEntryBuilder::new(AuditAction::AcknowledgeFddFault, "fdd_fault")
        .resource_id(&id.to_string());
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

/// POST /api/fdd/faults/ack-all
pub async fn acknowledge_all(
    State(state): State<ApiState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageFdd, &perms)?;

    let count = state.fdd_store.acknowledge_all().await;

    let builder = AuditEntryBuilder::new(AuditAction::AcknowledgeFddFault, "fdd_fault")
        .details(&format!("acknowledged {} faults", count));
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true, "count": count})))
}

/// GET /api/fdd/history
pub async fn fault_history(
    State(state): State<ApiState>,
    auth: AuthUser,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageFdd, &perms)?;

    let cursor_mode = q.cursor.is_some();
    let query = FddHistoryQuery {
        equip_id: q.equip_id,
        rule_id: q.rule_id,
        severity: q.severity,
        start_ms: q.cursor.or(q.from).or(q.start_ms),
        end_ms: q.to.or(q.end_ms),
        limit: Some(q.limit),
    };

    let events = state.fdd_store.query_history(query).await;
    let next_cursor = events
        .last()
        .map(|event| event.timestamp_ms.saturating_add(1).to_string());
    let values: Vec<serde_json::Value> = events
        .into_iter()
        .filter_map(|e| serde_json::to_value(e).ok())
        .collect();
    if cursor_mode {
        Ok(Json(serde_json::json!({
            "items": values,
            "limit": q.limit,
            "next_cursor": next_cursor
        })))
    } else {
        Ok(Json(serde_json::to_value(values).unwrap_or_default()))
    }
}
