//! Cross-protocol duplicate detection for discovered points.
//!
//! Given a flat list of `PointDescriptor`s (each from a protocol scan), returns
//! groups of suspected duplicates that span more than one protocol.
//!
//! **Heuristic:**
//! - Normalise display_name: lowercase, keep only ASCII alphanumeric chars.
//! - Same normalised name + same `kind` + same normalised `units` → High confidence.
//! - Same normalised name + same `kind`, units differ → Medium confidence.
//! - Same normalised name only, kind differs → Low confidence.
//! - Only cross-protocol pairs are reported (intra-protocol duplicates are out of scope).

/// A lightweight descriptor for a discovered point.
/// This struct is intentionally self-contained — no heavy storage types.
#[derive(Debug, Clone, PartialEq)]
pub struct PointDescriptor {
    /// Protocol-scoped device key (e.g. "bacnet-4194304" or "modbus-vav-101").
    pub device_key: String,
    /// Point identifier within the device.
    pub point_id: String,
    /// Human-readable display name.
    pub display_name: String,
    /// Normalised unit string (or empty).
    pub units: String,
    /// Point kind hint: "analog", "binary", "multistate", or other.
    pub kind: String,
    /// Protocol name (e.g. "bacnet", "modbus").
    pub protocol: String,
}

/// Confidence level of a duplicate group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// Same name, kind, and units across protocols.
    High,
    /// Same name and kind; units differ.
    Medium,
    /// Same name only; kind (and possibly units) differ.
    Low,
}

impl Confidence {
    pub fn label(&self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
        }
    }
}

/// A group of suspected duplicate points across protocols.
#[derive(Debug, Clone)]
pub struct DuplicateGroup {
    /// A stable identifier derived from the normalised display name.
    pub canonical_key: String,
    /// Points in this group (from different protocols).
    pub members: Vec<PointDescriptor>,
    /// How confident we are that these are real duplicates.
    pub confidence: Confidence,
}

// ----------------------------------------------------------------
// Normalization helpers
// ----------------------------------------------------------------

/// Normalise a display name for comparison:
/// - Lowercase.
/// - Retain only ASCII alphanumeric characters.
fn normalise_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Normalise a unit string for comparison (lowercase, strip whitespace).
fn normalise_unit(unit: &str) -> String {
    unit.trim().to_lowercase()
}

// ----------------------------------------------------------------
// Public API
// ----------------------------------------------------------------

/// Detect suspected cross-protocol duplicates in `points`.
///
/// Returns groups where at least two members come from different protocols.
/// Groups are sorted by confidence (High first) then by canonical_key.
pub fn find_duplicates(points: &[PointDescriptor]) -> Vec<DuplicateGroup> {
    if points.is_empty() {
        return Vec::new();
    }

    // Bucket by normalised name.
    let mut by_name: std::collections::HashMap<String, Vec<&PointDescriptor>> =
        std::collections::HashMap::new();

    for pt in points {
        let key = normalise_name(&pt.display_name);
        if key.is_empty() {
            continue; // skip points with no usable name
        }
        by_name.entry(key).or_default().push(pt);
    }

    let mut groups: Vec<DuplicateGroup> = Vec::new();

    for (norm_name, bucket) in by_name {
        if bucket.len() < 2 {
            continue;
        }

        // Only emit groups that span at least 2 different protocols.
        let protocols: std::collections::HashSet<&str> =
            bucket.iter().map(|p| p.protocol.as_str()).collect();
        if protocols.len() < 2 {
            continue;
        }

        // Assess confidence.
        let confidence = assess_confidence(&bucket);

        // Build canonical_key: normalised name.
        let canonical_key = norm_name.clone();

        groups.push(DuplicateGroup {
            canonical_key,
            members: bucket.iter().map(|p| (*p).clone()).collect(),
            confidence,
        });
    }

    // Sort: High > Medium > Low, then lexicographic by canonical_key.
    groups.sort_by(|a, b| {
        let ord = confidence_order(a.confidence).cmp(&confidence_order(b.confidence));
        if ord == std::cmp::Ordering::Equal {
            a.canonical_key.cmp(&b.canonical_key)
        } else {
            ord
        }
    });

    groups
}

fn confidence_order(c: Confidence) -> u8 {
    match c {
        Confidence::High => 0,
        Confidence::Medium => 1,
        Confidence::Low => 2,
    }
}

