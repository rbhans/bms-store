use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::Json;
use serde_json::Value as J;

use crate::server::content::ResponseBody;
use crate::server::handlers::common::{decode_request_grid, err_response, extract_ref, negotiate, single_row_grid};
use crate::server::AppState;

/// `/hisWrite` — POST a Hayson grid where each row is `{ts, val}` and the
/// grid `meta.id` is the target point ref.
pub async fn his_write(
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
    let id = match grid.meta.get("id") {
        Some(crate::val::Value::Ref(r)) => r.clone(),
        _ => match extract_ref(&grid, "id") {
            Ok(r) => r,
            Err(e) => return err_response(ct, e),
        },
    };
    match state.haystack.his_write(&id, &grid).await {
        Ok(d) => ResponseBody::ok(single_row_grid(d), ct),
        Err(e) => err_response(ct, e),
    }
}
