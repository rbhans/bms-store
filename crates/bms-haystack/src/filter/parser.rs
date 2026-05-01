//! Hand-rolled recursive-descent parser for Haystack filter expressions.
//!
//! Grammar (informal):
//! ```text
//! filter   := condOr
//! condOr   := condAnd ("or" condAnd)*
//! condAnd  := term ("and" term)*
//! term     := "(" condOr ")"
//!           | "not" term
//!           | path cmpOp value
//!           | path
//! cmpOp    := "==" | "!=" | "<=" | ">=" | "<" | ">"
//! path     := ident ("->" ident)*
//! value    := bool | number | string | ref
//! ```

use thiserror::Error;

use super::ast::{CmpOp, FilterExpr, FilterValue, Path};
use crate::val::{Number, Ref};

#[derive(Debug, Error, PartialEq)]
pub enum ParseError {
    #[error("unexpected end of input")]
    Eof,
    #[error("unexpected token at {pos}: {msg}")]
    Unexpected { pos: usize, msg: String },
    #[error("invalid number literal: {0}")]
    BadNumber(String),
    #[error("unterminated string literal")]
    UnterminatedString,
    #[error("unterminated ref literal")]
    UnterminatedRef,
}

pub fn parse(input: &str) -> Result<FilterExpr, ParseError> {
    let mut p = Parser::new(input);
    let expr = p.parse_or()?;
    p.skip_ws();
    if p.cursor < p.bytes.len() {
        return Err(ParseError::Unexpected {
            pos: p.cursor,
            msg: format!("trailing input near `{}`", p.peek_n(20)),
        });
    }
    Ok(expr)
}

