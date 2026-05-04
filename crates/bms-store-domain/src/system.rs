//! System / health / capability DTOs returned by the public `/api/health`
//! and `/api/system/*` routes. These are the shapes a UI uses to render
//! the server-status header and the about panel.

use serde::{Deserialize, Serialize};

/// `GET /api/health`. `status` is `"healthy"` when every component is
/// green, otherwise `"degraded"`. `components` is the per-subsystem
/// breakdown — each entry's `status` follows the same vocabulary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HealthResponse {
    pub status: String,
    pub components: Vec<ComponentHealth>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComponentHealth {
    pub name: String,
    pub status: String,
}

/// `GET /api/system/info` — server version + scenario summary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SystemInfoResponse {
    pub version: String,
    pub point_count: usize,
    pub device_count: usize,
    pub scenario_name: String,
}

/// `GET /api/system/capabilities` — what protocols and feature flags
/// this build supports. Use to gate UI affordances (e.g. show the
/// BACnet network panel only when `bridges` contains `"bacnet"`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapabilitiesResponse {
    pub version: String,
    pub bridges: Vec<String>,
    pub features: Vec<String>,
}
