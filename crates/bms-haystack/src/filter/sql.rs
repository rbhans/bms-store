//! Lower a [`FilterExpr`] to a SQL `WHERE` clause + bind parameters for
//! push-down evaluation against the bms-store SQLite schema.
//!
//! Schema (from `bms-store-storage`):
//! ```sql
//! entity     (id PK, entity_type, dis, parent_id, created_ms, updated_ms)
//! entity_tag (entity_id, tag_name, tag_value)  -- idx on tag_name
//! entity_ref (entity_id, ref_tag, target_id)   -- idx on target_id
//! ```
//!
//! Lowering uses correlated `EXISTS` subqueries against `entity_tag` keyed
//! on the outer `e.id`. NULL semantics are preserved: `not foo` lowers to
//! `NOT EXISTS (... tag_name = 'foo')`, NOT `tag_name != 'foo'`.
//!
//! The lowerer is **best-effort**. Anything it can't represent (currently:
//! arrow-path walks, comparisons against numbers with units, comparisons
//! against bools where the stored tag value is the string `"true"`/`"false"`)
//! returns [`SqlError::NotPushable`] so the caller can fall back to
//! load-then-eval.

use thiserror::Error;

use super::ast::{CmpOp, FilterExpr, FilterValue, Path};

/// Type-erased bind parameter — the storage layer downcasts to a
/// `rusqlite::types::Value` when executing.
#[derive(Debug, Clone, PartialEq)]
pub enum SqlParam {
    Text(String),
    Integer(i64),
    Real(f64),
    Bool(bool),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SqlFragment {
    /// SQL `WHERE` body (without leading `WHERE`). Always references the
    /// outer table alias `e` for `entity`.
    pub sql: String,
    pub params: Vec<SqlParam>,
}

#[derive(Debug, Error, PartialEq)]
pub enum SqlError {
    #[error("not pushable: {0}")]
    NotPushable(&'static str),
}

/// Lower a filter to a SQL fragment. Returns [`SqlError::NotPushable`] for
/// constructs the lowerer cannot handle.
pub fn lower(expr: &FilterExpr) -> Result<SqlFragment, SqlError> {
    let mut out = SqlFragment {
        sql: String::new(),
        params: Vec::new(),
    };
    lower_into(&mut out, expr)?;
    Ok(out)
}

fn lower_into(out: &mut SqlFragment, expr: &FilterExpr) -> Result<(), SqlError> {
    match expr {
        FilterExpr::Has(path) => lower_has(out, path),
        FilterExpr::Not(inner) => match inner.as_ref() {
            // `not <path>` ⇒ tag absent. Use NOT EXISTS, not !=.
            FilterExpr::Has(path) => lower_missing(out, path),
            other => {
                out.sql.push_str("NOT (");
                lower_into(out, other)?;
                out.sql.push(')');
                Ok(())
            }
        },
        FilterExpr::And(l, r) => {
            out.sql.push('(');
            lower_into(out, l)?;
            out.sql.push_str(") AND (");
            lower_into(out, r)?;
            out.sql.push(')');
            Ok(())
        }
        FilterExpr::Or(l, r) => {
            out.sql.push('(');
            lower_into(out, l)?;
            out.sql.push_str(") OR (");
            lower_into(out, r)?;
            out.sql.push(')');
            Ok(())
        }
        FilterExpr::Cmp(path, op, val) => lower_cmp(out, path, *op, val),
    }
}

fn lower_has(out: &mut SqlFragment, path: &Path) -> Result<(), SqlError> {
    if path.0.len() != 1 {
        return Err(SqlError::NotPushable("arrow paths"));
    }
    out.sql.push_str(
        "EXISTS (SELECT 1 FROM entity_tag t WHERE t.entity_id = e.id AND t.tag_name = ?)",
    );
    out.params.push(SqlParam::Text(path.0[0].clone()));
    Ok(())
}

fn lower_missing(out: &mut SqlFragment, path: &Path) -> Result<(), SqlError> {
    if path.0.len() != 1 {
        return Err(SqlError::NotPushable("arrow paths in negation"));
    }
    out.sql.push_str(
        "NOT EXISTS (SELECT 1 FROM entity_tag t WHERE t.entity_id = e.id AND t.tag_name = ?)",
    );
    out.params.push(SqlParam::Text(path.0[0].clone()));
    Ok(())
}

fn lower_cmp(
    out: &mut SqlFragment,
    path: &Path,
    op: CmpOp,
    val: &FilterValue,
) -> Result<(), SqlError> {
    if path.0.len() != 1 {
        return Err(SqlError::NotPushable("arrow paths in comparison"));
    }
    let tag_name = &path.0[0];
    let cmp = match op {
        CmpOp::Eq => "=",
        CmpOp::Ne => "!=",
        CmpOp::Lt => "<",
        CmpOp::Le => "<=",
        CmpOp::Gt => ">",
        CmpOp::Ge => ">=",
    };
    let (cast, param) = match val {
        FilterValue::Bool(b) => (
            "t.tag_value",
            SqlParam::Text(if *b { "true".into() } else { "false".into() }),
        ),
        FilterValue::Number(n) => {
            if n.unit.is_some() {
                return Err(SqlError::NotPushable(
                    "comparisons against numbers with units",
                ));
            }
            ("CAST(t.tag_value AS REAL)", SqlParam::Real(n.val))
        }
        FilterValue::Str(s) => ("t.tag_value", SqlParam::Text(s.clone())),
        FilterValue::Ref(r) => {
            // Refs can be stored either as a tag value `@id` or in
            // entity_ref. We pick the entity_ref join path because that
            // matches Haystack semantics for `equipRef == @ahu-1`.
            out.sql.push_str(
                "EXISTS (SELECT 1 FROM entity_ref r WHERE r.entity_id = e.id AND r.ref_tag = ? AND r.target_id ",
            );
            out.sql.push_str(cmp);
            out.sql.push_str(" ?)");
            out.params.push(SqlParam::Text(tag_name.clone()));
            out.params.push(SqlParam::Text(r.id.clone()));
            return Ok(());
        }
    };
    out.sql.push_str("EXISTS (SELECT 1 FROM entity_tag t WHERE t.entity_id = e.id AND t.tag_name = ? AND ");
    out.sql.push_str(cast);
    out.sql.push(' ');
    out.sql.push_str(cmp);
    out.sql.push_str(" ?)");
    out.params.push(SqlParam::Text(tag_name.clone()));
    out.params.push(param);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::parse;

    fn lower_str(s: &str) -> Result<SqlFragment, SqlError> {
        lower(&parse(s).unwrap())
    }

    #[test]
    fn has_lowers_to_exists() {
        let f = lower_str("ahu").unwrap();
        assert!(f.sql.contains("EXISTS (SELECT 1 FROM entity_tag"));
        assert_eq!(f.params, vec![SqlParam::Text("ahu".into())]);
    }

    #[test]
    fn not_has_lowers_to_not_exists() {
        let f = lower_str("not ahu").unwrap();
        assert!(f.sql.contains("NOT EXISTS"));
    }

    #[test]
    fn and_or_compose() {
        let f = lower_str("ahu and equip").unwrap();
        assert!(f.sql.contains(") AND ("));
        let f = lower_str("ahu or vav").unwrap();
        assert!(f.sql.contains(") OR ("));
    }

    #[test]
    fn cmp_number_unitless_pushable() {
        let f = lower_str("temp >= 70").unwrap();
        assert!(f.sql.contains("CAST(t.tag_value AS REAL)"));
        assert!(f.sql.contains(">= ?"));
        assert_eq!(f.params[0], SqlParam::Text("temp".into()));
        assert_eq!(f.params[1], SqlParam::Real(70.0));
    }

    #[test]
    fn cmp_number_with_unit_not_pushable() {
        let err = lower_str("temp >= 70°F").unwrap_err();
        assert!(matches!(err, SqlError::NotPushable(_)));
    }

    #[test]
    fn cmp_str() {
        let f = lower_str(r#"dis == "AHU 1""#).unwrap();
        assert!(f.sql.contains("t.tag_value = ?"));
        assert_eq!(f.params[1], SqlParam::Text("AHU 1".into()));
    }

    #[test]
    fn cmp_ref_uses_entity_ref_table() {
        let f = lower_str("equipRef == @ahu-1").unwrap();
        assert!(f.sql.contains("entity_ref"));
        assert!(f.sql.contains("r.target_id"));
        assert_eq!(f.params[0], SqlParam::Text("equipRef".into()));
        assert_eq!(f.params[1], SqlParam::Text("ahu-1".into()));
    }

    #[test]
    fn arrow_path_not_pushable() {
        let err = lower_str("equipRef->siteRef").unwrap_err();
        assert!(matches!(err, SqlError::NotPushable(_)));
    }

    #[test]
    fn null_semantics_for_negation() {
        // Critical correctness test: `not foo` must lower to NOT EXISTS,
        // never `tag_value != 'foo'` which would miss rows where the tag
        // is absent entirely.
        let f = lower_str("not foo").unwrap();
        assert!(f.sql.starts_with("NOT EXISTS"));
        assert!(!f.sql.contains("!="));
    }
}
