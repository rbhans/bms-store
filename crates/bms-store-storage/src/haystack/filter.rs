//! Haystack 4 filter language parser and evaluator.
//!
//! Implements a subset of the Haystack 4 filter grammar:
//!
//! ```text
//! filter    := orExpr
//! orExpr    := andExpr ( "or" andExpr )*
//! andExpr   := termExpr ( "and" termExpr )*
//! termExpr  := "not"? unaryExpr
//! unaryExpr := "(" filter ")" | cmpExpr | hasMarker
//! cmpExpr   := tagName op value
//! op        := "==" | "!=" | "<" | ">" | "<=" | ">="
//! hasMarker := tagName
//! value     := stringLit | numberLit | "true" | "false" | "now" | refLit
//! tagName   := identifier
//! stringLit := '"' chars '"'
//! refLit    := "@" identifier
//! ```
//!
//! # Examples
//!
//! ```rust
//! use std::collections::HashMap;
//! use bms_store_storage::haystack::filter::{parse_filter, matches};
//!
//! let expr = parse_filter("temp and air").unwrap();
//! let mut tags: HashMap<String, Option<String>> = HashMap::new();
//! tags.insert("temp".into(), None);
//! tags.insert("air".into(), None);
//! assert!(matches(&expr, &tags));
//! ```

use std::collections::HashMap;
use std::fmt;

// ---------------------------------------------------------------------------
// Public AST types
// ---------------------------------------------------------------------------

/// A parsed Haystack filter expression.
#[derive(Debug, Clone, PartialEq)]
pub enum FilterExpr {
    /// Tag presence check: entity must have the named tag (any value, including marker).
    Has(String),
    /// Comparison: tag op value.
    Cmp {
        tag: String,
        op: CmpOp,
        value: FilterValue,
    },
    /// Logical AND of two expressions.
    And(Box<FilterExpr>, Box<FilterExpr>),
    /// Logical OR of two expressions.
    Or(Box<FilterExpr>, Box<FilterExpr>),
    /// Logical NOT of an expression.
    Not(Box<FilterExpr>),
}

/// Comparison operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
}

/// Literal values in filter expressions.
#[derive(Debug, Clone, PartialEq)]
pub enum FilterValue {
    Str(String),
    Num(f64),
    Bool(bool),
    /// A ref literal: `@entity-id`.
    Ref(String),
    /// The special `now` keyword (matches current time; treated as a
    /// timestamp sentinel — evaluators may expand it as needed).
    Now,
}

// ---------------------------------------------------------------------------
// Parse error
// ---------------------------------------------------------------------------

/// Error type for filter parsing failures.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub offset: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "parse error at offset {}: {}", self.offset, self.message)
    }
}

impl std::error::Error for ParseError {}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Parse a Haystack filter string into a [`FilterExpr`].
///
/// Returns a [`ParseError`] if the input is invalid or contains unsupported syntax.
pub fn parse_filter(input: &str) -> Result<FilterExpr, ParseError> {
    let mut parser = Parser::new(input);
    let expr = parser.parse_or()?;
    parser.skip_ws();
    if parser.pos < parser.input.len() {
        return Err(ParseError {
            message: format!(
                "unexpected token '{}'",
                &parser.input[parser.pos..].chars().next().unwrap_or('?')
            ),
            offset: parser.pos,
        });
    }
    Ok(expr)
}

/// Evaluate a [`FilterExpr`] against an entity's tags map.
///
/// The tags map uses `tag_name → Option<String>`:
/// - `None` = marker tag (presence only)
/// - `Some(value)` = value tag
///
/// For comparison expressions the tag value is compared as a string (for
/// `==`/`!=`) or parsed as f64 for numeric comparisons.
pub fn matches(expr: &FilterExpr, entity_tags: &HashMap<String, Option<String>>) -> bool {
    eval(expr, entity_tags)
}

// ---------------------------------------------------------------------------
// Evaluator
// ---------------------------------------------------------------------------

