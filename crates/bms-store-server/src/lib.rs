//! HTTP and WebSocket server for bms-store.

use std::net::SocketAddr;

use axum::{routing::get, Json, Router};
use serde::Serialize;
use tokio::net::TcpListener;

pub mod api;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

pub fn router() -> Router {
    Router::new().route("/api/health", get(health))
}

pub fn api_router(state: api::ApiState) -> Router {
    api::routes::build_router(state)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: VERSION,
    })
}

pub async fn serve(addr: SocketAddr) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(%addr, "bms-store health server listening");
    axum::serve(listener, router()).await
}

pub async fn serve_api(
    addr: SocketAddr,
    state: api::ApiState,
) -> Result<(), Box<dyn std::error::Error>> {
    api::start_api_server(state, addr, true, None).await
}

pub mod auth {
    pub use bms_store_storage::auth::*;
}

pub mod backup {
    pub use bms_store_storage::backup::*;
}

pub mod bridge {
    pub use bms_store_bridges::bridge::*;
}

pub mod config {
    pub use bms_store_storage::config::*;
}

pub mod discovery {
    pub use bms_store_bridges::discovery::*;
}

pub mod event {
    pub use bms_store_storage::event::*;
}

pub mod export {
    pub use bms_store_storage::export::*;
}

pub mod health {
    pub use bms_store_storage::health::*;
}

pub mod logic {
    pub use bms_store_storage::logic::*;
}

pub mod node {
    pub use bms_store_storage::node::*;
}

pub mod plugin {
    pub use bms_store_bridges::plugin::*;
}

pub mod project {
    pub use bms_store_storage::project::*;
}

pub mod store {
    pub use bms_store_storage::store::*;
}

pub mod webhook {
    pub use bms_store_storage::webhook::*;
}
