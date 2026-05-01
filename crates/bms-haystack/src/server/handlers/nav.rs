use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::http::HeaderMap;

use crate::server::content::ResponseBody;
use crate::server::handlers::common::{err_response, negotiate};
use crate::server::AppState;
use crate::val::Ref;

/// `/nav?navId=@id` — hierarchical browse. `navId` omitted ⇒ root.
pub async fn nav(
    State(state): State<AppState>,
    headers: HeaderMap,
    q: Query<HashMap<String, String>>,
) -> ResponseBody {
    let ct = negotiate(&headers, &q);
    let nav_id = q.get("navId").map(|s| {
        let trimmed = s.trim_start_matches('@');
        Ref::new(trimmed)
    });
    match state.haystack.nav(nav_id.as_ref()).await {
        Ok(grid) => ResponseBody::ok(grid, ct),
        Err(e) => err_response(ct, e),
    }
}
