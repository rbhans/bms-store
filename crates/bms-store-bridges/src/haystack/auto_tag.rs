//! # Auto-Tag — Redesign Required
//!
//! Current implementation uses keyword + unit heuristics ("dat" → temp, °F → temp, etc.).
//! This is fundamentally limited:
//! - Misses context (e.g., "rtuDat" doesn't catch "rtu" if it's not in the keyword table).
//! - No semantic understanding of equipment-vs-point relationships.
//! - Heuristic rules don't compose (a tag suggested by name pattern may conflict with one
//!   suggested by units, with no resolution policy).
//! - New keywords require source-code edits.
//!
//! A real solution should:
//! 1. Use the Atlas naming hint engine (already wired in via bms_store_storage::atlas)
//!    as the primary path. Atlas can be a curated knowledge base or external service.
//! 2. Fall back to LLM-based semantic tagging for unfamiliar names — accept that this
//!    has cost / latency / privacy implications, so it should be optional.
//! 3. Treat current keyword heuristics as a third-tier fallback, not the primary engine.
//! 4. Provide a confidence score per suggested tag so the GUI can show "high-confidence
//!    auto-applied" vs "low-confidence suggestion needs review".
//! 5. Make the rule set data-driven (TOML/JSON), not hard-coded, so naming conventions
//!    can be customized per site.
//!
//! This is a significant subsystem — not a bandaid. Out of scope for the v0.3.0
//! standardization-UX pass; should be tackled as its own plan when downstream consumer
//! apps surface real auto-tag accuracy needs.

use std::collections::HashMap;

use crate::haystack::provider::TagProvider;

/// Given a point name/id and its parent equipment type, suggest tags.
pub fn suggest_point_tags(
    point_name: &str,
    point_units: Option<&str>,
    equip_tags: &HashMap<String, Option<String>>,
    _provider: &dyn TagProvider,
) -> Vec<(String, Option<String>)> {
    suggest_point_tags_multi(&[point_name], point_units, equip_tags, _provider)
}

