use serde_json::Value as J;

use crate::codec::CodecError;
use crate::val::{Coord, Dict, Grid, HDate, HDateTime, HTime, Number, Ref, Value, XStr};
use crate::val::grid::Col;

/// Decode any Hayson JSON value into a [`Value`].
pub fn json_to_value(v: &J) -> Result<Value, CodecError> {
    match v {
        J::Null => Err(CodecError::Decode("null is not a Haystack value".into())),
        J::Bool(b) => Ok(Value::Bool(*b)),
        J::String(s) => Ok(Value::Str(s.clone())),
        J::Number(n) => Ok(Value::Number(Number::unitless(
            n.as_f64()
                .ok_or_else(|| CodecError::Decode(format!("non-finite JSON number: {n}")))?,
        ))),
        J::Array(items) => {
            let list = items
                .iter()
                .map(json_to_value)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::List(list))
        }
        J::Object(map) => {
            // Distinguish a kinded value from a plain Dict by the presence of `_kind`.
            match map.get("_kind").and_then(|k| k.as_str()) {
                None => json_to_dict(v).map(Value::Dict),
                Some("marker") => Ok(Value::Marker),
                Some("na") => Ok(Value::NA),
                Some("remove") => Ok(Value::Remove),
                Some("number") => decode_number(map).map(Value::Number),
                Some("uri") => Ok(Value::Uri(get_str(map, "val")?.to_string())),
                Some("ref") => Ok(Value::Ref(decode_ref(map)?)),
                Some("symbol") => Ok(Value::Symbol(get_str(map, "val")?.to_string())),
                Some("xstr") => Ok(Value::XStr(decode_xstr(map)?)),
                Some("date") => HDate::parse(get_str(map, "val")?)
                    .map(Value::Date)
                    .ok_or_else(|| CodecError::Decode("invalid date".into())),
                Some("time") => HTime::parse(get_str(map, "val")?)
                    .map(Value::Time)
                    .ok_or_else(|| CodecError::Decode("invalid time".into())),
                Some("dateTime") => decode_datetime(map).map(Value::DateTime),
                Some("coord") => decode_coord(map).map(Value::Coord),
                Some("grid") => json_to_grid(v).map(|g| Value::Grid(Box::new(g))),
                Some(other) => Err(CodecError::Decode(format!(
                    "unknown _kind: {other}"
                ))),
            }
        }
    }
}

/// Decode a JSON object (without `_kind`) into a [`Dict`].
pub fn json_to_dict(v: &J) -> Result<Dict, CodecError> {
    let map = v
        .as_object()
        .ok_or_else(|| CodecError::Decode("dict requires object".into()))?;
    let mut d = Dict::default();
    for (k, val) in map {
        if k == "_kind" {
            continue; // ignore on input even if present
        }
        d.tags.insert(k.clone(), json_to_value(val)?);
    }
    Ok(d)
}

/// Decode a Hayson grid object.
pub fn json_to_grid(v: &J) -> Result<Grid, CodecError> {
    let map = v
        .as_object()
        .ok_or_else(|| CodecError::Decode("grid requires object".into()))?;
    if map.get("_kind").and_then(|k| k.as_str()) != Some("grid") {
        return Err(CodecError::Decode("grid missing _kind:grid".into()));
    }
    let meta = match map.get("meta") {
        Some(m) => json_to_dict(m)?,
        None => Dict::default(),
    };
    let cols = map
        .get("cols")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .map(|c| -> Result<Col, CodecError> {
                    let cmap = c
                        .as_object()
                        .ok_or_else(|| CodecError::Decode("col requires object".into()))?;
                    let name = cmap
                        .get("name")
                        .and_then(|n| n.as_str())
                        .ok_or_else(|| CodecError::Decode("col missing name".into()))?
                        .to_string();
                    let mut col_meta = Dict::default();
                    for (k, val) in cmap {
                        if k == "name" {
                            continue;
                        }
                        col_meta.tags.insert(k.clone(), json_to_value(val)?);
                    }
                    Ok(Col {
                        name,
                        meta: col_meta,
                    })
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();

    let rows = map
        .get("rows")
        .and_then(|r| r.as_array())
        .map(|arr| arr.iter().map(json_to_dict).collect::<Result<Vec<_>, _>>())
        .transpose()?
        .unwrap_or_default();

    Ok(Grid { meta, cols, rows })
}

fn get_str<'a>(
    map: &'a serde_json::Map<String, J>,
    key: &str,
) -> Result<&'a str, CodecError> {
    map.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| CodecError::Decode(format!("missing string field {key}")))
}

fn decode_number(map: &serde_json::Map<String, J>) -> Result<Number, CodecError> {
    let val = map
        .get("val")
        .ok_or_else(|| CodecError::Decode("number missing val".into()))?;
    let f = match val {
        J::Number(n) => n
            .as_f64()
            .ok_or_else(|| CodecError::Decode("number val not finite".into()))?,
        J::String(s) => match s.as_str() {
            "NaN" => f64::NAN,
            "INF" => f64::INFINITY,
            "-INF" => f64::NEG_INFINITY,
            other => other
                .parse::<f64>()
                .map_err(|_| CodecError::Decode(format!("bad number string: {other}")))?,
        },
        _ => return Err(CodecError::Decode("number val type".into())),
    };
    let unit = map.get("unit").and_then(|u| u.as_str()).map(String::from);
    Ok(Number { val: f, unit })
}

fn decode_ref(map: &serde_json::Map<String, J>) -> Result<Ref, CodecError> {
    let id = get_str(map, "val")?.to_string();
    let dis = map.get("dis").and_then(|d| d.as_str()).map(String::from);
    Ok(Ref { id, dis })
}

fn decode_xstr(map: &serde_json::Map<String, J>) -> Result<XStr, CodecError> {
    let type_name = get_str(map, "type")?.to_string();
    let val = get_str(map, "val")?.to_string();
    Ok(XStr { type_name, val })
}

fn decode_datetime(map: &serde_json::Map<String, J>) -> Result<HDateTime, CodecError> {
    let val = get_str(map, "val")?;
    let tz = map.get("tz").and_then(|t| t.as_str()).unwrap_or("UTC");
    HDateTime::parse(val, tz).ok_or_else(|| CodecError::Decode("invalid dateTime".into()))
}

fn decode_coord(map: &serde_json::Map<String, J>) -> Result<Coord, CodecError> {
    let lat = map
        .get("lat")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| CodecError::Decode("coord missing lat".into()))?;
    let lng = map
        .get("lng")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| CodecError::Decode("coord missing lng".into()))?;
    Ok(Coord { lat, lng })
}
