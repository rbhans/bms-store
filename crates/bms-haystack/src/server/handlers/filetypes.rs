use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::http::HeaderMap;

use crate::server::content::ResponseBody;
use crate::server::handlers::common::negotiate;
use crate::server::AppState;
use crate::val::{Dict, Grid, Value};

/// `/filetypes` — list MIME types this server can read/write.
pub async fn filetypes(
    State(_): State<AppState>,
    headers: HeaderMap,
    q: Query<HashMap<String, String>>,
) -> ResponseBody {
    let ct = negotiate(&headers, &q);
    // For now: hayson + json out, hayson + json in. Zinc/CSV stubs land later.
    let rows = vec![
        ftype("hayson", "application/vnd.haystack+json", "Hayson JSON encoding"),
        ftype("json", "application/json", "Plain JSON encoding (alias of Hayson)"),
        ftype("zinc", "text/zinc", "Zinc text encoding (decode-only stub)"),
        ftype("csv", "text/csv", "CSV encoding (encode stub)"),
    ];
    ResponseBody::ok(Grid::from_rows(rows), ct)
}

fn ftype(name: &str, mime: &str, doc: &str) -> Dict {
    let mut d = Dict::default();
    d.insert("name", Value::Str(name.to_string()));
    d.insert("mime", Value::Str(mime.to_string()));
    d.insert("doc", Value::Str(doc.to_string()));
    d
}
