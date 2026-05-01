//! Haystack Filter expressions.
//!
//! Drives the `read` HTTP op — given a filter string like
//! `equip and ahu and not equipRef`, parse → evaluate against a Dict.
//! Two evaluators share one AST:
//!
//! * In-memory: [`eval::eval`] against a `&Dict` (+ optional `Resolver`
//!   for cross-entity Ref walks).
//! * SQL push-down (forthcoming): lower the AST to `(sql, params)` for the
//!   SQLite store.

mod ast;
mod eval;
mod parser;
pub mod sql;

#[cfg(test)]
mod tests;

pub use ast::{CmpOp, FilterExpr, FilterValue, Path};
pub use eval::{eval, NoResolver, Resolver};
pub use parser::{parse, ParseError};
pub use sql::{lower as lower_to_sql, SqlError, SqlFragment, SqlParam};
