use std::collections::HashMap;

use super::ast::{CmpOp, FilterExpr, FilterValue, Path};
use super::eval::{eval, NoResolver, Resolver};
use super::parser::parse;
use crate::val::{Dict, Number, Ref, Value};

fn d(pairs: &[(&str, Value)]) -> Dict {
    let mut d = Dict::default();
    for (k, v) in pairs {
        d.tags.insert((*k).to_string(), v.clone());
    }
    d
}

struct MapResolver(HashMap<String, Dict>);
impl Resolver for MapResolver {
    fn resolve(&self, id: &str) -> Option<&Dict> {
        self.0.get(id)
    }
}

// ---- parser ----

#[test]
fn parse_has() {
    let e = parse("ahu").unwrap();
    assert_eq!(e, FilterExpr::Has(Path::single("ahu")));
}

#[test]
fn parse_not() {
    let e = parse("not ahu").unwrap();
    assert_eq!(e, FilterExpr::Not(Box::new(FilterExpr::Has(Path::single("ahu")))));
}

#[test]
fn parse_and() {
    let e = parse("ahu and equip").unwrap();
    if let FilterExpr::And(_, _) = e {
    } else {
        panic!("expected And, got {e:?}");
    }
}

#[test]
fn parse_or_chain() {
    let e = parse("a or b or c").unwrap();
    // Left-associative: ((a or b) or c)
    match e {
        FilterExpr::Or(left, _) => {
            assert!(matches!(*left, FilterExpr::Or(_, _)));
        }
        _ => panic!("expected Or"),
    }
}

#[test]
fn parse_precedence_and_over_or() {
    // a or b and c → a or (b and c)
    let e = parse("a or b and c").unwrap();
    match e {
        FilterExpr::Or(_, right) => {
            assert!(matches!(*right, FilterExpr::And(_, _)));
        }
        _ => panic!("expected Or"),
    }
}

#[test]
fn parse_parens() {
    let e = parse("(a or b) and c").unwrap();
    if let FilterExpr::And(left, _) = e {
        assert!(matches!(*left, FilterExpr::Or(_, _)));
    } else {
        panic!("expected And");
    }
}

#[test]
fn parse_cmp() {
    let e = parse("temp >= 70").unwrap();
    if let FilterExpr::Cmp(p, op, v) = e {
        assert_eq!(p.tail(), "temp");
        assert_eq!(op, CmpOp::Ge);
        assert_eq!(v, FilterValue::Number(Number::unitless(70.0)));
    } else {
        panic!("expected Cmp");
    }
}

#[test]
fn parse_cmp_unit_number() {
    let e = parse("temp == 72°F").unwrap();
    if let FilterExpr::Cmp(_, op, FilterValue::Number(n)) = e {
        assert_eq!(op, CmpOp::Eq);
        assert_eq!(n.val, 72.0);
        assert_eq!(n.unit.as_deref(), Some("°F"));
    } else {
        panic!("expected Cmp with unit");
    }
}

#[test]
fn parse_cmp_string() {
    let e = parse(r#"dis == "Building 1""#).unwrap();
    if let FilterExpr::Cmp(_, _, FilterValue::Str(s)) = e {
        assert_eq!(s, "Building 1");
    } else {
        panic!("expected string cmp");
    }
}

#[test]
fn parse_cmp_ref() {
    let e = parse("equipRef == @ahu-1").unwrap();
    if let FilterExpr::Cmp(_, _, FilterValue::Ref(r)) = e {
        assert_eq!(r.id, "ahu-1");
    } else {
        panic!("expected ref cmp");
    }
}

#[test]
fn parse_arrow_path() {
    let e = parse("equipRef->siteRef").unwrap();
    if let FilterExpr::Has(p) = e {
        assert_eq!(p.0, vec!["equipRef".to_string(), "siteRef".to_string()]);
    } else {
        panic!("expected Has with arrow path");
    }
}

#[test]
fn parse_trailing_garbage_rejected() {
    assert!(parse("ahu garbage").is_err());
}

// ---- eval ----

#[test]
fn eval_has() {
    let dict = d(&[("ahu", Value::Marker), ("dis", Value::Str("AHU 1".into()))]);
    let f = parse("ahu").unwrap();
    assert!(eval(&f, &dict, &NoResolver));
    let f2 = parse("vav").unwrap();
    assert!(!eval(&f2, &dict, &NoResolver));
}

#[test]
fn eval_and_or_not() {
    let dict = d(&[("ahu", Value::Marker), ("equip", Value::Marker)]);
    let f = parse("ahu and equip").unwrap();
    assert!(eval(&f, &dict, &NoResolver));
    let f = parse("ahu and vav").unwrap();
    assert!(!eval(&f, &dict, &NoResolver));
    let f = parse("ahu or vav").unwrap();
    assert!(eval(&f, &dict, &NoResolver));
    let f = parse("not vav").unwrap();
    assert!(eval(&f, &dict, &NoResolver));
    let f = parse("not ahu").unwrap();
    assert!(!eval(&f, &dict, &NoResolver));
}

#[test]
fn eval_cmp_number() {
    let dict = d(&[("temp", Value::Number(Number::with_unit(72.0, "degF")))]);
    let f = parse("temp >= 72°F").unwrap();
    assert!(!eval(&f, &dict, &NoResolver), "unit mismatch should be false");

    // Match exact unit
    let dict2 = d(&[("temp", Value::Number(Number::with_unit(72.0, "°F")))]);
    let f = parse("temp >= 72°F").unwrap();
    assert!(eval(&f, &dict2, &NoResolver));
}

#[test]
fn eval_cmp_str() {
    let dict = d(&[("dis", Value::Str("AHU 1".into()))]);
    let f = parse(r#"dis == "AHU 1""#).unwrap();
    assert!(eval(&f, &dict, &NoResolver));
    let f = parse(r#"dis != "VAV 1""#).unwrap();
    assert!(eval(&f, &dict, &NoResolver));
}

#[test]
fn eval_arrow_path() {
    // ahu-1 has equipRef → site-1; site-1 has geoCity = "Boston"
    let mut store = HashMap::new();
    store.insert(
        "site-1".to_string(),
        d(&[("geoCity", Value::Str("Boston".into()))]),
    );
    let resolver = MapResolver(store);

    let dict = d(&[
        ("ahu", Value::Marker),
        ("siteRef", Value::Ref(Ref::new("site-1"))),
    ]);
    let f = parse(r#"siteRef->geoCity == "Boston""#).unwrap();
    assert!(eval(&f, &dict, &resolver));
}

#[test]
fn eval_not_present_is_false() {
    let dict = d(&[]);
    let f = parse("missingTag == 5").unwrap();
    assert!(!eval(&f, &dict, &NoResolver));
}
