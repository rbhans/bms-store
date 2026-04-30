use std::collections::HashMap;

use crate::store::entity_store::Entity;

// ----------------------------------------------------------------
// Public types — rules-engine API
// ----------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    pub fn label(&self) -> &'static str {
        match self {
            Severity::Error => "Error",
            Severity::Warning => "Warning",
            Severity::Info => "Info",
        }
    }
}

/// An issue raised by the validation rules engine.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationIssue {
    /// Severity level.
    pub severity: Severity,
    /// Human-readable description of the problem.
    pub message: String,
    /// Tags involved in triggering this issue.
    pub tags_involved: Vec<String>,
    /// Optional machine-readable fix hint.
    pub suggested_fix: Option<String>,
    // Legacy fields used by validate_entity / validate_all below.
    #[doc(hidden)]
    pub entity_id: String,
    #[doc(hidden)]
    pub entity_dis: String,
}

impl ValidationIssue {
    /// Create a new issue (rules-engine path — entity fields are empty).
    pub fn new(
        severity: Severity,
        message: impl Into<String>,
        tags_involved: Vec<&str>,
        suggested_fix: Option<&str>,
    ) -> Self {
        Self {
            severity,
            message: message.into(),
            tags_involved: tags_involved.iter().map(|s| s.to_string()).collect(),
            suggested_fix: suggested_fix.map(|s| s.to_string()),
            entity_id: String::new(),
            entity_dis: String::new(),
        }
    }

    /// Create a legacy entity-scoped issue.
    fn entity_issue(
        entity_id: &str,
        entity_dis: &str,
        severity: Severity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity,
            message: message.into(),
            tags_involved: Vec::new(),
            suggested_fix: None,
            entity_id: entity_id.to_string(),
            entity_dis: entity_dis.to_string(),
        }
    }
}

// ----------------------------------------------------------------
// Rule types
// ----------------------------------------------------------------

/// A single validation rule.
pub enum Rule {
    /// If `trigger` tag is present, at least one of `options` must also be present.
    RequiresOneOf {
        trigger: &'static str,
        options: &'static [&'static str],
    },
    /// The listed tags are mutually exclusive; no two may coexist.
    MutuallyExclusive(&'static [&'static str]),
    /// If `trigger` tag is present, all tags in `required` must also be present.
    RequiresAll {
        trigger: &'static str,
        required: &'static [&'static str],
    },
}

// ----------------------------------------------------------------
// Built-in Haystack-4 rule set
// ----------------------------------------------------------------

static SUBSTANCE_TAGS: &[&str] = &["air", "water", "elec", "refrig", "steam", "gas"];

static BUILT_IN_RULES: &[Rule] = &[
    Rule::RequiresOneOf {
        trigger: "temp",
        options: SUBSTANCE_TAGS,
    },
    Rule::RequiresOneOf {
        trigger: "pressure",
        options: SUBSTANCE_TAGS,
    },
    Rule::RequiresOneOf {
        trigger: "flow",
        options: SUBSTANCE_TAGS,
    },
    Rule::RequiresOneOf {
        trigger: "humidity",
        options: &["air"],
    },
    // cmd is mutually exclusive with sensor, sp, setpoint
    Rule::MutuallyExclusive(&["cmd", "sensor"]),
    Rule::MutuallyExclusive(&["cmd", "sp"]),
    Rule::MutuallyExclusive(&["cmd", "setpoint"]),
];

// ----------------------------------------------------------------
// Main public API
// ----------------------------------------------------------------

