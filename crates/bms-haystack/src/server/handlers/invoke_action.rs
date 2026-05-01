use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::Json;
use serde_json::Value as J;

use crate::server::content::ResponseBody;
use crate::server::handlers::common::{decode_request_grid, err_response, extract_ref, negotiate};
use crate::server::{AppState, HaystackError};
use crate::val::Value;

pub async fn invoke_action(
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
    let id = match extract_ref(&grid, "id") {
        Ok(r) => r,
        Err(e) => return err_response(ct, e),
    };
    let row = grid.rows.first().expect("checked non-empty in extract_ref");
    let action = match row.get("action") {
        Some(Value::Str(s)) => s.clone(),
        _ => {
            return err_response(
                ct,
                HaystackError::BadRequest("missing `action` string".into()),
            )
        }
    };
    let args = row
        .get("args")
        .and_then(|v| if let Value::Dict(d) = v { Some(d.clone()) } else { None })
        .unwrap_or_default();
    match state.haystack.invoke_action(&id, &action, &args).await {
        Ok(grid) => ResponseBody::ok(grid, ct),
        Err(e) => err_response(ct, e),
    }
}
