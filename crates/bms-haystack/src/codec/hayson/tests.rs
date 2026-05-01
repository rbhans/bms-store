use chrono::{DateTime, TimeZone, Utc};

use crate::codec::hayson::{json_to_value, value_to_json, Hayson};
use crate::codec::Codec;
use crate::val::{Coord, Dict, Grid, HDate, HDateTime, Number, Ref, Value, XStr};

fn roundtrip(v: Value) {
    let j = value_to_json(&v);
    let back = json_to_value(&j).expect("decode");
    assert_eq!(v, back, "value round-trip mismatch via {j}");
}

#[test]
fn marker_na_remove() {
    roundtrip(Value::Marker);
    roundtrip(Value::NA);
    roundtrip(Value::Remove);
}

#[test]
fn bool_str() {
    roundtrip(Value::Bool(true));
    roundtrip(Value::Bool(false));
    roundtrip(Value::Str("hello".into()));
}

#[test]
fn unitless_number() {
    roundtrip(Value::Number(Number::unitless(42.0)));
    roundtrip(Value::Number(Number::unitless(3.14)));
    roundtrip(Value::Number(Number::unitless(-1.0)));
}

#[test]
fn unit_number() {
    roundtrip(Value::Number(Number::with_unit(72.0, "degF")));
    roundtrip(Value::Number(Number::with_unit(0.5, "%")));
    roundtrip(Value::Number(Number::with_unit(1234.56, "kW")));
}

#[test]
fn number_special_values() {
    let nan = value_to_json(&Value::Number(Number::with_unit(f64::NAN, "x")));
    assert_eq!(nan["val"], "NaN");
    let inf = value_to_json(&Value::Number(Number::with_unit(f64::INFINITY, "x")));
    assert_eq!(inf["val"], "INF");
}

#[test]
fn ref_with_and_without_dis() {
    roundtrip(Value::Ref(Ref::new("ahu-1")));
    roundtrip(Value::Ref(Ref::with_dis("ahu-1", "AHU 1")));
}

#[test]
fn uri_symbol_xstr() {
    roundtrip(Value::Uri("https://example.org".into()));
    roundtrip(Value::Symbol("^point".into()));
    roundtrip(Value::XStr(XStr {
        type_name: "Bin".into(),
        val: "application/json".into(),
    }));
}

#[test]
fn date_time_datetime() {
    roundtrip(Value::Date(HDate::parse("2025-12-31").unwrap()));
    let dt: DateTime<Utc> = Utc.with_ymd_and_hms(2025, 6, 15, 12, 30, 0).unwrap();
    roundtrip(Value::DateTime(HDateTime::new(dt, "UTC")));
}

#[test]
fn coord() {
    roundtrip(Value::Coord(Coord::new(37.7749, -122.4194)));
}

#[test]
fn list_and_dict() {
    let list = Value::List(vec![
        Value::Bool(true),
        Value::Number(Number::unitless(1.0)),
        Value::Str("x".into()),
    ]);
    roundtrip(list);

    let mut d = Dict::default();
    d.marker("ahu");
    d.insert("dis", "AHU 1");
    d.insert("temp", Number::with_unit(72.0, "degF"));
    roundtrip(Value::Dict(d));
}

#[test]
fn grid_round_trip() {
    let mut row = Dict::default();
    row.marker("ahu");
    row.marker("equip");
    row.insert("dis", "AHU 1");
    let grid = Grid::from_rows(vec![row]);

    let bytes = Hayson::encode_grid(&grid).expect("encode");
    let back = Hayson::decode_grid(&bytes).expect("decode");
    assert_eq!(grid, back);
}

#[test]
fn unknown_kind_errors() {
    let j: serde_json::Value = serde_json::json!({"_kind": "weird", "val": "x"});
    let err = json_to_value(&j).unwrap_err();
    assert!(format!("{err}").contains("unknown _kind"));
}
