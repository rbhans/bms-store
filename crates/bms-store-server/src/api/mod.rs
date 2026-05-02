pub mod api_keys;
pub mod auth;
pub mod error;
pub mod pagination;
pub mod routes;
pub mod write;
pub mod ws;

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Instant;

use bms_store_bridges::boot::BridgeRuntime;
use bms_store_bridges::discovery::service::DiscoveryService;
use bms_store_bridges::plugin::BridgeRegistry;
use bms_store_storage::backup::BackupScheduler;
use bms_store_storage::boot::StorageRuntime;
use bms_store_storage::event::bus::EventBus;
use bms_store_storage::event::journal::EventJournal;
use bms_store_storage::health::HealthRegistry;
use bms_store_storage::logic::store::ProgramStore;
use bms_store_storage::store::audit_store::AuditStore;
use bms_store_storage::store::discovery_store::DiscoveryStore;
use bms_store_storage::store::entity_store::EntityStore;
use bms_store_storage::store::export_store::ExportStore;
use bms_store_storage::store::history_store::HistoryStore;
use bms_store_storage::store::node_store::NodeStore;
use bms_store_storage::store::override_store::OverrideStore;
use bms_store_storage::store::point_store::PointStore;
use bms_store_storage::store::user_store::UserStore;
use bms_store_storage::store::webhook_store::WebhookStore;
use tokio::sync::Mutex;

use self::api_keys::ApiKeyStore;

// ---------------------------------------------------------------------------
// Login rate limiter
// ---------------------------------------------------------------------------

const RATE_LIMIT_WINDOW_SECS: u64 = 900;
const RATE_LIMIT_MAX_ATTEMPTS: usize = 10;

#[derive(Clone)]
pub struct LoginRateLimiter {
    attempts: Arc<Mutex<HashMap<IpAddr, Vec<Instant>>>>,
}

