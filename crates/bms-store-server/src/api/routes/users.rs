use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::auth::{require_permission, AuthUser};
use crate::api::error::ApiError;
use crate::api::pagination::{
    validate_password, validate_string, PaginatedResponse, PaginationParams,
};
use crate::api::ApiState;
use crate::auth::Permission;
use crate::store::audit_store::{AuditAction, AuditEntryBuilder};
use crate::store::user_store::{User, UserRole};

#[derive(Serialize)]
pub struct UserResponse {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub role: String,
    pub created_ms: i64,
    pub last_login_ms: Option<i64>,
    pub disabled: bool,
}

fn user_to_response(u: User) -> UserResponse {
    UserResponse {
        id: u.id,
        username: u.username,
        display_name: u.display_name,
        role: u.role.label().to_lowercase(),
        created_ms: u.created_ms,
        last_login_ms: u.last_login_ms,
        disabled: u.disabled,
    }
}

#[derive(Deserialize)]
pub struct ListUsersQuery {
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

/// GET /api/users
pub async fn list_users(
    State(state): State<ApiState>,
    auth: AuthUser,
    Query(q): Query<ListUsersQuery>,
) -> Result<Json<PaginatedResponse<UserResponse>>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageUsers, &perms)?;

    let users = state.user_store.list_users().await;
    let all: Vec<UserResponse> = users.into_iter().map(user_to_response).collect();
    Ok(Json(PaginatedResponse::from_vec(all, &q.pagination)))
}

#[derive(Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub display_name: String,
    pub password: String,
    pub role: String,
}

fn parse_role(s: &str) -> Option<UserRole> {
    match s {
        "admin" => Some(UserRole::Admin),
        "operator" => Some(UserRole::Operator),
        "viewer" => Some(UserRole::Viewer),
        _ => None,
    }
}

/// POST /api/users
pub async fn create_user(
    State(state): State<ApiState>,
    auth: AuthUser,
    Json(req): Json<CreateUserRequest>,
) -> Result<Json<UserResponse>, ApiError> {
    validate_string("username", &req.username, 128)?;
    validate_string("display_name", &req.display_name, 256)?;
    validate_password(&req.password)?;

    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageUsers, &perms)?;

    let role = parse_role(&req.role).ok_or_else(|| ApiError::BadRequest("invalid role".into()))?;

    let password_hash =
        crate::auth::hash_password(&req.password).map_err(|e| ApiError::Internal(e.to_string()))?;

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    let user = User {
        id: uuid::Uuid::new_v4().to_string(),
        username: req.username,
        display_name: req.display_name,
        role,
        password_hash,
        created_ms: now_ms,
        last_login_ms: None,
        disabled: false,
    };

    let created = state.user_store.create_user(user).await?;

    let builder = AuditEntryBuilder::new(AuditAction::CreateUser, "user")
        .resource_id(&created.id)
        .details(&created.username);
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(user_to_response(created)))
}

#[derive(Deserialize)]
pub struct UpdateUserRequest {
    pub display_name: String,
    pub role: String,
    pub disabled: Option<bool>,
}

/// PUT /api/users/:id
pub async fn update_user(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateUserRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_string("display_name", &req.display_name, 256)?;

    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageUsers, &perms)?;

    let role = parse_role(&req.role).ok_or_else(|| ApiError::BadRequest("invalid role".into()))?;

    // Preserve existing disabled state if not explicitly provided
    let disabled = match req.disabled {
        Some(d) => d,
        None => {
            let existing = state
                .user_store
                .get_user(&id)
                .await
                .map_err(|_| ApiError::NotFound(format!("user {id} not found")))?;
            existing.disabled
        }
    };

    state
        .user_store
        .update_user(&id, &req.display_name, role, disabled)
        .await?;

    let builder = AuditEntryBuilder::new(AuditAction::UpdateUser, "user").resource_id(&id);
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

/// DELETE /api/users/:id
pub async fn delete_user(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageUsers, &perms)?;

    if id == auth.user_id {
        return Err(ApiError::BadRequest("cannot delete yourself".into()));
    }

    state.user_store.delete_user(&id).await?;

    let builder = AuditEntryBuilder::new(AuditAction::DeleteUser, "user").resource_id(&id);
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}
