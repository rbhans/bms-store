//! Automatic equipment grouping by point set.
//!
//! Devices that share the same "point signature" (sorted set of point IDs and
//! kinds) are grouped under a shared parent node. This makes the device tree
//! naturally organize similar equipment together — e.g. all VAV boxes, all AHUs.
//!
//! Groups with overlapping point sets are linked as "related" so variant
//! equipment types (e.g. VAV vs VAV-with-reheat) surface naturally.

use std::collections::HashSet;

use crate::discovery::model::DiscoveredPoint;

/// A (point_id, point_kind) pair — the unit of a point set.
pub type PointEntry = (String, String);

/// Extract the canonical point set from discovered points.
/// Sorted for stable comparison and serialization.
pub fn canonical_point_set(points: &[DiscoveredPoint]) -> Vec<PointEntry> {
    let mut entries: Vec<PointEntry> = points
        .iter()
        .map(|p| (p.id.clone(), p.point_kind.as_str().to_string()))
        .collect();
    entries.sort();
    entries.dedup();
    entries
}

/// Serialize a point set to JSON for storage on a group node property.
pub fn point_set_to_json(entries: &[PointEntry]) -> String {
    serde_json::to_string(entries).unwrap_or_default()
}

/// Deserialize a point set from a JSON property value.
pub fn point_set_from_json(json: &str) -> Vec<PointEntry> {
    serde_json::from_str(json).unwrap_or_default()
}