fn eval(expr: &FilterExpr, tags: &HashMap<String, Option<String>>) -> bool {
    match expr {
        FilterExpr::Has(name) => tags.contains_key(name.as_str()),
        FilterExpr::Cmp { tag, op, value } => eval_cmp(tag, *op, value, tags),
        FilterExpr::And(a, b) => eval(a, tags) && eval(b, tags),
        FilterExpr::Or(a, b) => eval(a, tags) || eval(b, tags),
        FilterExpr::Not(inner) => !eval(inner, tags),
    }
}

fn eval_cmp(
    tag: &str,
    op: CmpOp,
    value: &FilterValue,
    tags: &HashMap<String, Option<String>>,
) -> bool {
    let tag_val = match tags.get(tag) {
        Some(v) => v,
        None => return false, // tag absent — no comparison possible
    };

    match value {
        FilterValue::Bool(b) => {
            let tag_str = match tag_val {
                Some(s) => s.as_str(),
                None => return false, // marker tag; can't compare to bool
            };
            let tag_bool = matches!(tag_str, "true" | "1" | "on" | "yes");
            match op {
                CmpOp::Eq => tag_bool == *b,
                CmpOp::Ne => tag_bool != *b,
                _ => false, // ordering doesn't make sense for bool
            }
        }
        FilterValue::Str(s) => {
            let tag_str = match tag_val {
                Some(v) => v.as_str(),
                None => return false,
            };
            match op {
                CmpOp::Eq => tag_str == s.as_str(),
                CmpOp::Ne => tag_str != s.as_str(),
                CmpOp::Lt => tag_str < s.as_str(),
                CmpOp::Gt => tag_str > s.as_str(),
                CmpOp::Le => tag_str <= s.as_str(),
                CmpOp::Ge => tag_str >= s.as_str(),
            }
        }
        FilterValue::Num(n) => {
            let tag_str = match tag_val {
                Some(v) => v.as_str(),
                None => return false,
            };
            let tag_num: f64 = match tag_str.parse() {
                Ok(v) => v,
                Err(_) => return false,
            };
            match op {
                CmpOp::Eq => (tag_num - n).abs() < f64::EPSILON,
                CmpOp::Ne => (tag_num - n).abs() >= f64::EPSILON,
                CmpOp::Lt => tag_num < *n,
                CmpOp::Gt => tag_num > *n,
                CmpOp::Le => tag_num <= *n,
                CmpOp::Ge => tag_num >= *n,
            }
        }
        FilterValue::Ref(r) => {
            let tag_str = match tag_val {
                Some(v) => v.as_str(),
                None => return false,
            };
            // Refs are compared by stripping leading "@" from stored refs
            let stored = tag_str.strip_prefix('@').unwrap_or(tag_str);
            let query = r.strip_prefix('@').unwrap_or(r.as_str());
            match op {
                CmpOp::Eq => stored == query,
                CmpOp::Ne => stored != query,
                _ => false,
            }
        }
        FilterValue::Now => {
            // `now` is a time sentinel; not meaningful for string-based comparison
            false
        }
    }
}

// ---------------------------------------------------------------------------
// Recursive-descent parser
// ---------------------------------------------------------------------------

