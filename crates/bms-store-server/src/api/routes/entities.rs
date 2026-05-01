//! Entity routes: Haystack-filter query API and relationship traversal.
//!
//! Endpoints:
//! - `GET /api/entities` — list or filter entities using Haystack-4 filter syntax
//! - `GET /api/entities/:id` — fetch a single entity
//! - `GET /api/entities/:id/referrers?tag=supplyRef` — who references this entity
//! - `GET /api/entities/:id/supply-chain` — walk supplyRef chain upstream
//! - `GET /api/entities/:id/return-chain` — walk returnRef chain
//! - `GET /api/relationships/issues` — orphaned-ref validation across the project

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Json, response::Response};
use serde::{Deserialize, Serialize};

use bms_store_storage::haystack::filter::{parse_filter, matches as filter_matches};
use bms_store_storage::store::entity_store::{Entity, EntityError};
use crate::api::auth::AuthUser;
use crate::api::error::ApiError;
use crate::api::ApiState;
use crate::store::relationships::{
    find_referrers, walk_supply_chain, walk_return_chain, validate_relationships, RelationshipIssue,
};

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct EntityResponse {
    pub id: String,
    pub entity_type: String,
    pub dis: String,
    pub parent_id: Option<String>,
    pub tags: std::collections::HashMap<String, Option<String>>,
    pub refs: std::collections::HashMap<String, String>,
    pub created_ms: i64,
    pub updated_ms: i64,
}

fn entity_to_response(e: Entity) -> EntityResponse {
    EntityResponse {
        id: e.id,
        entity_type: e.entity_type,
        dis: e.dis,
        parent_id: e.parent_id,
        tags: e.tags,
        refs: e.refs,
        created_ms: e.created_ms,
        updated_ms: e.updated_ms,
    }
}

// ---------------------------------------------------------------------------
// GET /api/entities  (with optional ?filter= Haystack filter)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ListEntitiesQuery {
    /// Haystack-4 filter expression.  If absent or empty, all entities are returned.
    pub filter: Option<String>,
    /// Limit result set size (default 1000).
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Entity type filter (independent of haystack filter).
    pub entity_type: Option<String>,
}

fn default_limit() -> usize {
    1000
}

/// GET /api/entities — list entities, optionally filtered by a Haystack-4 filter.
pub async fn list_entities(
    State(state): State<ApiState>,
    _auth: AuthUser,
    Query(q): Query<ListEntitiesQuery>,
) -> Response {
    // Parse filter expression if provided
    let filter_expr = match q.filter.as_deref().filter(|s| !s.is_empty()) {
        Some(filter_str) => match parse_filter(filter_str) {
            Ok(expr) => Some(expr),
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": e.to_string()})),
                )
                    .into_response();
            }
        },
        None => None,
    };

    let entities = state
        .entity_store
        .list_entities(q.entity_type.as_deref(), None)
        .await;

    let results: Vec<EntityResponse> = entities
        .into_iter()
        .filter(|e| {
            if let Some(ref expr) = filter_expr {
                filter_matches(expr, &e.tags)
            } else {
                true
            }
        })
        .take(q.limit)
        .map(entity_to_response)
        .collect();

    Json(results).into_response()
}

// ---------------------------------------------------------------------------
// GET /api/entities/:id
// ---------------------------------------------------------------------------

pub async fn get_entity(
    State(state): State<ApiState>,
    _auth: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<EntityResponse>, ApiError> {
    let entity = state.entity_store.get_entity(&id).await.map_err(|e| match e {
        EntityError::NotFound => ApiError::NotFound(format!("entity '{id}' not found")),
        other => ApiError::Internal(other.to_string()),
    })?;
    Ok(Json(entity_to_response(entity)))
}

// ---------------------------------------------------------------------------
// GET /api/entities/:id/referrers?tag=supplyRef
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ReferrersQuery {
    /// The ref tag to query (e.g. "supplyRef", "equipRef"). Required.
    pub tag: String,
}

pub async fn get_referrers(
    State(state): State<ApiState>,
    _auth: AuthUser,
    Path(id): Path<String>,
    Query(q): Query<ReferrersQuery>,
) -> Json<Vec<EntityResponse>> {
    let entities = find_referrers(&state.entity_store, &id, &q.tag).await;
    Json(entities.into_iter().map(entity_to_response).collect())
}

// ---------------------------------------------------------------------------
// GET /api/entities/:id/supply-chain
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ChainQuery {
    /// Maximum depth for chain traversal (default 10).
    #[serde(default = "default_chain_depth")]
    pub max_depth: usize,
}

fn default_chain_depth() -> usize {
    10
}

pub async fn get_supply_chain(
    State(state): State<ApiState>,
    _auth: AuthUser,
    Path(id): Path<String>,
    Query(q): Query<ChainQuery>,
) -> Json<Vec<EntityResponse>> {
    let entities = walk_supply_chain(&state.entity_store, &id, q.max_depth).await;
    Json(entities.into_iter().map(entity_to_response).collect())
}

// ---------------------------------------------------------------------------
// GET /api/entities/:id/return-chain
// ---------------------------------------------------------------------------

pub async fn get_return_chain(
    State(state): State<ApiState>,
    _auth: AuthUser,
    Path(id): Path<String>,
    Query(q): Query<ChainQuery>,
) -> Json<Vec<EntityResponse>> {
    let entities = walk_return_chain(&state.entity_store, &id, q.max_depth).await;
    Json(entities.into_iter().map(entity_to_response).collect())
}

// ---------------------------------------------------------------------------
// GET /api/relationships/issues
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct RelationshipIssueResponse {
    pub entity_id: String,
    pub tag_name: String,
    pub problem: String,
}

fn issue_to_response(i: RelationshipIssue) -> RelationshipIssueResponse {
    RelationshipIssueResponse {
        entity_id: i.entity_id,
        tag_name: i.tag_name,
        problem: i.problem,
    }
}

pub async fn get_relationship_issues(
    State(state): State<ApiState>,
    _auth: AuthUser,
) -> Json<Vec<RelationshipIssueResponse>> {
    let issues = validate_relationships(&state.entity_store).await;
    Json(issues.into_iter().map(issue_to_response).collect())
}
