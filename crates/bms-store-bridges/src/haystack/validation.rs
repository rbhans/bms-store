use crate::store::entity_store::Entity;

// ----------------------------------------------------------------
// Validation types
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

#[derive(Debug, Clone)]
pub struct ValidationIssue {
    pub entity_id: String,
    pub entity_dis: String,
    pub severity: Severity,
    pub message: String,
}

// ----------------------------------------------------------------
// Single entity validation
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
                issues.push(ValidationIssue {
                    entity_id: id.clone(),
                    entity_dis: dis.clone(),
                    severity: Severity::Error,
                    message: "Point must have one of: sensor, cmd, sp".into(),
                });
            } else if class_count > 1 {
                issues.push(ValidationIssue {
                    entity_id: id.clone(),
                    entity_dis: dis.clone(),
                    severity: Severity::Error,
                    message: "Point has multiple classifications (sensor/cmd/sp)".into(),
                });
            }

            // Should have kind tag
            if !tags.contains_key("kind") {
                issues.push(ValidationIssue {
                    entity_id: id.clone(),
                    entity_dis: dis.clone(),
                    severity: Severity::Warning,
                    message: "Point missing 'kind' tag (Bool, Number, Str)".into(),
                });
            }

            // Writable points should have writable marker
            if has_cmd && !tags.contains_key("writable") {
                issues.push(ValidationIssue {
                    entity_id: id.clone(),
                    entity_dis: dis.clone(),
                    severity: Severity::Info,
                    message: "Command point should have 'writable' marker".into(),
                });
            }

            // History-collected points should have his marker
            // (info level — may not always apply)
        }
        "equip" => {
            // Equipment should have at least one equipment type marker
            let equip_types = [
                "ahu",
                "rtu",
                "vav",
                "fcu",
                "mau",
                "boiler",
                "chiller",
                "coolingTower",
                "pump",
                "fan",
                "damper",
                "valve",
                "meter",
                "panel",
                "ups",
                "vfd",
                "thermostat",
                "heatPump",
                "heatExchanger",
                "humidifier",
                "dehumidifier",
                "filter",
                "tank",
                "generator",
                "coil",
            ];
            let has_type = equip_types.iter().any(|t| tags.contains_key(*t));
            if !has_type {
                issues.push(ValidationIssue {
                    entity_id: id.clone(),
                    entity_dis: dis.clone(),
                    severity: Severity::Warning,
                    message: "Equipment missing specific type marker (ahu, vav, pump, etc.)".into(),
                });
            }
        }
        "space" => {
            // Space should have a sub-type
            let space_types = ["building", "floor", "room", "wing", "roof", "zone"];
            let has_type = space_types.iter().any(|t| tags.contains_key(*t));
            if !has_type {
                issues.push(ValidationIssue {
                    entity_id: id.clone(),
                    entity_dis: dis.clone(),
                    severity: Severity::Info,
                    message: "Space missing sub-type (building, floor, room, etc.)".into(),
                });
            }
        }
        "site" => {
            // Site should have tz
            if !tags.contains_key("tz") {
                issues.push(ValidationIssue {
                    entity_id: id.clone(),
                    entity_dis: dis.clone(),
                    severity: Severity::Info,
                    message: "Site missing 'tz' (timezone) tag".into(),
                });
            }
        }
        _ => {}
    }

    issues
}

// ----------------------------------------------------------------
// Cross-entity validation
// ----------------------------------------------------------------

/// Validate all entities for cross-entity consistency.
pub fn validate_all(entities: &[Entity]) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    // First, validate each entity individually
    for entity in entities {
        issues.extend(validate_entity(entity));
    }

    // Cross-entity checks
    let entity_ids: std::collections::HashSet<&str> =
        entities.iter().map(|e| e.id.as_str()).collect();

    for entity in entities {
        // Check that refs point to existing entities
        for (ref_tag, target_id) in &entity.refs {
            if !entity_ids.contains(target_id.as_str()) {
                issues.push(ValidationIssue {
                    entity_id: entity.id.clone(),
                    entity_dis: entity.dis.clone(),
                    severity: Severity::Warning,
                    message: format!(
                        "Reference '{ref_tag}' points to non-existent entity '{target_id}'"
                    ),
                });
            }
        }

        // Equipment with spaceRef should have matching siteRef
        if entity.entity_type == "equip"
            && entity.refs.contains_key("spaceRef")
            && !entity.refs.contains_key("siteRef")
        {
            issues.push(ValidationIssue {
                entity_id: entity.id.clone(),
                entity_dis: entity.dis.clone(),
                severity: Severity::Warning,
                message: "Equipment has spaceRef but missing siteRef".into(),
            });
        }

        // Points should have equipRef
        if entity.entity_type == "point" && !entity.refs.contains_key("equipRef") {
            issues.push(ValidationIssue {
                entity_id: entity.id.clone(),
                entity_dis: entity.dis.clone(),
                severity: Severity::Info,
                message: "Point missing equipRef".into(),
            });
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
    use std::collections::HashMap;

    fn make_entity(id: &str, etype: &str, tags: &[&str]) -> Entity {
        let mut tag_map = HashMap::new();
        for &t in tags {
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
        // No errors expected
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
