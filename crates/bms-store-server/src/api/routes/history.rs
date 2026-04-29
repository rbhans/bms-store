use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::auth::AuthUser;
use crate::api::error::ApiError;
use crate::api::ApiState;
use crate::store::history_store::HistoryQuery;

#[derive(Deserialize)]
pub struct HistoryQueryParams {
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub cursor: Option<i64>,
    pub limit: Option<i64>,
    pub max_results: Option<i64>,
}

#[derive(Serialize)]
pub struct HistoryResponse {
    pub device_id: String,
    pub point_id: String,
    pub samples: Vec<SampleResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<i64>,
}

#[derive(Serialize)]
pub struct SampleResponse {
    pub timestamp_ms: i64,
    pub value: f64,
}

/// GET /api/history/:device_id/:point_id
pub async fn query_history(
    State(state): State<ApiState>,
    _auth: AuthUser,
    Path((device_id, point_id)): Path<(String, String)>,
    Query(q): Query<HistoryQueryParams>,
) -> Result<Json<HistoryResponse>, ApiError> {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    let start_ms = q
        .cursor
        .or(q.from)
        .or(q.start_ms)
        .unwrap_or(now_ms - 86_400_000);
    let end_ms = q.to.or(q.end_ms).unwrap_or(now_ms);
    let max_results = q.limit.or(q.max_results);

    let query = HistoryQuery {
        device_id: device_id.clone(),
        point_id: point_id.clone(),
        start_ms,
        end_ms,
        max_results,
    };

    let result = state.history_store.query(query).await?;
    let next_cursor = result
        .samples
        .last()
        .map(|sample| sample.timestamp_ms.saturating_add(1));

    Ok(Json(HistoryResponse {
        device_id: result.device_id,
        point_id: result.point_id,
        samples: result
            .samples
            .into_iter()
            .map(|s| SampleResponse {
                timestamp_ms: s.timestamp_ms,
                value: s.value,
            })
            .collect(),
        next_cursor,
    }))
}

#[derive(Serialize)]
pub struct TimeRangeResponse {
    pub device_id: String,
    pub point_id: String,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
}

/// GET /api/history/:device_id/:point_id/range
pub async fn time_range(
    State(state): State<ApiState>,
    _auth: AuthUser,
    Path((device_id, point_id)): Path<(String, String)>,
) -> Json<TimeRangeResponse> {
    let range = state.history_store.time_range(&device_id, &point_id).await;
    Json(TimeRangeResponse {
        device_id,
        point_id,
        start_ms: range.map(|(s, _)| s),
        end_ms: range.map(|(_, e)| e),
    })
}

/// GET /api/history/:device_id/:point_id/export?format=csv
pub async fn export_csv(
    State(state): State<ApiState>,
    _auth: AuthUser,
    Path((device_id, point_id)): Path<(String, String)>,
    Query(q): Query<HistoryQueryParams>,
) -> Result<impl IntoResponse, ApiError> {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    let query = HistoryQuery {
        device_id: device_id.clone(),
        point_id: point_id.clone(),
        start_ms: q
            .cursor
            .or(q.from)
            .or(q.start_ms)
            .unwrap_or(now_ms - 86_400_000),
        end_ms: q.to.or(q.end_ms).unwrap_or(now_ms),
        max_results: q.limit.or(q.max_results),
    };

    let result = state.history_store.query(query).await?;

    let mut csv = String::from("timestamp,datetime,value\n");
    for sample in &result.samples {
        // Convert timestamp_ms to ISO 8601-ish datetime
        let secs = sample.timestamp_ms / 1000;
        let millis = sample.timestamp_ms % 1000;
        csv.push_str(&format!(
            "{},{}.{:03},{}\n",
            sample.timestamp_ms, secs, millis, sample.value
        ));
    }

    let filename = format!("{}-{}-history.csv", device_id, point_id);
    let disposition = format!("attachment; filename=\"{filename}\"");
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv".to_string()),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        csv,
    ))
}
