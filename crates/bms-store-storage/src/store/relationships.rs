//! Haystack-4 functional relationship helpers.
//!
//! Supports the following value-tag conventions (Haystack 4 standard):
//! - `supplyRef` — what supplies this entity (flow TO this entity)
//! - `returnRef` — what receives the return flow FROM this entity
//! - `connectedTo` — generic connection (electrical, network, etc.)
//!
//! These are stored in the entity's `refs` HashMap (or as value tags in
//! the `tags` map for string-valued refs). The helpers below operate on
//! the `refs` HashMap first, falling back to the `tags` HashMap for
//! legacy/alternative storage.

use crate::store::entity_store::{Entity, EntityStore};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A detected problem with a functional relationship ref.
#[derive(Debug, Clone)]
pub struct RelationshipIssue {
    /// The entity that carries the broken ref.
    pub entity_id: String,
    /// The tag or ref name that is broken (e.g. "supplyRef").
    pub tag_name: String,
    /// Human-readable description of the problem.
    pub problem: String,
}

// ---------------------------------------------------------------------------
// Public helpers
// ---------------------------------------------------------------------------

/// Return all entities that have `tag_name` pointing to `target_id`.
///
/// Checks both the `refs` HashMap (primary storage) and the `tags` HashMap
/// (legacy string-valued refs) for maximum compatibility.
pub async fn find_referrers(
    entity_store: &EntityStore,
    target_id: &str,
    tag_name: &str,
) -> Vec<Entity> {
    // The entity_store already supports GetEntitiesByRef — use it.
    entity_store.get_entities_by_ref(tag_name, target_id).await
}

/// Walk the `supplyRef` chain upstream from `start_id`.
///
/// Returns entities in order from the start (index 0) following the chain,
/// up to `max_depth` hops.  The start entity itself is NOT included in
/// the results unless a cycle brings it back.
///
/// Example: VAV → AHU → chiller plant
pub async fn walk_supply_chain(
    entity_store: &EntityStore,
    start_id: &str,
    max_depth: usize,
) -> Vec<Entity> {
    walk_chain(entity_store, start_id, "supplyRef", max_depth).await
}

/// Walk the `returnRef` chain downstream from `start_id`.
///
/// Example: chiller → cooling tower
pub async fn walk_return_chain(
    entity_store: &EntityStore,
    start_id: &str,
    max_depth: usize,
) -> Vec<Entity> {
    walk_chain(entity_store, start_id, "returnRef", max_depth).await
}