/// Compute a kind-based fingerprint for a set of discovered points.
///
/// Hashes only the sorted list of point kind strings (ignoring IDs).
/// Two devices with different point IDs but the same set of analog/binary/multistate
/// points get the same fingerprint. This is the primary grouping mechanism.
pub fn point_kind_fingerprint(points: &[DiscoveredPoint]) -> u64 {
    let mut kinds: Vec<&str> = points.iter().map(|p| p.point_kind.as_str()).collect();
    kinds.sort();

    // FNV-1a 64-bit
    let mut hash: u64 = 0xcbf29ce484222325;
    for kind in &kinds {
        for b in kind.bytes() {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    // Mix in count
    for b in (kinds.len() as u64).to_le_bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Human-readable kind summary: "15 Analog, 5 Binary, 2 Multistate"
pub fn kind_signature(points: &[DiscoveredPoint]) -> String {
    use std::collections::BTreeMap;
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for p in points {
        *counts.entry(p.point_kind.as_str()).or_default() += 1;
    }
    let parts: Vec<String> = counts
        .into_iter()
        .map(|(kind, count)| {
            // Capitalize first letter
            let label = format!("{}{}", &kind[..1].to_uppercase(), &kind[1..]);
            format!("{count} {label}")
        })
        .collect();
    parts.join(", ")
}

/// Compute a fingerprint for a set of discovered points (legacy, ID-based).
///
/// Two devices with the same fingerprint have the same point IDs and kinds,
/// meaning they are the same type of equipment (same profile / object set).
/// NOTE: This produces unique fingerprints per device because point IDs differ.
/// Use `point_kind_fingerprint()` for grouping similar equipment types.
pub fn point_set_fingerprint(points: &[DiscoveredPoint]) -> u64 {
    let mut entries: Vec<(&str, &str)> = points
        .iter()
        .map(|p| (p.id.as_str(), p.point_kind.as_str()))
        .collect();
    entries.sort();

    // FNV-1a 64-bit: stable across Rust versions and platforms.
    let mut hash: u64 = 0xcbf29ce484222325;
    for (id, kind) in &entries {
        for b in id.bytes() {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        // separator to avoid "ab"+"c" == "a"+"bc"
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x100000001b3);
        for b in kind.bytes() {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    // Mix in length
    for b in (entries.len() as u64).to_le_bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Generate a stable group node ID from a fingerprint.
pub fn group_node_id(fingerprint: u64) -> String {
    format!("group-{fingerprint:016x}")
}

/// Suggest a group display name from the device's display name.
///
/// Strips trailing digits/dashes to generalize "VAV-1" → "VAV", "AHU-3" → "AHU".
/// Falls back to "Equipment Group" if nothing meaningful remains.
pub fn suggest_group_name(device_display_name: &str) -> String {
    let trimmed = device_display_name
        .trim_end_matches(|c: char| c.is_ascii_digit() || c == '-' || c == '_' || c == ' ')
        .trim();
    if trimmed.is_empty() {
        "Equipment Group".into()
    } else {
        trimmed.to_string()
    }
}

// ---------------------------------------------------------------------------
// Group similarity
// ---------------------------------------------------------------------------

/// Result of comparing two point sets.
#[derive(Debug, Clone)]
pub struct PointSetDiff {
    /// Points present in both sets.
    pub shared: Vec<PointEntry>,
    /// Points only in set A.
    pub only_a: Vec<PointEntry>,
    /// Points only in set B.
    pub only_b: Vec<PointEntry>,
    /// Jaccard similarity: |intersection| / |union|, 0.0–1.0.
    pub similarity: f64,
}

/// Compare two point sets and return their diff + similarity score.
pub fn point_set_diff(a: &[PointEntry], b: &[PointEntry]) -> PointSetDiff {
    let set_a: HashSet<&PointEntry> = a.iter().collect();
    let set_b: HashSet<&PointEntry> = b.iter().collect();

    let shared: Vec<PointEntry> = set_a.intersection(&set_b).map(|e| (*e).clone()).collect();
    let only_a: Vec<PointEntry> = set_a.difference(&set_b).map(|e| (*e).clone()).collect();
    let only_b: Vec<PointEntry> = set_b.difference(&set_a).map(|e| (*e).clone()).collect();

    let union_size = set_a.union(&set_b).count();
    let similarity = if union_size == 0 {
        0.0
    } else {
        shared.len() as f64 / union_size as f64
    };

    PointSetDiff {
        shared,
        only_a,
        only_b,
        similarity,
    }
}

/// A related group with its similarity info.
#[derive(Debug, Clone)]
pub struct RelatedGroup {
    pub group_id: String,
    pub group_name: String,
    pub diff: PointSetDiff,
}

/// Find groups related to a target point set.
///
/// `all_groups` is a list of `(group_id, group_name, point_set_json)`.
/// Returns groups with similarity above `min_similarity`, sorted by similarity descending.
/// The target group itself (if present) is excluded.
pub fn find_related_groups(
    target_group_id: &str,
    target_points: &[PointEntry],
    all_groups: &[(String, String, String)],
    min_similarity: f64,
) -> Vec<RelatedGroup> {
    let mut related = Vec::new();

    for (gid, gname, json) in all_groups {
        if gid == target_group_id {
            continue;
        }
        let other_points = point_set_from_json(json);
        if other_points.is_empty() {
            continue;
        }
        let diff = point_set_diff(target_points, &other_points);
        if diff.similarity >= min_similarity {
            related.push(RelatedGroup {
                group_id: gid.clone(),
                group_name: gname.clone(),
                diff,
            });
        }
    }

    related.sort_by(|a, b| b.diff.similarity.partial_cmp(&a.diff.similarity).unwrap());
    related
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::model::PointKindHint;
    use crate::node::ProtocolBinding;

    fn make_point(id: &str, kind: PointKindHint) -> DiscoveredPoint {
        DiscoveredPoint {
            id: id.into(),
            device_id: "dev-1".into(),
            display_name: id.into(),
            description: None,
            units: None,
            point_kind: kind,
            writable: false,
            binding: ProtocolBinding::virtual_binding(),
            protocol_meta: serde_json::Value::Null,
            state_labels: None,
        }
    }

    #[test]
    fn kind_fingerprint_same_kinds_different_ids() {
        // Two devices with different point IDs but same kind distribution
        let dev_a = vec![
            make_point("dat-1", PointKindHint::Analog),
            make_point("fan-cmd-1", PointKindHint::Binary),
        ];
        let dev_b = vec![
            make_point("dat-2", PointKindHint::Analog),
            make_point("fan-cmd-2", PointKindHint::Binary),
        ];
        assert_eq!(
            point_kind_fingerprint(&dev_a),
            point_kind_fingerprint(&dev_b)
        );
    }

    #[test]
    fn kind_fingerprint_different_distributions() {
        let a = vec![
            make_point("p1", PointKindHint::Analog),
            make_point("p2", PointKindHint::Binary),
        ];
        let b = vec![
            make_point("p1", PointKindHint::Analog),
            make_point("p2", PointKindHint::Analog),
        ];
        assert_ne!(point_kind_fingerprint(&a), point_kind_fingerprint(&b));
    }

    #[test]
    fn kind_fingerprint_different_count() {
        let a = vec![make_point("p1", PointKindHint::Analog)];
        let b = vec![
            make_point("p1", PointKindHint::Analog),
            make_point("p2", PointKindHint::Analog),
        ];
        assert_ne!(point_kind_fingerprint(&a), point_kind_fingerprint(&b));
    }

    #[test]
    fn kind_signature_output() {
        let pts = vec![
            make_point("p1", PointKindHint::Analog),
            make_point("p2", PointKindHint::Analog),
            make_point("p3", PointKindHint::Binary),
            make_point("p4", PointKindHint::Multistate),
        ];
        let sig = kind_signature(&pts);
        assert_eq!(sig, "2 Analog, 1 Binary, 1 Multistate");
    }

    #[test]
    fn same_points_same_fingerprint() {
        let a = vec![
            make_point("dat", PointKindHint::Analog),
            make_point("fan-cmd", PointKindHint::Binary),
        ];
        let b = vec![
            make_point("fan-cmd", PointKindHint::Binary),
            make_point("dat", PointKindHint::Analog),
        ];
        assert_eq!(point_set_fingerprint(&a), point_set_fingerprint(&b));
    }

    #[test]
    fn different_points_different_fingerprint() {
        let a = vec![make_point("dat", PointKindHint::Analog)];
        let b = vec![make_point("rat", PointKindHint::Analog)];
        assert_ne!(point_set_fingerprint(&a), point_set_fingerprint(&b));
    }

    #[test]
    fn different_kind_different_fingerprint() {
        let a = vec![make_point("cmd", PointKindHint::Binary)];
        let b = vec![make_point("cmd", PointKindHint::Analog)];
        assert_ne!(point_set_fingerprint(&a), point_set_fingerprint(&b));
    }

    #[test]
    fn suggest_group_name_strips_suffix() {
        assert_eq!(suggest_group_name("VAV-1"), "VAV");
        assert_eq!(suggest_group_name("AHU-3"), "AHU");
        assert_eq!(suggest_group_name("RTU 12"), "RTU");
        assert_eq!(suggest_group_name("Chiller_01"), "Chiller");
    }

    #[test]
    fn suggest_group_name_fallback() {
        assert_eq!(suggest_group_name("123"), "Equipment Group");
        assert_eq!(suggest_group_name(""), "Equipment Group");
    }

    #[test]
    fn canonical_point_set_sorted_and_deduped() {
        let pts = vec![
            make_point("fan-cmd", PointKindHint::Binary),
            make_point("dat", PointKindHint::Analog),
            make_point("dat", PointKindHint::Analog), // duplicate
        ];
        let set = canonical_point_set(&pts);
        assert_eq!(set.len(), 2);
        assert_eq!(set[0].0, "dat");
        assert_eq!(set[1].0, "fan-cmd");
    }

    #[test]
    fn point_set_json_roundtrip() {
        let set = vec![
            ("dat".into(), "analog".into()),
            ("fan-cmd".into(), "binary".into()),
        ];
        let json = point_set_to_json(&set);
        let back = point_set_from_json(&json);
        assert_eq!(set, back);
    }

    #[test]
    fn diff_identical_sets() {
        let a = vec![
            ("dat".into(), "analog".into()),
            ("cmd".into(), "binary".into()),
        ];
        let diff = point_set_diff(&a, &a);
        assert_eq!(diff.similarity, 1.0);
        assert_eq!(diff.shared.len(), 2);
        assert!(diff.only_a.is_empty());
        assert!(diff.only_b.is_empty());
    }

    #[test]
    fn diff_disjoint_sets() {
        let a: Vec<PointEntry> = vec![("dat".into(), "analog".into())];
        let b: Vec<PointEntry> = vec![("cmd".into(), "binary".into())];
        let diff = point_set_diff(&a, &b);
        assert_eq!(diff.similarity, 0.0);
        assert!(diff.shared.is_empty());
        assert_eq!(diff.only_a.len(), 1);
        assert_eq!(diff.only_b.len(), 1);
    }

    #[test]
    fn diff_superset_high_similarity() {
        // VAV base: dat, fan-cmd, damper-pos
        let base: Vec<PointEntry> = vec![
            ("damper-pos".into(), "analog".into()),
            ("dat".into(), "analog".into()),
            ("fan-cmd".into(), "binary".into()),
        ];
        // VAV with reheat: same + reheat-valve
        let reheat: Vec<PointEntry> = vec![
            ("damper-pos".into(), "analog".into()),
            ("dat".into(), "analog".into()),
            ("fan-cmd".into(), "binary".into()),
            ("reheat-valve".into(), "analog".into()),
        ];
        let diff = point_set_diff(&base, &reheat);
        assert_eq!(diff.shared.len(), 3);
        assert!(diff.only_a.is_empty());
        assert_eq!(diff.only_b.len(), 1);
        assert_eq!(diff.only_b[0].0, "reheat-valve");
        // Jaccard: 3/4 = 0.75
        assert!((diff.similarity - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn find_related_groups_filters_and_sorts() {
        let target: Vec<PointEntry> = vec![
            ("dat".into(), "analog".into()),
            ("fan-cmd".into(), "binary".into()),
        ];

        let groups = vec![
            // Same group (should be excluded)
            ("group-aaa".into(), "VAV".into(), point_set_to_json(&target)),
            // Superset (high similarity)
            (
                "group-bbb".into(),
                "VAV-Reheat".into(),
                point_set_to_json(&[
                    ("dat".into(), "analog".into()),
                    ("fan-cmd".into(), "binary".into()),
                    ("reheat".into(), "analog".into()),
                ]),
            ),
            // Completely different (low similarity)
            (
                "group-ccc".into(),
                "Chiller".into(),
                point_set_to_json(&[("cwst".into(), "analog".into())]),
            ),
            // Partial overlap
            (
                "group-ddd".into(),
                "FCU".into(),
                point_set_to_json(&[
                    ("dat".into(), "analog".into()),
                    ("valve".into(), "analog".into()),
                ]),
            ),
        ];

        let related = find_related_groups("group-aaa", &target, &groups, 0.5);
        // group-bbb: 2/3 = 0.67 (above 0.5), group-ddd: 1/3 = 0.33 (below 0.5), group-ccc: 0/3 = 0.0
        assert_eq!(related.len(), 1);
        assert_eq!(related[0].group_id, "group-bbb");

        // With lower threshold, more groups appear
        let related_low = find_related_groups("group-aaa", &target, &groups, 0.3);
        assert_eq!(related_low.len(), 2);
        assert_eq!(related_low[0].group_id, "group-bbb"); // highest similarity first
        assert_eq!(related_low[1].group_id, "group-ddd");
    }
}
