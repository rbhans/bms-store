use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::bridge::bacnet::{BacnetBridge, BacnetNetworks};
use crate::bridge::modbus::ModbusBridge;
use crate::config::profile::PointValue;
use crate::discovery::bacnet_adapter::{
    adapt_bacnet_device_with_network, adapt_bacnet_points_with_network,
};
use crate::discovery::grouping::{
    canonical_point_set, group_node_id, point_kind_fingerprint, point_set_to_json,
    suggest_group_name,
};
use crate::discovery::modbus_adapter::{adapt_modbus_device, adapt_modbus_points};
use crate::discovery::model::PointKindHint;
use crate::discovery::model::{ConnStatus, DeviceState};
use crate::event::bus::{Event, EventBus};
use crate::haystack::auto_tag::{suggest_equip_tags, suggest_point_tags_multi};
use crate::haystack::provider::Haystack5Provider;
use crate::node::{Node, NodeCapabilities, NodeType};
use crate::store::discovery_store::DiscoveryStore;
use crate::store::entity_store::EntityStore;
use crate::store::node_store::NodeStore;
use crate::store::point_store::{PointKey, PointStore};

/// Source of a tag suggestion in the dry-run preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TagSource {
    /// Atlas naming-hint engine matched a curated equipment / point alias.
    Atlas,
    /// Hand-coded keyword + unit heuristic in `bms-haystack::auto_tag`.
    Heuristic,
}

/// Tags `accept_device` would apply to a single point.
#[derive(Debug, Clone, PartialEq)]
pub struct PointTagPreview {
    pub point_id: String,
    pub point_dis: String,
    pub units: Option<String>,
    pub tags: Vec<(String, Option<String>)>,
    pub source: TagSource,
    /// Coarse 0.0..=1.0 confidence — atlas inherits matcher score, heuristic is constant 0.4.
    pub confidence: f32,
}

/// Tags `accept_device` would apply to a device + its points (dry-run).
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceTagPreview {
    pub device_id: String,
    pub device_dis: String,
    pub equip_tags: Vec<(String, Option<String>)>,
    pub equip_source: TagSource,
    pub equip_confidence: f32,
    pub points: Vec<PointTagPreview>,
}

/// Knobs that customise an [`DiscoveryService::accept_device_with_options`] call.
#[derive(Debug, Clone, Default)]
pub struct AcceptOptions {
    /// Reserved — when `true` will skip auto-tagging entirely. Currently a
    /// no-op; auto-tag still runs.
    pub skip_auto_tag: bool,
    /// `NodeStore` id of the spatial parent the device should be placed
    /// under (Room, FloorArea, Floor, Building, or Site). When set,
    /// `accept_device` walks the parent chain via `get_ancestors` and
    /// sets `siteRef` / `buildingRef` / `floorRef` / `spaceRef` on the
    /// new equip entity automatically.
    pub target_space_id: Option<String>,
}

/// Resolved spatial-ref targets, as classified from a NodeStore ancestor walk.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResolvedSpatialRefs {
    pub site_ref: Option<String>,
    pub building_ref: Option<String>,
    pub floor_ref: Option<String>,
    /// Innermost space — Room beats FloorArea on conflict.
    pub space_ref: Option<String>,
}

/// Central orchestrator for device discovery and acceptance.
/// Observes bridges — does not replace them.
pub struct DiscoveryService {
    pub store: DiscoveryStore,
    node_store: NodeStore,
    entity_store: EntityStore,
    event_bus: EventBus,
    point_store: PointStore,
    scan_lock: Arc<Mutex<()>>,
    #[cfg(feature = "atlas")]
    atlas_matcher: Arc<std::sync::RwLock<Option<Arc<crate::atlas::matcher::AtlasMatcher>>>>,
}

impl DiscoveryService {
    pub fn new(
        store: DiscoveryStore,
        node_store: NodeStore,
        entity_store: EntityStore,
        event_bus: EventBus,
        point_store: PointStore,
    ) -> Self {
        DiscoveryService {
            store,
            node_store,
            entity_store,
            event_bus,
            point_store,
            scan_lock: Arc::new(Mutex::new(())),
            #[cfg(feature = "atlas")]
            atlas_matcher: Arc::new(std::sync::RwLock::new(None)),
        }
    }

    /// Set the Atlas matcher for richer auto-tagging during device acceptance.
    #[cfg(feature = "atlas")]
    pub fn with_atlas(self, matcher: Arc<crate::atlas::matcher::AtlasMatcher>) -> Self {
        *self.atlas_matcher.write().unwrap() = Some(matcher);
        self
    }

    /// Swap the Atlas matcher at runtime (e.g. after download/enable/disable).
    #[cfg(feature = "atlas")]
    pub fn set_atlas(&self, matcher: Option<Arc<crate::atlas::matcher::AtlasMatcher>>) {
        *self.atlas_matcher.write().unwrap() = matcher;
    }

    /// Get a reference to the shared Atlas matcher lock (for GUI to swap at runtime).
    #[cfg(feature = "atlas")]
    pub fn atlas_lock(
        &self,
    ) -> &Arc<std::sync::RwLock<Option<Arc<crate::atlas::matcher::AtlasMatcher>>>> {
        &self.atlas_matcher
    }

    /// Run a BACnet scan on a single bridge instance.
    /// The bridge's `network_id()` is used to tag discovered devices.
    /// User-initiated only — never called automatically.
    pub async fn scan_bacnet(&self, bridge: &mut BacnetBridge) {
        let _guard = match self.scan_lock.try_lock() {
            Ok(guard) => guard,
            Err(_) => {
                tracing::warn!("Discovery: scan already in progress, skipping");
                return;
            }
        };
        self.scan_bacnet_inner(bridge).await;
    }

