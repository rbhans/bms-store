//! Node DTOs — the spatial / equip tree exposed by `/api/nodes/*`.
//!
//! Nodes are richer than the entity store rows ([`crate::entities`]) —
//! they carry runtime properties (key/value strings), a typed
//! capabilities block (readable/writable/historizable/alarmable/
//! schedulable), and an optional protocol binding describing where the
//! point's value comes from. Use this module from UI navigation panels
//! and the equip/point detail screens.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// One node row — site / space / equip / point / virtual_point.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeResponse {
    pub id: String,
    pub node_type: String,
    pub dis: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub tags: HashMap<String, Option<String>>,
    pub refs: HashMap<String, String>,
    pub properties: HashMap<String, String>,
    pub capabilities: NodeCapabilitiesResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding: Option<serde_json::Value>,
    pub created_ms: i64,
    pub updated_ms: i64,
}

/// Per-node capability flags. Drives UI affordances — e.g. "Write" button
/// only appears when `writable` is true.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeCapabilitiesResponse {
    pub readable: bool,
    pub writable: bool,
    pub historizable: bool,
    pub alarmable: bool,
    pub schedulable: bool,
}

/// `POST /api/nodes` body. `tags` are sent as ordered (k, v) pairs to
/// preserve insertion order on round-trip.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateNodeRequest {
    pub id: String,
    pub node_type: String,
    pub dis: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<(String, Option<String>)>>,
}

/// `PUT /api/nodes/:id` body — partial update. `parent_id: Some("")`
/// reparents to root; `Some(other)` reparents to other; `None` leaves
/// the parent untouched.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct UpdateNodeRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dis: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
}

/// `PUT /api/nodes/:id/tags` body. Replaces the tag set wholesale.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SetTagsRequest {
    pub tags: Vec<(String, Option<String>)>,
}
