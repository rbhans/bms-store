use crate::haystack::prototypes::{Prototype, EQUIP_PROTOTYPES, POINT_PROTOTYPES};
use crate::haystack::tags::{TagDef, TAGS, UNITS};

/// Abstraction over the tag dictionary source.
///
/// Default implementation reads from the static TAGS/UNITS/PROTOTYPES.
/// Future Xeto implementation could parse `.xeto` files or query a server.
pub trait TagProvider: Send + Sync {
    fn all_tags(&self) -> &[TagDef];
    fn find_tag(&self, name: &str) -> Option<&TagDef>;
    fn tags_for_entity(&self, entity_type: &str) -> Vec<&TagDef>;
    fn all_units(&self) -> &[(&str, &[&str])];
    fn equip_prototypes(&self) -> &[Prototype];
    fn point_prototypes(&self) -> &[Prototype];
}

/// Default Haystack 4 provider backed by static data.
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
}
