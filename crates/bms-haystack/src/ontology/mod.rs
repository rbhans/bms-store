//! Ontology layer — tag definitions, prototypes, providers.
//!
//! Two providers coexist:
//!
//! * [`Haystack5Provider`] — derived from build-time generated tables
//!   ([`GENERATED_GLOBALS`] / [`GENERATED_SPECS`]) populated by the xeto
//!   parser in `build.rs`. **Use this** for new code — it tracks upstream
//!   Project Haystack 5.
//!
//! * [`Haystack4Provider`] — hand-curated [`TAGS`] / [`EQUIP_PROTOTYPES`]
//!   tables. Retained for backward compatibility with existing call sites
//!   and tests. Marked deprecated; will be retired once all consumers
//!   migrate to the v5 provider.

pub mod codegen;
pub mod proto;
pub mod tags;

pub use codegen::{GeneratedGlobal, GeneratedSpec, GENERATED_GLOBALS, GENERATED_SPECS};
pub use proto::{Prototype, EQUIP_PROTOTYPES, POINT_PROTOTYPES};
pub use tags::{TagDef, TagKind, TAGS, UNITS};

use std::sync::OnceLock;

/// Abstraction over the tag dictionary source.
pub trait TagProvider: Send + Sync {
    fn all_tags(&self) -> &[TagDef];
    fn find_tag(&self, name: &str) -> Option<&TagDef>;
    fn tags_for_entity(&self, entity_type: &str) -> Vec<&TagDef>;
    fn all_units(&self) -> &[(&str, &[&str])];
    fn equip_prototypes(&self) -> &[Prototype];
    fn point_prototypes(&self) -> &[Prototype];

    /// Generated specs from the upstream xeto bundle. Empty for the legacy
    /// [`Haystack4Provider`].
    fn all_specs(&self) -> &[GeneratedSpec] {
        &[]
    }
}

/// Hand-curated Haystack 4 provider. Retained for backward compatibility;
/// new code should prefer [`Haystack5Provider`].
pub struct Haystack4Provider;

impl TagProvider for Haystack4Provider {
    fn all_tags(&self) -> &[TagDef] {
        TAGS
    }
    fn find_tag(&self, name: &str) -> Option<&TagDef> {
        TAGS.iter().find(|t| t.name == name)
    }
    fn tags_for_entity(&self, entity_type: &str) -> Vec<&TagDef> {
        TAGS.iter()
            .filter(|t| t.applies_to.contains(&entity_type))
            .collect()
    }
    fn all_units(&self) -> &[(&str, &[&str])] {
        UNITS
    }
    fn equip_prototypes(&self) -> &[Prototype] {
        EQUIP_PROTOTYPES
    }
    fn point_prototypes(&self) -> &[Prototype] {
        POINT_PROTOTYPES
    }
}

/// Provider backed by the build-time generated xeto tables.
pub struct Haystack5Provider;

static V5_TAGS: OnceLock<Vec<TagDef>> = OnceLock::new();

impl TagProvider for Haystack5Provider {
    fn all_tags(&self) -> &[TagDef] {
        V5_TAGS.get_or_init(build_v5_tags)
    }
    fn find_tag(&self, name: &str) -> Option<&TagDef> {
        self.all_tags().iter().find(|t| t.name == name)
    }
    fn tags_for_entity(&self, entity_type: &str) -> Vec<&TagDef> {
        // PhEntity slots apply to every entity kind; specs without a generated
        // global apply only to themselves. For now we return all globals — the
        // downstream caller can filter by spec hierarchy via `all_specs()`.
        let _ = entity_type;
        self.all_tags().iter().collect()
    }
    fn all_units(&self) -> &[(&str, &[&str])] {
        // TODO step 2 follow-on: generated unit table from sys/units.xeto.
        UNITS
    }
    fn equip_prototypes(&self) -> &[Prototype] {
        EQUIP_PROTOTYPES
    }
    fn point_prototypes(&self) -> &[Prototype] {
        POINT_PROTOTYPES
    }
    fn all_specs(&self) -> &[GeneratedSpec] {
        GENERATED_SPECS
    }
}

