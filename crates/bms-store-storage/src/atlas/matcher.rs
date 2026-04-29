use std::collections::HashMap;

use super::db::AtlasDb;
use super::model::{AtlasEquipMatch, AtlasEquipment, AtlasPoint, AtlasPointMatch};

/// Pre-loaded alias index for fast matching.
/// Bulk-loads all aliases from the Atlas database into memory (~1MB).
pub struct AtlasMatcher {
    /// Normalized alias → point_id
    point_alias_index: HashMap<String, String>,
    /// Normalized alias → equip_id
    equip_alias_index: HashMap<String, String>,
    /// point_id → AtlasPoint
    points: HashMap<String, AtlasPoint>,
    /// equip_id → AtlasEquipment
    equipment: HashMap<String, AtlasEquipment>,
}

impl AtlasMatcher {
    /// Bulk-load all aliases and definitions into memory.
    pub fn load(db: &AtlasDb) -> Result<Self, rusqlite::Error> {
        let all_points = db.all_points()?;
        let all_equipment = db.all_equipment()?;
        let point_aliases = db.all_point_aliases()?;
        let equip_aliases = db.all_equip_aliases()?;

        let points: HashMap<String, AtlasPoint> =
            all_points.into_iter().map(|p| (p.id.clone(), p)).collect();

        let equipment: HashMap<String, AtlasEquipment> = all_equipment
            .into_iter()
            .map(|e| (e.id.clone(), e))
            .collect();

        let point_alias_index: HashMap<String, String> = point_aliases
            .into_iter()
            .map(|a| (a.alias, a.target_id))
            .collect();

        let equip_alias_index: HashMap<String, String> = equip_aliases
            .into_iter()
            .map(|a| (a.alias, a.target_id))
            .collect();

        Ok(AtlasMatcher {
            point_alias_index,
            equip_alias_index,
            points,
            equipment,
        })
    }

    /// Number of loaded point definitions.
    pub fn point_count(&self) -> usize {
        self.points.len()
    }

    /// Number of loaded equipment definitions.
    pub fn equipment_count(&self) -> usize {
        self.equipment.len()
    }

    /// Try to match a point using multiple name sources.
    /// Tries each name in order: exact alias, then token-subset, then abbreviation.
    pub fn match_point(&self, names: &[&str], _units: Option<&str>) -> Option<AtlasPointMatch> {
        // 1. Exact normalized alias match (highest confidence)
        for name in names {
            let normalized = normalize(name);
            if let Some(point_id) = self.point_alias_index.get(&normalized) {
                if let Some(point) = self.points.get(point_id) {
                    return Some(AtlasPointMatch {
                        point: point.clone(),
                        confidence: 1.0,
                        matched_alias: normalized,
                    });
                }
            }
        }

        // 2. Token-subset match: all tokens in an alias are present in the input name
        let combined_lower: Vec<String> = names.iter().map(|n| normalize(n)).collect();
        let input_tokens: Vec<&str> = combined_lower
            .iter()
            .flat_map(|s| s.split_whitespace())
            .collect();

        let mut best: Option<(AtlasPointMatch, usize)> = None;

        for (alias, point_id) in &self.point_alias_index {
            let alias_tokens: Vec<&str> = alias.split_whitespace().collect();
            if alias_tokens.len() < 2 {
                continue; // Skip single-token aliases for subset matching
            }
            if alias_tokens.iter().all(|t| input_tokens.contains(t)) {
                if let Some(point) = self.points.get(point_id) {
                    let token_count = alias_tokens.len();
                    let is_better = match &best {
                        Some((_, prev_count)) => token_count > *prev_count,
                        None => true,
                    };
                    if is_better {
                        best = Some((
                            AtlasPointMatch {
                                point: point.clone(),
                                confidence: 0.7,
                                matched_alias: alias.clone(),
                            },
                            token_count,
                        ));
                    }
                }
            }
        }

        if let Some((m, _)) = best {
            return Some(m);
        }

        // 3. Point name substring match (lower confidence)
        for name in names {
            let norm = normalize(name);
            for point in self.points.values() {
                let point_norm = normalize(&point.name);
                if norm.contains(&point_norm) || point_norm.contains(&norm) {
                    if norm.len() >= 3 && point_norm.len() >= 3 {
                        return Some(AtlasPointMatch {
                            point: point.clone(),
                            confidence: 0.5,
                            matched_alias: point.name.clone(),
                        });
                    }
                }
            }
        }

        None
    }

