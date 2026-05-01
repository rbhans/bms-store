//! Build-time generated ontology tables — populated by `build.rs` from
//! `assets/xeto-master/`. Each entry carries the metadata needed to drive a
//! richer `TagDef` and to populate the eventual prototype + unit tables.

#[derive(Debug, Clone, Copy)]
pub struct GeneratedSpec {
    pub name: &'static str,
    pub supertype: &'static str,
    pub lib: &'static str,
    pub doc: &'static str,
    pub abstract_: bool,
    pub sealed: bool,
    pub of_type: Option<&'static str>,
    pub quantity: Option<&'static str>,
    pub unit: Option<&'static str>,
    pub default_val: Option<&'static str>,
}

#[derive(Debug, Clone, Copy)]
pub struct GeneratedGlobal {
    pub name: &'static str,
    pub kind: &'static str,
    pub lib: &'static str,
    pub doc: &'static str,
    pub of_type: Option<&'static str>,
    pub quantity: Option<&'static str>,
    pub unit: Option<&'static str>,
}

include!(concat!(env!("OUT_DIR"), "/generated.rs"));

/// Find a generated spec by name (first match across libs).
pub fn find_spec(name: &str) -> Option<&'static GeneratedSpec> {
    GENERATED_SPECS.iter().find(|s| s.name == name)
}

/// Find a generated global tag by name.
pub fn find_global(name: &str) -> Option<&'static GeneratedGlobal> {
    GENERATED_GLOBALS.iter().find(|g| g.name == name)
}

/// Walk the spec supertype chain. `<sealed, abstract>` flags propagate.
/// Returns chain in narrow→wide order (self first, then parents up to root).
pub fn supertype_chain(name: &str) -> Vec<&'static GeneratedSpec> {
    let mut out = Vec::new();
    let mut cursor = find_spec(name);
    while let Some(s) = cursor {
        if out.iter().any(|p: &&GeneratedSpec| std::ptr::eq(*p, s)) {
            break; // cycle guard
        }
        out.push(s);
        // Supertype expression may be intersection (`A & B`) or union (`A | B`)
        // or simple `Name`. Take the first identifier as the canonical parent.
        let sup = first_identifier(s.supertype);
        if sup.is_empty() {
            break;
        }
        cursor = find_spec(sup);
    }
    out
}

fn first_identifier(s: &str) -> &str {
    let s = s.trim();
    let end = s
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == ':'))
        .unwrap_or(s.len());
    &s[..end]
}