/// Suggest tags using multiple name sources (e.g. both ID and display name).
pub fn suggest_point_tags_multi(
    names: &[&str],
    point_units: Option<&str>,
    _equip_tags: &HashMap<String, Option<String>>,
    _provider: &dyn TagProvider,
) -> Vec<(String, Option<String>)> {
    let mut tags: Vec<(String, Option<String>)> = Vec::new();
    // Combine all name sources into a single set of parts for matching
    let combined_lower: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
    let lower = combined_lower.join(" ");
    let parts: Vec<&str> = combined_lower
        .iter()
        .flat_map(|l| l.split(['-', '_', ' ', '.']))
        .filter(|p| !p.is_empty())
        .collect();

    // Always mark as point
    tags.push(("point".into(), None));

    // Classification: sensor / cmd / sp
    let is_cmd = parts
        .iter()
        .any(|p| *p == "cmd" || *p == "command" || *p == "output");
    let is_sp = parts
        .iter()
        .any(|p| *p == "sp" || *p == "setpoint" || *p == "stpt");
    let is_sensor = parts.iter().any(|p| {
        *p == "sensor"
            || *p == "status"
            || *p == "input"
            || *p == "feedback"
            || *p == "fbk"
            || *p == "pos"
            || *p == "position"
    });

    if is_cmd {
        tags.push(("cmd".into(), None));
    } else if is_sp {
        tags.push(("sp".into(), None));
    } else if is_sensor || (!is_cmd && !is_sp) {
        tags.push(("sensor".into(), None));
    }

    // Substance detection
    if parts.contains(&"air") || lower.contains("air") {
        tags.push(("air".into(), None));
    }
    if parts
        .iter()
        .any(|p| *p == "water" || *p == "hw" || *p == "chw" || *p == "cw")
    {
        tags.push(("water".into(), None));
    }
    if parts
        .iter()
        .any(|p| *p == "elec" || *p == "electric" || *p == "electrical")
    {
        tags.push(("elec".into(), None));
    }

    // Measurement tags
    if parts.iter().any(|p| *p == "temp" || *p == "temperature") {
        tags.push(("temp".into(), None));
    }
    if parts.iter().any(|p| *p == "humidity" || *p == "rh") {
        tags.push(("humidity".into(), None));
    }
    if parts
        .iter()
        .any(|p| *p == "pressure" || *p == "press" || *p == "dp")
    {
        tags.push(("pressure".into(), None));
    }
    if parts.iter().any(|p| *p == "flow" || *p == "cfm") {
        tags.push(("flow".into(), None));
    }
    if parts.iter().any(|p| *p == "speed" || *p == "spd") {
        tags.push(("speed".into(), None));
    }
    if parts.iter().any(|p| *p == "power" || *p == "kw") {
        tags.push(("power".into(), None));
    }
    if parts.iter().any(|p| *p == "energy" || *p == "kwh") {
        tags.push(("energy".into(), None));
    }
    if parts.contains(&"co2") {
        tags.push(("co2".into(), None));
    }
    if parts.iter().any(|p| *p == "volt" || *p == "voltage") {
        tags.push(("volt".into(), None));
    }
    if parts
        .iter()
        .any(|p| *p == "current" || *p == "amp" || *p == "amps")
    {
        tags.push(("current".into(), None));
    }

    // Functional qualifiers
    if parts
        .iter()
        .any(|p| *p == "discharge" || *p == "da" || *p == "dat")
    {
        tags.push(("discharge".into(), None));
    }
    if parts
        .iter()
        .any(|p| *p == "return" || *p == "ra" || *p == "rat")
    {
        tags.push(("return".into(), None));
    }
    if parts
        .iter()
        .any(|p| *p == "supply" || *p == "sa" || *p == "sat")
    {
        tags.push(("supply".into(), None));
    }
    if parts
        .iter()
        .any(|p| *p == "mixed" || *p == "ma" || *p == "mat")
        && lower.contains("air")
    {
        tags.push(("mixed".into(), None));
    }
    if parts
        .iter()
        .any(|p| *p == "outside" || *p == "outdoor" || *p == "oa" || *p == "oat")
    {
        tags.push(("outside".into(), None));
    }
    if parts.iter().any(|p| *p == "exhaust" || *p == "ea") {
        tags.push(("exhaust".into(), None));
    }
    if parts.iter().any(|p| *p == "zone" || *p == "zn") {
        tags.push(("zone".into(), None));
    }
    if parts.iter().any(|p| *p == "entering" || *p == "ent") {
        tags.push(("entering".into(), None));
    }
    if parts.iter().any(|p| *p == "leaving" || *p == "lvg") {
        tags.push(("leaving".into(), None));
    }

    // Hot / cold / chilled
    if parts
        .iter()
        .any(|p| *p == "hot" || *p == "htg" || *p == "hw")
    {
        tags.push(("hot".into(), None));
    }
    if parts
        .iter()
        .any(|p| *p == "chilled" || *p == "chw" || *p == "clg")
    {
        tags.push(("chilled".into(), None));
    }
    if parts.iter().any(|p| *p == "condenser" || *p == "cw") && !parts.contains(&"chw") {
        tags.push(("condenser".into(), None));
    }

    // Equipment component tags
    if parts.contains(&"fan") {
        tags.push(("fan".into(), None));
    }
    if parts.contains(&"pump") {
        tags.push(("pump".into(), None));
    }
    if parts.iter().any(|p| *p == "damper" || *p == "dmp") {
        tags.push(("damper".into(), None));
    }
    if parts.iter().any(|p| *p == "valve" || *p == "vlv") {
        tags.push(("valve".into(), None));
    }

    // Run/enable/alarm
    if parts.iter().any(|p| *p == "run" || *p == "running") {
        tags.push(("run".into(), None));
    }
    if parts.iter().any(|p| *p == "enable" || *p == "enabled") {
        tags.push(("enable".into(), None));
    }
    if parts.iter().any(|p| *p == "alarm" || *p == "alm") {
        tags.push(("alarm".into(), None));
    }
    if parts.iter().any(|p| *p == "fault" || *p == "flt") {
        tags.push(("fault".into(), None));
    }
    if parts.iter().any(|p| *p == "occupied" || *p == "occ") && !parts.contains(&"unocc") {
        tags.push(("occ".into(), None));
    }

    // Unit-based inference
    if let Some(unit) = point_units {
        match unit {
            "°F" | "°C" | "K" => {
                if !tags.iter().any(|(n, _)| n == "temp") {
                    tags.push(("temp".into(), None));
                }
                tags.push(("kind".into(), Some("Number".into())));
                tags.push(("unit".into(), Some(unit.into())));
            }
            "%RH" => {
                if !tags.iter().any(|(n, _)| n == "humidity") {
                    tags.push(("humidity".into(), None));
                }
                tags.push(("kind".into(), Some("Number".into())));
                tags.push(("unit".into(), Some(unit.into())));
            }
            "psi" | "kPa" | "inH₂O" | "Pa" | "bar" => {
                if !tags.iter().any(|(n, _)| n == "pressure") {
                    tags.push(("pressure".into(), None));
                }
                tags.push(("kind".into(), Some("Number".into())));
                tags.push(("unit".into(), Some(unit.into())));
            }
            "cfm" | "L/s" | "m³/s" | "gpm" => {
                if !tags.iter().any(|(n, _)| n == "flow") {
                    tags.push(("flow".into(), None));
                }
                tags.push(("kind".into(), Some("Number".into())));
                tags.push(("unit".into(), Some(unit.into())));
            }
            "%" => {
                tags.push(("kind".into(), Some("Number".into())));
                tags.push(("unit".into(), Some("%".into())));
            }
            "kW" | "W" | "hp" => {
                if !tags.iter().any(|(n, _)| n == "power") {
                    tags.push(("power".into(), None));
                }
                tags.push(("kind".into(), Some("Number".into())));
                tags.push(("unit".into(), Some(unit.into())));
            }
            "kWh" | "MWh" | "BTU" => {
                if !tags.iter().any(|(n, _)| n == "energy") {
                    tags.push(("energy".into(), None));
                }
                tags.push(("kind".into(), Some("Number".into())));
                tags.push(("unit".into(), Some(unit.into())));
            }
            _ => {
                tags.push(("kind".into(), Some("Number".into())));
                tags.push(("unit".into(), Some(unit.into())));
            }
        }
    }

    // Mark as cur (has current value)
    tags.push(("cur".into(), None));

    // Deduplicate
    let mut seen = std::collections::HashSet::new();
    tags.retain(|(name, _)| seen.insert(name.clone()));

    tags
}

