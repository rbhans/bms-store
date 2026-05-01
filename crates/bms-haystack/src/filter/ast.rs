use crate::val::{Number, Ref};

/// A dotted/arrow path through tag space, e.g. `equipRef->siteRef->geoCity`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path(pub Vec<String>);

impl Path {
    pub fn single(name: impl Into<String>) -> Self {
        Self(vec![name.into()])
    }

    /// Returns the final segment of the path.
    pub fn tail(&self) -> &str {
        self.0.last().map(String::as_str).unwrap_or("")
    }

    /// Returns the leading segments (everything except the tail) — these are
    /// Ref-typed tags that must be dereferenced to reach the final one.
    pub fn arrow_prefix(&self) -> &[String] {
        let n = self.0.len();
        if n == 0 {
            &[]
        } else {
            &self.0[..n - 1]
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// A literal value usable on the right-hand side of a filter comparison.
/// Filter values are a subset of [`crate::val::Value`] — Marker / NA / List
/// / Dict / Grid don't appear in filter literals.
#[derive(Debug, Clone, PartialEq)]
pub enum FilterValue {
    Bool(bool),
    Number(Number),
    Str(String),
    Ref(Ref),
}

#[derive(Debug, Clone, PartialEq)]
pub enum FilterExpr {
    /// `tagName` — entity has the named tag (any value).
    Has(Path),
    /// `path == val`, `path != val`, etc.
    Cmp(Path, CmpOp, FilterValue),
    And(Box<FilterExpr>, Box<FilterExpr>),
    Or(Box<FilterExpr>, Box<FilterExpr>),
    /// `not <expr>`. `not foo` lowers to `Not(Has("foo"))` meaning "foo is
    /// absent" — important for SQL push-down.
    Not(Box<FilterExpr>),
}
