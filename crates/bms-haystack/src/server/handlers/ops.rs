use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::http::HeaderMap;

use crate::server::content::ResponseBody;
use crate::server::handlers::common::negotiate;
use crate::server::AppState;
use crate::val::{Dict, Grid, Value};

/// `/ops` — advertise the standard Haystack ops this server implements.
/// Cargo features could trim this list at compile time; for now it always
/// returns the full set.
pub async fn ops(
    State(_): State<AppState>,
    headers: HeaderMap,
    q: Query<HashMap<String, String>>,
) -> ResponseBody {
    let ct = negotiate(&headers, &q);
    let names = [
        ("about", "Server identity and version"),
        ("defs", "All symbol definitions"),
        ("libs", "Loaded xeto libraries"),
        ("ops", "List of supported operations"),
        ("filetypes", "List of supported wire formats"),
        ("read", "Filter and return entities"),
        ("nav", "Hierarchical navigation"),
        ("watchSub", "Open a watch on entities"),
        ("watchUnsub", "Close an open watch"),
        ("watchPoll", "Read changes since previous poll"),
        ("pointWrite", "Write a point at a priority level"),
        ("hisRead", "Read point history"),
        ("hisWrite", "Append samples to point history"),
        ("invokeAction", "Invoke a named action on an entity"),
    ];
    let rows: Vec<Dict> = names
        .iter()
        .map(|(n, d)| {
            let mut row = Dict::default();
            row.insert("def", Value::Symbol(format!("^op:{}", n)));
            row.insert("name", Value::Str(n.to_string()));
            row.insert("doc", Value::Str(d.to_string()));
            row
        })
        .collect();
    ResponseBody::ok(Grid::from_rows(rows), ct)
}