    /// Inner scan logic — caller must already hold `scan_lock`.
    async fn scan_bacnet_inner(&self, bridge: &mut BacnetBridge) {
        let network_id = bridge.network_id().to_string();
        tracing::info!(network_id, "Discovery: starting BACnet scan");
        let scan_id = self.store.record_scan("bacnet").await;

        // Perform a live network re-scan: Who-Is → walk → merge into bridge.
        // rescan() returns Err on transport/discovery failure, Ok on success.
        match bridge.rescan(self.point_store.clone()).await {
            Ok(new_devices) => {
                tracing::info!(
                    network_id,
                    new = new_devices.len(),
                    total = bridge.discovered_devices().len(),
                    "Discovery: rescan complete"
                );
            }
            Err(e) => {
                tracing::error!(network_id, "Discovery: BACnet rescan error: {e}");
                self.event_bus.publish(bms_core::Event::toast_with_detail(
                    bms_core::ToastLevel::Error,
                    "discovery.bacnet",
                    format!("BACnet scan failed on network `{network_id}`"),
                    e.to_string(),
                ));
                self.store.finish_scan(scan_id, 0).await;
                self.event_bus.publish(Event::DiscoveryScanComplete {
                    protocol: "bacnet".into(),
                    device_count: 0,
                });
                return;
            }
        };

        // Which instances actually responded to Who-Is in this scan.
        // Only these get marked Online — cached devices that didn't respond stay unchanged.
        let scanned_instances = bridge.last_scan_instances();

        // Iterate the full (merged) device list from the bridge
        let devices = bridge.discovered_devices();
        let mut device_count = 0;

        for dev in devices {
            let adapted_device = adapt_bacnet_device_with_network(dev, &network_id);
            let adapted_points = adapt_bacnet_points_with_network(dev, &network_id);
            let device_id = adapted_device.id.clone();

            if let Err(e) = self.store.upsert_device(adapted_device).await {
                tracing::error!(device_id, "Discovery: failed to upsert device: {e}");
                continue;
            }

            // Only set Online for devices that responded to Who-Is in this scan.
            // Don't mark others Offline — Who-Is non-response doesn't mean the device
            // is down (it may just have missed the broadcast). The bridge's poll loop
            // and DeviceDown/DeviceDiscovered events are the authority on BACnet health.
            let instance = dev.device_id.instance();
            if scanned_instances.contains(&instance) {
                let _ = self
                    .store
                    .set_conn_status(&device_id, ConnStatus::Online)
                    .await;
            }

            if let Err(e) = self.store.upsert_points(&device_id, adapted_points).await {
                tracing::error!(device_id, "Discovery: failed to upsert points: {e}");
            }

            device_count += 1;
        }

        self.store.finish_scan(scan_id, device_count).await;

        tracing::info!(
            network_id,
            device_count,
            "Discovery: BACnet scan complete, upserted devices"
        );

        self.event_bus.publish(Event::DiscoveryScanComplete {
            protocol: "bacnet".into(),
            device_count,
        });
    }

    /// Scan all BACnet networks in parallel. Each network's bridge is temporarily
    /// removed from the map, scanned concurrently, then reinserted.
    /// Acquires the scan lock once for the entire multi-network scan.
    pub async fn scan_bacnet_all(&self, networks: &mut BacnetNetworks) {
        let _guard = match self.scan_lock.try_lock() {
            Ok(guard) => guard,
            Err(_) => {
                tracing::warn!("Discovery: scan already in progress, skipping");
                return;
            }
        };

        let ids = networks.network_ids();
        if ids.len() <= 1 {
            // Single network — no parallelism overhead needed
            for network_id in ids {
                if let Some(bridge) = networks.get_mut(&network_id) {
                    self.scan_bacnet_inner(bridge).await;
                }
            }
            return;
        }

        // Remove all bridges to allow concurrent mutable access
        let mut bridges: Vec<(String, BacnetBridge)> = ids
            .iter()
            .filter_map(|id| networks.remove(id).map(|b| (id.clone(), b)))
            .collect();

        // Scan all in parallel (scan_bacnet_inner does not re-acquire the lock)
        let futures: Vec<_> = bridges
            .iter_mut()
            .map(|(_, bridge)| self.scan_bacnet_inner(bridge))
            .collect();
        futures::future::join_all(futures).await;

        // Put bridges back
        for (id, bridge) in bridges {
            networks.insert(id, bridge);
        }
    }

    /// Run a Modbus scan: read configured devices from the bridge,
    /// verify connectivity, enrich with FC43 device identification, and record in DiscoveryStore.
    /// User-initiated only — never called automatically.
    pub async fn scan_modbus(&self, bridge: &ModbusBridge) {
        let _guard = match self.scan_lock.try_lock() {
            Ok(guard) => guard,
            Err(_) => {
                tracing::warn!("Discovery: scan already in progress, skipping");
                return;
            }
        };
        let scan_id = self.store.record_scan("modbus").await;

        let mut devices = bridge.discovered_devices();
        let mut device_count = 0;

        // Probe each device for connectivity, enrich reachable ones with FC43
        let mut online_set: HashSet<String> = HashSet::new();
        for dev in &mut devices {
            if bridge
                .check_device_online(&dev.instance_id, dev.unit_id)
                .await
            {
                online_set.insert(dev.instance_id.clone());
                bridge.enrich_device_id(dev).await;
            }
        }

        for dev in &devices {
            let adapted_device = adapt_modbus_device(dev);
            let adapted_points = adapt_modbus_points(dev);
            let device_id = adapted_device.id.clone();

            if let Err(e) = self.store.upsert_device(adapted_device).await {
                tracing::error!(device_id, "Discovery: failed to upsert device: {e}");
                continue;
            }

            let status = if online_set.contains(&dev.instance_id) {
                ConnStatus::Online
            } else {
                ConnStatus::Offline
            };
            let _ = self.store.set_conn_status(&device_id, status).await;

            if let Err(e) = self.store.upsert_points(&device_id, adapted_points).await {
                tracing::error!(device_id, "Discovery: failed to upsert points: {e}");
            }

            device_count += 1;
        }

        self.store.finish_scan(scan_id, device_count).await;

        self.event_bus.publish(Event::DiscoveryScanComplete {
            protocol: "modbus".into(),
            device_count,
        });
    }

