use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;

use crate::api::auth::{require_permission, AuthUser};
use crate::api::error::ApiError;
use crate::api::ApiState;
use crate::auth::Permission;
use crate::store::audit_store::{AuditAction, AuditQuery};

#[derive(Deserialize)]
pub struct AuditQueryParams {
    pub user_id: Option<String>,
    pub action: Option<String>,
    pub resource_type: Option<String>,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub cursor: Option<i64>,
}

/// GET /api/audit
pub async fn query_audit(
    State(state): State<ApiState>,
    auth: AuthUser,
    Query(q): Query<AuditQueryParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ViewAudit, &perms)?;

    let cursor_mode = q.cursor.is_some();
    let offset = q.offset.or(q.cursor);
    let limit = q.limit.unwrap_or(100).min(1000);

    let query = AuditQuery {
        user_id: q.user_id,
        action: q.action.as_deref().and_then(AuditAction::from_str),
        resource_type: q.resource_type,
        start_ms: q.start_ms,
        end_ms: q.end_ms,
        limit: Some(limit),
        offset,
    };

    let entries = state
        .audit_store
        .query(query)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    if cursor_mode {
        let next_offset = offset.unwrap_or(0) + entries.len() as i64;
        let next_cursor = if entries.len() as i64 == limit {
            Some(next_offset.to_string())
        } else {
            None
        };
        Ok(Json(serde_json::json!({
            "items": entries,
            "limit": limit,
            "offset": offset.unwrap_or(0),
            "next_cursor": next_cursor
        })))
    } else {
        Ok(Json(serde_json::to_value(entries).unwrap_or_default()))
    }
}

/// GET /api/audit/count
pub async fn count_audit(
    State(state): State<ApiState>,
    auth: AuthUser,
    Query(q): Query<AuditQueryParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ViewAudit, &perms)?;

    let query = AuditQuery {
        user_id: q.user_id,
        action: q.action.as_deref().and_then(AuditAction::from_str),
        resource_type: q.resource_type,
        start_ms: q.start_ms,
        end_ms: q.end_ms,
        limit: None,
        offset: None,
    };

    let count = state
        .audit_store
        .count(query)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(serde_json::json!({"count": count})))
}
