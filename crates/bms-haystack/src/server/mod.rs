//! Haystack HTTP facade — `/api/haystack/*` routes that follow the
//! standard Project Haystack REST contract.
//!
//! Mount the router from a parent service:
//! ```ignore
//! use std::sync::Arc;
//! use bms_haystack::server::{router, HaystackState};
//!
//! fn build(state: Arc<dyn HaystackState>) -> axum::Router {
//!     axum::Router::new().nest("/api/haystack", bms_haystack::server::router(state))
//! }
//! ```
//!
//! All handlers content-negotiate from the `Accept` header (Hayson default,
//! Zinc/CSV stubs) and accept POST bodies as either Hayson grids or
//! JSON-encoded `{filter, limit, ...}` shorthand for convenience.

mod content;
pub mod handlers;
mod state;
mod watch;

pub use content::{ContentType, ResponseBody};
pub use state::{HaystackError, HaystackState, PointWriteRequest};
pub use watch::{WatchHandle, WatchId, WatchState};

use std::sync::Arc;

use axum::{routing, Router};

/// Build the Haystack router. Caller mounts at `/api/haystack` (or another
/// prefix). The shared [`WatchState`] is created here and outlives request
/// handling.
pub fn router(state: Arc<dyn HaystackState>) -> Router {
    let watches = Arc::new(WatchState::new());
    let app_state = AppState {
        haystack: state,
        watches,
    };

    Router::new()
        .route("/about", routing::get(handlers::about::about))
        .route("/defs", routing::get(handlers::defs::defs))
        .route("/libs", routing::get(handlers::libs::libs))
        .route("/ops", routing::get(handlers::ops::ops))
        .route("/filetypes", routing::get(handlers::filetypes::filetypes))
        .route("/read", routing::get(handlers::read::read_get).post(handlers::read::read_post))
        .route("/nav", routing::get(handlers::nav::nav).post(handlers::nav::nav))
        .route("/watchSub", routing::post(handlers::watch_sub::watch_sub))
        .route("/watchUnsub", routing::post(handlers::watch_unsub::watch_unsub))
        .route("/watchPoll", routing::post(handlers::watch_poll::watch_poll))
        .route("/pointWrite", routing::post(handlers::point_write::point_write))
        .route("/hisRead", routing::get(handlers::his_read::his_read).post(handlers::his_read::his_read))
        .route("/hisWrite", routing::post(handlers::his_write::his_write))
        .route("/invokeAction", routing::post(handlers::invoke_action::invoke_action))
        .with_state(app_state)
}

/// Shared application state passed to every handler.
#[derive(Clone)]
pub struct AppState {
    pub haystack: Arc<dyn HaystackState>,
    pub watches: Arc<WatchState>,
}
