//! In-memory evaluation of filter ASTs against a [`Dict`].

use super::ast::{CmpOp, FilterExpr, FilterValue, Path};
use crate::val::{Dict, Number, Value};

/// Resolves a `Ref` in arrow-path traversal to the target [`Dict`].
/// Implementations look up entities by id; an in-memory test resolver
/// might back the lookup with a `HashMap<String, Dict>`.
pub trait Resolver {
    fn resolve(&self, id: &str) -> Option<&Dict>;
}

/// No-op resolver — every ref returns `None`. Use when the filter doesn't
/// involve arrow paths.
pub struct NoResolver;
impl Resolver for NoResolver {
    fn resolve(&self, _id: &str) -> Option<&Dict> {
        None
    }
}

/// Evaluate a filter against a single dict.
pub fn eval(expr: &FilterExpr, dict: &Dict, resolver: &dyn Resolver) -> bool {
    match expr {
        FilterExpr::Has(path) => walk(path, dict, resolver).is_some(),
        FilterExpr::Not(inner) => !eval(inner, dict, resolver),
        FilterExpr::And(l, r) => eval(l, dict, resolver) && eval(r, dict, resolver),
        FilterExpr::Or(l, r) => eval(l, dict, resolver) || eval(r, dict, resolver),
        FilterExpr::Cmp(path, op, val) => match walk(path, dict, resolver) {
            Some(actual) => compare(actual, *op, val),
            None => false,
        },
    }
}

/// Walk a path through a dict (and any number of refs via the resolver).
/// Returns the leaf [`Value`] if all path segments resolve.
fn walk<'a>(path: &Path, root: &'a Dict, resolver: &'a dyn Resolver) -> Option<&'a Value> {
    if path.0.is_empty() {
        return None;
    }
    if path.0.len() == 1 {
        return root.get(&path.0[0]);
    }
    // For a multi-segment path, every segment EXCEPT the last must be a Ref.
    let mut current: &Dict = root;
    let prefix = path.arrow_prefix();
    for seg in prefix {
        let v = current.get(seg)?;
        let id = match v {
            Value::Ref(r) => &r.id,
            _ => return None,
        };
        current = resolver.resolve(id)?;
    }
    current.get(path.tail())
}

fn compare(actual: &Value, op: CmpOp, expected: &FilterValue) -> bool {
    match (actual, expected) {
        (Value::Bool(a), FilterValue::Bool(b)) => cmp_ord(a, b, op),
        (Value::Str(a), FilterValue::Str(b)) => cmp_ord(a.as_str(), b.as_str(), op),
        (Value::Ref(a), FilterValue::Ref(b)) => cmp_ord(a.id.as_str(), b.id.as_str(), op),
        (Value::Number(a), FilterValue::Number(b)) => cmp_number(a, b, op),
        // Cross-kind comparisons fail (false); equality of incompatible types is false.
        _ => false,
    }
}

fn cmp_ord<T: PartialOrd>(a: T, b: T, op: CmpOp) -> bool {
    match op {
        CmpOp::Eq => a == b,
        CmpOp::Ne => a != b,
        CmpOp::Lt => a < b,
        CmpOp::Le => a <= b,
        CmpOp::Gt => a > b,
        CmpOp::Ge => a >= b,
    }
}

fn cmp_number(a: &Number, b: &Number, op: CmpOp) -> bool {
    // Cross-unit comparisons are conservative: only allow exact unit match
    // (or both unitless). Mismatched units produce `false` so callers can
    // detect the gap at higher levels (e.g. via a query-time warning).
    if a.unit != b.unit {
        return false;
    }
    if a.val.is_nan() || b.val.is_nan() {
        return matches!(op, CmpOp::Ne);
    }
    cmp_ord(a.val, b.val, op)
}

// Marker test helpers
#[allow(dead_code)]
pub(crate) fn ref_id(v: &Value) -> Option<&str> {
    match v {
        Value::Ref(r) => Some(r.id.as_str()),
        _ => None,
    }
}
