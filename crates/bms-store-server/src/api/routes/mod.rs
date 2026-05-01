pub mod alarms;
pub mod audit;
#[cfg(feature = "cloud")]
pub mod cloud;
pub mod discovery;
pub mod energy;
pub mod export;
pub mod fdd;
pub mod history;
pub mod nodes;
pub mod overrides;
pub mod points;
pub mod programs;
pub mod reports;
pub mod schedules;
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
use super::haystack_state::StoreAdapter;
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

    // Auth routes with rate limiting (20 requests per 60 seconds)
    let rate_limiter = AuthRateLimiter::new(20, Duration::from_secs(60));
    let auth_routes = Router::new()
        .route("/login", post(auth::login))
        .route("/refresh", post(auth::refresh))
        .route("/me", get(auth::me))
        .route("/setup", post(auth::setup))
        .route(
            "/api-keys",
            get(auth::list_api_keys).post(auth::create_api_key),
        )
        .route(
            "/api-keys/{id}",
            put(auth::update_api_key).delete(auth::delete_api_key),
        )
        .layer(middleware::from_fn(move |req, next| {
            let limiter = rate_limiter.clone();
            async move { limiter.check(req, next).await }
        }));

    let api = Router::new()
        .nest("/auth", auth_routes)
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
        // Alarms
        .route("/alarms/active", get(alarms::active_alarms))
        .route("/alarms/{id}/ack", post(alarms::acknowledge_alarm))
        .route("/alarms/ack-all", post(alarms::acknowledge_all))
        .route("/alarms/configs", get(alarms::list_configs))
        .route("/alarms/history", get(alarms::alarm_history))
        .route("/alarms/history/export", get(alarms::alarm_history_export))
        // Schedules
        .route(
            "/schedules",
            get(schedules::list_schedules).post(schedules::create_schedule),
        )
        .route(
            "/schedules/{id}",
            get(schedules::get_schedule).put(schedules::update_schedule),
        )
        .route("/schedules/{id}/delete", delete(schedules::delete_schedule))
        .route(
            "/schedules/{id}/assignments",
            get(schedules::list_assignments).post(schedules::create_assignment),
        )
        .route(
            "/schedules/assignments/{id}/delete",
            delete(schedules::delete_assignment),
        )
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
        // Reports
        .route(
            "/reports",
            get(reports::list_reports).post(reports::create_report),
        )
        .route(
            "/reports/{id}",
            get(reports::get_report)
                .put(reports::update_report)
                .delete(reports::delete_report),
        )
        .route("/reports/{id}/run", post(reports::run_report))
        .route(
            "/reports/{id}/schedules",
            get(reports::list_schedules).post(reports::create_schedule),
        )
        .route(
            "/reports/schedules/{id}",
            put(reports::update_schedule).delete(reports::delete_schedule),
        )
        .route("/reports/{id}/executions", get(reports::list_executions))
        .route(
            "/reports/executions/{id}/html",
            get(reports::get_execution_html),
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
        // Energy
        .route(
            "/energy/meters",
            get(energy::list_meters).post(energy::create_meter),
        )
        .route(
            "/energy/meters/{id}",
            get(energy::get_meter)
                .put(energy::update_meter)
                .delete(energy::delete_meter),
        )
        .route(
            "/energy/rates",
            get(energy::list_rates).post(energy::create_rate),
        )
        .route(
            "/energy/rates/{id}",
            put(energy::update_rate).delete(energy::delete_rate),
        )
        .route(
            "/energy/baselines",
            get(energy::list_baselines).post(energy::create_baseline),
        )
        .route("/energy/baselines/{id}", delete(energy::delete_baseline))
        .route("/energy/summary", get(energy::get_summary))
        .route("/energy/consumption", get(energy::get_consumption))
        .route("/energy/export", get(energy::export_csv))
        // FDD
        .route("/fdd/rules", get(fdd::list_rules).post(fdd::create_rule))
        .route(
            "/fdd/rules/{id}",
            get(fdd::get_rule)
                .put(fdd::update_rule)
                .delete(fdd::delete_rule),
        )
        .route(
            "/fdd/bindings",
            get(fdd::list_bindings).post(fdd::create_binding),
        )
        .route("/fdd/bindings/auto", post(fdd::auto_bind))
        .route(
            "/fdd/bindings/{id}",
            put(fdd::update_binding).delete(fdd::delete_binding),
        )
        .route("/fdd/faults/active", get(fdd::active_faults))
        .route("/fdd/faults/ack-all", post(fdd::acknowledge_all))
        .route("/fdd/faults/{id}/ack", post(fdd::acknowledge_fault))
        .route("/fdd/history", get(fdd::fault_history))
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

    // Cloud bridges (feature-gated)
    #[cfg(feature = "cloud")]
    let api = api
        .route(
            "/cloud/bridges",
            get(cloud::list_bridges).post(cloud::create_bridge),
        )
        .route("/cloud/status", get(cloud::list_statuses))
        .route(
            "/cloud/bridges/{id}",
            get(cloud::get_bridge)
                .put(cloud::update_bridge)
                .delete(cloud::delete_bridge),
        )
        .route("/cloud/bridges/{id}/test", post(cloud::test_bridge));

    // WebSocket
    let api = api.route("/ws", get(ws::ws_handler));

    // Bind ApiState onto the api router so it can compose with the
    // independently-stated Haystack router below.
    let api = api.with_state(state.clone());

    // Haystack 5 facade: standard /api/haystack/* ops backed by the same
    // EntityStore / PointStore / HistoryStore / OverrideStore. Mounted at
    // the same /api prefix so it's reachable as /api/haystack/about, etc.
    let haystack_adapter = StoreAdapter::from_api_state(&state);
    let haystack_router = bms_haystack::server::router(haystack_adapter);

    Router::new()
        .nest("/api", api)
        .nest("/api/haystack", haystack_router)
        .layer(DefaultBodyLimit::max(1_048_576)) // 1 MB
        .layer(middleware::from_fn(security_headers))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
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