impl Default for LoginRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl LoginRateLimiter {
    pub fn new() -> Self {
        Self {
            attempts: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn check(&self, ip: IpAddr) -> Result<(), error::ApiError> {
        let mut map = self.attempts.lock().await;
        let cutoff = Instant::now() - std::time::Duration::from_secs(RATE_LIMIT_WINDOW_SECS);

        let entries = map.entry(ip).or_default();
        entries.retain(|timestamp| *timestamp > cutoff);

        if entries.len() >= RATE_LIMIT_MAX_ATTEMPTS {
            return Err(error::ApiError::TooManyRequests);
        }

        entries.push(Instant::now());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// API state
// ---------------------------------------------------------------------------

/// Shared state available to all API handlers via axum `State`.
#[derive(Clone)]
pub struct ApiState {
    pub point_store: PointStore,
    pub node_store: NodeStore,
    pub history_store: HistoryStore,
    pub entity_store: EntityStore,
    pub discovery_store: DiscoveryStore,
    pub discovery_service: Arc<DiscoveryService>,
    pub user_store: UserStore,
    pub audit_store: AuditStore,
    pub program_store: ProgramStore,
    pub override_store: OverrideStore,
    pub event_bus: EventBus,
    pub event_journal: Option<EventJournal>,
    pub bridge_registry: Arc<BridgeRegistry>,
    pub jwt_secret: String,
    pub health: HealthRegistry,
    pub scenario_name: String,
    pub backup_scheduler: Arc<Mutex<BackupScheduler>>,
    pub webhook_store: WebhookStore,
    pub export_store: ExportStore,
    pub api_key_store: Arc<ApiKeyStore>,
    pub login_rate_limiter: LoginRateLimiter,
    pub ws_connections: Arc<Mutex<HashMap<String, usize>>>,
}

impl ApiState {
    pub fn from_runtimes(
        storage: &StorageRuntime,
        bridges: &BridgeRuntime,
        backup_scheduler: BackupScheduler,
        api_key_store: ApiKeyStore,
        jwt_secret: String,
    ) -> Self {
        Self {
            point_store: storage.point_store.clone(),
            node_store: storage.node_store.clone(),
            history_store: storage.history_store.clone(),
            entity_store: storage.entity_store.clone(),
            discovery_store: storage.discovery_store.clone(),
            discovery_service: bridges.discovery_service.clone(),
            user_store: storage.user_store.clone(),
            audit_store: storage.audit_store.clone(),
            program_store: storage.program_store.clone(),
            override_store: storage.override_store.clone(),
            event_bus: storage.event_bus.clone(),
            event_journal: storage.event_journal.clone(),
            bridge_registry: bridges.bridge_registry.clone(),
            jwt_secret,
            health: storage.health.clone(),
            scenario_name: storage.loaded.config.scenario.name.clone(),
            backup_scheduler: Arc::new(Mutex::new(backup_scheduler)),
            webhook_store: storage.webhook_store.clone(),
            export_store: storage.export_store.clone(),
            api_key_store: Arc::new(api_key_store),
            login_rate_limiter: LoginRateLimiter::new(),
            ws_connections: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

/// TLS configuration for HTTPS server.
#[derive(Clone)]
pub struct TlsConfig {
    pub addr: std::net::SocketAddr,
    pub cert_file: String,
    pub key_file: String,
}

/// Start the API server. Starts HTTP and optionally HTTPS listeners.
pub async fn start_api_server(
    state: ApiState,
    http_addr: std::net::SocketAddr,
    http_enabled: bool,
    tls: Option<TlsConfig>,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = routes::build_router(state);
    let has_https = tls.is_some();

    if !http_enabled && !has_https {
        tracing::warn!("Both HTTP and HTTPS are disabled; no listeners started");
        shutdown_signal().await;
        return Ok(());
    }

    let mut handles = Vec::new();

    if http_enabled {
        let http_app = app
            .clone()
            .into_make_service_with_connect_info::<std::net::SocketAddr>();
        handles.push(tokio::spawn(async move {
            match tokio::net::TcpListener::bind(http_addr).await {
                Ok(listener) => {
                    tracing::info!(%http_addr, "bms-store HTTP server listening");
                    axum::serve(listener, http_app)
                        .with_graceful_shutdown(shutdown_signal())
                        .await
                        .ok();
                }
                Err(error) => tracing::error!(%http_addr, "Failed to bind HTTP: {error}"),
            }
        }));
    }

    if let Some(tls_cfg) = tls {
        let https_app = app.into_make_service_with_connect_info::<std::net::SocketAddr>();
        let https_addr = tls_cfg.addr;
        handles.push(tokio::spawn(async move {
            match axum_server::tls_rustls::RustlsConfig::from_pem_file(
                &tls_cfg.cert_file,
                &tls_cfg.key_file,
            )
            .await
            {
                Ok(rustls_config) => {
                    tracing::info!(%https_addr, "bms-store HTTPS server listening");
                    axum_server::bind_rustls(https_addr, rustls_config)
                        .serve(https_app)
                        .await
                        .ok();
                }
                Err(error) => tracing::error!("Failed to load TLS certificates: {error}"),
            }
        }));
    }

    for handle in handles {
        handle.await.ok();
    }

    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install CTRL+C handler");
    tracing::info!("shutdown signal received");
}

/// Load or generate JWT secret from a file.
pub fn load_or_create_jwt_secret(secret_path: &std::path::Path) -> String {
    if let Ok(secret) = std::fs::read_to_string(secret_path) {
        let secret = secret.trim().to_string();
        if !secret.is_empty() {
            return secret;
        }
    }

    use rand::Rng;

    let mut rng = rand::thread_rng();
    let secret: String = (0..32)
        .map(|_| format!("{:02x}", rng.gen::<u8>()))
        .collect();

    if let Some(parent) = secret_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(secret_path, &secret).ok();
    secret
}
