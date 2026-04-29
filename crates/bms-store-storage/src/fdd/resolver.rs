use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::fdd::model::PointRef;
use crate::store::node_store::NodeStore;

// ---------------------------------------------------------------------------
// PointResolver — tag-based point resolution with cache
// ---------------------------------------------------------------------------

/// Cache: equip_id -> (tag_key -> resolved_node_id)
type ResolverCache = HashMap<String, HashMap<String, Option<String>>>;

/// Caches tag-based point resolution within equipment.
/// Invalidated when the node store version changes.
pub struct PointResolver {
    node_store: NodeStore,
    /// tag_key is the sorted, joined tags string (e.g., "heating,valve")
    cache: Arc<RwLock<ResolverCache>>,
    last_version: Arc<RwLock<u64>>,
}

impl PointResolver {
    pub fn new(node_store: NodeStore) -> Self {
        Self {
            node_store,
            cache: Arc::new(RwLock::new(HashMap::new())),
            last_version: Arc::new(RwLock::new(0)),
        }
    }

    /// Resolve a [`PointRef`] within an equipment's child points.
    ///
    /// Returns the `node_id` of the first child whose tags are a superset of
    /// the required tags in the [`PointRef`]. Logs a warning when more than one
    /// child matches (ambiguous resolution). Returns `None` when no child
    /// matches.
    pub async fn resolve(&self, equip_id: &str, point_ref: &PointRef) -> Option<String> {
        self.check_invalidation().await;

        let tag_key = Self::tag_key(&point_ref.tags);

        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(equip_cache) = cache.get(equip_id) {
                if let Some(result) = equip_cache.get(&tag_key) {
                    return result.clone();
                }
            }
        }

        // Cache miss — resolve from NodeStore
        let result = self.resolve_from_store(equip_id, &point_ref.tags).await;

        // Store in cache
        {
            let mut cache = self.cache.write().await;
            let equip_cache = cache.entry(equip_id.to_string()).or_default();
            equip_cache.insert(tag_key, result.clone());
        }

