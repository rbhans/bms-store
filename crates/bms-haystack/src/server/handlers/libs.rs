use std::collections::{BTreeMap, HashMap};

use axum::extract::{Query, State};
use axum::http::HeaderMap;

use crate::ontology::GENERATED_SPECS;
use crate::server::content::ResponseBody;
use crate::server::handlers::common::negotiate;
use crate::server::AppState;
use crate::val::{Dict, Grid, Number, Value};
use crate::xeto::version::VENDORED_PH_VERSION;

/// `/libs` — list xeto libraries currently loaded into the server.
/// One row per library with name, version, and number of contained specs.
pub async fn libs(
    State(_): State<AppState>,
    headers: HeaderMap,
    q: Query<HashMap<String, String>>,
) -> ResponseBody {
    let ct = negotiate(&headers, &q);

    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    for s in GENERATED_SPECS {
        *counts.entry(s.lib).or_insert(0) += 1;
    }

    let rows: Vec<Dict> = counts
        .into_iter()
        .map(|(name, count)| {
            let mut d = Dict::default();
            d.insert("name", Value::Str(name.to_string()));
            d.insert("version", Value::Str(VENDORED_PH_VERSION.to_string()));
            d.insert("specCount", Value::Number(Number::unitless(count as f64)));
            d.marker("loaded");
            d
        })
        .collect();

    ResponseBody::ok(Grid::from_rows(rows), ct)
}
