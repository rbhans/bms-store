use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::auth::{require_permission, AuthUser};
use crate::api::error::ApiError;
use crate::api::ApiState;
use crate::auth::Permission;
use crate::store::audit_store::{AuditAction, AuditEntryBuilder};
use crate::webhook::model::{Provider, WebhookEndpoint};

// ----------------------------------------------------------------
// Request / response types
// ----------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateEndpointRequest {
    pub name: String,
    pub provider: Option<String>,
    pub url: String,
    pub headers: Option<String>,
    pub secret: Option<String>,
    #[serde(default = "default_true")]
    pub on_alarm_raised: bool,
    #[serde(default = "default_true")]
    pub on_alarm_cleared: bool,
    #[serde(default)]
    pub on_alarm_acknowledged: bool,
    #[serde(default = "default_true")]
    pub on_device_down: bool,
    #[serde(default = "default_true")]
    pub on_device_recovered: bool,
    #[serde(default = "default_true")]
    pub on_fdd_fault_raised: bool,
    #[serde(default = "default_true")]
    pub on_fdd_fault_cleared: bool,
    #[serde(default = "default_info")]
    pub min_severity: String,
    pub tag_filters: Option<String>,
}

fn default_true() -> bool {
    true
}
fn default_info() -> String {
    "info".into()
}

#[derive(Deserialize)]
pub struct UpdateEndpointRequest {
    pub name: String,
    pub provider: Option<String>,
    pub url: String,
    pub headers: Option<String>,
    pub secret: Option<String>,
    pub enabled: bool,
    pub on_alarm_raised: bool,
    pub on_alarm_cleared: bool,
    pub on_alarm_acknowledged: bool,
    pub on_device_down: bool,
    pub on_device_recovered: bool,
    pub on_fdd_fault_raised: bool,
    pub on_fdd_fault_cleared: bool,
    pub min_severity: String,
    pub tag_filters: Option<String>,
}

#[derive(Deserialize)]
pub struct DeliveryQuery {
    pub endpoint_id: Option<String>,
    pub status: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 {
    200
}

#[derive(Serialize)]
pub struct EndpointResponse {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub provider_label: String,
    pub url: String,
    /// True if custom headers are configured (contents redacted).
    pub has_headers: bool,
    /// True if a signing secret is configured (value redacted).
    pub has_secret: bool,
    pub enabled: bool,
    pub on_alarm_raised: bool,
    pub on_alarm_cleared: bool,
    pub on_alarm_acknowledged: bool,
    pub on_device_down: bool,
    pub on_device_recovered: bool,
    pub on_fdd_fault_raised: bool,
    pub on_fdd_fault_cleared: bool,
    pub min_severity: String,
    pub tag_filters: Option<String>,
    pub created_ms: i64,
    pub updated_ms: i64,
}

fn endpoint_to_response(ep: WebhookEndpoint) -> EndpointResponse {
    let provider_label = Provider::from_str(&ep.provider)
        .map(|p| p.label())
        .unwrap_or("Generic")
        .to_string();
    EndpointResponse {
        id: ep.id,
        name: ep.name,
        provider: ep.provider,
        provider_label,
        url: ep.url,
        has_headers: ep.headers.as_ref().is_some_and(|h| !h.is_empty()),
        has_secret: ep.secret.as_ref().is_some_and(|s| !s.is_empty()),
        enabled: ep.enabled,
        on_alarm_raised: ep.on_alarm_raised,
        on_alarm_cleared: ep.on_alarm_cleared,
        on_alarm_acknowledged: ep.on_alarm_acknowledged,
        on_device_down: ep.on_device_down,
        on_device_recovered: ep.on_device_recovered,
        on_fdd_fault_raised: ep.on_fdd_fault_raised,
        on_fdd_fault_cleared: ep.on_fdd_fault_cleared,
        min_severity: ep.min_severity,
        tag_filters: ep.tag_filters,
        created_ms: ep.created_ms,
        updated_ms: ep.updated_ms,
    }
}

// ----------------------------------------------------------------
// Handlers
// ----------------------------------------------------------------

pub async fn list_endpoints(
    State(state): State<ApiState>,
    auth: AuthUser,
) -> Result<Json<Vec<EndpointResponse>>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageWebhooks, &perms)?;
    let eps = state.webhook_store.list_endpoints().await;
    let result: Vec<EndpointResponse> = eps.into_iter().map(endpoint_to_response).collect();
    Ok(Json(result))
}

pub async fn get_endpoint(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<EndpointResponse>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageWebhooks, &perms)?;
    let ep = state
        .webhook_store
        .get_endpoint(&id)
        .await
        .ok_or(ApiError::NotFound("webhook endpoint not found".into()))?;
    Ok(Json(endpoint_to_response(ep)))
}

