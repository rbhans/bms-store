use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::http::HeaderMap;

use crate::ontology::GENERATED_GLOBALS;
use crate::server::content::ResponseBody;
use crate::server::handlers::common::negotiate;
use crate::server::AppState;
use crate::val::{Dict, Grid, Number, Value};

/// `/defs` — return all symbol definitions (the union of every spec and
/// every PhEntity global slot in the loaded ontology).
pub async fn defs(
    State(_): State<AppState>,
    headers: HeaderMap,
    q: Query<HashMap<String, String>>,
) -> ResponseBody {
    let ct = negotiate(&headers, &q);
    let mut rows = Vec::with_capacity(GENERATED_GLOBALS.len());
    for g in GENERATED_GLOBALS {
        let mut row = Dict::default();
        row.insert("def", Value::Symbol(format!("^{}", g.name)));
        row.insert("kind", Value::Str(g.kind.to_string()));
        row.insert("lib", Value::Str(g.lib.to_string()));
        if !g.doc.is_empty() {
            row.insert("doc", Value::Str(g.doc.to_string()));
        }
        if let Some(q) = g.quantity {
            row.insert("quantity", Value::Str(q.to_string()));
        }
        if let Some(of) = g.of_type {
            row.insert("of", Value::Str(of.to_string()));
        }
        if let Some(u) = g.unit {
            row.insert("unit", Value::Str(u.to_string()));
        }
        rows.push(row);
    }
    let mut grid = Grid::from_rows(rows);
    grid.meta.insert("count", Value::Number(Number::unitless(GENERATED_GLOBALS.len() as f64)));
    ResponseBody::ok(grid, ct)
}