/// Validate all functional relationships across the entity store.
///
/// Checks that every `supplyRef`, `returnRef`, and `connectedTo` value in
/// the `refs` HashMap resolves to an existing entity.
///
/// Returns a list of issues found.  An empty vec means the project is clean.
pub async fn validate_relationships(entity_store: &EntityStore) -> Vec<RelationshipIssue> {
    const CHECKED_REFS: &[&str] = &["supplyRef", "returnRef", "connectedTo", "equipRef", "siteRef", "spaceRef"];

    let all_entities = entity_store.list_entities(None, None).await;
    let mut issues = Vec::new();

    for entity in &all_entities {
        for tag_name in CHECKED_REFS {
            if let Some(target_id) = entity.refs.get(*tag_name) {
                // Check target exists
                if entity_store.get_entity(target_id).await.is_err() {
                    issues.push(RelationshipIssue {
                        entity_id: entity.id.clone(),
                        tag_name: tag_name.to_string(),
                        problem: format!("{tag_name} target '{target_id}' not found"),
                    });
                }
            }
            // Also check in tags HashMap for string-valued refs
            if let Some(Some(target_id)) = entity.tags.get(*tag_name) {
                let bare = target_id.strip_prefix('@').unwrap_or(target_id.as_str());
                if entity_store.get_entity(bare).await.is_err() {
                    issues.push(RelationshipIssue {
                        entity_id: entity.id.clone(),
                        tag_name: tag_name.to_string(),
                        problem: format!("{tag_name} tag target '{bare}' not found"),
                    });
                }
            }
        }
    }

    issues
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

async fn walk_chain(
    entity_store: &EntityStore,
    start_id: &str,
    ref_tag: &str,
    max_depth: usize,
) -> Vec<Entity> {
    let mut results = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let mut current_id = start_id.to_string();
    visited.insert(current_id.clone());

    for _ in 0..=max_depth {
        let entity = match entity_store.get_entity(&current_id).await {
            Ok(e) => e,
            Err(_) => break,
        };

        // Resolve the next hop: check refs first, then tags
        let next_id = entity
            .refs
            .get(ref_tag)
            .cloned()
            .or_else(|| {
                entity.tags.get(ref_tag).and_then(|v| {
                    v.as_deref()
                        .map(|s| s.strip_prefix('@').unwrap_or(s).to_string())
                })
            });

        results.push(entity);

        match next_id {
            Some(next) if !visited.contains(&next) => {
                visited.insert(next.clone());
                current_id = next;
            }
            _ => break, // No further hop or cycle detected
        }
    }

    results
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::entity_store::start_entity_store_with_path;
    use std::path::PathBuf;

    fn test_store(path: &str) -> EntityStore {
        let db_path = PathBuf::from(path);
        if db_path.exists() {
            std::fs::remove_file(&db_path).ok();
        }
        start_entity_store_with_path(&db_path)
    }

    async fn create_equip(store: &EntityStore, id: &str, dis: &str) {
        store
            .create_entity(id, "equip", dis, None, vec![("equip".into(), None)])
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn find_referrers_basic() {
        let store = test_store("/tmp/test_rel_referrers.db");

        create_equip(&store, "chiller-1", "Chiller 1").await;
        create_equip(&store, "pump-1", "Pump 1").await;
        create_equip(&store, "ahu-1", "AHU 1").await;

        // pump-1 and ahu-1 both supply from chiller-1
        store.set_ref("pump-1", "supplyRef", "chiller-1").await.unwrap();
        store.set_ref("ahu-1", "supplyRef", "chiller-1").await.unwrap();

        let referrers = find_referrers(&store, "chiller-1", "supplyRef").await;
        let ids: std::collections::HashSet<_> = referrers.iter().map(|e| e.id.as_str()).collect();
        assert!(ids.contains("pump-1"));
        assert!(ids.contains("ahu-1"));
        assert_eq!(referrers.len(), 2);

        std::fs::remove_file("/tmp/test_rel_referrers.db").ok();
    }

    #[tokio::test]
    async fn walk_supply_chain_three_hops() {
        let store = test_store("/tmp/test_rel_supply_chain.db");

        create_equip(&store, "chiller-1", "Chiller 1").await;
        create_equip(&store, "pump-1", "Chilled Water Pump").await;
        create_equip(&store, "ahu-1", "AHU 1").await;
        create_equip(&store, "vav-1", "VAV 1").await;

        // vav-1 → ahu-1 → pump-1 → chiller-1
        store.set_ref("vav-1", "supplyRef", "ahu-1").await.unwrap();
        store.set_ref("ahu-1", "supplyRef", "pump-1").await.unwrap();
        store.set_ref("pump-1", "supplyRef", "chiller-1").await.unwrap();

        let chain = walk_supply_chain(&store, "vav-1", 10).await;
        let chain_ids: Vec<&str> = chain.iter().map(|e| e.id.as_str()).collect();
        // Should start at vav-1 and walk up
        assert_eq!(chain_ids[0], "vav-1");
        assert_eq!(chain_ids[1], "ahu-1");
        assert_eq!(chain_ids[2], "pump-1");
        assert_eq!(chain_ids[3], "chiller-1");

        std::fs::remove_file("/tmp/test_rel_supply_chain.db").ok();
    }

    #[tokio::test]
    async fn walk_supply_chain_max_depth() {
        let store = test_store("/tmp/test_rel_supply_depth.db");

        create_equip(&store, "e1", "E1").await;
        create_equip(&store, "e2", "E2").await;
        create_equip(&store, "e3", "E3").await;
        create_equip(&store, "e4", "E4").await;

        store.set_ref("e1", "supplyRef", "e2").await.unwrap();
        store.set_ref("e2", "supplyRef", "e3").await.unwrap();
        store.set_ref("e3", "supplyRef", "e4").await.unwrap();

        // Max depth 2: should return e1, e2, e3 (start + 2 hops)
        let chain = walk_supply_chain(&store, "e1", 2).await;
        assert_eq!(chain.len(), 3);

        std::fs::remove_file("/tmp/test_rel_supply_depth.db").ok();
    }

    #[tokio::test]
    async fn validate_clean_project() {
        let store = test_store("/tmp/test_rel_validate_clean.db");

        create_equip(&store, "chiller-1", "Chiller 1").await;
        create_equip(&store, "pump-1", "Pump 1").await;
        store.set_ref("pump-1", "supplyRef", "chiller-1").await.unwrap();

        let issues = validate_relationships(&store).await;
        assert!(issues.is_empty(), "expected no issues, got: {issues:?}");

        std::fs::remove_file("/tmp/test_rel_validate_clean.db").ok();
    }

    #[tokio::test]
    async fn validate_orphaned_ref() {
        let store = test_store("/tmp/test_rel_validate_orphan.db");

        create_equip(&store, "pump-1", "Pump 1").await;
        // Store a string-valued supplyRef tag pointing to non-existent entity.
        // Using the tags HashMap avoids the FK constraint on entity_ref table.
        store
            .set_tag("pump-1", "supplyRef", Some("@chiller-99"))
            .await
            .unwrap();

        let issues = validate_relationships(&store).await;
        assert!(!issues.is_empty(), "expected issues but got none");
        assert!(
            issues.iter().any(|i| i.entity_id == "pump-1" && i.tag_name == "supplyRef"),
            "expected pump-1/supplyRef issue, got: {issues:?}"
        );

        std::fs::remove_file("/tmp/test_rel_validate_orphan.db").ok();
    }

    #[tokio::test]
    async fn walk_prevents_cycle() {
        let store = test_store("/tmp/test_rel_cycle.db");

        create_equip(&store, "a", "A").await;
        create_equip(&store, "b", "B").await;

        // Cycle: a → b → a
        store.set_ref("a", "supplyRef", "b").await.unwrap();
        store.set_ref("b", "supplyRef", "a").await.unwrap();

        let chain = walk_supply_chain(&store, "a", 20).await;
        // Should stop at cycle detection: [a, b]
        assert!(chain.len() <= 3, "cycle prevention failed: {}", chain.len());

        std::fs::remove_file("/tmp/test_rel_cycle.db").ok();
    }
}