/// Validate a tag set for the given entity type using the built-in rule set.
///
/// `tags` maps tag name → optional string value (None means marker tag).
/// Returns a `Vec<ValidationIssue>` — empty means no issues found.
pub fn validate_tags(
    entity_type: &str,
    tags: &HashMap<String, Option<String>>,
) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    // Run built-in structural rules.
    for rule in BUILT_IN_RULES {
        apply_rule(rule, tags, &mut issues);
    }

    // Entity-type-specific checks.
    match entity_type {
        "point" => {
            // equip marker on a point → should have equipRef
            if tags.contains_key("equip") && !tags.contains_key("equipRef") {
                issues.push(ValidationIssue::new(
                    Severity::Warning,
                    "Point carries 'equip' marker but is missing 'equipRef'",
                    vec!["equip", "equipRef"],
                    Some("Add an equipRef tag pointing to the parent equipment entity"),
                ));
            }

            // space marker on a point → should have spaceRef
            if tags.contains_key("space") && !tags.contains_key("spaceRef") {
                issues.push(ValidationIssue::new(
                    Severity::Warning,
                    "Point carries 'space' marker but is missing 'spaceRef'",
                    vec!["space", "spaceRef"],
                    Some("Add a spaceRef tag pointing to the parent space entity"),
                ));
            }
        }
        "equip" => {
            // equip marker → should have equipRef (Haystack 4 parent ref)
            if tags.contains_key("equip") && !tags.contains_key("equipRef") {
                // Only a warning, not an error — top-level equip doesn't need one.
                // No issue here; that's handled at graph-validation level.
            }

            // space marker → should have spaceRef
            if tags.contains_key("space") && !tags.contains_key("spaceRef") {
                issues.push(ValidationIssue::new(
                    Severity::Warning,
                    "Entity carries 'space' marker but is missing 'spaceRef'",
                    vec!["space", "spaceRef"],
                    Some("Add a spaceRef tag pointing to the parent space entity"),
                ));
            }
        }
        _ => {
            // Generic: if equip/space marker is present, check for refs.
            if tags.contains_key("equip") && !tags.contains_key("equipRef") {
                // Only warn on non-equip entity-types (e.g. a point with both markers).
            }
        }
    }

    // Empty dis → Info
    match tags.get("dis") {
        None => {
            issues.push(ValidationIssue::new(
                Severity::Info,
                "Missing 'dis' tag — consider adding a human-readable display name",
                vec!["dis"],
                Some("Set dis to a descriptive name, e.g. \"Zone Air Temp Sensor\""),
            ));
        }
        Some(Some(val)) if val.trim().is_empty() => {
            issues.push(ValidationIssue::new(
                Severity::Info,
                "Empty 'dis' tag — consider adding a human-readable display name",
                vec!["dis"],
                Some("Set dis to a descriptive name, e.g. \"Zone Air Temp Sensor\""),
            ));
        }
        _ => {}
    }

    issues
}

