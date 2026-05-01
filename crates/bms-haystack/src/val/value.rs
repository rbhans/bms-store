use super::coord::Coord;
use super::datetime::{HDate, HDateTime, HTime};
use super::dict::Dict;
use super::grid::Grid;
use super::number::Number;
use super::ref_::Ref;

/// `XStr` — typed string. The `kind` names a foreign type whose value is
/// transported as a string (e.g. `XStr{type:"Bin","application/json"}`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct XStr {
    /// Foreign type name (e.g. `"Bin"`, `"Color"`).
    pub type_name: String,
    pub val: String,
}

/// All Haystack value kinds.
///
/// `null` is represented by `Option<Value>::None` at the carrier; `NA`,
/// `Remove`, and `Marker` are distinct singleton variants because Hayson
/// and Zinc distinguish them on the wire. Use [`crate::codec`] to convert
/// to/from a wire format — this enum has no `serde::Serialize` derive
/// because Haystack value encoding is non-trivial.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// `Marker` singleton — used as the value of a tag-only attribute.
    Marker,
    /// `NA` — "not available".
    NA,
    /// `Remove` — instruction to delete a tag (only valid as a write input).
    Remove,
    Bool(bool),
    Number(Number),
    Str(String),
    Uri(String),
    Ref(Ref),
    Symbol(String),
    XStr(XStr),
    Date(HDate),
    Time(HTime),
    DateTime(HDateTime),
    Coord(Coord),
    List(Vec<Value>),
    Dict(Dict),
    Grid(Box<Grid>),
}

impl Value {
    pub fn is_marker(&self) -> bool {
        matches!(self, Value::Marker)
    }
    pub fn is_na(&self) -> bool {
        matches!(self, Value::NA)
    }
    pub fn is_remove(&self) -> bool {
        matches!(self, Value::Remove)
    }
    pub fn as_bool(&self) -> Option<bool> {
        if let Value::Bool(b) = self {
            Some(*b)
        } else {
            None
        }
    }
    pub fn as_number(&self) -> Option<&Number> {
        if let Value::Number(n) = self {
            Some(n)
        } else {
            None
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        if let Value::Str(s) = self {
            Some(s.as_str())
        } else {
            None
        }
    }
    pub fn as_ref(&self) -> Option<&Ref> {
        if let Value::Ref(r) = self {
            Some(r)
        } else {
            None
        }
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}
impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::Number(Number::unitless(v))
    }
}
impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::Number(Number::unitless(v as f64))
    }
}
impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::Str(v.to_string())
    }
}
impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::Str(v)
    }
}
impl From<Number> for Value {
    fn from(v: Number) -> Self {
        Value::Number(v)
    }
}
impl From<Ref> for Value {
    fn from(v: Ref) -> Self {
        Value::Ref(v)
    }
}
impl From<Dict> for Value {
    fn from(v: Dict) -> Self {
        Value::Dict(v)
    }
}
impl From<Grid> for Value {
    fn from(v: Grid) -> Self {
        Value::Grid(Box::new(v))
    }
}
