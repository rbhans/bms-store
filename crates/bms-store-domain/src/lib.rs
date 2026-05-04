//! Wire DTOs for the bms-store HTTP and WebSocket APIs.
//!
//! Consumers (the bms-store UI repo, CLI tools, integrators) pull this
//! crate to get strongly-typed request/response shapes that match the
//! axum routes exposed by `bms-store-server`. Anything in this crate is
//! a stable wire contract — bumps follow semver.
//!
//! Modules group DTOs by API surface:
//!
//! - [`points`] — point reads (latest value), writes, status flags
//! - [`entities`] — Site/Building/Floor/Space/Equip/Point entities,
//!   tag/ref graph, Haystack-filter queries
//! - [`history`] — time-series sample queries for one point
//! - … more lifted incrementally as routes are migrated to typed DTOs
//!
//! Re-exports from `bms-core` are available under [`core`] for
//! convenience.

pub use bms_core as core;

pub mod entities;
pub mod history;
pub mod nodes;
pub mod pagination;
pub mod points;
pub mod system;