    /// Try to match equipment by name.
    pub fn match_equipment(&self, name: &str) -> Option<AtlasEquipMatch> {
        let normalized = normalize(name);

        // 1. Exact alias match
        if let Some(equip_id) = self.equip_alias_index.get(&normalized) {
            if let Some(equip) = self.equipment.get(equip_id) {
                return Some(AtlasEquipMatch {
                    equipment: equip.clone(),
                    confidence: 1.0,
                    matched_alias: normalized,
                });
            }
        }

        // 2. Token-subset match
        let input_tokens: Vec<&str> = normalized.split_whitespace().collect();
        let mut best: Option<(AtlasEquipMatch, usize)> = None;

        for (alias, equip_id) in &self.equip_alias_index {
            let alias_tokens: Vec<&str> = alias.split_whitespace().collect();
            if alias_tokens.len() < 2 {
                continue;
            }
            if alias_tokens.iter().all(|t| input_tokens.contains(t)) {
                if let Some(equip) = self.equipment.get(equip_id) {
                    let token_count = alias_tokens.len();
                    let is_better = match &best {
                        Some((_, prev_count)) => token_count > *prev_count,
                        None => true,
                    };
                    if is_better {
                        best = Some((
                            AtlasEquipMatch {
                                equipment: equip.clone(),
                                confidence: 0.7,
                                matched_alias: alias.clone(),
                            },
                            token_count,
                        ));
                    }
                }
            }
        }

        if let Some((m, _)) = best {
            return Some(m);
        }

        // 3. Abbreviation match
        for equip in self.equipment.values() {
            if let Some(ref abbr) = equip.abbreviation {
                if normalize(abbr) == normalized || normalized.contains(&normalize(abbr)) {
                    return Some(AtlasEquipMatch {
                        equipment: equip.clone(),
                        confidence: 0.5,
                        matched_alias: abbr.clone(),
                    });
                }
            }
        }

        None
    }

    /// Parse a haystack_tags string into tag tuples for a point.
    /// E.g. "discharge air temp sensor point" → [("discharge", None), ("air", None), ...]
    pub fn suggest_point_tags(point: &AtlasPoint) -> Vec<(String, Option<String>)> {
        let mut tags: Vec<(String, Option<String>)> = Vec::new();

        // Parse haystack_tags string into individual marker tags
        for tag in point.haystack_tags.split_whitespace() {
            let tag = tag.trim();
            if tag.is_empty() {
                continue;
            }
            // Check for key:value format
            if let Some((key, value)) = tag.split_once(':') {
                tags.push((key.to_string(), Some(value.to_string())));
            } else {
                tags.push((tag.to_string(), None));
            }
        }

        // Ensure "point" tag is present
        if !tags.iter().any(|(n, _)| n == "point") {
            tags.push(("point".to_string(), None));
        }

        // Add kind/unit from the point definition
        if !point.kind.is_empty() && !tags.iter().any(|(n, _)| n == "kind") {
            tags.push(("kind".to_string(), Some(point.kind.clone())));
        }
        if let Some(ref units) = point.units {
            if !units.is_empty() && !tags.iter().any(|(n, _)| n == "unit") {
                tags.push(("unit".to_string(), Some(units.clone())));
            }
        }

        // Add cur tag
        if !tags.iter().any(|(n, _)| n == "cur") {
            tags.push(("cur".to_string(), None));
        }

        // Add point function classification
        match point.point_function.as_str() {
            "sensor" | "cmd" | "sp" => {
                if !tags.iter().any(|(n, _)| n == &point.point_function) {
                    tags.push((point.point_function.clone(), None));
                }
            }
            _ => {}
        }

        // Deduplicate
        let mut seen = std::collections::HashSet::new();
        tags.retain(|(name, _)| seen.insert(name.clone()));

        tags
    }

    /// Parse a haystack_tags string into tag tuples for equipment.
    pub fn suggest_equip_tags(equip: &AtlasEquipment) -> Vec<(String, Option<String>)> {
        let mut tags: Vec<(String, Option<String>)> = Vec::new();

        for tag in equip.haystack_tags.split_whitespace() {
            let tag = tag.trim();
            if tag.is_empty() {
                continue;
            }
            if let Some((key, value)) = tag.split_once(':') {
                tags.push((key.to_string(), Some(value.to_string())));
            } else {
                tags.push((tag.to_string(), None));
            }
        }

        // Ensure "equip" tag is present
        if !tags.iter().any(|(n, _)| n == "equip") {
            tags.push(("equip".to_string(), None));
        }

        // Deduplicate
        let mut seen = std::collections::HashSet::new();
        tags.retain(|(name, _)| seen.insert(name.clone()));

        tags
    }
}

/// Normalize a name for alias matching: lowercase, strip `-_. `, collapse whitespace.
fn normalize(input: &str) -> String {
    let lower = input.to_lowercase();
    let cleaned: String = lower
        .chars()
        .map(|c| {
            if c == '-' || c == '_' || c == '.' {
                ' '
            } else {
                c
            }
        })
        .collect();
    // Collapse whitespace
    cleaned.split_whitespace().collect::<Vec<&str>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_separators() {
        assert_eq!(normalize("Discharge-Air-Temp"), "discharge air temp");
        assert_eq!(normalize("zone_air_temp_sp"), "zone air temp sp");
        assert_eq!(normalize("  supply.fan.run  "), "supply fan run");
    }
}