    /// Scan a TCP host for responding Modbus unit IDs and record them in DiscoveryStore.
    /// This is a network-level scan — probes unit IDs in the given range.
    /// Previously-scanned devices not found in this pass are marked Offline.
    /// Returns the number of responding devices found.
    pub async fn scan_modbus_network(
        &self,
        bridge: &ModbusBridge,
        host: &str,
        port: u16,
        start_unit: u8,
        end_unit: u8,
    ) -> usize {
        let _guard = match self.scan_lock.try_lock() {
            Ok(guard) => guard,
            Err(_) => {
                tracing::warn!("Discovery: scan already in progress, skipping");
                return 0;
            }
        };
        let scan_id = self.store.record_scan("modbus").await;

        let devices = bridge.scan_unit_ids(host, port, start_unit, end_unit).await;
        let found_ids: HashSet<String> =
            devices.iter().map(|d| adapt_modbus_device(d).id).collect();
        let mut device_count = 0;

        for dev in &devices {
            let adapted_device = adapt_modbus_device(dev);
            let adapted_points = adapt_modbus_points(dev);
            let device_id = adapted_device.id.clone();

            if let Err(e) = self.store.upsert_device(adapted_device).await {
                tracing::error!(device_id, "Discovery: failed to upsert scanned device: {e}");
                continue;
            }

            let _ = self
                .store
                .set_conn_status(&device_id, ConnStatus::Online)
                .await;

            if let Err(e) = self.store.upsert_points(&device_id, adapted_points).await {
                tracing::error!(device_id, "Discovery: failed to upsert scanned points: {e}");
            }

            device_count += 1;
        }

        // Mark previously-scanned devices (scan- prefix for this host) that weren't
        // found in this pass as Offline so stale entries don't linger as Online.
        let scan_prefix = format!("modbus-scan-{host}-{port}-");
        self.mark_missing_offline(&scan_prefix, &found_ids).await;

        self.store.finish_scan(scan_id, device_count).await;

        self.event_bus.publish(Event::DiscoveryScanComplete {
            protocol: "modbus".into(),
            device_count,
        });

        device_count
    }

    /// Scan an RTU serial bus for responding Modbus unit IDs.
    /// Previously-scanned devices not found in this pass are marked Offline.
    /// Returns the number of responding devices found.
    pub async fn scan_modbus_rtu(
        &self,
        bridge: &ModbusBridge,
        start_unit: u8,
        end_unit: u8,
    ) -> usize {
        let _guard = match self.scan_lock.try_lock() {
            Ok(guard) => guard,
            Err(_) => {
                tracing::warn!("Discovery: scan already in progress, skipping");
                return 0;
            }
        };
        let scan_id = self.store.record_scan("modbus").await;

        let devices = bridge.scan_rtu_unit_ids(start_unit, end_unit).await;
        let found_ids: HashSet<String> =
            devices.iter().map(|d| adapt_modbus_device(d).id).collect();
        let mut device_count = 0;

        for dev in &devices {
            let adapted_device = adapt_modbus_device(dev);
            let adapted_points = adapt_modbus_points(dev);
            let device_id = adapted_device.id.clone();

            if let Err(e) = self.store.upsert_device(adapted_device).await {
                tracing::error!(device_id, "Discovery: failed to upsert RTU device: {e}");
                continue;
            }

            let _ = self
                .store
                .set_conn_status(&device_id, ConnStatus::Online)
                .await;

            if let Err(e) = self.store.upsert_points(&device_id, adapted_points).await {
                tracing::error!(device_id, "Discovery: failed to upsert RTU points: {e}");
            }

            device_count += 1;
        }

        // Mark previously-scanned RTU devices not found in this pass as Offline
        let scan_prefix = "modbus-scan-rtu-";
        self.mark_missing_offline(scan_prefix, &found_ids).await;

        self.store.finish_scan(scan_id, device_count).await;

        self.event_bus.publish(Event::DiscoveryScanComplete {
            protocol: "modbus".into(),
            device_count,
        });

        device_count
    }

    /// Mark devices with IDs matching `prefix` that are NOT in `found_ids` as Offline.
    async fn mark_missing_offline(&self, prefix: &str, found_ids: &HashSet<String>) {
        let all_devices = self.store.list_devices(None).await;
        for dev in &all_devices {
            if dev.id.starts_with(prefix) && !found_ids.contains(&dev.id) {
                let _ = self
                    .store
                    .set_conn_status(&dev.id, ConnStatus::Offline)
                    .await;
            }
        }
    }

