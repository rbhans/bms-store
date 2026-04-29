use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::http::HeaderMap;
use axum::Json;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::auth::{AllRolePermissions, Permission};
use crate::store::user_store::UserRole;

use super::error::ApiError;
use super::ApiState;

// ----------------------------------------------------------------
// JWT Claims
// ----------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // user_id
    pub username: String,
    pub role: String,
    pub exp: u64,
}

const TOKEN_EXPIRY_HOURS: u64 = 24;

pub fn create_token(
    user_id: &str,
    username: &str,
    role: &UserRole,
    secret: &str,
) -> Result<String, ApiError> {
    let exp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + TOKEN_EXPIRY_HOURS * 3600;

    let claims = Claims {
        sub: user_id.to_string(),
        username: username.to_string(),
        role: role.label().to_lowercase(),
        exp,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| ApiError::Internal(format!("token creation failed: {e}")))
}

pub fn validate_token(token: &str, secret: &str) -> Result<Claims, ApiError> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map(|data| data.claims)
    .map_err(|e| match e.kind() {
        jsonwebtoken::errors::ErrorKind::ExpiredSignature => ApiError::Unauthorized,
        _ => ApiError::Unauthorized,
    })
}

// ----------------------------------------------------------------
// AuthUser extractor
// ----------------------------------------------------------------

/// Authenticated user extracted from JWT in Authorization header.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: String,
    pub username: String,
    pub role: UserRole,
    pub is_api_key: bool,
}

fn parse_role(s: &str) -> UserRole {
    match s {
        "admin" => UserRole::Admin,
        "operator" => UserRole::Operator,
        _ => UserRole::Viewer,
    }
}

impl FromRequestParts<ApiState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &ApiState,
    ) -> Result<Self, Self::Rejection> {
        let token = extract_token(&parts.headers, parts.uri.query())
            .or_else(|| {
                // Also check query param for WebSocket connections
                parts.uri.query().and_then(|q| {
                    q.split('&')
                        .find_map(|pair| pair.strip_prefix("token="))
                        .map(|t| t.to_string())
                })
            })
            .ok_or(ApiError::Unauthorized)?;

        if token.starts_with("bsk_") {
            let principal = state.api_key_store.authenticate(&token).await?;
            return Ok(AuthUser {
                user_id: format!("api-key:{}", principal.id),
                username: principal.name,
                role: principal.role,
                is_api_key: true,
            });
        }

        let claims = validate_token(&token, &state.jwt_secret)?;

        // Verify user still exists and is not disabled
        let user = state
            .user_store
            .get_user(&claims.sub)
            .await
            .map_err(|_| ApiError::Unauthorized)?;
        if user.disabled {
            return Err(ApiError::Forbidden("user is disabled".into()));
        }

        Ok(AuthUser {
            user_id: claims.sub,
            username: claims.username,
            role: parse_role(&claims.role),
            is_api_key: false,
        })
    }
}

fn extract_token(headers: &HeaderMap, query: Option<&str>) -> Option<String> {
    extract_bearer(headers)
        .or_else(|| extract_api_key(headers))
        .or_else(|| query.and_then(extract_token_query))
}

fn extract_bearer(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

fn extract_api_key(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string())
}

fn extract_token_query(query: &str) -> Option<String> {
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix("token="))
        .map(|token| token.to_string())
}

// ----------------------------------------------------------------
// Permission check helper
// ----------------------------------------------------------------

pub fn require_permission(
    user: &AuthUser,
    perm: Permission,
    perms: &AllRolePermissions,
) -> Result<(), ApiError> {
    let role_perms = perms.for_role(&user.role);
    if role_perms.get(perm) {
        Ok(())
    } else {
        Err(ApiError::Forbidden(format!(
            "insufficient permissions: {} required",
            perm.label()
        )))
    }
}

// ----------------------------------------------------------------
// Login / refresh / me endpoints
// ----------------------------------------------------------------

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: UserInfo,
}

#[derive(Serialize)]
pub struct UserInfo {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub role: String,
}