struct Parser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Parser { input, pos: 0 }
    }

    fn err(&self, msg: impl Into<String>) -> ParseError {
        ParseError {
            message: msg.into(),
            offset: self.pos,
        }
    }

    fn remaining(&self) -> &str {
        &self.input[self.pos..]
    }

    fn peek(&self) -> Option<char> {
        self.remaining().chars().next()
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.pos += c.len_utf8();
            } else {
                break;
            }
        }
    }

    fn try_consume(&mut self, prefix: &str) -> bool {
        if self.remaining().starts_with(prefix) {
            self.pos += prefix.len();
            true
        } else {
            false
        }
    }

    // Parse orExpr := andExpr ( "or" andExpr )*
    fn parse_or(&mut self) -> Result<FilterExpr, ParseError> {
        let mut left = self.parse_and()?;
        loop {
            self.skip_ws();
            if self.starts_with_word("or") {
                self.pos += 2;
                let right = self.parse_and()?;
                left = FilterExpr::Or(Box::new(left), Box::new(right));
            } else {
                break;
            }
        }
        Ok(left)
    }

    // Parse andExpr := termExpr ( "and" termExpr )*
    fn parse_and(&mut self) -> Result<FilterExpr, ParseError> {
        let mut left = self.parse_term()?;
        loop {
            self.skip_ws();
            if self.starts_with_word("and") {
                self.pos += 3;
                let right = self.parse_term()?;
                left = FilterExpr::And(Box::new(left), Box::new(right));
            } else {
                break;
            }
        }
        Ok(left)
    }

    // Parse termExpr := "not"? unaryExpr
    fn parse_term(&mut self) -> Result<FilterExpr, ParseError> {
        self.skip_ws();
        if self.starts_with_word("not") {
            self.pos += 3;
            let inner = self.parse_unary()?;
            Ok(FilterExpr::Not(Box::new(inner)))
        } else {
            self.parse_unary()
        }
    }

    // Parse unaryExpr := "(" filter ")" | cmpExpr | hasMarker
    fn parse_unary(&mut self) -> Result<FilterExpr, ParseError> {
        self.skip_ws();
        if self.peek() == Some('(') {
            self.pos += 1; // consume '('
            let inner = self.parse_or()?;
            self.skip_ws();
            if !self.try_consume(")") {
                return Err(self.err("expected closing ')'"));
            }
            return Ok(inner);
        }

        // Must be an identifier (tagName)
        let tag = self.parse_identifier()?;

        self.skip_ws();
        // Check if followed by a comparison operator
        if let Some(op) = self.try_parse_op() {
            self.skip_ws();
            let value = self.parse_value()?;
            Ok(FilterExpr::Cmp { tag, op, value })
        } else {
            // Has-marker form
            Ok(FilterExpr::Has(tag))
        }
    }

    fn try_parse_op(&mut self) -> Option<CmpOp> {
        let ops: &[(&str, CmpOp)] = &[
            ("==", CmpOp::Eq),
            ("!=", CmpOp::Ne),
            ("<=", CmpOp::Le),
            (">=", CmpOp::Ge),
            ("<", CmpOp::Lt),
            (">", CmpOp::Gt),
        ];
        for (s, op) in ops {
            if self.remaining().starts_with(s) {
                self.pos += s.len();
                return Some(*op);
            }
        }
        None
    }

    fn parse_value(&mut self) -> Result<FilterValue, ParseError> {
        self.skip_ws();
        match self.peek() {
            Some('"') => {
                let s = self.parse_string()?;
                Ok(FilterValue::Str(s))
            }
            Some('@') => {
                self.pos += 1; // consume '@'
                let id = self.parse_ref_id()?;
                Ok(FilterValue::Ref(id))
            }
            Some(c) if c.is_ascii_digit() || c == '-' => {
                let n = self.parse_number()?;
                Ok(FilterValue::Num(n))
            }
            _ => {
                // Keyword: true, false, now, or identifier
                let kw = self.parse_identifier()?;
                match kw.as_str() {
                    "true" => Ok(FilterValue::Bool(true)),
                    "false" => Ok(FilterValue::Bool(false)),
                    "now" => Ok(FilterValue::Now),
                    other => Err(self.err(format!("unknown value keyword '{other}'"))),
                }
            }
        }
    }

    fn parse_string(&mut self) -> Result<String, ParseError> {
        assert_eq!(self.peek(), Some('"'));
        self.pos += 1; // consume opening quote
        let mut result = String::new();
        loop {
            match self.peek() {
                None => return Err(self.err("unterminated string literal")),
                Some('"') => {
                    self.pos += 1;
                    return Ok(result);
                }
                Some('\\') => {
                    self.pos += 1;
                    match self.peek() {
                        Some('"') => {
                            result.push('"');
                            self.pos += 1;
                        }
                        Some('\\') => {
                            result.push('\\');
                            self.pos += 1;
                        }
                        Some('n') => {
                            result.push('\n');
                            self.pos += 1;
                        }
                        Some('t') => {
                            result.push('\t');
                            self.pos += 1;
                        }
                        Some(c) => {
                            result.push(c);
                            self.pos += c.len_utf8();
                        }
                        None => return Err(self.err("unterminated escape sequence")),
                    }
                }
                Some(c) => {
                    result.push(c);
                    self.pos += c.len_utf8();
                }
            }
        }
    }

    fn parse_ref_id(&mut self) -> Result<String, ParseError> {
        // ref identifier: alphanumeric, hyphens, underscores, dots
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                self.pos += c.len_utf8();
            } else {
                break;
            }
        }
        if self.pos == start {
            return Err(self.err("expected ref identifier after '@'"));
        }
        Ok(self.input[start..self.pos].to_string())
    }

    fn parse_number(&mut self) -> Result<f64, ParseError> {
        let start = self.pos;
        if self.peek() == Some('-') {
            self.pos += 1;
        }
        // integer part
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.pos += 1;
            } else {
                break;
            }
        }
        // fractional
        if self.peek() == Some('.') {
            self.pos += 1;
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    self.pos += 1;
                } else {
                    break;
                }
            }
        }
        // exponent
        if matches!(self.peek(), Some('e') | Some('E')) {
            self.pos += 1;
            if matches!(self.peek(), Some('+') | Some('-')) {
                self.pos += 1;
            }
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    self.pos += 1;
                } else {
                    break;
                }
            }
        }
        let s = &self.input[start..self.pos];
        s.parse::<f64>()
            .map_err(|_| ParseError { message: format!("invalid number '{s}'"), offset: start })
    }

    fn parse_identifier(&mut self) -> Result<String, ParseError> {
        self.skip_ws();
        let start = self.pos;
        match self.peek() {
            Some(c) if c.is_alphabetic() || c == '_' => {
                self.pos += c.len_utf8();
            }
            other => {
                return Err(self.err(format!(
                    "expected identifier, got '{}'",
                    other.map(|c| c.to_string()).unwrap_or_else(|| "EOF".into())
                )));
            }
        }
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                self.pos += c.len_utf8();
            } else {
                break;
            }
        }
        Ok(self.input[start..self.pos].to_string())
    }

    /// True if the remaining input starts with `word` followed by a non-identifier character.
    fn starts_with_word(&self, word: &str) -> bool {
        if !self.remaining().starts_with(word) {
            return false;
        }
        let after = &self.remaining()[word.len()..];
        match after.chars().next() {
            None => true,
            Some(c) => !c.is_alphanumeric() && c != '_',
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(pairs: &[(&str, Option<&str>)]) -> HashMap<String, Option<String>> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.map(|s| s.to_string())))
            .collect()
    }

    // --- Parse valid expressions ---

    #[test]
    fn parse_has_marker() {
        let expr = parse_filter("temp").unwrap();
        assert_eq!(expr, FilterExpr::Has("temp".into()));
    }

    #[test]
    fn parse_and_two_markers() {
        let expr = parse_filter("temp and air").unwrap();
        assert_eq!(
            expr,
            FilterExpr::And(
                Box::new(FilterExpr::Has("temp".into())),
                Box::new(FilterExpr::Has("air".into())),
            )
        );
    }

    #[test]
    fn parse_three_and_markers() {
        let expr = parse_filter("temp and air and equip").unwrap();
        // Left-associative: (temp and air) and equip
        assert!(matches!(expr, FilterExpr::And(_, _)));
    }

    #[test]
    fn parse_or_expression() {
        let expr = parse_filter("temp or pressure").unwrap();
        assert_eq!(
            expr,
            FilterExpr::Or(
                Box::new(FilterExpr::Has("temp".into())),
                Box::new(FilterExpr::Has("pressure".into())),
            )
        );
    }

    #[test]
    fn parse_not_expression() {
        let expr = parse_filter("not site").unwrap();
        assert_eq!(expr, FilterExpr::Not(Box::new(FilterExpr::Has("site".into()))));
    }

    #[test]
    fn parse_eq_string() {
        let expr = parse_filter(r#"kind=="Number""#).unwrap();
        assert_eq!(
            expr,
            FilterExpr::Cmp {
                tag: "kind".into(),
                op: CmpOp::Eq,
                value: FilterValue::Str("Number".into()),
            }
        );
    }

    #[test]
    fn parse_ne_string() {
        let expr = parse_filter(r#"unit!="°C""#).unwrap();
        assert!(matches!(
            expr,
            FilterExpr::Cmp { op: CmpOp::Ne, .. }
        ));
    }

    #[test]
    fn parse_lt_number() {
        let expr = parse_filter("floor<3").unwrap();
        assert_eq!(
            expr,
            FilterExpr::Cmp {
                tag: "floor".into(),
                op: CmpOp::Lt,
                value: FilterValue::Num(3.0),
            }
        );
    }

    #[test]
    fn parse_ge_number() {
        let expr = parse_filter("floor>=2").unwrap();
        assert!(matches!(expr, FilterExpr::Cmp { op: CmpOp::Ge, .. }));
    }

    #[test]
    fn parse_ref_literal() {
        let expr = parse_filter("equipRef==@ahu-1").unwrap();
        assert_eq!(
            expr,
            FilterExpr::Cmp {
                tag: "equipRef".into(),
                op: CmpOp::Eq,
                value: FilterValue::Ref("ahu-1".into()),
            }
        );
    }

    #[test]
    fn parse_bool_true() {
        let expr = parse_filter("enabled==true").unwrap();
        assert_eq!(
            expr,
            FilterExpr::Cmp {
                tag: "enabled".into(),
                op: CmpOp::Eq,
                value: FilterValue::Bool(true),
            }
        );
    }

    #[test]
    fn parse_bool_false() {
        let expr = parse_filter("disabled==false").unwrap();
        assert!(matches!(expr, FilterExpr::Cmp { value: FilterValue::Bool(false), .. }));
    }

    #[test]
    fn parse_now_keyword() {
        let expr = parse_filter("ts==now").unwrap();
        assert!(matches!(expr, FilterExpr::Cmp { value: FilterValue::Now, .. }));
    }

    #[test]
    fn parse_parenthesized() {
        let expr = parse_filter("(temp or pressure) and not site").unwrap();
        assert!(matches!(expr, FilterExpr::And(_, _)));
    }

    #[test]
    fn parse_complex() {
        parse_filter("temp and air and equipRef==@ahu-1").unwrap();
    }

    #[test]
    fn parse_kind_and_unit() {
        parse_filter(r#"kind=="Number" and unit=="°F""#).unwrap();
    }

    #[test]
    fn parse_space_floor() {
        parse_filter(r#"space and floor=="2""#).unwrap();
    }

    // --- Parse errors ---

    #[test]
    fn parse_error_trailing_garbage() {
        assert!(parse_filter("temp !!!").is_err());
    }

    #[test]
    fn parse_error_unclosed_paren() {
        assert!(parse_filter("(temp and air").is_err());
    }

    #[test]
    fn parse_error_empty_string_after_op() {
        // Invalid: no value after ==
        assert!(parse_filter("temp==").is_err());
    }

    #[test]
    fn parse_error_unknown_keyword() {
        assert!(parse_filter("temp==maybe").is_err());
    }

    #[test]
    fn parse_error_empty_input() {
        assert!(parse_filter("").is_err());
    }

    #[test]
    fn parse_error_just_operator() {
        assert!(parse_filter("==foo").is_err());
    }

    // --- Evaluator ---

    #[test]
    fn eval_has_marker_present() {
        let expr = parse_filter("temp").unwrap();
        let t = tags(&[("temp", None)]);
        assert!(matches(&expr, &t));
    }

    #[test]
    fn eval_has_marker_absent() {
        let expr = parse_filter("temp").unwrap();
        let t = tags(&[("air", None)]);
        assert!(!matches(&expr, &t));
    }

    #[test]
    fn eval_and_both_present() {
        let expr = parse_filter("temp and air").unwrap();
        let t = tags(&[("temp", None), ("air", None)]);
        assert!(matches(&expr, &t));
    }

    #[test]
    fn eval_and_one_missing() {
        let expr = parse_filter("temp and air").unwrap();
        let t = tags(&[("temp", None)]);
        assert!(!matches(&expr, &t));
    }

    #[test]
    fn eval_or_one_present() {
        let expr = parse_filter("temp or pressure").unwrap();
        let t = tags(&[("pressure", None)]);
        assert!(matches(&expr, &t));
    }

    #[test]
    fn eval_or_neither() {
        let expr = parse_filter("temp or pressure").unwrap();
        let t = tags(&[("air", None)]);
        assert!(!matches(&expr, &t));
    }

    #[test]
    fn eval_not() {
        let expr = parse_filter("not site").unwrap();
        let t = tags(&[("equip", None)]);
        assert!(matches(&expr, &t));
        let t2 = tags(&[("site", None)]);
        assert!(!matches(&expr, &t2));
    }

    #[test]
    fn eval_eq_string() {
        let expr = parse_filter(r#"kind=="Number""#).unwrap();
        let t = tags(&[("kind", Some("Number"))]);
        assert!(matches(&expr, &t));
        let t2 = tags(&[("kind", Some("Bool"))]);
        assert!(!matches(&expr, &t2));
    }

    #[test]
    fn eval_ne_string() {
        let expr = parse_filter(r#"kind!="Number""#).unwrap();
        let t = tags(&[("kind", Some("Bool"))]);
        assert!(matches(&expr, &t));
    }

    #[test]
    fn eval_lt_number() {
        let expr = parse_filter("floor<3").unwrap();
        let t = tags(&[("floor", Some("2"))]);
        assert!(matches(&expr, &t));
        let t2 = tags(&[("floor", Some("3"))]);
        assert!(!matches(&expr, &t2));
    }

    #[test]
    fn eval_ge_number() {
        let expr = parse_filter("floor>=2").unwrap();
        let t = tags(&[("floor", Some("2"))]);
        assert!(matches(&expr, &t));
        let t2 = tags(&[("floor", Some("1"))]);
        assert!(!matches(&expr, &t2));
    }

    #[test]
    fn eval_ref_eq() {
        let expr = parse_filter("equipRef==@ahu-1").unwrap();
        let t = tags(&[("equipRef", Some("ahu-1"))]);
        assert!(matches(&expr, &t));
        let t2 = tags(&[("equipRef", Some("ahu-2"))]);
        assert!(!matches(&expr, &t2));
    }

    #[test]
    fn eval_complex_and_ref() {
        let expr = parse_filter("temp and air and equipRef==@ahu-1").unwrap();
        let t = tags(&[("temp", None), ("air", None), ("equipRef", Some("ahu-1"))]);
        assert!(matches(&expr, &t));
        let t2 = tags(&[("temp", None), ("air", None), ("equipRef", Some("vav-1"))]);
        assert!(!matches(&expr, &t2));
    }

    #[test]
    fn eval_paren_or_and_not() {
        let expr = parse_filter("(temp or pressure) and not site").unwrap();
        let t_ok = tags(&[("pressure", None), ("equip", None)]);
        assert!(matches(&expr, &t_ok));
        let t_fail_site = tags(&[("pressure", None), ("site", None)]);
        assert!(!matches(&expr, &t_fail_site));
        let t_fail_neither = tags(&[("equip", None)]);
        assert!(!matches(&expr, &t_fail_neither));
    }

    #[test]
    fn eval_bool_true() {
        let expr = parse_filter("enabled==true").unwrap();
        let t = tags(&[("enabled", Some("true"))]);
        assert!(matches(&expr, &t));
        let t2 = tags(&[("enabled", Some("false"))]);
        assert!(!matches(&expr, &t2));
    }

    #[test]
    fn eval_space_floor_string() {
        let expr = parse_filter(r#"space and floor=="2""#).unwrap();
        let t = tags(&[("space", None), ("floor", Some("2"))]);
        assert!(matches(&expr, &t));
        let t2 = tags(&[("space", None), ("floor", Some("3"))]);
        assert!(!matches(&expr, &t2));
    }

    #[test]
    fn eval_negative_number() {
        let expr = parse_filter("temperature>=-10").unwrap();
        let t = tags(&[("temperature", Some("0"))]);
        assert!(matches(&expr, &t));
        let t2 = tags(&[("temperature", Some("-20"))]);
        assert!(!matches(&expr, &t2));
    }

    #[test]
    fn eval_float_number() {
        let expr = parse_filter("setpoint==21.5").unwrap();
        let t = tags(&[("setpoint", Some("21.5"))]);
        assert!(matches(&expr, &t));
    }
}