    /// Compute the tags `accept_device` would apply, **without writing
    /// anything to storage**. Lets the GUI render a preview / dry-run modal
    /// so the operator can confirm or override before commit.
    ///
    /// Each suggestion reports its `source` (`"atlas"` or `"heuristic"`)
    /// and a coarse `confidence` value (0.0..=1.0). Atlas matches inherit
    /// the matcher's score; heuristic suggestions report a constant 0.4
    /// (low — the heuristic engine has no real notion of confidence).
    pub async fn preview_device_tags(
        &self,
        device_id: &str,
    ) -> Result<DeviceTagPreview, String> {
        let device = self
            .store
            .get_device(device_id)
            .await
            .map_err(|e| format!("Device not found: {e}"))?;
        let points = self.store.get_points(device_id).await;
        let provider = Haystack5Provider;

        #[cfg(feature = "atlas")]
        let atlas_snapshot: Option<Arc<crate::atlas::matcher::AtlasMatcher>> =
            self.atlas_matcher.read().unwrap().clone();

        // ---- equip ----
        let (equip_tags, equip_source, equip_confidence) = {
            #[cfg(feature = "atlas")]
            {
                if let Some(ref atlas) = atlas_snapshot {
                    if let Some(m) = atlas.match_equipment(&device.display_name) {
                        let tags =
                            crate::atlas::matcher::AtlasMatcher::suggest_equip_tags(&m.equipment);
                        (tags, TagSource::Atlas, m.confidence)
                    } else {
                        (
                            suggest_equip_tags(&device.display_name, &provider),
                            TagSource::Heuristic,
                            0.4,
                        )
                    }
                } else {
                    (
                        suggest_equip_tags(&device.display_name, &provider),
                        TagSource::Heuristic,
                        0.4,
                    )
                }
            }
            #[cfg(not(feature = "atlas"))]
            {
                (
                    suggest_equip_tags(&device.display_name, &provider),
                    TagSource::Heuristic,
                    0.4f32,
                )
            }
        };
        let equip_tag_map: HashMap<String, Option<String>> =
            equip_tags.iter().cloned().collect();

        // ---- points ----
        let mut point_previews = Vec::with_capacity(points.len());
        for pt in &points {
            let names: Vec<&str> = vec![&pt.id, &pt.display_name];
            let (tags, source, confidence) = {
                #[cfg(feature = "atlas")]
                {
                    if let Some(ref atlas) = atlas_snapshot {
                        if let Some(m) = atlas.match_point(&names, pt.units.as_deref()) {
                            (
                                crate::atlas::matcher::AtlasMatcher::suggest_point_tags(&m.point),
                                TagSource::Atlas,
                                m.confidence,
                            )
                        } else {
                            (
                                suggest_point_tags_multi(
                                    &names,
                                    pt.units.as_deref(),
                                    &equip_tag_map,
                                    &provider,
                                ),
                                TagSource::Heuristic,
                                0.4,
                            )
                        }
                    } else {
                        (
                            suggest_point_tags_multi(
                                &names,
                                pt.units.as_deref(),
                                &equip_tag_map,
                                &provider,
                            ),
                            TagSource::Heuristic,
                            0.4,
                        )
                    }
                }
                #[cfg(not(feature = "atlas"))]
                {
                    (
                        suggest_point_tags_multi(
                            &names,
                            pt.units.as_deref(),
                            &equip_tag_map,
                            &provider,
                        ),
                        TagSource::Heuristic,
                        0.4f32,
                    )
                }
            };
            point_previews.push(PointTagPreview {
                point_id: pt.id.clone(),
                point_dis: pt.display_name.clone(),
                units: pt.units.clone(),
                tags,
                source,
                confidence,
            });
        }

        Ok(DeviceTagPreview {
            device_id: device_id.to_string(),
            device_dis: device.display_name.clone(),
            equip_tags,
            equip_source,
            equip_confidence,
            points: point_previews,
        })
    }

    /// Accept a discovered device — creates nodes + entities + tags in one pass.
    /// See [`Self::accept_device_with_options`] to override defaults.
    pub async fn accept_device(&self, device_id: &str) -> Result<(), String> {
        self.accept_device_with_options(device_id, AcceptOptions::default())
            .await
    }

    /// Accept a discovered device with caller-supplied options. The default
    /// invocation matches [`Self::accept_device`].
    pub async fn accept_device_with_options(
        &self,
        device_id: &str,
        opts: AcceptOptions,
    ) -> Result<(), String> {
        self.accept_device_inner(device_id).await?;

        // Place the new equip into a spatial parent by setting the
        // siteRef / buildingRef / floorRef / spaceRef chain on it. The
        // ancestor walk is done from NodeStore so the chain reflects the
        // actual building/campus tree, not whatever the caller assumed.
        if let Some(target_id) = opts.target_space_id.as_deref() {
            let refs = self.resolve_spatial_refs(target_id).await;
            self.apply_spatial_refs(device_id, &refs).await;
        }
        Ok(())
    }

    /// Walk a NodeStore spatial node's ancestor chain (root-first) and
    /// classify each ancestor by node_type / tag into a ref bucket.
    /// Innermost wins for `space_ref` (Room beats FloorArea).
    pub async fn resolve_spatial_refs(&self, target_space_id: &str) -> ResolvedSpatialRefs {
        let mut out = ResolvedSpatialRefs::default();
        let chain = self.node_store.get_ancestors(target_space_id).await.unwrap_or_default();

        // Also classify the target node itself — get_ancestors returns
        // strict ancestors only, so include the target so a Room id maps
        // straight to spaceRef.
        let mut all = chain;
        if let Ok(target) = self.node_store.get_node(target_space_id).await {
            all.push(target);
        }
        for n in all {
            // Site identified by node_type only (no tag needed in this codebase)
            if n.node_type == "site" {
                out.site_ref = Some(n.id.clone());
                continue;
            }
            // Other spatial kinds tagged on Space-typed nodes
            if n.tags.contains_key("building") {
                out.building_ref = Some(n.id.clone());
            } else if n.tags.contains_key("floor") {
                out.floor_ref = Some(n.id.clone());
            } else if n.tags.contains_key("room")
                || n.tags.contains_key("floorArea")
            {
                // Room is innermost — overwrite floorArea if both seen.
                if n.tags.contains_key("room") {
                    out.space_ref = Some(n.id.clone());
                } else if out.space_ref.is_none() {
                    out.space_ref = Some(n.id.clone());
                }
            }
        }
        out
    }

    async fn apply_spatial_refs(&self, equip_id: &str, refs: &ResolvedSpatialRefs) {
        if let Some(id) = &refs.site_ref {
            let _ = self.entity_store.set_ref(equip_id, "siteRef", id).await;
        }
        if let Some(id) = &refs.building_ref {
            let _ = self.entity_store.set_ref(equip_id, "buildingRef", id).await;
        }
        if let Some(id) = &refs.floor_ref {
            let _ = self.entity_store.set_ref(equip_id, "floorRef", id).await;
        }
        if let Some(id) = &refs.space_ref {
            let _ = self.entity_store.set_ref(equip_id, "spaceRef", id).await;
        }
    }

