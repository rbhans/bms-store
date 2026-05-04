//! Pagination wire shapes.
//!
//! Most list endpoints (`/api/points`, `/api/nodes`, `/api/audit`, …)
//! wrap their result rows in [`PaginatedResponse`] and accept
//! [`PaginationParams`] in the query string. The server crate retains
//! the `resolve()` and `from_vec()` helpers that depend on its own
//! types — this module is the wire contract only.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Query string fragment used by list endpoints. Either `offset` or
/// `cursor` may be set; `cursor` is the recommended round-tripped form.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct PaginationParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// Paginated wrapper. `total` is the unfiltered population size; `limit`
/// and `offset` echo the resolved values used for this page.
/// `next_cursor` is set when more pages remain — pass it back as
/// `?cursor=...` to fetch the next page.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct PaginatedResponse<T: ToSchema> {
    pub items: Vec<T>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}
