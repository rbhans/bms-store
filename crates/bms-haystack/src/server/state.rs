use std::collections::HashMap;

use async_trait::async_trait;
use thiserror::Error;

use crate::filter::FilterExpr;
use crate::val::{Dict, Grid, Ref, Value};

#[derive(Debug, Error)]
pub enum HaystackError {
    #[error("not found")]
    NotFound,
    #[error("not implemented")]
    NotImplemented,
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("backend: {0}")]
    Backend(String),
}

/// Arguments to `pointWrite`.
#[derive(Debug, Clone)]
pub struct PointWriteRequest {
    pub id: Ref,
    /// Haystack priority level 1..=17 (`null` to relinquish).
    pub level: u8,
    pub val: Option<Value>,
    pub who: Option<String>,
    pub duration_ms: Option<i64>,
}

/// Backend abstraction the [`super::router`] depends on. Implement this in
/// the host service (e.g. `bms-store-server`) to bridge the HTTP facade
/// onto the actual entity / point / history stores.
///
/// Every method is async and `Send + Sync` — handlers are tokio tasks.
/// Implementations should treat unknown ids as [`HaystackError::NotFound`]
/// and unsupported ops (e.g. `invoke_action` when no actions are defined)
/// as [`HaystackError::NotImplemented`].
#[async_trait]
pub trait HaystackState: Send + Sync {
    /// Server identity tags returned by `/about`. Recommended keys:
    /// `serverName`, `vendorName`, `productName`, `productVersion`,
    /// `tz`, `serverTime`, `serverBootTime`, `whoami`.
    async fn about(&self) -> Dict;

    /// Run a Haystack filter and return matching entities as a grid.
    async fn read(
        &self,
        filter: &FilterExpr,
        limit: Option<usize>,
    ) -> Result<Grid, HaystackError>;

    /// Read a single entity by id (used by `nav` and as a fallback for
    /// `read?id=@x`). Returns `NotFound` if the id is unknown.
    async fn read_by_id(&self, id: &Ref) -> Result<Dict, HaystackError>;

    /// Hierarchical navigation. `nav_id` of `None` returns root entities.
    async fn nav(&self, nav_id: Option<&Ref>) -> Result<Grid, HaystackError>;

    /// Read history samples for a point over a span.
    async fn his_read(&self, id: &Ref, range: &str) -> Result<Grid, HaystackError>;

    /// Write history samples for a point.
    async fn his_write(&self, id: &Ref, items: &Grid) -> Result<Dict, HaystackError>;

    /// Write a point at the given priority level.
    async fn point_write(&self, req: &PointWriteRequest) -> Result<Dict, HaystackError>;

    /// Invoke a named action on an entity.
    async fn invoke_action(
        &self,
        id: &Ref,
        action: &str,
        args: &Dict,
    ) -> Result<Grid, HaystackError>;

    /// All entities in the store, indexed by id. Used by [`super::WatchState`]
    /// to compute deltas on `watchPoll`. Returning a snapshot is acceptable
    /// — implementations should aim for `O(N)` traversal.
    async fn snapshot_entities(&self) -> HashMap<String, Dict> {
        // Default: empty — watchPoll returns no deltas. Override for
        // implementations that support watches.
        HashMap::new()
    }
}