    async fn accept_device_inner(&self, device_id: &str) -> Result<(), String> {
        let device = self
            .store
            .get_device(device_id)
            .await
            .map_err(|e| format!("Device not found: {e}"))?;

        if device.state == DeviceState::Accepted {
            return Ok(()); // Already accepted
        }

        let points = self.store.get_points(device_id).await;
        let provider = Haystack5Provider;

        // 1. Auto-group: compute kind-based fingerprint and create/reuse a group node
        let fingerprint = point_kind_fingerprint(&points);
        let group_id = group_node_id(fingerprint);
        let point_set = canonical_point_set(&points);

        // Create group (Space) node if it doesn't already exist
        if self.node_store.get_node(&group_id).await.is_err() {
            let group_name = suggest_group_name(&device.display_name);
            let group_node = Node::new(&group_id, NodeType::Space, &group_name);
            if let Err(e) = self.node_store.create_node(group_node).await {
                tracing::warn!(group_id, "Failed to create group node: {e}");
            }

            // Store canonical point set on the group for similarity comparison
            if let Err(e) = self
                .node_store
                .set_property(&group_id, "pointSet", &point_set_to_json(&point_set))
                .await
            {
                tracing::warn!(group_id, "Failed to set pointSet property: {e}");
            }
        }

        // Create equip node parented under the group (ignore error if already exists)
        let equip_node =
            Node::new(device_id, NodeType::Equip, &device.display_name).with_parent(&group_id);
        let _ = self.node_store.create_node(equip_node).await;

        // Snapshot the Atlas matcher (cheap Arc clone, drops the lock immediately)
        #[cfg(feature = "atlas")]
        let atlas_snapshot: Option<Arc<crate::atlas::matcher::AtlasMatcher>> =
            self.atlas_matcher.read().unwrap().clone();

        // 2. Auto-tag equipment — Atlas first, fallback to heuristics
        let equip_tags = {
            #[cfg(feature = "atlas")]
            {
                if let Some(ref atlas) = atlas_snapshot {
                    if let Some(m) = atlas.match_equipment(&device.display_name) {
                        tracing::debug!(device_id, alias = %m.matched_alias, confidence = m.confidence, "Atlas equipment match");
                        crate::atlas::matcher::AtlasMatcher::suggest_equip_tags(&m.equipment)
                    } else {
                        suggest_equip_tags(&device.display_name, &provider)
                    }
                } else {
                    suggest_equip_tags(&device.display_name, &provider)
                }
            }
            #[cfg(not(feature = "atlas"))]
            {
                suggest_equip_tags(&device.display_name, &provider)
            }
        };

        // 3. Create equip entity with tags
        if let Err(e) = self
            .entity_store
            .create_entity(
                device_id,
                "equip",
                &device.display_name,
                None,
                equip_tags.clone(),
            )
            .await
        {
            tracing::warn!(device_id, "Failed to create equip entity: {e}");
        }

        // Build equip_tags as HashMap for point tagging
        let equip_tag_map: HashMap<String, Option<String>> = equip_tags.into_iter().collect();

        // 4. Create point nodes + entities
        for pt in &points {
            let point_node_id = format!("{}/{}", device_id, pt.id);

            // Build node with capabilities + binding
            let caps = NodeCapabilities::new(true, pt.writable, true, true, pt.writable);

            let node = Node::new(&point_node_id, NodeType::Point, &pt.display_name)
                .with_parent(device_id)
                .with_capabilities(caps)
                .with_binding(pt.binding.clone());

            if let Err(e) = self.node_store.create_node(node).await {
                tracing::warn!(point_node_id, "Failed to create point node: {e}");
            }

            // Auto-tag point — Atlas first, fallback to heuristics
            let names: Vec<&str> = vec![&pt.id, &pt.display_name];
            let point_tags = {
                #[cfg(feature = "atlas")]
                {
                    if let Some(ref atlas) = atlas_snapshot {
                        if let Some(m) = atlas.match_point(&names, pt.units.as_deref()) {
                            tracing::debug!(point = %pt.id, alias = %m.matched_alias, confidence = m.confidence, "Atlas point match");
                            let tags =
                                crate::atlas::matcher::AtlasMatcher::suggest_point_tags(&m.point);
                            // Store atlas_point_id as a property for traceability
                            let point_node_id_for_prop = format!("{}/{}", device_id, pt.id);
                            let _ = self
                                .node_store
                                .set_property(&point_node_id_for_prop, "atlasPointId", &m.point.id)
                                .await;
                            tags
                        } else {
                            suggest_point_tags_multi(
                                &names,
                                pt.units.as_deref(),
                                &equip_tag_map,
                                &provider,
                            )
                        }
                    } else {
                        suggest_point_tags_multi(
                            &names,
                            pt.units.as_deref(),
                            &equip_tag_map,
                            &provider,
                        )
                    }
                }
                #[cfg(not(feature = "atlas"))]
                {
                    suggest_point_tags_multi(&names, pt.units.as_deref(), &equip_tag_map, &provider)
                }
            };

            // Create point entity
            if let Err(e) = self
                .entity_store
                .create_entity(
                    &point_node_id,
                    "point",
                    &pt.display_name,
                    Some(device_id),
                    point_tags,
                )
                .await
            {
                tracing::warn!(point_node_id, "Failed to create point entity: {e}");
            }

            // Set equipRef on point entity
            if let Err(e) = self
                .entity_store
                .set_ref(&point_node_id, "equipRef", device_id)
                .await
            {
                tracing::warn!(point_node_id, "Failed to set equipRef: {e}");
            }
        }

        // 5. Register points in PointStore so they appear on the home page
        for pt in &points {
            let key = PointKey {
                device_instance_id: device_id.to_string(),
                point_id: pt.id.clone(),
            };
            let default_value = match pt.point_kind {
                PointKindHint::Binary => PointValue::Bool(false),
                PointKindHint::Analog => PointValue::Float(0.0),
                PointKindHint::Multistate => PointValue::Integer(0),
            };
            self.point_store.insert_default(key, default_value);
        }
        self.point_store.bump_version();

        // 6. Update device state
        self.store
            .set_device_state(device_id, DeviceState::Accepted)
            .await
            .map_err(|e| format!("Failed to update state: {e}"))?;

        // 7. Publish event
        self.event_bus.publish(Event::DeviceAccepted {
            device_key: device_id.to_string(),
            protocol: device.protocol.as_str().to_string(),
            point_count: points.len(),
        });

        Ok(())
    }