pub async fn create_endpoint(
    State(state): State<ApiState>,
    auth: AuthUser,
    Json(req): Json<CreateEndpointRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageWebhooks, &perms)?;

    let id = uuid::Uuid::new_v4().to_string();
    let provider = req.provider.as_deref().unwrap_or("generic");

    state
        .webhook_store
        .create_endpoint(
            &id,
            &req.name,
            provider,
            &req.url,
            req.headers.as_deref(),
            req.secret.as_deref(),
            req.on_alarm_raised,
            req.on_alarm_cleared,
            req.on_alarm_acknowledged,
            req.on_device_down,
            req.on_device_recovered,
            req.on_fdd_fault_raised,
            req.on_fdd_fault_cleared,
            &req.min_severity,
            req.tag_filters.as_deref(),
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder = AuditEntryBuilder::new(AuditAction::CreateWebhook, "webhook")
        .resource_id(&id)
        .details(&format!("name={}, provider={}", req.name, provider));
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true, "id": id})))
}

pub async fn update_endpoint(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateEndpointRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageWebhooks, &perms)?;

    let provider = req.provider.as_deref().unwrap_or("generic");

    state
        .webhook_store
        .update_endpoint(
            &id,
            &req.name,
            provider,
            &req.url,
            req.headers.as_deref(),
            req.secret.as_deref(),
            req.enabled,
            req.on_alarm_raised,
            req.on_alarm_cleared,
            req.on_alarm_acknowledged,
            req.on_device_down,
            req.on_device_recovered,
            req.on_fdd_fault_raised,
            req.on_fdd_fault_cleared,
            &req.min_severity,
            req.tag_filters.as_deref(),
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder = AuditEntryBuilder::new(AuditAction::UpdateWebhook, "webhook").resource_id(&id);
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

pub async fn delete_endpoint(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageWebhooks, &perms)?;

    state
        .webhook_store
        .delete_endpoint(&id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let builder = AuditEntryBuilder::new(AuditAction::DeleteWebhook, "webhook").resource_id(&id);
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

pub async fn test_endpoint(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageWebhooks, &perms)?;

    let ep = state
        .webhook_store
        .get_endpoint(&id)
        .await
        .ok_or(ApiError::NotFound("webhook endpoint not found".into()))?;

    let provider = Provider::from_str(&ep.provider).unwrap_or(Provider::Generic);
    let payload = crate::webhook::model::WebhookPayload {
        event_type: crate::webhook::model::WebhookEventType::AlarmRaised,
        alarm_id: Some(0),
        node_id: Some("test/test-point".into()),
        device_id: Some("test".into()),
        point_id: Some("test-point".into()),
        alarm_type: Some("high_limit".into()),
        severity: Some("warning".into()),
        trigger_value: Some(85.0),
        message: Some("Test webhook from OpenCrate BMS".into()),
        timestamp_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64,
        project_name: state.scenario_name.clone(),
    };

    let formatted = crate::webhook::providers::format_for_provider(
        provider,
        &payload,
        ep.secret.as_deref(),
        ep.secret.as_deref().unwrap_or(""),
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();

    let mut req = client
        .post(&ep.url)
        .header("Content-Type", &formatted.content_type);

    if let Some(ref headers_json) = ep.headers {
        if let Ok(headers) = serde_json::from_str::<serde_json::Value>(headers_json) {
            if let Some(obj) = headers.as_object() {
                for (k, v) in obj {
                    if let Some(val) = v.as_str() {
                        req = req.header(k, val);
                    }
                }
            }
        }
    }
    for (k, v) in &formatted.extra_headers {
        req = req.header(k, v);
    }

    let result = req.body(formatted.body).send().await;

    let builder = AuditEntryBuilder::new(AuditAction::TestWebhook, "webhook").resource_id(&id);
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    match result {
        Ok(resp) => {
            let status = resp.status().as_u16();
            if resp.status().is_success() {
                Ok(Json(serde_json::json!({"ok": true, "http_status": status})))
            } else {
                let body = resp.text().await.unwrap_or_default();
                Ok(Json(
                    serde_json::json!({"ok": false, "http_status": status, "error": body}),
                ))
            }
        }
        Err(e) => Ok(Json(
            serde_json::json!({"ok": false, "error": e.to_string()}),
        )),
    }
}

pub async fn list_deliveries(
    State(state): State<ApiState>,
    auth: AuthUser,
    Query(q): Query<DeliveryQuery>,
) -> Result<Json<Vec<crate::webhook::model::WebhookDelivery>>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageWebhooks, &perms)?;
    let deliveries = state
        .webhook_store
        .list_deliveries(q.endpoint_id.as_deref(), q.status.as_deref(), q.limit)
        .await;
    Ok(Json(deliveries))
}

pub async fn get_config(
    State(state): State<ApiState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageWebhooks, &perms)?;
    let paused = state
        .webhook_store
        .get_config("paused")
        .await
        .unwrap_or_default();
    Ok(Json(serde_json::json!({"paused": paused == "true"})))
}

pub async fn set_config(
    State(state): State<ApiState>,
    auth: AuthUser,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageWebhooks, &perms)?;

    if let Some(paused) = body.get("paused").and_then(|v| v.as_bool()) {
        state
            .webhook_store
            .set_config("paused", if paused { "true" } else { "false" })
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    }

    Ok(Json(serde_json::json!({"ok": true})))
}