pub async fn login(
    State(state): State<ApiState>,
    axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, ApiError> {
    // Rate limit: prefer x-forwarded-for (behind proxy), fall back to peer address
    let ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .and_then(|s| s.trim().parse::<std::net::IpAddr>().ok())
        .unwrap_or_else(|| peer.ip());
    state.login_rate_limiter.check(ip).await?;

    crate::api::pagination::validate_string("username", &req.username, 128)?;

    let user = state
        .user_store
        .authenticate(&req.username, &req.password)
        .await
        .map_err(|e| match e {
            crate::auth::AuthError::InvalidCredentials => ApiError::Unauthorized,
            crate::auth::AuthError::UserDisabled => ApiError::Forbidden("user is disabled".into()),
            other => ApiError::Internal(other.to_string()),
        })?;

    let token = create_token(&user.id, &user.username, &user.role, &state.jwt_secret)?;

    // Audit log
    let builder = crate::store::audit_store::AuditEntryBuilder::new(
        crate::store::audit_store::AuditAction::Login,
        "session",
    );
    let _ = state
        .audit_store
        .log_action(&user.id, &user.username, builder)
        .await;

    Ok(Json(LoginResponse {
        token,
        user: UserInfo {
            id: user.id,
            username: user.username,
            display_name: user.display_name,
            role: user.role.label().to_lowercase(),
        },
    }))
}

pub async fn refresh(
    State(state): State<ApiState>,
    auth: AuthUser,
) -> Result<Json<LoginResponse>, ApiError> {
    if auth.is_api_key {
        return Err(ApiError::Unauthorized);
    }

    // Re-fetch user to ensure they're still valid
    let user = state
        .user_store
        .get_user(&auth.user_id)
        .await
        .map_err(|_| ApiError::Unauthorized)?;

    if user.disabled {
        return Err(ApiError::Forbidden("user is disabled".into()));
    }

    let token = create_token(&user.id, &user.username, &user.role, &state.jwt_secret)?;

    Ok(Json(LoginResponse {
        token,
        user: UserInfo {
            id: user.id,
            username: user.username,
            display_name: user.display_name,
            role: user.role.label().to_lowercase(),
        },
    }))
}

// ----------------------------------------------------------------
// Initial setup (first admin user)
// ----------------------------------------------------------------

#[derive(Deserialize)]
pub struct SetupRequest {
    pub username: String,
    pub password: String,
    pub display_name: Option<String>,
}

/// POST /api/auth/setup — creates the first admin user.
/// Only works when no users exist yet. Returns 403 otherwise.
pub async fn setup(
    State(state): State<ApiState>,
    Json(req): Json<SetupRequest>,
) -> Result<Json<LoginResponse>, ApiError> {
    // Only allow if no users exist
    if state.user_store.has_any_users().await {
        return Err(ApiError::Forbidden("setup already completed".into()));
    }

    crate::api::pagination::validate_string("username", &req.username, 128)?;
    crate::api::pagination::validate_password(&req.password)?;

    let password_hash =
        crate::auth::hash_password(&req.password).map_err(|e| ApiError::Internal(e.to_string()))?;

    let now_ms = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    let user = crate::store::user_store::User {
        id: uuid::Uuid::new_v4().to_string(),
        username: req.username,
        display_name: req.display_name.unwrap_or_else(|| "Administrator".into()),
        role: crate::store::user_store::UserRole::Admin,
        password_hash,
        created_ms: now_ms,
        last_login_ms: None,
        disabled: false,
    };

    let created = state
        .user_store
        .create_user(user)
        .await
        .map_err(|e| ApiError::Internal(format!("failed to create user: {e}")))?;

    let token = create_token(
        &created.id,
        &created.username,
        &created.role,
        &state.jwt_secret,
    )?;

    Ok(Json(LoginResponse {
        token,
        user: UserInfo {
            id: created.id,
            username: created.username,
            display_name: created.display_name,
            role: created.role.label().to_lowercase(),
        },
    }))
}

pub async fn me(State(state): State<ApiState>, auth: AuthUser) -> Result<Json<UserInfo>, ApiError> {
    if auth.is_api_key {
        return Ok(Json(UserInfo {
            id: auth.user_id,
            username: auth.username,
            display_name: "API key".to_string(),
            role: auth.role.label().to_lowercase(),
        }));
    }

    let user = state
        .user_store
        .get_user(&auth.user_id)
        .await
        .map_err(|_| ApiError::Unauthorized)?;

    Ok(Json(UserInfo {
        id: user.id,
        username: user.username,
        display_name: user.display_name,
        role: user.role.label().to_lowercase(),
    }))
}

pub async fn list_api_keys(
    State(state): State<ApiState>,
    auth: AuthUser,
) -> Result<Json<Vec<crate::api::api_keys::ApiKeyInfo>>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageUsers, &perms)?;

    Ok(Json(state.api_key_store.list().await))
}

pub async fn create_api_key(
    State(state): State<ApiState>,
    auth: AuthUser,
    Json(req): Json<crate::api::api_keys::CreateApiKeyRequest>,
) -> Result<Json<crate::api::api_keys::CreatedApiKey>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageUsers, &perms)?;

    Ok(Json(state.api_key_store.create(req).await?))
}

pub async fn update_api_key(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<crate::api::api_keys::UpdateApiKeyRequest>,
) -> Result<Json<crate::api::api_keys::ApiKeyInfo>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageUsers, &perms)?;

    Ok(Json(state.api_key_store.update(&id, req).await?))
}

pub async fn delete_api_key(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageUsers, &perms)?;

    state.api_key_store.delete(&id).await?;
    Ok(Json(serde_json::json!({"ok": true})))
}