    /// Ignore a discovered device.
    pub async fn ignore_device(&self, device_id: &str) -> Result<(), String> {
        self.store
            .set_device_state(device_id, DeviceState::Ignored)
            .await
            .map_err(|e| format!("Failed to ignore device: {e}"))
    }

    /// Un-ignore a device (move back to Discovered).
    pub async fn unignore_device(&self, device_id: &str) -> Result<(), String> {
        self.store
            .set_device_state(device_id, DeviceState::Discovered)
            .await
            .map_err(|e| format!("Failed to unignore device: {e}"))
    }

    /// Un-accept a device — moves it back to Discovered state and cleans up nodes/entities.
    pub async fn unaccept_device(&self, device_id: &str) -> Result<(), String> {
        let device = self
            .store
            .get_device(device_id)
            .await
            .map_err(|e| format!("Device not found: {e}"))?;

        if device.state != DeviceState::Accepted {
            return Err("Device is not accepted".to_string());
        }

        // Remove point nodes and entities
        let points = self.store.get_points(device_id).await;
        for pt in &points {
            let node_id = format!("{device_id}/{}", pt.id);
            let _ = self.node_store.delete_node(&node_id).await;
            let _ = self.entity_store.delete_entity(&node_id).await;
        }

        // Remove equip node and entity
        let _ = self.node_store.delete_node(device_id).await;
        let _ = self.entity_store.delete_entity(device_id).await;

        // Remove points from PointStore
        self.point_store.remove_device_points(device_id);

        // Clean up empty groups
        let groups = self.node_store.list_nodes(Some("space"), None).await;
        for group in groups {
            if !group.id.starts_with("group-") {
                continue;
            }
            let children = self.node_store.list_nodes(None, Some(&group.id)).await;
            if children.is_empty() {
                let _ = self.node_store.delete_node(&group.id).await;
            }
        }

        // Set state back to Discovered
        self.store
            .set_device_state(device_id, DeviceState::Discovered)
            .await
            .map_err(|e| format!("Failed to update state: {e}"))?;

        Ok(())
    }

    /// Apply a Modbus device profile to a discovered device, creating points
    /// from the profile's Modbus point mappings.
    /// Returns the number of points created.
    pub async fn apply_modbus_profile(
        &self,
        device_id: &str,
        profile: &crate::config::profile::DeviceProfile,
    ) -> Result<usize, String> {
        use crate::discovery::model::{DiscoveredPoint, PointKindHint, PROTOCOL_MODBUS};
        use crate::node::ProtocolBinding;

        // Verify device exists
        let _device = self
            .store
            .get_device(device_id)
            .await
            .map_err(|e| format!("Device not found: {e}"))?;

        // Convert profile points to DiscoveredPoints with Modbus bindings
        let points: Vec<DiscoveredPoint> = profile
            .points
            .iter()
            .filter_map(|pt| {
                let modbus = pt.protocols.as_ref()?.modbus.as_ref()?;
                let point_kind = match pt.kind {
                    crate::config::profile::PointKind::Analog => PointKindHint::Analog,
                    crate::config::profile::PointKind::Binary => PointKindHint::Binary,
                    crate::config::profile::PointKind::Multistate => PointKindHint::Multistate,
                };

                let binding_config = serde_json::json!({
                    "protocol": PROTOCOL_MODBUS,
                    "register_type": modbus.register_type,
                    "address": modbus.address,
                    "data_type": modbus.data_type,
                    "scale": modbus.scale,
                    "register_count": modbus.register_count,
                });

                Some(DiscoveredPoint {
                    id: pt.id.clone(),
                    device_id: device_id.to_string(),
                    display_name: pt.name.clone(),
                    description: pt.description.clone(),
                    units: pt.units.clone(),
                    point_kind,
                    writable: matches!(
                        pt.access,
                        crate::config::profile::PointAccess::Output
                            | crate::config::profile::PointAccess::Value
                    ),
                    binding: ProtocolBinding::new(PROTOCOL_MODBUS, binding_config),
                    protocol_meta: serde_json::json!({
                        "source": "profile",
                        "profile_id": profile.profile.id,
                    }),
                    state_labels: None,
                })
            })
            .collect();

        let count = points.len();
        if count == 0 {
            return Err("Profile has no Modbus point mappings".into());
        }

        self.store
            .upsert_points(device_id, points)
            .await
            .map_err(|e| format!("Failed to upsert points: {e}"))?;

        Ok(count)
    }

    /// Probe a Modbus device's registers and store the discovered points.
    /// This is an on-demand operation triggered by the user from the discovery UI.
    /// Requires access to the ModbusBridge for network I/O.
    pub async fn probe_modbus_registers(
        &self,
        device_id: &str,
        bridge: &ModbusBridge,
    ) -> Result<usize, String> {
        let device = self
            .store
            .get_device(device_id)
            .await
            .map_err(|e| format!("Device not found: {e}"))?;

        // Extract unit_id from protocol_meta
        let unit_id = device
            .protocol_meta
            .get("unit_id")
            .and_then(|v| v.as_u64())
            .map(|v| v as u8)
            .ok_or_else(|| "Missing unit_id in device metadata".to_string())?;

        // Extract host/port for TCP connection
        let host = device
            .protocol_meta
            .get("host")
            .and_then(|v| v.as_str())
            .unwrap_or(&device.address);
        let port = device
            .protocol_meta
            .get("port")
            .and_then(|v| v.as_u64())
            .map(|v| v as u16)
            .unwrap_or(502);

        // Get or create a transport for probing
        let transport = bridge
            .get_or_connect_transport(host, port)
            .await
            .map_err(|e| format!("Failed to connect: {e}"))?;

        let points = ModbusBridge::probe_registers(&transport, unit_id).await;
        let count = points.len();

        if points.is_empty() {
            return Ok(0);
        }

        // Convert to ModbusDeviceInfo to reuse adapt_modbus_points
        let info = crate::bridge::modbus::ModbusDeviceInfo {
            instance_id: device_id
                .strip_prefix("modbus-")
                .unwrap_or(device_id)
                .to_string(),
            host: host.to_string(),
            port,
            unit_id,
            vendor: None,
            model: None,
            firmware_revision: None,
            points,
        };
        let adapted = adapt_modbus_points(&info);

        self.store
            .upsert_points(device_id, adapted)
            .await
            .map_err(|e| format!("Failed to store points: {e}"))?;

        Ok(count)
    }

