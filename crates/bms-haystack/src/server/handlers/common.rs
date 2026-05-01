//! Shared helpers used by every handler.

use std::collections::HashMap;

use axum::extract::Query;
use axum::http::{HeaderMap, StatusCode};

use crate::codec::hayson::{json_to_grid, json_to_value};
use crate::server::content::{error_grid, ContentType, ResponseBody};
use crate::server::state::HaystackError;
use crate::val::{Dict, Grid, Ref, Value};

/// Pull the negotiated content type from `Accept` and the optional
/// `?format=` query parameter.
pub fn negotiate(headers: &HeaderMap, q: &Query<HashMap<String, String>>) -> ContentType {
    ContentType::from_headers(headers, q.get("format").map(String::as_str))
}

/// Wrap a `HaystackError` in an HTTP error response (Hayson error grid).
pub fn err_response(ct: ContentType, e: HaystackError) -> ResponseBody {
    let status = match e {
        HaystackError::NotFound => StatusCode::NOT_FOUND,
        HaystackError::NotImplemented => StatusCode::NOT_IMPLEMENTED,
        HaystackError::BadRequest(_) => StatusCode::BAD_REQUEST,
        HaystackError::Backend(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    ResponseBody {
        status,
        content_type: ct,
        grid: error_grid(e.to_string()),
    }
}

/// Decode a JSON request body into a one-row grid (Hayson `_kind:"grid"`)
/// OR — for clients that send a bare object — a one-row grid containing
/// that object as the first row.
pub fn decode_request_grid(body: &serde_json::Value) -> Result<Grid, HaystackError> {
    if let Some(map) = body.as_object() {
        match map.get("_kind").and_then(|k| k.as_str()) {
            Some("grid") => json_to_grid(body)
                .map_err(|e| HaystackError::BadRequest(format!("invalid grid: {e}"))),
            _ => {
                // Treat the bare object as a one-row request grid.
                let row = crate::codec::hayson::json_to_dict(body)
                    .map_err(|e| HaystackError::BadRequest(format!("invalid dict: {e}")))?;
                Ok(Grid::from_rows(vec![row]))
            }
        }
    } else {
        Err(HaystackError::BadRequest(
            "request body must be a JSON object".into(),
        ))
    }
}

/// Read a `Ref` value from the first row of a grid by column name.
pub fn extract_ref(grid: &Grid, col: &str) -> Result<Ref, HaystackError> {
    let row = grid
        .rows
        .first()
        .ok_or_else(|| HaystackError::BadRequest("missing row".into()))?;
    match row.get(col) {
        Some(Value::Ref(r)) => Ok(r.clone()),
        Some(Value::Str(s)) => Ok(Ref::new(s)),
        _ => Err(HaystackError::BadRequest(format!(
            "missing or invalid `{col}` ref"
        ))),
    }
}

/// Convenience: build a single-row grid from a Dict.
pub fn single_row_grid(d: Dict) -> Grid {
    Grid::from_rows(vec![d])
}

/// Decode a JSON value (e.g. point-write `val` field) into a Haystack
/// `Value`, accepting bare scalars or kinded objects.
pub fn decode_value(j: &serde_json::Value) -> Result<Value, HaystackError> {
    json_to_value(j).map_err(|e| HaystackError::BadRequest(format!("invalid value: {e}")))
}
