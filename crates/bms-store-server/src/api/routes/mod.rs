pub mod audit;
pub mod bridges;
pub mod discovery;
pub mod entities;
pub mod export;
pub mod history;
pub mod nodes;
pub mod overrides;
pub mod points;
pub mod programs;
pub mod system;
pub mod users;
pub mod webhooks;

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::DefaultBodyLimit;
use axum::http::{header, StatusCode};
use axum::middleware::{self, Next};
use axum::response::IntoResponse;
use axum::routing::{delete, get, post, put};
use axum::Router;
use tokio::sync::Mutex;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tower_http::trace::TraceLayer;

use super::auth;
use super::ws;
use super::ApiState;

pub fn build_router(state: ApiState) -> Router {
    // CORS: configurable via OPENCRATE_CORS_ORIGINS env var
    let cors = match std::env::var("OPENCRATE_CORS_ORIGINS") {
        Ok(origins) if !origins.is_empty() => {
            let parsed: Vec<_> = origins
                .split(',')
                .filter_map(|o| o.trim().parse().ok())
                .collect();
            CorsLayer::new()
                .allow_origin(AllowOrigin::list(parsed))
                .allow_methods(Any)
                .allow_headers(Any)
        }
        _ => {
            // Default: allow common local origins
            let defaults = [
                "http://localhost:3000",
                "http://localhost:5173",
                "http://localhost:8080",
                "http://127.0.0.1:3000",
                "http://127.0.0.1:5173",
                "http://127.0.0.1:8080",
            ];
            let parsed: Vec<_> = defaults.iter().filter_map(|o| o.parse().ok()).collect();
            CorsLayer::new()
                .allow_origin(AllowOrigin::list(parsed))
                .allow_methods(Any)
                .allow_headers(Any)
        }
    };

    // Bruteforce limiter — only on the credential-accepting endpoints.
    // Everything else (refresh, /me, api-key CRUD) is auth-token-gated
    // already, and a global counter risked locking out legitimate
    // refresh/me calls when bound to API-key polling traffic.
    let credential_limiter = AuthRateLimiter::new(20, Duration::from_secs(60));
    let credential_routes = Router::new()
        .route("/login", post(auth::login))
        .route("/setup", post(auth::setup))
        .layer(middleware::from_fn(move |req, next| {
            let limiter = credential_limiter.clone();
            async move { limiter.check(req, next).await }
        }));
    let session_routes = Router::new()
        .route("/refresh", post(auth::refresh))
        .route("/me", get(auth::me))
        .route(
            "/api-keys",
            get(auth::list_api_keys).post(auth::create_api_key),
        )
        .route(
            "/api-keys/{id}",
            put(auth::update_api_key).delete(auth::delete_api_key),
        );
    let auth_routes = Router::new().merge(credential_routes).merge(session_routes);

    let api = Router::new()
        .nest("/auth", auth_routes)
        // Entities (Haystack filter API + relationship traversal)
        .route("/entities", get(entities::list_entities))
        .route("/entities/{id}", get(entities::get_entity))
        .route("/entities/{id}/referrers", get(entities::get_referrers))
        .route("/entities/{id}/supply-chain", get(entities::get_supply_chain))
        .route("/entities/{id}/return-chain", get(entities::get_return_chain))
        .route("/relationships/issues", get(entities::get_relationship_issues))
        // Bulk endpoints — drive multi-select GUI actions in one round trip
        .route("/entities/tags-batch", post(entities::set_tags_batch))
        .route("/entities/tags-batch/remove", post(entities::remove_tags_batch))
        .route("/entities/refs-batch", post(entities::set_ref_batch))
        // Bridge config — register/edit/delete BACnet networks + Modbus buses
        // from the GUI instead of hand-editing scenario.json + restarting.
        .route("/bridges/bacnet", get(bridges::list_bacnet).post(bridges::create_bacnet))
        .route(
            "/bridges/bacnet/{id}",
            get(bridges::get_bacnet)
                .put(bridges::update_bacnet)
                .delete(bridges::delete_bacnet),
        )
        .route("/bridges/modbus", get(bridges::list_modbus).post(bridges::create_modbus))
        .route(
            "/bridges/modbus/{id}",
            get(bridges::get_modbus)
                .put(bridges::update_modbus)
                .delete(bridges::delete_modbus),
        )
        // Points
        .route("/points", get(points::list_points))
        .route("/points/{device_id}", get(points::device_points))
        .route("/points/{device_id}/{point_id}", get(points::get_point))
        .route(
            "/points/{device_id}/{point_id}/write",
            post(points::write_point),
        )
        .route(
            "/points/{device_id}/{point_id}/relinquish",
            post(overrides::relinquish_point),
        )
        // Nodes
        .route("/nodes", get(nodes::list_nodes).post(nodes::create_node))
        .route("/nodes/{id}", get(nodes::get_node).put(nodes::update_node))
        .route("/nodes/{id}/delete", delete(nodes::delete_node))
        .route("/nodes/{id}/tags", put(nodes::set_tags))
        .route("/nodes/{id}/hierarchy", get(nodes::get_hierarchy))
        .route("/nodes/{id}/ancestors", get(nodes::get_ancestors))
        // History
        .route(
            "/history/{device_id}/{point_id}",
            get(history::query_history),
        )
        .route(
            "/history/{device_id}/{point_id}/range",
            get(history::time_range),
        )
        .route(
            "/history/{device_id}/{point_id}/export",
            get(history::export_csv),
        )
        // Discovery
        .route("/discovery/devices", get(discovery::list_devices))
        .route("/discovery/devices/{id}", get(discovery::get_device))
        .route(
            "/discovery/devices/{id}/points",
            get(discovery::get_device_points),
        )
        .route(
            "/discovery/devices/{id}/accept",
            post(discovery::accept_device),
        )
        .route(
            "/discovery/devices/{id}/preview-tags",
            get(discovery::preview_device_tags),
        )
        .route(
            "/discovery/devices/{id}/ignore",
            post(discovery::ignore_device),
        )
        .route(
            "/discovery/devices/{id}/rename",
            post(discovery::rename_device),
        )
        .route(
            "/discovery/devices/bulk-accept",
            post(discovery::bulk_accept),
        )
        .route(
            "/discovery/devices/bulk-ignore",
            post(discovery::bulk_ignore),
        )
        .route("/discovery/scan/bacnet", post(discovery::scan_bacnet))
        .route("/discovery/scan/modbus", post(discovery::scan_modbus))
        // Programs
        .route(
            "/programs",
            get(programs::list_programs).post(programs::create_program),
        )
        .route(
            "/programs/{id}",
            get(programs::get_program)
                .put(programs::update_program)
                .delete(programs::delete_program),
        )
        .route("/programs/{id}/enabled", put(programs::set_enabled))
        .route("/programs/{id}/log", get(programs::get_execution_log))
        // Overrides
        .route("/overrides", get(overrides::list_all))
        .route("/overrides/active", get(overrides::list_active))
        .route("/overrides/{id}", put(overrides::update_override))
        // Audit
        .route("/audit", get(audit::query_audit))
        .route("/audit/count", get(audit::count_audit))
        // System
        .route("/health", get(system::health))
        .route("/system/info", get(system::system_info))
        .route("/system/capabilities", get(system::capabilities))
        .route("/system/backup", post(system::trigger_backup))
        .route("/system/backups", get(system::list_backups))
        .route(
            "/system/backup-config",
            get(system::get_backup_config).put(system::set_backup_config),
        )
        // Webhooks
        .route(
            "/webhooks",
            get(webhooks::list_endpoints).post(webhooks::create_endpoint),
        )
        .route(
            "/webhooks/config",
            get(webhooks::get_config).put(webhooks::set_config),
        )
        .route("/webhooks/deliveries", get(webhooks::list_deliveries))
        .route(
            "/webhooks/{id}",
            get(webhooks::get_endpoint)
                .put(webhooks::update_endpoint)
                .delete(webhooks::delete_endpoint),
        )
        .route("/webhooks/{id}/test", post(webhooks::test_endpoint))
        // Export
        .route(
            "/export/connectors",
            get(export::list_connectors).post(export::create_connector),
        )
        .route("/export/status", get(export::list_statuses))
        .route(
            "/export/connectors/{id}",
            get(export::get_connector)
                .put(export::update_connector)
                .delete(export::delete_connector),
        )
        .route("/export/connectors/{id}/test", post(export::test_connector))
        .route(
            "/export/connectors/{id}/backfill",
            post(export::backfill_connector),
        );

    // WebSocket
    let api = api.route("/ws", get(ws::ws_handler));

    Router::new()
        .nest("/api", api)
        .layer(DefaultBodyLimit::max(1_048_576)) // 1 MB
        .layer(middleware::from_fn(security_headers))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}

