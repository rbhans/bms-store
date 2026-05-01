//! Parity check between the build-time generated ontology (sourced from
//! `assets/xeto-master/`) and the hand-curated [`bms_haystack::ontology::TAGS`]
//! / [`bms_haystack::ontology::EQUIP_PROTOTYPES`] tables.
//!
//! Step 2 goal: prove the generator reaches the canonical Haystack 5 names so
//! the generated tables can replace the hand-curated ones in a follow-on.
//! Until then, the hand-curated tables stay authoritative; this test guards
//! against the generator silently regressing.

use bms_haystack::ontology::{
    GENERATED_GLOBALS, GENERATED_SPECS, EQUIP_PROTOTYPES, POINT_PROTOTYPES, TAGS,
};
use std::collections::HashSet;

fn generated_names() -> HashSet<&'static str> {
    let mut set: HashSet<&'static str> = HashSet::new();
    set.extend(GENERATED_SPECS.iter().map(|s| s.name));
    set.extend(GENERATED_GLOBALS.iter().map(|g| g.name));
    set
}

#[test]
fn generator_emits_nontrivial_output() {
    assert!(
        GENERATED_SPECS.len() >= 500,
        "expected ≥500 specs, got {}",
        GENERATED_SPECS.len()
    );
    assert!(
        GENERATED_GLOBALS.len() >= 300,
        "expected ≥300 globals (PhEntity tags), got {}",
        GENERATED_GLOBALS.len()
    );
}

/// Anchor specs that MUST exist in upstream Haystack 5 — if any disappear
/// the generator is broken or the bundle was swapped for an empty one.
/// (`Pump` and `Fan` aren't in upstream — H5 uses `PumpMotor` / `FanMotor`.)
#[test]
fn generator_finds_anchor_specs() {
    let names = generated_names();
    let must_have = [
        // sys
        "Marker", "Bool", "Number", "Str", "Ref", "Uri", "Date", "Time",
        "DateTime", "Coord", "List", "Dict", "Grid", "Enum", "Choice",
        // ph entities
        "Site", "Space", "Equip", "Point", "Device", "System", "Weather",
        // ph point hierarchy
        "BoolPoint", "EnumPoint", "NumberPoint", "CmdPoint", "SensorPoint", "SpPoint",
        "CurPoint", "HisPoint", "WritablePoint",
        // ph equip
        "Ahu", "Vav", "Chiller", "Boiler", "CoolingTower",
        "PumpMotor", "FanMotor", "Coil", "AirTerminalUnit",
        // ph.points anchors
        "AirTempPoint", "AirTempSensor", "AirTempSp",
        "DischargeAirTempSensor", "ZoneAirTempSensor", "OutsideAirTempSensor",
    ];
    let missing: Vec<&str> = must_have
        .iter()
        .copied()
        .filter(|n| !names.contains(n))
        .collect();
    assert!(
        missing.is_empty(),
        "{} anchor specs missing from upstream xeto: {:?}",
        missing.len(),
        missing
    );
}

/// Anchor globals (tag names) that MUST appear as PhEntity slots.
#[test]
fn generator_finds_anchor_globals() {
    let globals: HashSet<&'static str> =
        GENERATED_GLOBALS.iter().map(|g| g.name).collect();
    // `supply` is not a standalone marker in Haystack 5; it appears as a
    // qualifier inside spec bodies (e.g. `SupplyAirTempSensor`).
    let must_have = [
        "ahu", "vav", "boiler", "chiller", "fan", "pump",
        "air", "water", "elec", "temp", "humidity", "pressure", "flow",
        "discharge", "return", "outside", "zone", "exhaust",
        "sensor", "cmd", "sp", "writable", "point", "equip",
    ];
    let missing: Vec<&str> = must_have
        .iter()
        .copied()
        .filter(|n| !globals.contains(n))
        .collect();
    assert!(
        missing.is_empty(),
        "{} anchor tag globals missing: {:?}",
        missing.len(),
        missing
    );
}

