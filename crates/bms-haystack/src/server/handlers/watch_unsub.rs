use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::Json;
use serde_json::Value as J;

use crate::server::content::ResponseBody;
use crate::server::handlers::common::{decode_request_grid, err_response, negotiate, single_row_grid};
use crate::server::watch::WatchId;
use crate::server::{AppState, HaystackError};
use crate::val::{Dict, Value};

pub async fn watch_unsub(
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
    let watch_id = match grid.meta.get("watchId") {
        Some(Value::Str(s)) => s.clone(),
        _ => {
            return err_response(
                ct,
                HaystackError::BadRequest("missing meta.watchId".into()),
            )
        }
    };
    state.watches.unsubscribe(&WatchId(watch_id.clone()));
    let mut row = Dict::default();
    row.insert("watchId", Value::Str(watch_id));
    row.marker("closed");
    ResponseBody::ok(single_row_grid(row), ct)
}