#[derive(Clone)]
struct AuthRateLimiter {
    state: Arc<Mutex<RateLimitState>>,
    max_requests: u64,
    window: Duration,
}

struct RateLimitState {
    count: u64,
    window_start: Instant,
}

impl AuthRateLimiter {
    fn new(max_requests: u64, window: Duration) -> Self {
        Self {
            state: Arc::new(Mutex::new(RateLimitState {
                count: 0,
                window_start: Instant::now(),
            })),
            max_requests,
            window,
        }
    }

    async fn check(
        &self,
        req: axum::http::Request<axum::body::Body>,
        next: Next,
    ) -> axum::response::Response {
        let mut state = self.state.lock().await;
        let now = Instant::now();
        if now.duration_since(state.window_start) >= self.window {
            state.count = 0;
            state.window_start = now;
        }
        state.count += 1;
        if state.count > self.max_requests {
            drop(state);
            return (StatusCode::TOO_MANY_REQUESTS, "Rate limit exceeded").into_response();
        }
        drop(state);
        next.run(req).await
    }
}

async fn security_headers(
    req: axum::http::Request<axum::body::Body>,
    next: Next,
) -> axum::response::Response {
    let mut res = next.run(req).await;
    let headers = res.headers_mut();
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    headers.insert(header::X_FRAME_OPTIONS, "DENY".parse().unwrap());
    headers.insert(
        header::STRICT_TRANSPORT_SECURITY,
        "max-age=31536000; includeSubDomains".parse().unwrap(),
    );
    headers.insert(
        header::REFERRER_POLICY,
        "strict-origin-when-cross-origin".parse().unwrap(),
    );
    res
}