/// Parity diagnostic: counts how many hand-curated [`TAGS`] entries map to
/// upstream Haystack 5 (verbatim global, PascalCase spec, or known drift
/// allow-list). The remaining "legacy-only" set is a fixed baseline — if it
/// grows the test fails so we notice unintentional drift, and if it shrinks
/// the test fails so we remember to update the baseline (and likely retire
/// some hand-curated entries from `tags.rs`).
#[test]
fn report_legacy_tag_coverage() {
    let names = generated_names();
    // H4 markers and hand-curated additions without a single-name H5 equivalent.
    // Many were reorganized into PascalCase types (Site/Space/Equip subclasses)
    // or `Choice`-typed taxonomies; some are pure bms-store extensions.
    let h4_only_drift: HashSet<&str> = [
        // H4 substance/measurement markers folded into Phenomenon/Quantity in H5
        "supply", "gas", "smoke", "static", "noise", "lux",
        "particulate", "phLevel", "conductivity", "turbidity", "dissolved",
        "occupancyCount", "peopleSensor", "motionSensor",
        "differential", "torque", "vibration",
        "damperPosition", "valvePosition",
        "filterDp", "ductPressure", "windDir", "proof", "preheat",
        // H4 equipment-style markers replaced by spec hierarchy in H5
        "constantVolume", "variableVolume", "directExpansion",
        "chilledWaterPlant", "hotWaterPlant", "steamPlant", "condenserWaterPlant",
        "gasHeat", "oilHeat", "energyRecovery",
        "crah", "unitHeater", "radiantPanel", "baseboard",
        "splitSystem", "packagedUnit", "miniSplit",
        "ductwork", "airTerminalUnit",
        "dehumidifier", "generator", "fault",
        // H4 space sub-type markers — H5 uses Space subclasses / choices
        "building", "wing", "mechanical", "lobby", "corridor", "stairwell",
        "shaft", "parking", "exterior", "basement", "penthouse", "mezzanine",
        "openOffice", "privateOffice", "conferenceRoom", "kitchen", "restroom",
        "serverRoom", "idf", "mdf", "cleanroom", "laboratory", "operatingRoom",
        // H4 ref tag conventions — H5 keeps `equipRef`/`siteRef`/`spaceRef`/`systemRef`
        // but most H4-flavored equipRef-derivatives are gone
        "elecMeterRef", "hotWaterPlantRef", "chilledWaterPlantRef",
        "steamPlantRef", "ahuRef", "vavRef", "meterRef", "panelRef",
        // hisInterpolate / hisInterval / navName: H4 history/nav metadata
        "hisInterpolate", "hisInterval", "navName",
    ].into_iter().collect();

    fn pascal_case(name: &str) -> String {
        let mut chars = name.chars();
        match chars.next() {
            Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
            None => String::new(),
        }
    }

    let mut covered = 0usize;
    let mut missing: Vec<&str> = Vec::new();
    for tag in TAGS {
        let pc = pascal_case(tag.name);
        if names.contains(tag.name)
            || names.contains(pc.as_str())
            || h4_only_drift.contains(tag.name)
        {
            covered += 1;
        } else {
            missing.push(tag.name);
        }
    }
    let total = TAGS.len();
    let pct = covered as f64 * 100.0 / total as f64;
    eprintln!(
        "legacy TAGS overlap: {}/{} ({:.1}%) match upstream H5; {} legacy-only: {:?}",
        covered,
        total,
        pct,
        missing.len(),
        missing.iter().take(60).collect::<Vec<_>>()
    );

    // Baseline: hand-curated table currently has ~100 H4/bms-store-specific
    // entries with no upstream H5 equivalent. Grows = unintentional drift;
    // shrinks = retirement opportunity in `tags.rs`. Either way, refresh the
    // baseline deliberately.
    const LEGACY_ONLY_MAX: usize = 110;
    const LEGACY_ONLY_MIN: usize = 60;
    assert!(
        missing.len() <= LEGACY_ONLY_MAX,
        "{} legacy-only tags exceeds baseline ({}); investigate before raising",
        missing.len(),
        LEGACY_ONLY_MAX
    );
    assert!(
        missing.len() >= LEGACY_ONLY_MIN,
        "{} legacy-only tags fell below baseline ({}); retire allow-list entries \
         or curated tags now covered by upstream",
        missing.len(),
        LEGACY_ONLY_MIN
    );
}

/// Soft parity: same for equip + point prototypes vs generated specs.
#[test]
fn report_legacy_prototype_coverage() {
    let spec_names: HashSet<&'static str> =
        GENERATED_SPECS.iter().map(|s| s.name).collect();

    // Prototype names are kebab/camel mixes like "ahu" or "vav-reheat".
    // Map them to candidate xeto spec names heuristically: split on '-',
    // PascalCase each part, concatenate.
    fn proto_to_pascal(name: &str) -> String {
        name.split('-')
            .map(|p| {
                let mut chars = p.chars();
                match chars.next() {
                    Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
                    None => String::new(),
                }
            })
            .collect()
    }

    let all_protos: Vec<&'static str> = EQUIP_PROTOTYPES
        .iter()
        .chain(POINT_PROTOTYPES.iter())
        .map(|p| p.name)
        .collect();
    let total = all_protos.len();
    let mut covered = 0usize;
    let mut missing: Vec<&str> = Vec::new();
    for n in &all_protos {
        let candidate = proto_to_pascal(n);
        if spec_names.contains(candidate.as_str()) {
            covered += 1;
        } else {
            missing.push(*n);
        }
    }
    let pct = covered as f64 * 100.0 / total as f64;
    eprintln!(
        "legacy prototype coverage: {}/{} ({:.1}%) — {} missing: {:?}",
        covered,
        total,
        pct,
        missing.len(),
        missing.iter().take(20).collect::<Vec<_>>()
    );
    // Looser floor — prototype name conventions diverge more than tag names.
    assert!(
        pct >= 25.0,
        "legacy prototype coverage dropped below 25% ({:.1}%)",
        pct
    );
}
