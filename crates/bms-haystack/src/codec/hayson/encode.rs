use serde_json::{json, Map, Value as J};

use crate::val::{Col, Dict, Grid, Number, Ref, Value, XStr};

/// Encode a [`Value`] into its Hayson JSON form.
pub fn value_to_json(v: &Value) -> J {
    match v {
        Value::Marker => json!({"_kind": "marker"}),
        Value::NA => json!({"_kind": "na"}),
        Value::Remove => json!({"_kind": "remove"}),
        Value::Bool(b) => J::Bool(*b),
        Value::Str(s) => J::String(s.clone()),
        Value::Number(n) => number_to_json(n),
        Value::Uri(u) => json!({"_kind": "uri", "val": u}),
        Value::Ref(r) => ref_to_json(r),
        Value::Symbol(s) => json!({"_kind": "symbol", "val": s}),
        Value::XStr(x) => xstr_to_json(x),
        Value::Date(d) => json!({"_kind": "date", "val": d.to_iso()}),
        Value::Time(t) => json!({"_kind": "time", "val": t.to_iso()}),
        Value::DateTime(dt) => {
            json!({"_kind": "dateTime", "val": dt.to_iso(), "tz": dt.tz})
        }
        Value::Coord(c) => json!({"_kind": "coord", "lat": c.lat, "lng": c.lng}),
        Value::List(items) => J::Array(items.iter().map(value_to_json).collect()),
        Value::Dict(d) => J::Object(dict_to_object(d)),
        Value::Grid(g) => grid_to_json(g),
    }
}

fn number_to_json(n: &Number) -> J {
    match &n.unit {
        None => {
            // Unitless: bare JSON number.
            if let Some(i) = f64_as_i64(n.val) {
                J::Number(serde_json::Number::from(i))
            } else if let Some(num) = serde_json::Number::from_f64(n.val) {
                J::Number(num)
            } else {
                // NaN/Inf — Hayson uses `{"_kind":"number","val":"NaN"}`.
                let s = if n.val.is_nan() {
                    "NaN"
                } else if n.val.is_sign_positive() {
                    "INF"
                } else {
                    "-INF"
                };
                json!({"_kind": "number", "val": s})
            }
        }
        Some(u) => {
            if let Some(num) = serde_json::Number::from_f64(n.val) {
                json!({"_kind": "number", "val": num, "unit": u})
            } else {
                let s = if n.val.is_nan() {
                    "NaN"
                } else if n.val.is_sign_positive() {
                    "INF"
                } else {
                    "-INF"
                };
                json!({"_kind": "number", "val": s, "unit": u})
            }
        }
    }
}

fn ref_to_json(r: &Ref) -> J {
    match &r.dis {
        Some(dis) => json!({"_kind": "ref", "val": r.id, "dis": dis}),
        None => json!({"_kind": "ref", "val": r.id}),
    }
}

fn xstr_to_json(x: &XStr) -> J {
    json!({"_kind": "xstr", "type": x.type_name, "val": x.val})
}

/// Encode a [`Dict`] into a Hayson JSON object (without `_kind`).
pub fn dict_to_json(d: &Dict) -> J {
    J::Object(dict_to_object(d))
}

fn dict_to_object(d: &Dict) -> Map<String, J> {
    let mut out = Map::with_capacity(d.len());
    for (k, v) in d.iter() {
        out.insert(k.clone(), value_to_json(v));
    }
    out
}

/// Encode a [`Grid`] into Hayson form: `{_kind:"grid", meta, cols, rows}`.
pub fn grid_to_json(g: &Grid) -> J {
    let cols: Vec<J> = g.cols.iter().map(col_to_json).collect();
    let rows: Vec<J> = g.rows.iter().map(dict_to_json).collect();
    json!({
        "_kind": "grid",
        "meta": dict_to_json(&g.meta),
        "cols": cols,
        "rows": rows,
    })
}

fn col_to_json(c: &Col) -> J {
    let mut m = Map::with_capacity(2 + c.meta.len());
    m.insert("name".into(), J::String(c.name.clone()));
    for (k, v) in c.meta.iter() {
        m.insert(k.clone(), value_to_json(v));
    }
    J::Object(m)
}

fn f64_as_i64(v: f64) -> Option<i64> {
    if v.is_finite() && v.fract() == 0.0 && (i64::MIN as f64..=i64::MAX as f64).contains(&v) {
        Some(v as i64)
    } else {
        None
    }
}
