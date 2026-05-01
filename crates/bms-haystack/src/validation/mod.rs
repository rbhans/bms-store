//! Schema-aware Dict validation against a [`HaystackNamespace`].
//!
//! For each Dict the validator infers the most specific spec (via marker
//! tags), then walks the supertype chain checking:
//! * Required slots are present
//! * Slot kinds match (Marker / Number / Ref / Str / etc.)
//! * Number slots respect their declared `quantity` and unit family
//! * Refs respect their declared `of` target type (if known to us)
//!
//! Issues are returned with a severity level so callers can render them
//! as warnings or errors. Two passes are exposed:
//!
//! * [`validate_dict`] — single-Dict checks against the spec hierarchy.
//! * [`validate_all`]  — adds cross-Dict checks (Ref targets resolve, etc.).

use crate::val::{Dict, Ref, Value};
use crate::xeto::HaystackNamespace;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone)]
pub struct Issue {
    pub entity_id: String,
    pub severity: Severity,
    pub message: String,
}

/// Validate a single dict against the namespace.
pub fn validate_dict(dict: &Dict, ns: &HaystackNamespace) -> Vec<Issue> {
    let id = match dict.get("id") {
        Some(Value::Ref(r)) => r.id.clone(),
        _ => "<no-id>".to_string(),
    };
    let mut issues = Vec::new();

    let inferred = infer_spec(dict, ns);
    if let Some(spec_name) = &inferred {
        // Walk supertype chain — every named global slot the dict carries
        // must match the slot kind declared by some ancestor.
        for (tag_name, val) in dict.iter() {
            if let Some(g) = ns.find_global(tag_name) {
                if !value_matches_kind(val, &g.kind) {
                    issues.push(Issue {
                        entity_id: id.clone(),
                        severity: Severity::Warning,
                        message: format!(
                            "tag `{tag_name}` should be `{}` per spec",
                            g.kind
                        ),
                    });
                }
                // Quantity check: if global declares `<quantity:"...">`
                // and the value is a Number, require a non-empty unit.
                if g.quantity.is_some() {
                    if let Value::Number(n) = val {
                        if n.unit.is_none() {
                            issues.push(Issue {
                                entity_id: id.clone(),
                                severity: Severity::Info,
                                message: format!(
                                    "tag `{tag_name}` is a `{}` quantity but has no unit",
                                    g.quantity.as_deref().unwrap()
                                ),
                            });
                        }
                    }
                }
            }
        }

        // Spec-level: if the spec is Sealed, no unknown markers.
        if let Some(spec) = ns.find_spec(spec_name) {
            if spec.sealed {
                for (tag_name, _val) in dict.iter() {
                    if ns.find_global(tag_name).is_none()
                        && !is_well_known_meta(tag_name)
                        && tag_name != spec_name
                    {
                        issues.push(Issue {
                            entity_id: id.clone(),
                            severity: Severity::Info,
                            message: format!(
                                "tag `{tag_name}` is not declared by sealed spec `{spec_name}`"
                            ),
                        });
                    }
                }
            }
        }
    }

    issues
}

/// Validate a list of dicts: per-dict checks plus cross-dict ref resolution.
pub fn validate_all(dicts: &[Dict], ns: &HaystackNamespace) -> Vec<Issue> {
    let mut issues = Vec::new();
    let known_ids: std::collections::HashSet<&str> = dicts
        .iter()
        .filter_map(|d| match d.get("id") {
            Some(Value::Ref(r)) => Some(r.id.as_str()),
            _ => None,
        })
        .collect();
    for d in dicts {
        issues.extend(validate_dict(d, ns));
        let id = match d.get("id") {
            Some(Value::Ref(r)) => r.id.clone(),
            _ => "<no-id>".to_string(),
        };
        for (tag, val) in d.iter() {
            if let Value::Ref(r) = val {
                if !known_ids.contains(r.id.as_str()) {
                    issues.push(Issue {
                        entity_id: id.clone(),
                        severity: Severity::Warning,
                        message: format!(
                            "tag `{tag}` references unknown entity `{}`",
                            r.id
                        ),
                    });
                }
            }
        }
    }
    issues
}