/// Convert the build-time `GENERATED_GLOBALS` table into a `Vec<TagDef>`.
/// Each global's xeto kind string is mapped to [`TagKind`].
fn build_v5_tags() -> Vec<TagDef> {
    static ALL_ENTITY_TYPES: &[&str] = &[
        "site", "space", "equip", "point", "device", "system", "weather",
    ];
    GENERATED_GLOBALS
        .iter()
        .map(|g| TagDef {
            name: g.name,
            kind: xeto_kind_to_tagkind(g.kind),
            doc: g.doc,
            supertype: None,
            applies_to: ALL_ENTITY_TYPES,
        })
        .collect()
}

/// Map an xeto kind identifier to [`TagKind`].
///
/// Unknown / specialized kinds (TimeZone, Quantity, Phenomenon, custom specs)
/// fall back to `Str`, which is the closest scalar representation. The
/// generated global retains the original kind string for callers that need
/// to recover the precise type.
fn xeto_kind_to_tagkind(kind: &str) -> TagKind {
    match kind {
        "Marker" => TagKind::Marker,
        "Bool" => TagKind::Bool,
        "Number" | "Int" | "Float" | "Duration" => TagKind::Number,
        "Str" | "Symbol" | "Version" | "Span" => TagKind::Str,
        "Ref" | "MultiRef" => TagKind::Ref,
        "Uri" => TagKind::Uri,
        "Date" => TagKind::Date,
        "Time" => TagKind::Time,
        "DateTime" => TagKind::DateTime,
        "Coord" => TagKind::Coord,
        "List" => TagKind::List,
        "Dict" | "Choice" => TagKind::Dict,
        "Grid" => TagKind::Grid,
        // Choice ranges and Enum / specialized kinds resolve to the type
        // declaration's underlying scalar — best-effort fallback to Str.
        _ => TagKind::Str,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_find_tag() {
        let p = Haystack4Provider;
        assert!(p.find_tag("site").is_some());
        assert!(p.find_tag("ahu").is_some());
        assert!(p.find_tag("bogus").is_none());
    }

    #[test]
    fn provider_tags_for_entity() {
        let p = Haystack4Provider;
        let point_tags = p.tags_for_entity("point");
        assert!(point_tags.len() > 20);
    }

    #[test]
    fn provider_units() {
        let p = Haystack4Provider;
        assert!(p.all_units().len() > 5);
    }

    #[test]
    fn provider_prototypes() {
        let p = Haystack4Provider;
        assert!(!p.equip_prototypes().is_empty());
        assert!(!p.point_prototypes().is_empty());
    }

    // ---- Haystack5Provider ----

    #[test]
    fn v5_provider_emits_nontrivial_tags() {
        let p = Haystack5Provider;
        assert!(p.all_tags().len() >= 300, "expected >=300 v5 tags");
    }

    #[test]
    fn v5_provider_finds_anchor_tags() {
        let p = Haystack5Provider;
        for name in ["ahu", "vav", "boiler", "chiller", "fan", "pump", "air", "temp"] {
            assert!(p.find_tag(name).is_some(), "missing v5 tag {name}");
        }
    }

    #[test]
    fn v5_provider_kind_mapping() {
        let p = Haystack5Provider;
        // ahu is a Marker
        assert_eq!(p.find_tag("ahu").unwrap().kind, TagKind::Marker);
        // area is a Number with quantity:"area"
        let area = p.find_tag("area").unwrap();
        assert_eq!(area.kind, TagKind::Number);
        // equipRef is a Ref/MultiRef
        let eref = p
            .find_tag("equipRef")
            .or_else(|| p.find_tag("airRef"))
            .expect("at least one *Ref tag");
        assert_eq!(eref.kind, TagKind::Ref);
    }

    #[test]
    fn v5_provider_exposes_specs() {
        let p = Haystack5Provider;
        assert!(p.all_specs().len() >= 500);
    }

    #[test]
    fn v5_global_retains_quantity_meta() {
        let area = codegen::find_global("area").expect("area global");
        assert_eq!(area.quantity, Some("area"));
    }
}
