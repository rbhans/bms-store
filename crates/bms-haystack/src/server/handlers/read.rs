use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::Json;
use serde_json::Value as J;

use crate::filter::parse;
use crate::server::content::ResponseBody;
use crate::server::handlers::common::{decode_request_grid, err_response, negotiate};
use crate::server::AppState;

/// `GET /read?filter=<expr>&limit=<n>` — Haystack-conventional read.
pub async fn read_get(
    State(state): State<AppState>,
    headers: HeaderMap,
    q: Query<HashMap<String, String>>,
) -> ResponseBody {
    let ct = negotiate(&headers, &q);
    let filter_str = match q.get("filter") {
        Some(s) if !s.is_empty() => s.clone(),
        _ => {
            return err_response(
                ct,
                crate::server::HaystackError::BadRequest("missing `filter` query param".into()),
            )
        }
    };
    let limit = q
        .get("limit")
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|&n| n > 0);
    do_read(&state, ct, &filter_str, limit).await
}

/// `POST /read` — Hayson-encoded request grid with `filter` (and optional
/// `limit`) on the first row.
pub async fn read_post(
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
                crate::server::HaystackError::BadRequest("empty request grid".into()),
            )
        }
    };
    let filter_str = match row.get("filter") {
        Some(crate::val::Value::Str(s)) => s.clone(),
        _ => {
            return err_response(
                ct,
                crate::server::HaystackError::BadRequest(
                    "request row missing `filter` string".into(),
                ),
            )
        }
    };
    let limit = match row.get("limit") {
        Some(crate::val::Value::Number(n)) if n.unit.is_none() => Some(n.val as usize),
        _ => None,
    };
    do_read(&state, ct, &filter_str, limit).await
}

async fn do_read(
    state: &AppState,
    ct: crate::server::ContentType,
    filter_str: &str,
    limit: Option<usize>,
) -> ResponseBody {
    let expr = match parse(filter_str) {
        Ok(e) => e,
        Err(e) => {
            return err_response(
                ct,
                crate::server::HaystackError::BadRequest(format!("filter parse: {e}")),
            )
        }
    };
    match state.haystack.read(&expr, limit).await {
        Ok(grid) => ResponseBody::ok(grid, ct),
        Err(e) => err_response(ct, e),
    }
}
