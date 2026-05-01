use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::Json;
use serde_json::Value as J;

use crate::server::content::ResponseBody;
use crate::server::handlers::common::{
    decode_request_grid, decode_value, err_response, extract_ref, negotiate, single_row_grid,
};
use crate::server::{AppState, HaystackError, PointWriteRequest};
use crate::val::Value;

pub async fn point_write(
    State(state): State<AppState>,
    headers: HeaderMap,
    q: Query<HashMap<String, String>>,
    Json(body): Json<J>,
) -> ResponseBody {
    let ct = negotiate(&headers, &q);
    let grid = match decode_request_grid(&body) {
        Ok(g) => g,
        Err(e) => return err_response(ct, e),
    };
    let row = match grid.rows.first() {
        Some(r) => r,
        None => {
            return err_response(
                ct,
                HaystackError::BadRequest("empty pointWrite grid".into()),
            )
        }
    };
    let id = match extract_ref(&grid, "id") {
        Ok(r) => r,
        Err(e) => return err_response(ct, e),
    };
    let level = match row.get("level") {
        Some(Value::Number(n)) => n.val as u8,
        _ => {
            return err_response(
                ct,
                HaystackError::BadRequest("missing `level` (1..=17)".into()),
            )
        }
    };
    if level == 0 || level > 17 {
        return err_response(
            ct,
            HaystackError::BadRequest(format!("level {level} outside 1..=17")),
        );
    }
    let val = match row.get("val") {
        None | Some(Value::NA) => None,
        Some(v) => Some(v.clone()),
    };
    // optional metadata
    let who = row.get("who").and_then(|v| v.as_str()).map(String::from);
    let duration_ms = match row.get("duration") {
        Some(Value::Number(n)) => Some(n.val as i64),
        _ => None,
    };

    // also accept top-level `val` JSON if not in row
    let val_top = body
        .as_object()
        .and_then(|m| m.get("val"))
        .map(decode_value)
        .transpose();
    let val_resolved = match (val, val_top) {
        (Some(v), _) => Some(v),
        (None, Ok(Some(v))) => Some(v),
        (None, Err(e)) => return err_response(ct, e),
        (None, Ok(None)) => None,
    };

    let req = PointWriteRequest {
        id,
        level,
        val: val_resolved,
        who,
        duration_ms,
    };

    match state.haystack.point_write(&req).await {
        Ok(d) => ResponseBody::ok(single_row_grid(d), ct),
        Err(e) => err_response(ct, e),
    }
}