        result
    }

    /// Resolve multiple [`PointRef`]s at once for an equipment.
    ///
    /// Returns a map from each point ref's `role` to the resolved `node_id`
    /// (or `None` if unresolvable).
    pub async fn resolve_many(
        &self,
        equip_id: &str,
        refs: &[&PointRef],
    ) -> HashMap<String, Option<String>> {
        let mut results = HashMap::new();
        for pr in refs {
            let resolved = self.resolve(equip_id, pr).await;
            results.insert(pr.role.clone(), resolved);
        }
        results
    }

    /// Clear the entire resolution cache.
    pub async fn invalidate(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    /// Check if node store version has changed and invalidate if needed.
    async fn check_invalidation(&self) {
        let current = *self.node_store.subscribe().borrow();
        let mut last = self.last_version.write().await;
        if current != *last {
            *last = current;
            let mut cache = self.cache.write().await;
            cache.clear();
        }
    }

    /// Resolve by looking up the equipment's children and matching tags.
    ///
    /// Both `Point` and `VirtualPoint` children are considered. A child matches
    /// when it carries **all** of the required tags (the tag value is ignored —
    /// only presence matters).
    async fn resolve_from_store(&self, equip_id: &str, tags: &[String]) -> Option<String> {
        // Get all child nodes of this equipment (points and virtual points)
        let children = self
            .node_store
            .list_nodes(Some("point"), Some(equip_id))
            .await;
        let mut virtual_children = self
            .node_store
            .list_nodes(Some("virtual_point"), Some(equip_id))
            .await;

        let mut all_children = children;
        all_children.append(&mut virtual_children);

        // Find children where ALL required tags are present
        let mut matches = Vec::new();
        for child in &all_children {
            let all_match = tags
                .iter()
                .all(|required_tag| child.tags.contains_key(required_tag));
            if all_match {
                matches.push(child.id.clone());
            }
        }

        if matches.len() > 1 {
            tracing::warn!(
                equip_id,
                tags = ?tags,
                count = matches.len(),
                "Ambiguous FDD point resolution — multiple points match, using first"
            );
        }

        matches.into_iter().next()
    }

    /// Produce a deterministic cache key from a set of tags.
    fn tag_key(tags: &[String]) -> String {
        let mut sorted = tags.to_vec();
        sorted.sort();
        sorted.join(",")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fdd::model::PointRef;
    use crate::node::{Node, NodeCapabilities, NodeType};
    use crate::store::node_store::start_node_store_with_path;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_db_path() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join("opencrate_fdd_resolver_tests");
        std::fs::create_dir_all(&dir).ok();
        dir.join(format!("test_resolver_{}_{}.db", std::process::id(), n))
    }

    #[test]
    fn test_tag_key_sorted_join() {
        assert_eq!(PointResolver::tag_key(&[]), "");
        assert_eq!(
            PointResolver::tag_key(&["valve".into(), "heating".into()]),
            "heating,valve"
        );
        assert_eq!(
            PointResolver::tag_key(&["c".into(), "a".into(), "b".into()]),
            "a,b,c"
        );
        // Identical input order should produce the same key
        assert_eq!(
            PointResolver::tag_key(&["heating".into(), "valve".into()]),
            PointResolver::tag_key(&["valve".into(), "heating".into()]),
        );
    }

    /// Helper: create a point node with the given tags under a parent.
    fn make_point(id: &str, parent: &str, tag_names: &[&str]) -> Node {
        let mut tags = HashMap::new();
        for t in tag_names {
            tags.insert(t.to_string(), None);
        }
        let mut node = Node::new(id, NodeType::Point, id)
            .with_parent(parent)
            .with_capabilities(NodeCapabilities::new(true, false, false, false, false));
        node.tags = tags;
        node
    }

    #[tokio::test]
    async fn test_resolve_single_match() {
        let store = start_node_store_with_path(&temp_db_path());

        // Create equipment
        store
            .create_node(Node::new("ahu-1", NodeType::Equip, "AHU-1"))
            .await
            .unwrap();

        // Create child points with different tag sets
        store
            .create_node(make_point("ahu-1/htg-vlv", "ahu-1", &["valve", "heating"]))
            .await
            .unwrap();
        store
            .create_node(make_point("ahu-1/clg-vlv", "ahu-1", &["valve", "cooling"]))
            .await
            .unwrap();
        store
            .create_node(make_point(
                "ahu-1/sat",
                "ahu-1",
                &["supply", "air", "temp", "sensor"],
            ))
            .await
            .unwrap();

        let resolver = PointResolver::new(store);

        // Resolve heating valve
        let htg_ref = PointRef {
            tags: vec!["valve".into(), "heating".into()],
            role: "HtgVlv".into(),
        };
        let result = resolver.resolve("ahu-1", &htg_ref).await;
        assert_eq!(result.as_deref(), Some("ahu-1/htg-vlv"));

        // Resolve cooling valve
        let clg_ref = PointRef {
            tags: vec!["valve".into(), "cooling".into()],
            role: "ClgVlv".into(),
        };
        let result = resolver.resolve("ahu-1", &clg_ref).await;
        assert_eq!(result.as_deref(), Some("ahu-1/clg-vlv"));

        // Resolve supply air temp
        let sat_ref = PointRef {
            tags: vec![
                "supply".into(),
                "air".into(),
                "temp".into(),
                "sensor".into(),
            ],
            role: "SAT".into(),
        };
        let result = resolver.resolve("ahu-1", &sat_ref).await;
        assert_eq!(result.as_deref(), Some("ahu-1/sat"));
    }

    #[tokio::test]
    async fn test_resolve_no_match() {
        let store = start_node_store_with_path(&temp_db_path());

        store
            .create_node(Node::new("ahu-2", NodeType::Equip, "AHU-2"))
            .await
            .unwrap();
        store
            .create_node(make_point("ahu-2/sat", "ahu-2", &["supply", "air", "temp"]))
            .await
            .unwrap();

        let resolver = PointResolver::new(store);

        // Require a tag that no child has
        let ref_missing = PointRef {
            tags: vec!["valve".into(), "heating".into()],
            role: "HtgVlv".into(),
        };
        let result = resolver.resolve("ahu-2", &ref_missing).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_resolve_many() {
        let store = start_node_store_with_path(&temp_db_path());

        store
            .create_node(Node::new("ahu-3", NodeType::Equip, "AHU-3"))
            .await
            .unwrap();
        store
            .create_node(make_point("ahu-3/htg-vlv", "ahu-3", &["valve", "heating"]))
            .await
            .unwrap();
        store
            .create_node(make_point(
                "ahu-3/sat",
                "ahu-3",
                &["supply", "air", "temp", "sensor"],
            ))
            .await
            .unwrap();

        let resolver = PointResolver::new(store);

        let htg_ref = PointRef {
            tags: vec!["valve".into(), "heating".into()],
            role: "HtgVlv".into(),
        };
        let sat_ref = PointRef {
            tags: vec![
                "supply".into(),
                "air".into(),
                "temp".into(),
                "sensor".into(),
            ],
            role: "SAT".into(),
        };
        let missing_ref = PointRef {
            tags: vec!["damper".into(), "outside".into()],
            role: "OaDmpr".into(),
        };

        let results = resolver
            .resolve_many("ahu-3", &[&htg_ref, &sat_ref, &missing_ref])
            .await;

        assert_eq!(
            results.get("HtgVlv").unwrap().as_deref(),
            Some("ahu-3/htg-vlv")
        );
        assert_eq!(results.get("SAT").unwrap().as_deref(), Some("ahu-3/sat"));
        assert!(results.get("OaDmpr").unwrap().is_none());
    }

    #[tokio::test]
    async fn test_cache_invalidation() {
        let store = start_node_store_with_path(&temp_db_path());

        store
            .create_node(Node::new("ahu-4", NodeType::Equip, "AHU-4"))
            .await
            .unwrap();
        store
            .create_node(make_point("ahu-4/htg-vlv", "ahu-4", &["valve", "heating"]))
            .await
            .unwrap();

        let resolver = PointResolver::new(store.clone());

        let htg_ref = PointRef {
            tags: vec!["valve".into(), "heating".into()],
            role: "HtgVlv".into(),
        };

        // Prime the cache
        let result = resolver.resolve("ahu-4", &htg_ref).await;
        assert_eq!(result.as_deref(), Some("ahu-4/htg-vlv"));

        // Delete the old point and add a new one — this bumps the store version
        store.delete_node("ahu-4/htg-vlv").await.unwrap();
        store
            .create_node(make_point(
                "ahu-4/htg-vlv-v2",
                "ahu-4",
                &["valve", "heating"],
            ))
            .await
            .unwrap();

        // The resolver should invalidate its cache and find the new point
        let result = resolver.resolve("ahu-4", &htg_ref).await;
        assert_eq!(result.as_deref(), Some("ahu-4/htg-vlv-v2"));
    }

    #[tokio::test]
    async fn test_resolve_virtual_point() {
        let store = start_node_store_with_path(&temp_db_path());

        store
            .create_node(Node::new("ahu-5", NodeType::Equip, "AHU-5"))
            .await
            .unwrap();

        // Create a virtual point child
        let mut tags = HashMap::new();
        tags.insert("effective".to_string(), None);
        tags.insert("setpoint".to_string(), None);
        let mut vp = Node::new("ahu-5/eff-sp", NodeType::VirtualPoint, "Effective Setpoint")
            .with_parent("ahu-5")
            .with_capabilities(NodeCapabilities::new(true, false, false, false, false));
        vp.tags = tags;
        store.create_node(vp).await.unwrap();

        let resolver = PointResolver::new(store);

        let sp_ref = PointRef {
            tags: vec!["effective".into(), "setpoint".into()],
            role: "EffSp".into(),
        };
        let result = resolver.resolve("ahu-5", &sp_ref).await;
        assert_eq!(result.as_deref(), Some("ahu-5/eff-sp"));
    }

    #[tokio::test]
    async fn test_resolve_superset_tags_match() {
        let store = start_node_store_with_path(&temp_db_path());

        store
            .create_node(Node::new("ahu-6", NodeType::Equip, "AHU-6"))
            .await
            .unwrap();

        // Child has MORE tags than we require — should still match
        store
            .create_node(make_point(
                "ahu-6/sat",
                "ahu-6",
                &["supply", "air", "temp", "sensor", "discharge"],
            ))
            .await
            .unwrap();

        let resolver = PointResolver::new(store);

        let sat_ref = PointRef {
            tags: vec!["supply".into(), "air".into(), "temp".into()],
            role: "SAT".into(),
        };
        let result = resolver.resolve("ahu-6", &sat_ref).await;
        assert_eq!(result.as_deref(), Some("ahu-6/sat"));
    }
}