fn assess_confidence(bucket: &[&PointDescriptor]) -> Confidence {
    // All must share the same kind.
    let kinds: std::collections::HashSet<&str> =
        bucket.iter().map(|p| p.kind.as_str()).collect();
    if kinds.len() > 1 {
        return Confidence::Low;
    }

    // All must share the same normalised units.
    let units: std::collections::HashSet<String> = bucket
        .iter()
        .map(|p| normalise_unit(&p.units))
        .collect();

    if units.len() == 1 {
        Confidence::High
    } else {
        Confidence::Medium
    }
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(protocol: &str, device_key: &str, point_id: &str, display_name: &str, kind: &str, units: &str) -> PointDescriptor {
        PointDescriptor {
            device_key: device_key.to_string(),
            point_id: point_id.to_string(),
            display_name: display_name.to_string(),
            units: units.to_string(),
            kind: kind.to_string(),
            protocol: protocol.to_string(),
        }
    }

    #[test]
    fn high_confidence_same_name_kind_units() {
        let points = vec![
            pt("bacnet", "bacnet-100", "ai-1", "Zone Air Temp", "analog", "°F"),
            pt("modbus", "modbus-vav-101", "temp-reg", "Zone Air Temp", "analog", "°F"),
        ];
        let groups = find_duplicates(&points);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].confidence, Confidence::High);
        assert_eq!(groups[0].members.len(), 2);
    }

    #[test]
    fn medium_confidence_same_name_kind_different_units() {
        let points = vec![
            pt("bacnet", "bacnet-100", "ai-1", "Zone Air Temp", "analog", "°F"),
            pt("modbus", "modbus-vav-101", "temp-reg", "Zone Air Temp", "analog", "°C"),
        ];
        let groups = find_duplicates(&points);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].confidence, Confidence::Medium);
    }

    #[test]
    fn low_confidence_same_name_different_kind() {
        let points = vec![
            pt("bacnet", "bacnet-100", "bi-1", "Fan Status", "binary", ""),
            pt("modbus", "modbus-fan", "fan-reg", "Fan Status", "analog", ""),
        ];
        let groups = find_duplicates(&points);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].confidence, Confidence::Low);
    }

    #[test]
    fn no_cross_protocol_no_group() {
        let points = vec![
            pt("bacnet", "bacnet-100", "ai-1", "Zone Temp", "analog", "°F"),
            pt("bacnet", "bacnet-200", "ai-1", "Zone Temp", "analog", "°F"),
        ];
        let groups = find_duplicates(&points);
        assert!(
            groups.is_empty(),
            "intra-protocol pairs must not be flagged"
        );
    }

    #[test]
    fn empty_input() {
        assert!(find_duplicates(&[]).is_empty());
    }

    #[test]
    fn unique_names_no_group() {
        let points = vec![
            pt("bacnet", "bacnet-100", "ai-1", "Zone Temp", "analog", "°F"),
            pt("modbus", "modbus-1", "reg-1", "Supply Pressure", "analog", "psi"),
        ];
        assert!(find_duplicates(&points).is_empty());
    }

    #[test]
    fn case_insensitive_match() {
        let points = vec![
            pt("bacnet", "bacnet-100", "ai-1", "Zone Air Temp", "analog", "°F"),
            pt("modbus", "modbus-1", "reg-1", "ZONE AIR TEMP", "analog", "°F"),
        ];
        let groups = find_duplicates(&points);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].confidence, Confidence::High);
    }

    #[test]
    fn punctuation_stripped_in_name() {
        let points = vec![
            pt("bacnet", "bacnet-100", "ai-1", "Zone-Air-Temp", "analog", "°F"),
            pt("modbus", "modbus-1", "reg-1", "Zone Air Temp", "analog", "°F"),
        ];
        let groups = find_duplicates(&points);
        assert_eq!(groups.len(), 1, "hyphens/spaces stripped → same normalised name");
    }

    #[test]
    fn three_protocols_one_group() {
        let points = vec![
            pt("bacnet", "bacnet-100", "ai-1", "Discharge Air Temp", "analog", "°F"),
            pt("modbus", "modbus-1", "reg-1", "Discharge Air Temp", "analog", "°F"),
            pt("mqtt", "mqtt-bridge-1", "dat", "Discharge Air Temp", "analog", "°F"),
        ];
        let groups = find_duplicates(&points);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].members.len(), 3);
        assert_eq!(groups[0].confidence, Confidence::High);
    }

    #[test]
    fn sorted_high_before_low() {
        let points = vec![
            pt("bacnet", "bacnet-1", "a1", "Fan Status", "binary", ""),
            pt("modbus", "modbus-1", "r1", "Fan Status", "analog", ""),
            pt("bacnet", "bacnet-2", "a2", "Zone Temp", "analog", "°F"),
            pt("modbus", "modbus-2", "r2", "Zone Temp", "analog", "°F"),
        ];
        let groups = find_duplicates(&points);
        assert_eq!(groups.len(), 2);
        // High confidence group should come first.
        assert_eq!(groups[0].confidence, Confidence::High);
        assert_eq!(groups[1].confidence, Confidence::Low);
    }

    #[test]
    fn normalise_name_strips_non_alnum() {
        // The degree symbol is non-ASCII and stripped; 'F' is alphanumeric and kept.
        assert_eq!(super::normalise_name("Zone-Air-Temp (°F)"), "zoneairtempf");
        assert_eq!(super::normalise_name("VAV #101"), "vav101");
        assert_eq!(super::normalise_name(""), "");
    }

    #[test]
    fn empty_display_name_skipped() {
        let points = vec![
            pt("bacnet", "bacnet-1", "a1", "", "analog", "°F"),
            pt("modbus", "modbus-1", "r1", "", "analog", "°F"),
        ];
        // Empty normalised name → skipped, no groups.
        assert!(find_duplicates(&points).is_empty());
    }
}
