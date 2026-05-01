use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::http::HeaderMap;

use crate::server::handlers::common::{negotiate, single_row_grid};
use crate::server::content::ResponseBody;
use crate::server::AppState;

pub async fn about(
    State(state): State<AppState>,
    headers: HeaderMap,
    q: Query<HashMap<String, String>>,
) -> ResponseBody {
    let ct = negotiate(&headers, &q);
    let dict = state.haystack.about().await;
    ResponseBody::ok(single_row_grid(dict), ct)
}
