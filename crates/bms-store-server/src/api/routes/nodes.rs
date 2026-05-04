use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;

use bms_store_domain::nodes::{
    CreateNodeRequest, NodeCapabilitiesResponse, NodeResponse, SetTagsRequest, UpdateNodeRequest,
};

use crate::api::auth::{require_permission, AuthUser};
use crate::api::error::ApiError;
use crate::api::pagination::{validate_string, PaginatedResponse, PaginationParams};
use crate::api::ApiState;
use crate::auth::Permission;
use crate::node::{Node, NodeCapabilities, NodeType};
use crate::store::audit_store::{AuditAction, AuditEntryBuilder};
use crate::store::node_store::NodeRecord;

fn record_to_response(r: NodeRecord) -> NodeResponse {
    NodeResponse {
        id: r.id,
        node_type: r.node_type,
        dis: r.dis,
        parent_id: r.parent_id,
        tags: r.tags,
        refs: r.refs,
        properties: r.properties,
        capabilities: NodeCapabilitiesResponse {
            readable: r.capabilities.readable,
            writable: r.capabilities.writable,
            historizable: r.capabilities.historizable,
            alarmable: r.capabilities.alarmable,
            schedulable: r.capabilities.schedulable,
        },
        binding: r.binding.as_ref().map(|b| {
            serde_json::json!({
                "protocol": b.protocol,
                "config": b.config,
            })
        }),
        created_ms: r.created_ms,
        updated_ms: r.updated_ms,
    }
}

#[derive(Deserialize)]
pub struct ListNodesQuery {
    pub node_type: Option<String>,
    pub parent_id: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

/// GET /api/nodes
pub async fn list_nodes(
    State(state): State<ApiState>,
    _auth: AuthUser,
    Query(q): Query<ListNodesQuery>,
) -> Json<PaginatedResponse<NodeResponse>> {
    let records = state
        .node_store
        .list_nodes(q.node_type.as_deref(), q.parent_id.as_deref())
        .await;
    let all: Vec<NodeResponse> = records.into_iter().map(record_to_response).collect();
    Json(PaginatedResponse::from_vec(all, &q.pagination))
}

/// GET /api/nodes/:id
pub async fn get_node(
    State(state): State<ApiState>,
    _auth: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<NodeResponse>, ApiError> {
    let record = state.node_store.get_node(&id).await?;
    Ok(Json(record_to_response(record)))
}

/// POST /api/nodes
pub async fn create_node(
    State(state): State<ApiState>,
    auth: AuthUser,
    Json(req): Json<CreateNodeRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_string("id", &req.id, 256)?;
    validate_string("dis", &req.dis, 512)?;

    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageDiscovery, &perms)?;

    let node_type = match req.node_type.as_str() {
        "site" => NodeType::Site,
        "space" => NodeType::Space,
        "equip" => NodeType::Equip,
        "point" => NodeType::Point,
        "virtual_point" => NodeType::VirtualPoint,
        other => return Err(ApiError::BadRequest(format!("invalid node_type: {other}"))),
    };

    let mut tags = std::collections::HashMap::new();
    if let Some(t) = req.tags {
        for (k, v) in t {
            tags.insert(k, v);
        }
    }

    let node = Node {
        id: req.id.clone(),
        node_type,
        dis: req.dis,
        parent_id: req.parent_id,
        value: None,
        timestamp: None,
        status: crate::store::point_store::PointStatusFlags::default(),
        tags,
        refs: std::collections::HashMap::new(),
        properties: std::collections::HashMap::new(),
        capabilities: NodeCapabilities {
            readable: false,
            writable: false,
            historizable: false,
            alarmable: false,
            schedulable: false,
        },
        binding: None,
    };

    state.node_store.create_node(node).await?;

    let builder = AuditEntryBuilder::new(AuditAction::CreateEntity, "node").resource_id(&req.id);
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true, "id": req.id})))
}

/// DELETE /api/nodes/:id
pub async fn delete_node(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageDiscovery, &perms)?;
    state.node_store.delete_node(&id).await?;

    let builder = AuditEntryBuilder::new(AuditAction::DeleteEntity, "node").resource_id(&id);
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

/// PUT /api/nodes/:id/tags
pub async fn set_tags(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<SetTagsRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if req.tags.len() > 100 {
        return Err(ApiError::BadRequest("too many tags (max 100)".into()));
    }
    for (key, _) in &req.tags {
        validate_string("tag key", key, 128)?;
    }

    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageDiscovery, &perms)?;
    state.node_store.set_tags(&id, req.tags).await?;

    let builder = AuditEntryBuilder::new(AuditAction::SetTag, "node").resource_id(&id);
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

/// PUT /api/nodes/:id
pub async fn update_node(
    State(state): State<ApiState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateNodeRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let perms = state.user_store.get_all_role_permissions().await;
    require_permission(&auth, Permission::ManageDiscovery, &perms)?;

    if let Some(ref dis) = req.dis {
        validate_string("dis", dis, 512)?;
        state.node_store.update_dis(&id, dis).await?;
    }
    if let Some(ref parent_id) = req.parent_id {
        let parent = if parent_id.is_empty() {
            None
        } else {
            Some(parent_id.as_str())
        };
        state.node_store.update_parent(&id, parent).await?;
    }

    let builder = AuditEntryBuilder::new(AuditAction::UpdateEntity, "node").resource_id(&id);
    let _ = state
        .audit_store
        .log_action(&auth.user_id, &auth.username, builder)
        .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

/// GET /api/nodes/:id/hierarchy
pub async fn get_hierarchy(
    State(state): State<ApiState>,
    _auth: AuthUser,
    Path(id): Path<String>,
) -> Json<Vec<NodeResponse>> {
    let records = state.node_store.get_hierarchy(Some(&id)).await;
    Json(records.into_iter().map(record_to_response).collect())
}

/// GET /api/nodes/:id/ancestors
pub async fn get_ancestors(
    State(state): State<ApiState>,
    _auth: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<Vec<NodeResponse>>, ApiError> {
    let records = state.node_store.get_ancestors(&id).await?;
    Ok(Json(records.into_iter().map(record_to_response).collect()))
}