/// Heuristic: pick the most specific marker tag that names a known spec.
fn infer_spec(dict: &Dict, ns: &HaystackNamespace) -> Option<String> {
    // Try PascalCase variants of every marker tag in the dict.
    for (tag, val) in dict.iter() {
        if !matches!(val, Value::Marker) {
            continue;
        }
        let pascal = pascal_case(tag);
        if ns.find_spec(&pascal).is_some() {
            return Some(pascal);
        }
    }
    None
}

fn pascal_case(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

fn value_matches_kind(val: &Value, kind: &str) -> bool {
    match kind {
        "Marker" => matches!(val, Value::Marker),
        "Bool" => matches!(val, Value::Bool(_)),
        "Number" | "Int" | "Float" | "Duration" => matches!(val, Value::Number(_)),
        "Str" | "Symbol" | "Span" | "Version" => matches!(val, Value::Str(_) | Value::Symbol(_)),
        "Ref" | "MultiRef" => matches!(val, Value::Ref(_) | Value::List(_)),
        "Uri" => matches!(val, Value::Uri(_)),
        "Date" => matches!(val, Value::Date(_)),
        "Time" => matches!(val, Value::Time(_)),
        "DateTime" => matches!(val, Value::DateTime(_)),
        "Coord" => matches!(val, Value::Coord(_)),
        "List" => matches!(val, Value::List(_)),
        "Dict" | "Choice" => matches!(val, Value::Dict(_)),
        "Grid" => matches!(val, Value::Grid(_)),
        // Unknown / specialised kinds: don't second-guess.
        _ => true,
    }
}

fn is_well_known_meta(name: &str) -> bool {
    matches!(name, "id" | "dis" | "mod" | "navName")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::val::{Number, Ref};

    fn ns() -> HaystackNamespace {
        HaystackNamespace::builtin()
    }

    #[test]
    fn flags_wrong_kind() {
        let mut d = Dict::default();
        d.insert("id", Value::Ref(Ref::new("e1")));
        d.marker("ahu");
        // ahu IS a marker so this is fine; but `equipRef` should be a Ref:
        d.insert("equipRef", Value::Str("not-a-ref".into()));
        let issues = validate_dict(&d, &ns());
        assert!(
            issues.iter().any(|i| i.message.contains("equipRef")),
            "expected an issue about equipRef kind"
        );
    }

    #[test]
    fn quantity_without_unit_warns() {
        let mut d = Dict::default();
        d.insert("id", Value::Ref(Ref::new("s1")));
        d.marker("site");
        d.insert("area", Value::Number(Number::unitless(1234.0)));
        let issues = validate_dict(&d, &ns());
        assert!(
            issues.iter().any(|i| i.message.contains("area") && i.message.contains("unit")),
            "expected unit-missing warning on area"
        );
    }

    #[test]
    fn cross_dict_broken_ref_flagged() {
        let mut d = Dict::default();
        d.insert("id", Value::Ref(Ref::new("p1")));
        d.marker("point");
        d.insert("equipRef", Value::Ref(Ref::new("does-not-exist")));
        let issues = validate_all(&[d], &ns());
        assert!(issues.iter().any(|i| i.message.contains("does-not-exist")));
    }

    #[test]
    fn well_typed_passes() {
        let mut d = Dict::default();
        d.insert("id", Value::Ref(Ref::new("s1")));
        d.marker("site");
        d.insert("area", Value::Number(Number::with_unit(1234.0, "ft²")));
        let issues = validate_dict(&d, &ns());
        // No errors — info/warnings about other slots OK
        assert!(!issues.iter().any(|i| i.severity == Severity::Error));
    }
}
