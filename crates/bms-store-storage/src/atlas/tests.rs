use std::path::PathBuf;

use rusqlite::Connection;

use super::db::{create_atlas_schema, AtlasDb};
use super::matcher::AtlasMatcher;
use super::model::AtlasStats;

/// Create a temporary in-memory Atlas database with test data.
fn setup_test_db() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("atlas-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let db_path = dir.join("test-atlas.db");

    let conn = Connection::open(&db_path).unwrap();
    create_atlas_schema(&conn).unwrap();

    // Insert test equipment
    conn.execute(
        "INSERT INTO atlas_equipment (id, name, abbreviation, category, haystack_tags, brick) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params!["equip-ahu", "Air Handling Unit", "AHU", "hvac", "ahu equip air", "HVAC:Air_Handling_Unit"],
    ).unwrap();
    conn.execute(
        "INSERT INTO atlas_equipment (id, name, abbreviation, category, haystack_tags, brick) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params!["equip-vav", "Variable Air Volume Box", "VAV", "hvac", "vav equip air variableVolume", "HVAC:Variable_Air_Volume_Box"],
    ).unwrap();
    conn.execute(
        "INSERT INTO atlas_equipment (id, name, abbreviation, category, haystack_tags, brick) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params!["equip-boiler", "Boiler", None::<String>, "hvac", "boiler equip water hot heating", "HVAC:Boiler"],
    ).unwrap();

    // Insert test points
    conn.execute(
        "INSERT INTO atlas_points (id, name, category, haystack_tags, kind, point_function, units, brick) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params!["point-dat", "Discharge Air Temperature", "hvac", "discharge air temp sensor point", "Number", "sensor", "°F", "HVAC:Discharge_Air_Temperature_Sensor"],
    ).unwrap();
    conn.execute(
        "INSERT INTO atlas_points (id, name, category, haystack_tags, kind, point_function, units, brick) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params!["point-zat-sp", "Zone Air Temperature Setpoint", "hvac", "zone air temp sp point", "Number", "sp", "°F", "HVAC:Zone_Air_Temperature_Setpoint"],
    ).unwrap();
    conn.execute(
        "INSERT INTO atlas_points (id, name, category, haystack_tags, kind, point_function, units, brick) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params!["point-sf-cmd", "Supply Fan Command", "hvac", "supply fan run cmd point", "Bool", "cmd", None::<String>, "HVAC:Supply_Fan_Command"],
    ).unwrap();

    // Insert aliases
    conn.execute(
        "INSERT INTO atlas_point_aliases (alias, point_id) VALUES (?1, ?2)",
        rusqlite::params!["discharge air temp", "point-dat"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO atlas_point_aliases (alias, point_id) VALUES (?1, ?2)",
        rusqlite::params!["dat", "point-dat"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO atlas_point_aliases (alias, point_id) VALUES (?1, ?2)",
        rusqlite::params!["discharge air temperature sensor", "point-dat"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO atlas_point_aliases (alias, point_id) VALUES (?1, ?2)",
        rusqlite::params!["zone air temp setpoint", "point-zat-sp"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO atlas_point_aliases (alias, point_id) VALUES (?1, ?2)",
        rusqlite::params!["zone air temp sp", "point-zat-sp"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO atlas_point_aliases (alias, point_id) VALUES (?1, ?2)",
        rusqlite::params!["supply fan command", "point-sf-cmd"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO atlas_point_aliases (alias, point_id) VALUES (?1, ?2)",
        rusqlite::params!["supply fan run cmd", "point-sf-cmd"],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO atlas_equip_aliases (alias, equip_id) VALUES (?1, ?2)",
        rusqlite::params!["air handling unit", "equip-ahu"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO atlas_equip_aliases (alias, equip_id) VALUES (?1, ?2)",
        rusqlite::params!["ahu", "equip-ahu"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO atlas_equip_aliases (alias, equip_id) VALUES (?1, ?2)",
        rusqlite::params!["variable air volume box", "equip-vav"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO atlas_equip_aliases (alias, equip_id) VALUES (?1, ?2)",
        rusqlite::params!["vav", "equip-vav"],
    )
    .unwrap();

    // Insert metadata
    conn.execute(
        "INSERT INTO atlas_meta (key, value) VALUES ('version', '1.0')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO atlas_meta (key, value) VALUES ('updated_ms', '1700000000000')",
        [],
    )
    .unwrap();

    // Insert typical points
    conn.execute(
        "INSERT INTO atlas_equip_typical_points (equip_id, point_id) VALUES (?1, ?2)",
        rusqlite::params!["equip-ahu", "point-dat"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO atlas_equip_typical_points (equip_id, point_id) VALUES (?1, ?2)",
        rusqlite::params!["equip-ahu", "point-sf-cmd"],
    )
    .unwrap();

    db_path
}

fn cleanup_test_db(path: &PathBuf) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn test_atlas_db_open_and_stats() {
    let path = setup_test_db();
    let db = AtlasDb::open(&path).unwrap();
    let stats = db.stats().unwrap();

    assert_eq!(stats.version, "1.0");
    assert_eq!(stats.total_points, 3);
    assert_eq!(stats.total_equipment, 3);
    assert!(stats.updated_ms > 0);

    cleanup_test_db(&path);
}

#[test]
fn test_atlas_is_available() {
    let path = setup_test_db();
    assert!(AtlasDb::is_available(&path));

    // Non-existent path
    assert!(!AtlasDb::is_available(&PathBuf::from(
        "/nonexistent/atlas.db"
    )));

    cleanup_test_db(&path);
}

#[test]
fn test_matcher_load() {
    let path = setup_test_db();
    let db = AtlasDb::open(&path).unwrap();
    let matcher = AtlasMatcher::load(&db).unwrap();

    assert_eq!(matcher.point_count(), 3);
    assert_eq!(matcher.equipment_count(), 3);

    cleanup_test_db(&path);
}

#[test]
fn test_matcher_exact_point_alias() {
    let path = setup_test_db();
    let db = AtlasDb::open(&path).unwrap();
    let matcher = AtlasMatcher::load(&db).unwrap();

    // Exact alias match — "dat" is registered as an alias for point-dat
    let result = matcher.match_point(&["dat"], None);
    assert!(result.is_some());
    let m = result.unwrap();
    assert_eq!(m.point.id, "point-dat");
    assert_eq!(m.confidence, 1.0);

    cleanup_test_db(&path);
}

#[test]
fn test_matcher_normalized_point_alias() {
    let path = setup_test_db();
    let db = AtlasDb::open(&path).unwrap();
    let matcher = AtlasMatcher::load(&db).unwrap();

    // Should normalize "Discharge-Air-Temp" to "discharge air temp" and match
    let result = matcher.match_point(&["Discharge-Air-Temp"], None);
    assert!(result.is_some());
    let m = result.unwrap();
    assert_eq!(m.point.id, "point-dat");
    assert_eq!(m.confidence, 1.0);

    cleanup_test_db(&path);
}

#[test]
fn test_matcher_token_subset_point() {
    let path = setup_test_db();
    let db = AtlasDb::open(&path).unwrap();
    let matcher = AtlasMatcher::load(&db).unwrap();

    // Token subset: "zone air temp sp" is in "zone air temp sp extra"
    let result = matcher.match_point(&["zone-air-temp-sp-extra"], None);
    assert!(result.is_some());
    let m = result.unwrap();
    assert_eq!(m.point.id, "point-zat-sp");
    assert!(m.confidence <= 0.7);

    cleanup_test_db(&path);
}

#[test]
fn test_matcher_equipment_exact() {
    let path = setup_test_db();
    let db = AtlasDb::open(&path).unwrap();
    let matcher = AtlasMatcher::load(&db).unwrap();

    let result = matcher.match_equipment("AHU");
    assert!(result.is_some());
    let m = result.unwrap();
    assert_eq!(m.equipment.id, "equip-ahu");
    assert_eq!(m.confidence, 1.0);

    cleanup_test_db(&path);
}

#[test]
fn test_matcher_equipment_abbreviation() {
    let path = setup_test_db();
    let db = AtlasDb::open(&path).unwrap();
    let matcher = AtlasMatcher::load(&db).unwrap();

    // "VAV" is a registered alias
    let result = matcher.match_equipment("VAV");
    assert!(result.is_some());
    let m = result.unwrap();
    assert_eq!(m.equipment.id, "equip-vav");

    cleanup_test_db(&path);
}

#[test]
fn test_suggest_point_tags() {
    let path = setup_test_db();
    let db = AtlasDb::open(&path).unwrap();
    let matcher = AtlasMatcher::load(&db).unwrap();

    let result = matcher.match_point(&["discharge-air-temp-sensor"], None);
    assert!(result.is_some());
    let m = result.unwrap();
    let tags = AtlasMatcher::suggest_point_tags(&m.point);

    let tag_names: Vec<&str> = tags.iter().map(|(n, _)| n.as_str()).collect();
    assert!(tag_names.contains(&"discharge"));
    assert!(tag_names.contains(&"air"));
    assert!(tag_names.contains(&"temp"));
    assert!(tag_names.contains(&"sensor"));
    assert!(tag_names.contains(&"point"));
    assert!(tag_names.contains(&"cur"));

    // Check unit was included
    assert!(tags
        .iter()
        .any(|(n, v)| n == "unit" && v.as_deref() == Some("°F")));

    cleanup_test_db(&path);
}

#[test]
fn test_suggest_equip_tags() {
    let path = setup_test_db();
    let db = AtlasDb::open(&path).unwrap();
    let matcher = AtlasMatcher::load(&db).unwrap();

    let result = matcher.match_equipment("Air Handling Unit");
    assert!(result.is_some());
    let m = result.unwrap();
    let tags = AtlasMatcher::suggest_equip_tags(&m.equipment);

    let tag_names: Vec<&str> = tags.iter().map(|(n, _)| n.as_str()).collect();
    assert!(tag_names.contains(&"ahu"));
    assert!(tag_names.contains(&"equip"));
    assert!(tag_names.contains(&"air"));

    cleanup_test_db(&path);
}

#[test]
fn test_equip_typical_points() {
    let path = setup_test_db();
    let db = AtlasDb::open(&path).unwrap();

    let typical = db.equip_typical_points("equip-ahu").unwrap();
    assert_eq!(typical.len(), 2);
    assert!(typical.contains(&"point-dat".to_string()));
    assert!(typical.contains(&"point-sf-cmd".to_string()));

    cleanup_test_db(&path);
}

#[test]
fn test_no_match_returns_none() {
    let path = setup_test_db();
    let db = AtlasDb::open(&path).unwrap();
    let matcher = AtlasMatcher::load(&db).unwrap();

    let result = matcher.match_point(&["xyzzy-unknown-gibberish"], None);
    assert!(result.is_none());

    let result = matcher.match_equipment("xyzzy-unknown-gibberish");
    assert!(result.is_none());

    cleanup_test_db(&path);
}