    /// Update a device's display name. Propagates to node/entity stores if accepted.
    pub async fn update_device_name(&self, device_id: &str, name: &str) -> Result<(), String> {
        self.store
            .update_device_name(device_id, name)
            .await
            .map_err(|e| format!("Failed to update name: {e}"))?;

        // Propagate to node + entity stores for accepted devices
        let device = self.store.get_device(device_id).await;
        if let Ok(dev) = device {
            if dev.state == DeviceState::Accepted {
                let _ = self.node_store.update_dis(device_id, name).await;
                let _ = self.entity_store.update_entity(device_id, name).await;
            }
        }
        Ok(())
    }

    /// Update a single point's properties. Propagates to node/entity stores if accepted.
    pub async fn update_point(
        &self,
        device_id: &str,
        point_id: &str,
        display_name: Option<&str>,
        units: Option<&str>,
        description: Option<&str>,
        state_labels: Option<Option<&std::collections::HashMap<String, String>>>,
    ) -> Result<(), String> {
        self.store
            .update_point(
                device_id,
                point_id,
                display_name,
                units,
                description,
                state_labels,
            )
            .await
            .map_err(|e| format!("Failed to update point: {e}"))?;

        // Propagate to node + entity stores for accepted devices
        let device = self.store.get_device(device_id).await;
        if let Ok(dev) = device {
            if dev.state == DeviceState::Accepted {
                let node_id = format!("{device_id}/{point_id}");
                if let Some(name) = display_name {
                    let _ = self.node_store.update_dis(&node_id, name).await;
                    let _ = self.entity_store.update_entity(&node_id, name).await;
                }
                if let Some(u) = units {
                    let _ = self.node_store.set_property(&node_id, "units", u).await;
                }
                match state_labels {
                    Some(Some(labels)) => {
                        if let Ok(json) = serde_json::to_string(labels) {
                            let _ = self
                                .node_store
                                .set_property(&node_id, "stateLabels", &json)
                                .await;
                        }
                    }
                    Some(None) => {
                        // Labels explicitly cleared — remove from node
                        let _ = self
                            .node_store
                            .set_property(&node_id, "stateLabels", "")
                            .await;
                    }
                    None => {}
                }
            }
        }
        Ok(())
    }

    /// Rename multiple devices, propagating to NodeStore/EntityStore for accepted ones.
    pub async fn bulk_rename_devices(
        &self,
        ids: &[String],
        names: &[String],
    ) -> Result<usize, String> {
        let count = self
            .store
            .bulk_rename_devices(ids, names)
            .await
            .map_err(|e| format!("Bulk rename failed: {e}"))?;

        // Propagate to node + entity stores for accepted devices
        for (id, name) in ids.iter().zip(names.iter()) {
            if let Ok(dev) = self.store.get_device(id).await {
                if dev.state == DeviceState::Accepted {
                    let _ = self.node_store.update_dis(id, name).await;
                    let _ = self.entity_store.update_entity(id, name).await;
                }
            }
        }

        Ok(count)
    }

    /// Apply a point name template across all devices in a group.
    /// Template placeholders: `{device}`, `{point}`, `{kind}`, `{units}`
    pub async fn apply_point_name_template(
        &self,
        device_ids: &[String],
        template: &str,
    ) -> Result<usize, String> {
        let mut count = 0;
        for device_id in device_ids {
            let device = self
                .store
                .get_device(device_id)
                .await
                .map_err(|e| format!("Device not found: {e}"))?;
            let points = self.store.get_points(device_id).await;

            for pt in &points {
                let name = template
                    .replace("{device}", &device.display_name)
                    .replace("{point}", &pt.display_name)
                    .replace("{kind}", pt.point_kind.as_str())
                    .replace("{units}", pt.units.as_deref().unwrap_or(""));

                let _ = self
                    .store
                    .update_point(device_id, &pt.id, Some(&name), None, None, None)
                    .await;

                // Propagate for accepted devices
                if device.state == DeviceState::Accepted {
                    let node_id = format!("{device_id}/{}", pt.id);
                    let _ = self.node_store.update_dis(&node_id, &name).await;
                    let _ = self.entity_store.update_entity(&node_id, &name).await;
                }

                count += 1;
            }
        }
        Ok(count)
    }

