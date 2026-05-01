use std::collections::HashMap;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::Json;
use serde_json::Value as J;

use crate::server::content::ResponseBody;
use crate::server::handlers::common::{decode_request_grid, err_response, negotiate, single_row_grid};
use crate::server::{AppState, HaystackError};
use crate::val::{Dict, Number, Ref, Value};

pub async fn watch_sub(
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
    // Refs to subscribe come from each row's `id` field.
    let mut ids = Vec::with_capacity(grid.rows.len());
    for row in &grid.rows {
        match row.get("id") {
            Some(Value::Ref(r)) => ids.push(r.clone()),
            Some(Value::Str(s)) => ids.push(Ref::new(s.trim_start_matches('@'))),
            _ => {}
        }
    }
    if ids.is_empty() {
        return err_response(
            ct,
            HaystackError::BadRequest("watchSub: no `id` refs in grid".into()),
        );
    }

    // Optional lease (in ms) from grid meta.
    let lease = grid.meta.get("lease").and_then(|v| {
        if let Value::Number(n) = v {
            Some(Duration::from_millis(n.val as u64))
        } else {
            None
        }
    });

    let snapshot = state.haystack.snapshot_entities().await;
    let handle = state.watches.subscribe(ids, snapshot, lease);

    let mut row = Dict::default();
    row.insert("watchId", Value::Str(handle.id.0));
    row.insert("lease", Value::Number(Number::with_unit(handle.lease_ms as f64, "ms")));
    ResponseBody::ok(single_row_grid(row), ct)
}
