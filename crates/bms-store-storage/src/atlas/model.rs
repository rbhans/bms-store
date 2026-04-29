use serde::{Deserialize, Serialize};

/// A point definition from the BAS Atlas taxonomy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtlasPoint {
    pub id: String,
    pub name: String,
    pub category: String,
    /// Haystack tag string, e.g. "discharge air temp sensor point"
    pub haystack_tags: String,
    /// Point kind: "Number", "Bool", "Str"
    pub kind: String,
    /// Functional classification: "sensor", "cmd", "sp"
    pub point_function: String,
    /// Engineering units, e.g. "°F"
    pub units: Option<String>,
    /// Brick ontology class reference
    pub brick: Option<String>,
}

/// An equipment definition from the BAS Atlas taxonomy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtlasEquipment {
    pub id: String,
    pub name: String,
    pub abbreviation: Option<String>,
    pub category: String,
    /// Haystack tag string, e.g. "ahu equip"
    pub haystack_tags: String,
    /// Brick ontology class reference
    pub brick: Option<String>,
}

/// A successful point match from Atlas.
#[derive(Debug, Clone)]
pub struct AtlasPointMatch {
    pub point: AtlasPoint,
    /// Match confidence: 1.0 = exact alias, 0.7 = token subset, 0.5 = abbreviation
    pub confidence: f32,
    /// The alias string that matched
    pub matched_alias: String,
}

/// A successful equipment match from Atlas.
#[derive(Debug, Clone)]
pub struct AtlasEquipMatch {
    pub equipment: AtlasEquipment,
    /// Match confidence: 1.0 = exact alias, 0.7 = token subset, 0.5 = abbreviation
    pub confidence: f32,
    /// The alias string that matched
    pub matched_alias: String,
}

/// Summary statistics for the Atlas database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtlasStats {
    pub version: String,
    pub total_points: u32,
    pub total_equipment: u32,
    pub updated_ms: i64,
}
