//! History query DTOs — time-series samples for a single point as
//! returned by `GET /api/history/:device_id/:point_id` and
//! `/range`.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Query string for `GET /api/history/:device_id/:point_id`.
///
/// Time bounds: `cursor` > `from` > `start_ms` (`from` is the spec'd
/// name; `start_ms` and `cursor` are accepted aliases — `cursor` for
/// pagination round-tripping). Default range is the last 24 h when no
/// bound is supplied. `to` mirrors `end_ms`. `limit` mirrors
/// `max_results`.
#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct HistoryQueryParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_results: Option<i64>,
}

/// Response from `GET /api/history/:device_id/:point_id`.
///
/// `next_cursor` is the timestamp_ms (+1) of the last sample. Pass it
/// back as `?cursor=...` for the next page. Absent when the result set
/// reached the end of the available range.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct HistoryResponse {
    pub device_id: String,
    pub point_id: String,
    pub samples: Vec<SampleResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<i64>,
}

/// One historical sample. `value` is f64 across all variants — bools
/// store as 0.0/1.0, integers cast to f64. The wire is uniform; the
/// caller knows the point's underlying kind from the entity tags.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct SampleResponse {
    pub timestamp_ms: i64,
    pub value: f64,
}

/// Response from `GET /api/history/:device_id/:point_id/range` —
/// the earliest and latest sample timestamps the historian holds for
/// the point. Empty when the point has no samples.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct TimeRangeResponse {
    pub device_id: String,
    pub point_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_ms: Option<i64>,
}