    /// Update points across all devices in a group.
    /// `point_names` maps point_id → new display name.
    /// `point_units` maps point_id → new units string.
    /// `point_state_labels` maps point_id → state labels (or None to clear).
    /// All are applied to every device in `device_ids`.
    pub async fn bulk_update_group_points(
        &self,
        device_ids: &[String],
        point_names: &HashMap<String, String>,
        point_units: &HashMap<String, String>,
        point_state_labels: &HashMap<String, Option<HashMap<String, String>>>,
    ) -> Result<usize, String> {
        let mut count = 0;
        // Merge all maps to get the set of point IDs to update
        let mut all_point_ids: HashSet<&String> = point_names.keys().collect();
        all_point_ids.extend(point_units.keys());
        all_point_ids.extend(point_state_labels.keys());

        for device_id in device_ids {
            let device = self.store.get_device(device_id).await.ok();
            let is_accepted = device
                .as_ref()
                .map(|d| d.state == DeviceState::Accepted)
                .unwrap_or(false);

            for point_id in &all_point_ids {
                let new_name = point_names.get(*point_id).filter(|n| !n.is_empty());
                let new_units = point_units.get(*point_id).filter(|u| !u.is_empty());
                let new_labels = point_state_labels.get(*point_id);

                if new_name.is_none() && new_units.is_none() && new_labels.is_none() {
                    continue;
                }

                let sl_param = new_labels.map(|opt| opt.as_ref());

                let _ = self
                    .store
                    .update_point(
                        device_id,
                        point_id,
                        new_name.map(|s| s.as_str()),
                        new_units.map(|s| s.as_str()),
                        None,
                        sl_param,
                    )
                    .await;

                if is_accepted {
                    let node_id = format!("{device_id}/{point_id}");
                    if let Some(name) = new_name {
                        let _ = self.node_store.update_dis(&node_id, name).await;
                        let _ = self.entity_store.update_entity(&node_id, name).await;
                    }
                    if let Some(units) = new_units {
                        let _ = self.node_store.set_property(&node_id, "units", units).await;
                    }
                    match new_labels {
                        Some(Some(labels)) => {
                            if let Ok(json) = serde_json::to_string(labels) {
                                let _ = self
                                    .node_store
                                    .set_property(&node_id, "stateLabels", &json)
                                    .await;
                            }
                        }
                        Some(None) => {
                            let _ = self
                                .node_store
                                .set_property(&node_id, "stateLabels", "")
                                .await;
                        }
                        None => {}
                    }
                }
                count += 1;
            }
        }
        Ok(count)
    }

    /// Bulk-update units/description for multiple points. Propagates to node/entity stores if accepted.
    pub async fn bulk_update_points(
        &self,
        device_id: &str,
        point_ids: &[String],
        units: Option<&str>,
        description: Option<&str>,
    ) -> Result<usize, String> {
        let count = self
            .store
            .bulk_update_points(device_id, point_ids, units, description)
            .await
            .map_err(|e| format!("Failed to bulk update: {e}"))?;

        // Propagate to node + entity stores for accepted devices
        let device = self.store.get_device(device_id).await;
        if let Ok(dev) = device {
            if dev.state == DeviceState::Accepted {
                for pid in point_ids {
                    let node_id = format!("{device_id}/{pid}");
                    if let Some(u) = units {
                        let _ = self.node_store.set_property(&node_id, "units", u).await;
                    }
                }
            }
        }
        Ok(count)
    }

    /// Regroup all accepted devices using kind-based fingerprints.
    ///
    /// Moves equip nodes to the correct group Space node based on
    /// `point_kind_fingerprint`. Cleans up empty old group nodes.
    /// Returns the number of devices regrouped.
    pub async fn regroup_accepted_devices(&self) -> Result<usize, String> {
        let devices = self.store.list_devices(Some(DeviceState::Accepted)).await;
        let all_device_points = self.store.get_all_device_points().await;
        let point_map: HashMap<String, Vec<crate::discovery::model::DiscoveredPoint>> =
            all_device_points.into_iter().collect();

        let mut regrouped = 0;

        for device in &devices {
            let points = match point_map.get(&device.id) {
                Some(pts) => pts,
                None => continue,
            };

            let fingerprint = point_kind_fingerprint(points);
            let new_group_id = group_node_id(fingerprint);
            let point_set = canonical_point_set(points);

            // Check current parent of the equip node
            let current_parent = self
                .node_store
                .get_node(&device.id)
                .await
                .ok()
                .and_then(|n| n.parent_id);

            if current_parent.as_deref() == Some(&new_group_id) {
                continue; // Already in the right group
            }

            // Create new group Space node if needed
            if self.node_store.get_node(&new_group_id).await.is_err() {
                let group_name = suggest_group_name(&device.display_name);
                let group_node = Node::new(&new_group_id, NodeType::Space, &group_name);
                let _ = self.node_store.create_node(group_node).await;
                let _ = self
                    .node_store
                    .set_property(&new_group_id, "pointSet", &point_set_to_json(&point_set))
                    .await;
            }

            // Move equip node to new group
            let _ = self
                .node_store
                .update_parent(&device.id, Some(&new_group_id))
                .await;

            regrouped += 1;
        }

        // Clean up empty old group nodes
        let groups = self.node_store.list_nodes(Some("space"), None).await;
        for group in groups {
            if !group.id.starts_with("group-") {
                continue;
            }
            let children = self.node_store.list_nodes(None, Some(&group.id)).await;
            if children.is_empty() {
                let _ = self.node_store.delete_node(&group.id).await;
            }
        }

        Ok(regrouped)
    }

    /// Ensure all accepted devices have their points registered in the PointStore.
    /// This is needed on startup so the home page can display points for
    /// previously-accepted devices.
    pub async fn hydrate_point_store(&self) {
        let devices = self.store.list_devices(Some(DeviceState::Accepted)).await;
        let all_device_points = self.store.get_all_device_points().await;
        let point_map: HashMap<String, Vec<crate::discovery::model::DiscoveredPoint>> =
            all_device_points.into_iter().collect();

        let mut inserted = false;
        for device in &devices {
            let points = match point_map.get(&device.id) {
                Some(pts) => pts,
                None => continue,
            };

            for pt in points {
                let key = PointKey {
                    device_instance_id: device.id.clone(),
                    point_id: pt.id.clone(),
                };
                let default_value = match pt.point_kind {
                    PointKindHint::Binary => PointValue::Bool(false),
                    PointKindHint::Analog => PointValue::Float(0.0),
                    PointKindHint::Multistate => PointValue::Integer(0),
                };
                self.point_store.insert_default(key, default_value);
                inserted = true;
            }
        }
        if inserted {
            self.point_store.bump_version();
        }
    }
}