/// Given an equipment profile name, suggest equipment tags.
pub fn suggest_equip_tags(
    profile_name: &str,
    _provider: &dyn TagProvider,
) -> Vec<(String, Option<String>)> {
    let mut tags: Vec<(String, Option<String>)> = vec![("equip".into(), None)];
    let lower = profile_name.to_lowercase();
    let parts: Vec<&str> = lower.split(['-', '_', ' ']).collect();

    // Equipment type detection
    if parts.contains(&"ahu") || lower.contains("air-handling") || lower.contains("air_handling") {
        tags.push(("ahu".into(), None));
        tags.push(("air".into(), None));
    }
    if parts.contains(&"rtu") || lower.contains("rooftop") {
        tags.push(("rtu".into(), None));
        tags.push(("air".into(), None));
    }
    if parts.contains(&"vav") {
        tags.push(("vav".into(), None));
        tags.push(("air".into(), None));
        tags.push(("variableVolume".into(), None));
    }
    if parts.contains(&"fcu") || lower.contains("fan-coil") || lower.contains("fan_coil") {
        tags.push(("fcu".into(), None));
        tags.push(("air".into(), None));
    }
    if parts.contains(&"mau") || lower.contains("makeup") {
        tags.push(("mau".into(), None));
        tags.push(("air".into(), None));
    }
    if parts.contains(&"boiler") {
        tags.push(("boiler".into(), None));
        tags.push(("water".into(), None));
        tags.push(("hot".into(), None));
        tags.push(("heating".into(), None));
    }
    if parts.contains(&"chiller") {
        tags.push(("chiller".into(), None));
        tags.push(("water".into(), None));
        tags.push(("chilled".into(), None));
        tags.push(("cooling".into(), None));
    }
    if parts.contains(&"pump") {
        tags.push(("pump".into(), None));
    }
    if parts.contains(&"fan") {
        tags.push(("fan".into(), None));
        tags.push(("air".into(), None));
    }
    if parts.contains(&"meter") {
        tags.push(("meter".into(), None));
    }
    if parts.contains(&"thermostat") {
        tags.push(("thermostat".into(), None));
    }

    // Sub-type qualifiers
    if parts.contains(&"single") || parts.contains(&"singleduct") {
        tags.push(("singleDuct".into(), None));
    }
    if parts.contains(&"dual") || parts.contains(&"dualduct") {
        tags.push(("dualDuct".into(), None));
    }
    if parts.contains(&"reheat") {
        tags.push(("reheat".into(), None));
    }

    // Deduplicate
    let mut seen = std::collections::HashSet::new();
    tags.retain(|(name, _)| seen.insert(name.clone()));

    tags
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::haystack::provider::Haystack4Provider;

    fn tag_names(tags: &[(String, Option<String>)]) -> Vec<String> {
        tags.iter().map(|(n, _)| n.clone()).collect()
    }

    #[test]
    fn suggest_dat_sensor() {
        let provider = Haystack4Provider;
        let equip_tags = HashMap::new();
        let tags = suggest_point_tags(
            "discharge-air-temp-sensor",
            Some("°F"),
            &equip_tags,
            &provider,
        );
        let names = tag_names(&tags);
        assert!(names.contains(&"point".to_string()));
        assert!(names.contains(&"sensor".to_string()));
        assert!(names.contains(&"discharge".to_string()));
        assert!(names.contains(&"air".to_string()));
        assert!(names.contains(&"temp".to_string()));
    }

    #[test]
    fn suggest_zone_temp_sp() {
        let provider = Haystack4Provider;
        let equip_tags = HashMap::new();
        let tags = suggest_point_tags("zone-air-temp-sp", Some("°F"), &equip_tags, &provider);
        let names = tag_names(&tags);
        assert!(names.contains(&"sp".to_string()));
        assert!(names.contains(&"zone".to_string()));
        assert!(names.contains(&"air".to_string()));
        assert!(names.contains(&"temp".to_string()));
    }

    #[test]
    fn suggest_fan_run_cmd() {
        let provider = Haystack4Provider;
        let equip_tags = HashMap::new();
        let tags = suggest_point_tags("supply-fan-run-cmd", None, &equip_tags, &provider);
        let names = tag_names(&tags);
        assert!(names.contains(&"cmd".to_string()));
        assert!(names.contains(&"fan".to_string()));
        assert!(names.contains(&"run".to_string()));
        assert!(names.contains(&"supply".to_string()));
    }

    #[test]
    fn suggest_valve_position() {
        let provider = Haystack4Provider;
        let equip_tags = HashMap::new();
        let tags = suggest_point_tags("hw-valve-cmd", Some("%"), &equip_tags, &provider);
        let names = tag_names(&tags);
        assert!(names.contains(&"cmd".to_string()));
        assert!(names.contains(&"valve".to_string()));
        assert!(names.contains(&"hot".to_string()));
    }

    #[test]
    fn suggest_ahu_equip() {
        let provider = Haystack4Provider;
        let tags = suggest_equip_tags("ahu-single-duct", &provider);
        let names = tag_names(&tags);
        assert!(names.contains(&"equip".to_string()));
        assert!(names.contains(&"ahu".to_string()));
        assert!(names.contains(&"air".to_string()));
        assert!(names.contains(&"singleDuct".to_string()));
    }

    #[test]
    fn suggest_vav_reheat() {
        let provider = Haystack4Provider;
        let tags = suggest_equip_tags("vav-reheat", &provider);
        let names = tag_names(&tags);
        assert!(names.contains(&"equip".to_string()));
        assert!(names.contains(&"vav".to_string()));
        assert!(names.contains(&"reheat".to_string()));
    }

    #[test]
    fn suggest_boiler() {
        let provider = Haystack4Provider;
        let tags = suggest_equip_tags("boiler", &provider);
        let names = tag_names(&tags);
        assert!(names.contains(&"boiler".to_string()));
        assert!(names.contains(&"water".to_string()));
        assert!(names.contains(&"hot".to_string()));
        assert!(names.contains(&"heating".to_string()));
    }

    #[test]
    fn no_duplicate_tags() {
        let provider = Haystack4Provider;
        let equip_tags = HashMap::new();
        let tags = suggest_point_tags(
            "discharge-air-temp-sensor",
            Some("°F"),
            &equip_tags,
            &provider,
        );
        let names = tag_names(&tags);
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(names.len(), unique.len(), "duplicate tags in suggestion");
    }
}
