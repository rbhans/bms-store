//! OpenAPI 3.1 spec exposed at `GET /api/openapi.json`.
//!
//! Phase 1 (this module): every wire shape from `bms-store-domain` is
//! registered as a `components.schemas` entry. Consumers can codegen
//! types from the schema section even before per-endpoint
//! `#[utoipa::path]` annotations are added.
//!
//! Phase 2 (incremental): each axum handler gets `#[utoipa::path]` so
//! the paths section fills in. Until then, the spec advertises an
//! `info.description` linking back to the README for endpoint docs.
//!
//! The spec is served from [`crate::api::routes::openapi_json`], wired
//! into the router at `/api/openapi.json`.

use bms_store_domain::{entities, history, nodes, pagination, points, system};
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "bms-store API",
        description = "REST + WebSocket API for the bms-store building data layer. \
This phase 1 spec advertises the wire shapes (`components.schemas`); per-\
endpoint paths are annotated incrementally — consult the README until the \
paths section is complete.",
        version = env!("CARGO_PKG_VERSION"),
        license(name = "MIT OR Apache-2.0"),
    ),
    components(schemas(
        // points
        points::PointResponse,
        points::WriteRequest,
        points::WriteResponse,
        // entities
        entities::EntityResponse,
        entities::ListEntitiesQuery,
        // history
        history::HistoryQueryParams,
        history::HistoryResponse,
        history::SampleResponse,
        history::TimeRangeResponse,
        // nodes
        nodes::NodeResponse,
        nodes::NodeCapabilitiesResponse,
        nodes::CreateNodeRequest,
        nodes::UpdateNodeRequest,
        nodes::SetTagsRequest,
        // system
        system::HealthResponse,
        system::ComponentHealth,
        system::SystemInfoResponse,
        system::CapabilitiesResponse,
        // pagination
        pagination::PaginationParams,
    )),
    tags(
        (name = "points", description = "Live point reads + writes"),
        (name = "entities", description = "Site/Building/Floor/Space/Equip/Point + tag/ref graph"),
        (name = "nodes", description = "Spatial / equip tree"),
        (name = "history", description = "Time-series sample queries"),
        (name = "discovery", description = "Device discovery + accept lifecycle"),
        (name = "system", description = "Health + capabilities + backups"),
    ),
)]
pub struct ApiDoc;

/// Build the JSON spec body for the `/api/openapi.json` route.
pub fn openapi_json_string() -> String {
    ApiDoc::openapi()
        .to_pretty_json()
        .unwrap_or_else(|e| format!("{{\"error\":\"openapi serialize failed: {e}\"}}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openapi_json_renders() {
        let json = openapi_json_string();
        assert!(json.contains("\"openapi\""));
        assert!(json.contains("PointResponse"));
        assert!(json.contains("EntityResponse"));
        assert!(json.contains("NodeResponse"));
    }
}
