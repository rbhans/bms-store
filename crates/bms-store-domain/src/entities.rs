//! Entity DTOs — typed Site → Building → Floor → Space → Equipment →
//! Point hierarchy plus tag / ref graph as exposed by `/api/entities/*`.
//!
//! Tags are JSON-encoded as `{ "tagName": "value" or null }` — `null`
//! denotes a marker tag (e.g. `equip` with no value). Refs are
//! JSON-encoded as `{ "refTag": "targetEntityId" }` (e.g. `equipRef`,
//! `siteRef`, `spaceRef`).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// One entity row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct EntityResponse {
    pub id: String,
    pub entity_type: String,
    pub dis: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub tags: HashMap<String, Option<String>>,
    pub refs: HashMap<String, String>,
    pub created_ms: i64,
    pub updated_ms: i64,
}

/// Query string for `GET /api/entities`.
#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct ListEntitiesQuery {
    /// Haystack-4 filter expression. Empty / absent returns all rows
    /// (subject to `limit`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    /// Maximum rows to return. Default 1000.
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Filter by entity_type (`site`, `space`, `equip`, `point`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_type: Option<String>,
}

fn default_limit() -> usize {
    1000
}
