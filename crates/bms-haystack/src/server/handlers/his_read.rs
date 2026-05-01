use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::http::HeaderMap;

use crate::server::content::ResponseBody;
use crate::server::handlers::common::{err_response, negotiate};
use crate::server::{AppState, HaystackError};
use crate::val::Ref;

/// `/hisRead?id=@x&range=...` — read a point history span.
pub async fn his_read(
    State(state): State<AppState>,
    headers: HeaderMap,
    q: Query<HashMap<String, String>>,
) -> ResponseBody {
    let ct = negotiate(&headers, &q);
    let id = match q.get("id") {
        Some(s) if !s.is_empty() => Ref::new(s.trim_start_matches('@')),
        _ => return err_response(ct, HaystackError::BadRequest("missing id".into())),
    };
    let range = q.get("range").cloned().unwrap_or_else(|| "today".into());
    match state.haystack.his_read(&id, &range).await {
        Ok(grid) => ResponseBody::ok(grid, ct),
        Err(e) => err_response(ct, e),
    }
}