fn apply_rule(
    rule: &Rule,
    tags: &HashMap<String, Option<String>>,
    issues: &mut Vec<ValidationIssue>,
) {
    match rule {
        Rule::RequiresOneOf { trigger, options } => {
            if tags.contains_key(*trigger) {
                let has_any = options.iter().any(|opt| tags.contains_key(*opt));
                if !has_any {
                    let opts_str = options.join(", ");
                    issues.push(ValidationIssue::new(
                        Severity::Warning,
                        format!(
                            "Tag '{}' requires at least one substance tag: [{}]",
                            trigger, opts_str
                        ),
                        {
                            let mut v = vec![*trigger];
                            v.extend_from_slice(options);
                            v
                        },
                        Some("Add one of the substance tags to disambiguate the measurement medium"),
                    ));
                }
            }
        }

        Rule::MutuallyExclusive(group) => {
            let present: Vec<&str> = group.iter().filter(|t| tags.contains_key(**t)).copied().collect();
            if present.len() > 1 {
                issues.push(ValidationIssue::new(
                    Severity::Error,
                    format!(
                        "Mutually exclusive tags coexist: [{}]",
                        present.join(", ")
                    ),
                    present.clone(),
                    Some("Remove all but one of the conflicting tags"),
                ));
            }
        }

        Rule::RequiresAll { trigger, required } => {
            if tags.contains_key(*trigger) {
                let missing: Vec<&str> = required
                    .iter()
                    .filter(|t| !tags.contains_key(**t))
                    .copied()
                    .collect();
                if !missing.is_empty() {
                    let missing_str = missing.join(", ");
                    issues.push(ValidationIssue::new(
                        Severity::Warning,
                        format!(
                            "Tag '{}' requires all of [{}] but missing: [{}]",
                            trigger,
                            required.join(", "),
                            missing_str
                        ),
                        {
                            let mut v = vec![*trigger];
                            v.extend_from_slice(required);
                            v
                        },
                        Some("Add the missing required tags"),
                    ));
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Legacy entity-based validation (preserved for existing callers)
// ----------------------------------------------------------------

/// Validate an entity's tags against Haystack 4 rules.
pub fn validate_entity(entity: &Entity) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    let id = &entity.id;
    let dis = &entity.dis;
    let tags = &entity.tags;

    match entity.entity_type.as_str() {
        "point" => {
            // Point must have exactly one of: sensor, cmd, sp
            let has_sensor = tags.contains_key("sensor");
            let has_cmd = tags.contains_key("cmd");
            let has_sp = tags.contains_key("sp");
            let class_count = [has_sensor, has_cmd, has_sp].iter().filter(|&&b| b).count();

            if class_count == 0 {
                issues.push(ValidationIssue::entity_issue(
                    id,
                    dis,
                    Severity::Error,
                    "Point must have one of: sensor, cmd, sp",
                ));
            } else if class_count > 1 {
                issues.push(ValidationIssue::entity_issue(
                    id,
                    dis,
                    Severity::Error,
                    "Point has multiple classifications (sensor/cmd/sp)",
                ));
            }

            // Should have kind tag
            if !tags.contains_key("kind") {
                issues.push(ValidationIssue::entity_issue(
                    id,
                    dis,
                    Severity::Warning,
                    "Point missing 'kind' tag (Bool, Number, Str)",
                ));
            }

            // Writable points should have writable marker
            if has_cmd && !tags.contains_key("writable") {
                issues.push(ValidationIssue::entity_issue(
                    id,
                    dis,
                    Severity::Info,
                    "Command point should have 'writable' marker",
                ));
            }
        }
        "equip" => {
            // Equipment should have at least one equipment type marker
            let equip_types = [
                "ahu", "rtu", "vav", "fcu", "mau", "boiler", "chiller", "coolingTower", "pump",
                "fan", "damper", "valve", "meter", "panel", "ups", "vfd", "thermostat", "heatPump",
                "heatExchanger", "humidifier", "dehumidifier", "filter", "tank", "generator",
                "coil",
            ];
            let has_type = equip_types.iter().any(|t| tags.contains_key(*t));
            if !has_type {
                issues.push(ValidationIssue::entity_issue(
                    id,
                    dis,
                    Severity::Warning,
                    "Equipment missing specific type marker (ahu, vav, pump, etc.)",
                ));
            }
        }
        "space" => {
            // Space should have a sub-type
            let space_types = ["building", "floor", "room", "wing", "roof", "zone"];
            let has_type = space_types.iter().any(|t| tags.contains_key(*t));
            if !has_type {
                issues.push(ValidationIssue::entity_issue(
                    id,
                    dis,
                    Severity::Info,
                    "Space missing sub-type (building, floor, room, etc.)",
                ));
            }
        }
        "site" => {
            // Site should have tz
            if !tags.contains_key("tz") {
                issues.push(ValidationIssue::entity_issue(
                    id,
                    dis,
                    Severity::Info,
                    "Site missing 'tz' (timezone) tag",
                ));
            }
        }
        _ => {}
    }

    issues
}

/// Validate all entities for cross-entity consistency.
pub fn validate_all(entities: &[Entity]) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    for entity in entities {
        issues.extend(validate_entity(entity));
    }

    let entity_ids: std::collections::HashSet<&str> =
        entities.iter().map(|e| e.id.as_str()).collect();

    for entity in entities {
        // Check that refs point to existing entities
        for (ref_tag, target_id) in &entity.refs {
            if !entity_ids.contains(target_id.as_str()) {
                issues.push(ValidationIssue::entity_issue(
                    &entity.id,
                    &entity.dis,
                    Severity::Warning,
                    format!("Reference '{ref_tag}' points to non-existent entity '{target_id}'"),
                ));
            }
        }

        // Equipment with spaceRef should have matching siteRef
        if entity.entity_type == "equip"
            && entity.refs.contains_key("spaceRef")
            && !entity.refs.contains_key("siteRef")
        {
            issues.push(ValidationIssue::entity_issue(
                &entity.id,
                &entity.dis,
                Severity::Warning,
                "Equipment has spaceRef but missing siteRef",
            ));
        }

        // Points should have equipRef
        if entity.entity_type == "point" && !entity.refs.contains_key("equipRef") {
            issues.push(ValidationIssue::entity_issue(
                &entity.id,
                &entity.dis,
                Severity::Info,
                "Point missing equipRef",
            ));
        }
    }

    issues
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(pairs: &[(&str, Option<&str>)]) -> HashMap<String, Option<String>> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.map(|s| s.to_string())))
            .collect()
    }

    fn marker_tags(names: &[&str]) -> HashMap<String, Option<String>> {
        names.iter().map(|n| (n.to_string(), None)).collect()
    }

    // ── validate_tags tests ──────────────────────────────────────

    #[test]
    fn temp_requires_substance() {
        let t = marker_tags(&["point", "sensor", "temp", "dis"]);
        // dis is present so no dis warning; temp without substance → warning
        let issues = validate_tags("point", &t);
        assert!(
            issues.iter().any(|i| i.severity == Severity::Warning
                && i.tags_involved.contains(&"temp".to_string())),
            "expected warning about temp missing substance tag"
        );
    }

    #[test]
    fn temp_with_air_is_ok() {
        let t = tags(&[
            ("point", None),
            ("sensor", None),
            ("temp", None),
            ("air", None),
            ("dis", Some("Zone Air Temp")),
        ]);
        let issues = validate_tags("point", &t);
        // No warning about temp
        assert!(!issues
            .iter()
            .any(|i| i.tags_involved.contains(&"temp".to_string())));
    }

    #[test]
    fn pressure_requires_substance() {
        let t = marker_tags(&["point", "sensor", "pressure"]);
        let issues = validate_tags("point", &t);
        assert!(issues
            .iter()
            .any(|i| i.tags_involved.contains(&"pressure".to_string())));
    }

    #[test]
    fn flow_requires_substance() {
        let t = marker_tags(&["point", "sensor", "flow"]);
        let issues = validate_tags("point", &t);
        assert!(issues
            .iter()
            .any(|i| i.tags_involved.contains(&"flow".to_string())));
    }

    #[test]
    fn humidity_requires_air() {
        let t = marker_tags(&["point", "sensor", "humidity", "water"]);
        let issues = validate_tags("point", &t);
        // water doesn't satisfy humidity → warning
        assert!(issues
            .iter()
            .any(|i| i.tags_involved.contains(&"humidity".to_string())));
    }

    #[test]
    fn humidity_with_air_ok() {
        let t = tags(&[
            ("point", None),
            ("sensor", None),
            ("humidity", None),
            ("air", None),
            ("dis", Some("Zone Humidity")),
        ]);
        let issues = validate_tags("point", &t);
        assert!(!issues
            .iter()
            .any(|i| i.tags_involved.contains(&"humidity".to_string())));
    }

    #[test]
    fn cmd_and_sensor_mutually_exclusive() {
        let t = tags(&[
            ("point", None),
            ("cmd", None),
            ("sensor", None),
            ("dis", Some("Bad Point")),
        ]);
        let issues = validate_tags("point", &t);
        assert!(issues
            .iter()
            .any(|i| i.severity == Severity::Error && i.tags_involved.contains(&"cmd".to_string())));
    }

    #[test]
    fn cmd_and_sp_mutually_exclusive() {
        let t = tags(&[
            ("point", None),
            ("cmd", None),
            ("sp", None),
            ("dis", Some("Bad")),
        ]);
        let issues = validate_tags("point", &t);
        assert!(issues
            .iter()
            .any(|i| i.severity == Severity::Error && i.tags_involved.contains(&"sp".to_string())));
    }

    #[test]
    fn equip_marker_without_equipref_warns() {
        let t = tags(&[
            ("point", None),
            ("sensor", None),
            ("equip", None),
            ("dis", Some("Weird Point")),
        ]);
        let issues = validate_tags("point", &t);
        assert!(issues.iter().any(|i| i.severity == Severity::Warning
            && i.tags_involved.contains(&"equipRef".to_string())));
    }

    #[test]
    fn space_marker_without_spaceref_warns() {
        let t = tags(&[
            ("equip", None),
            ("ahu", None),
            ("space", None),
            ("dis", Some("AHU in space")),
        ]);
        let issues = validate_tags("equip", &t);
        assert!(issues.iter().any(|i| i.severity == Severity::Warning
            && i.tags_involved.contains(&"spaceRef".to_string())));
    }

    #[test]
    fn missing_dis_produces_info() {
        let t = marker_tags(&["point", "sensor", "air", "temp"]);
        let issues = validate_tags("point", &t);
        assert!(issues.iter().any(|i| i.severity == Severity::Info
            && i.tags_involved.contains(&"dis".to_string())));
    }

    #[test]
    fn empty_dis_produces_info() {
        let t = tags(&[
            ("point", None),
            ("sensor", None),
            ("temp", None),
            ("air", None),
            ("dis", Some("  ")),
        ]);
        let issues = validate_tags("point", &t);
        assert!(issues.iter().any(|i| i.severity == Severity::Info
            && i.tags_involved.contains(&"dis".to_string())));
    }

    #[test]
    fn clean_point_no_issues() {
        let t = tags(&[
            ("point", None),
            ("sensor", None),
            ("temp", None),
            ("air", None),
            ("dis", Some("Zone Air Temp Sensor")),
            ("kind", Some("Number")),
        ]);
        let issues = validate_tags("point", &t);
        // No errors or warnings about structural rules
        assert!(!issues.iter().any(|i| i.severity == Severity::Error));
        assert!(!issues.iter().any(|i| i.severity == Severity::Warning));
    }

    // ── Legacy validate_entity tests (existing, preserved) ──────

    fn make_entity(id: &str, etype: &str, entity_tags: &[&str]) -> Entity {
        let mut tag_map = HashMap::new();
        for &t in entity_tags {
            tag_map.insert(t.to_string(), None);
        }
        Entity {
            id: id.into(),
            entity_type: etype.into(),
            dis: id.into(),
            parent_id: None,
            tags: tag_map,
            refs: HashMap::new(),
            created_ms: 0,
            updated_ms: 0,
        }
    }

    #[test]
    fn point_missing_classification() {
        let entity = make_entity("p1", "point", &["point", "temp"]);
        let issues = validate_entity(&entity);
        assert!(issues
            .iter()
            .any(|i| i.severity == Severity::Error && i.message.contains("sensor, cmd, sp")));
    }

    #[test]
    fn point_valid_sensor() {
        let entity = make_entity("p1", "point", &["point", "sensor", "temp", "kind"]);
        let issues = validate_entity(&entity);
        assert!(!issues.iter().any(|i| i.severity == Severity::Error));
    }

    #[test]
    fn point_multiple_classifications() {
        let entity = make_entity("p1", "point", &["point", "sensor", "cmd"]);
        let issues = validate_entity(&entity);
        assert!(issues
            .iter()
            .any(|i| i.message.contains("multiple classifications")));
    }

    #[test]
    fn equip_missing_type() {
        let entity = make_entity("e1", "equip", &["equip"]);
        let issues = validate_entity(&entity);
        assert!(issues
            .iter()
            .any(|i| i.severity == Severity::Warning && i.message.contains("type marker")));
    }

    #[test]
    fn equip_valid() {
        let entity = make_entity("e1", "equip", &["equip", "ahu"]);
        let issues = validate_entity(&entity);
        assert!(issues.is_empty());
    }

    #[test]
    fn cross_entity_broken_ref() {
        let mut entity = make_entity("p1", "point", &["point", "sensor"]);
        entity.refs.insert("equipRef".into(), "nonexistent".into());
        let issues = validate_all(&[entity]);
        assert!(issues.iter().any(|i| i.message.contains("non-existent")));
    }

    #[test]
    fn cross_entity_space_ref_without_site_ref() {
        let mut equip = make_entity("e1", "equip", &["equip", "ahu"]);
        equip.refs.insert("spaceRef".into(), "room-1".into());
        let room = make_entity("room-1", "space", &["space", "room"]);
        let issues = validate_all(&[equip, room]);
        assert!(issues.iter().any(|i| i.message.contains("missing siteRef")));
    }
}