struct Parser<'a> {
    src: &'a str,
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            cursor: 0,
        }
    }

    fn skip_ws(&mut self) {
        while self.cursor < self.bytes.len() {
            let b = self.bytes[self.cursor];
            if b == b' ' || b == b'\t' || b == b'\r' || b == b'\n' {
                self.cursor += 1;
            } else {
                break;
            }
        }
    }

    fn peek_n(&self, n: usize) -> &str {
        let end = (self.cursor + n).min(self.bytes.len());
        &self.src[self.cursor..end]
    }

    fn parse_or(&mut self) -> Result<FilterExpr, ParseError> {
        let mut left = self.parse_and()?;
        loop {
            self.skip_ws();
            if self.consume_keyword("or") {
                let right = self.parse_and()?;
                left = FilterExpr::Or(Box::new(left), Box::new(right));
            } else {
                break Ok(left);
            }
        }
    }

    fn parse_and(&mut self) -> Result<FilterExpr, ParseError> {
        let mut left = self.parse_term()?;
        loop {
            self.skip_ws();
            if self.consume_keyword("and") {
                let right = self.parse_term()?;
                left = FilterExpr::And(Box::new(left), Box::new(right));
            } else {
                break Ok(left);
            }
        }
    }

    fn parse_term(&mut self) -> Result<FilterExpr, ParseError> {
        self.skip_ws();
        if self.consume_char('(') {
            let inner = self.parse_or()?;
            self.skip_ws();
            if !self.consume_char(')') {
                return Err(ParseError::Unexpected {
                    pos: self.cursor,
                    msg: "expected ')'".into(),
                });
            }
            return Ok(inner);
        }
        if self.consume_keyword("not") {
            let inner = self.parse_term()?;
            return Ok(FilterExpr::Not(Box::new(inner)));
        }
        // path (then optional cmp)
        let path = self.parse_path()?;
        self.skip_ws();
        if let Some(op) = self.consume_cmp_op() {
            self.skip_ws();
            let val = self.parse_value()?;
            Ok(FilterExpr::Cmp(path, op, val))
        } else {
            Ok(FilterExpr::Has(path))
        }
    }

    fn parse_path(&mut self) -> Result<Path, ParseError> {
        let first = self.parse_ident()?;
        let mut parts = vec![first];
        loop {
            self.skip_ws();
            if self.consume_str("->") {
                self.skip_ws();
                parts.push(self.parse_ident()?);
            } else {
                break;
            }
        }
        Ok(Path(parts))
    }

    fn parse_ident(&mut self) -> Result<String, ParseError> {
        self.skip_ws();
        let start = self.cursor;
        if start >= self.bytes.len() {
            return Err(ParseError::Eof);
        }
        let first = self.bytes[start] as char;
        if !(first.is_ascii_alphabetic() || first == '_') {
            return Err(ParseError::Unexpected {
                pos: start,
                msg: format!("expected identifier, got `{first}`"),
            });
        }
        self.cursor += 1;
        while self.cursor < self.bytes.len() {
            let c = self.bytes[self.cursor] as char;
            if c.is_ascii_alphanumeric() || c == '_' {
                self.cursor += 1;
            } else {
                break;
            }
        }
        Ok(self.src[start..self.cursor].to_string())
    }

    fn parse_value(&mut self) -> Result<FilterValue, ParseError> {
        self.skip_ws();
        if self.cursor >= self.bytes.len() {
            return Err(ParseError::Eof);
        }
        let b = self.bytes[self.cursor];
        if b == b'"' {
            return self.parse_string().map(FilterValue::Str);
        }
        if b == b'@' {
            return self.parse_ref().map(FilterValue::Ref);
        }
        if b == b'-' || b.is_ascii_digit() {
            return self.parse_number().map(FilterValue::Number);
        }
        // bool / ident: `true`, `false`, or a tag name as a comparison RHS
        let ident = self.parse_ident()?;
        match ident.as_str() {
            "true" => Ok(FilterValue::Bool(true)),
            "false" => Ok(FilterValue::Bool(false)),
            other => Ok(FilterValue::Str(other.to_string())),
        }
    }

    fn parse_string(&mut self) -> Result<String, ParseError> {
        if !self.consume_char('"') {
            return Err(ParseError::Unexpected {
                pos: self.cursor,
                msg: "expected '\"'".into(),
            });
        }
        let mut out = String::new();
        while self.cursor < self.bytes.len() {
            let b = self.bytes[self.cursor];
            if b == b'"' {
                self.cursor += 1;
                return Ok(out);
            }
            if b == b'\\' && self.cursor + 1 < self.bytes.len() {
                let next = self.bytes[self.cursor + 1] as char;
                match next {
                    '\\' => out.push('\\'),
                    '"' => out.push('"'),
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    other => out.push(other),
                }
                self.cursor += 2;
                continue;
            }
            out.push(b as char);
            self.cursor += 1;
        }
        Err(ParseError::UnterminatedString)
    }

    fn parse_ref(&mut self) -> Result<Ref, ParseError> {
        if !self.consume_char('@') {
            return Err(ParseError::Unexpected {
                pos: self.cursor,
                msg: "expected '@'".into(),
            });
        }
        let start = self.cursor;
        while self.cursor < self.bytes.len() {
            let c = self.bytes[self.cursor] as char;
            if c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == ':' || c == '-' || c == '~' {
                self.cursor += 1;
            } else {
                break;
            }
        }
        if self.cursor == start {
            return Err(ParseError::UnterminatedRef);
        }
        Ok(Ref::new(&self.src[start..self.cursor]))
    }

    fn parse_number(&mut self) -> Result<Number, ParseError> {
        let start = self.cursor;
        if self.bytes.get(self.cursor) == Some(&b'-') {
            self.cursor += 1;
        }
        while self.cursor < self.bytes.len() {
            let c = self.bytes[self.cursor] as char;
            if c.is_ascii_digit() || c == '.' {
                self.cursor += 1;
            } else {
                break;
            }
        }
        let num_end = self.cursor;
        let num_str = &self.src[start..num_end];
        let val: f64 = num_str
            .parse()
            .map_err(|_| ParseError::BadNumber(num_str.to_string()))?;

        // optional unit suffix: ASCII letters / digits / %, /, _, plus any
        // non-ASCII char that's not whitespace, parens, comparator, comma,
        // arrow start, or string-quote (covers `°`, `²`, `³`, `Ω`, etc.).
        let unit_start = self.cursor;
        let tail_str = &self.src[unit_start..];
        for (idx, ch) in tail_str.char_indices() {
            let is_unit_char = ch.is_ascii_alphabetic()
                || matches!(ch, '%' | '/' | '_' | '$')
                || (!ch.is_ascii() && !ch.is_whitespace());
            if is_unit_char {
                self.cursor = unit_start + idx + ch.len_utf8();
            } else {
                break;
            }
        }
        let unit_str = &self.src[unit_start..self.cursor];
        let unit = if unit_str.is_empty() {
            None
        } else {
            Some(unit_str.to_string())
        };
        Ok(Number { val, unit })
    }

    fn consume_char(&mut self, c: char) -> bool {
        self.skip_ws();
        if self.cursor < self.bytes.len() && self.bytes[self.cursor] as char == c {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    fn consume_str(&mut self, s: &str) -> bool {
        let bytes = s.as_bytes();
        if self.cursor + bytes.len() <= self.bytes.len()
            && &self.bytes[self.cursor..self.cursor + bytes.len()] == bytes
        {
            self.cursor += bytes.len();
            true
        } else {
            false
        }
    }

    /// Consume a keyword only if it's a whole word (not a prefix of an ident).
    fn consume_keyword(&mut self, kw: &str) -> bool {
        self.skip_ws();
        let bytes = kw.as_bytes();
        let end = self.cursor + bytes.len();
        if end > self.bytes.len() {
            return false;
        }
        if &self.bytes[self.cursor..end] != bytes {
            return false;
        }
        // peek next char — must NOT be ident-continuation
        if end < self.bytes.len() {
            let next = self.bytes[end] as char;
            if next.is_ascii_alphanumeric() || next == '_' {
                return false;
            }
        }
        self.cursor = end;
        true
    }

    fn consume_cmp_op(&mut self) -> Option<CmpOp> {
        self.skip_ws();
        if self.consume_str("==") {
            Some(CmpOp::Eq)
        } else if self.consume_str("!=") {
            Some(CmpOp::Ne)
        } else if self.consume_str("<=") {
            Some(CmpOp::Le)
        } else if self.consume_str(">=") {
            Some(CmpOp::Ge)
        } else if self.consume_str("<") {
            Some(CmpOp::Lt)
        } else if self.consume_str(">") {
            Some(CmpOp::Gt)
        } else {
            None
        }
    }
}
