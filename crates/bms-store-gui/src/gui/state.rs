use std::sync::Arc;

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc::UnboundedSender, Mutex};

use super::theme::ThemeConfig;
use bms_store_storage::auth::{AllRolePermissions, Permission};
use bms_store_storage::config::loader::LoadedScenario;
use bms_store_storage::config::profile::PointValue;
use bms_store_bridges::discovery::service::DiscoveryService;
use bms_core::event::EventBus;
use bms_store_storage::logic::store::ProgramStore;
use bms_store_bridges::plugin::{BridgeRegistry, ProtocolBridgeHandle};
use bms_store_storage::project::{ProjectMeta, ProjectPaths};
use bms_store_storage::store::audit_store::{AuditEntryBuilder, AuditStore};
use bms_store_storage::store::discovery_store::DiscoveryStore;
use bms_store_storage::store::entity_store::EntityStore;
use bms_store_storage::store::history_store::HistoryStore;
use bms_store_storage::store::bridge_store::BridgeStore;
use bms_store_storage::store::mqtt_store::MqttStore;
use bms_store_storage::store::node_store::NodeStore;
use bms_store_storage::store::point_store::PointStore;
use bms_store_storage::store::user_store::{User, UserStore};
use bms_store_storage::store::naming_rule_store::NamingRuleStore;
use bms_store_storage::store::override_store::OverrideStore;
use bms_store_storage::store::webhook_store::WebhookStore;
use bms_store_storage::backup::BackupScheduler;
use bms_store_storage::api_key_store::ApiKeyStore;


#[derive(Debug, Clone)]
pub struct WriteCommand {
    pub device_id: String,
    pub point_id: String,
    pub value: PointValue,
    pub priority: Option<u8>,
}

/// Equipment symbol choices — used by the point-group animated symbol display.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EquipSymbol {
    Gear,
    Fan,
    Thermometer,
    Valve,
    Pump,
    AHU,
    Coil,
    Damper,
    Filter,
    Compressor,
    HeatExchanger,
    Sensor,
}

impl EquipSymbol {
    pub fn all() -> &'static [EquipSymbol] {
        &[
            Self::Gear,
            Self::Fan,
            Self::Thermometer,
            Self::Valve,
            Self::Pump,
            Self::AHU,
            Self::Coil,
            Self::Damper,
            Self::Filter,
            Self::Compressor,
            Self::HeatExchanger,
            Self::Sensor,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Gear => "Gear",
            Self::Fan => "Fan",
            Self::Thermometer => "Thermometer",
            Self::Valve => "Valve",
            Self::Pump => "Pump",
            Self::AHU => "AHU",
            Self::Coil => "Coil",
            Self::Damper => "Damper",
            Self::Filter => "Filter",
            Self::Compressor => "Compressor",
            Self::HeatExchanger => "Heat Exchanger",
            Self::Sensor => "Sensor",
        }
    }

    pub fn id(&self) -> &'static str {
        match self {
            Self::Gear => "gear",
            Self::Fan => "fan",
            Self::Thermometer => "thermometer",
            Self::Valve => "valve",
            Self::Pump => "pump",
            Self::AHU => "ahu",
            Self::Coil => "coil",
            Self::Damper => "damper",
            Self::Filter => "filter",
            Self::Compressor => "compressor",
            Self::HeatExchanger => "heat_exchanger",
            Self::Sensor => "sensor",
        }
    }

    pub fn from_id(id: &str) -> Self {
        match id {
            "fan" => Self::Fan,
            "thermometer" => Self::Thermometer,
            "valve" => Self::Valve,
            "pump" => Self::Pump,
            "ahu" => Self::AHU,
            "coil" => Self::Coil,
            "damper" => Self::Damper,
            "filter" => Self::Filter,
            "compressor" => Self::Compressor,
            "heat_exchanger" => Self::HeatExchanger,
            "sensor" => Self::Sensor,
            _ => Self::Gear,
        }
    }
}


/// What's shown in the main content area.
#[derive(Debug, Clone, PartialEq)]
pub enum ActiveView {
    Home,
    Config,
    /// A graphic page canvas, keyed by node id.
    Page(String),
    /// A device view (point table), keyed by node id. Carries the device_id to look up.
    Device {
        node_id: String,
        device_id: String,
    },
}


#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SidebarTab {
    Devices,
    Nav,
}

/// What the file menu requested when closing a project.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CloseAction {
    /// Return to launcher on the Recent tab.
    ToRecent,
    /// Return to launcher on the New Project tab.
    ToNewProject,
}

/// What the launcher selected. Currently single-project only; enum shape
/// preserved so a future Multi-site variant can be added cleanly.
#[derive(Debug, Clone)]
pub enum LaunchSelection {
    Single(bms_store_storage::project::ProjectPaths),
}

/// What kind of physical-hierarchy node this represents.
///
/// The Nav tab is the building/campus hierarchy only — every variant is a
/// spatial node. Devices show up in the Devices sidebar tab; they're
/// linked into the spatial tree by refs (`siteRef` / `spaceRef`), not by
/// appearing as nav nodes.
///
/// Hierarchy:
/// ```text
/// Site
/// └── Building
///     └── Floor
///         ├── FloorArea   (optional — east wing, north quadrant, etc.)
///         │   └── Room
///         └── Room
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NavNodeKind {
    /// Site / campus — creates `NodeType::Site` in `NodeStore`.
    Site { node_id: String },
    /// Physical building — creates `NodeType::Space` + tag `"building"`.
    Building { node_id: String },
    /// Floor within a building — `NodeType::Space` + tag `"floor"`.
    Floor { node_id: String },
    /// Sub-floor area (wing, quadrant, section) — `NodeType::Space` + tag `"floorArea"`.
    FloorArea { node_id: String },
    /// Room — `NodeType::Space` + tag `"room"`.
    Room { node_id: String },
}

impl NavNodeKind {
    /// Returns true for every variant (kept for compat with old call sites
    /// that branched between spatial and non-spatial nodes).
    pub fn is_spatial(&self) -> bool {
        true
    }

    /// Returns the `NodeStore` node_id this nav node points at.
    pub fn node_store_id(&self) -> Option<&str> {
        Some(match self {
            NavNodeKind::Site { node_id }
            | NavNodeKind::Building { node_id }
            | NavNodeKind::Floor { node_id }
            | NavNodeKind::FloorArea { node_id }
            | NavNodeKind::Room { node_id } => node_id.as_str(),
        })
    }

    /// Kind tag string — drives the icon, label, and NodeStore tag.
    pub fn kind_str(&self) -> &'static str {
        match self {
            NavNodeKind::Site { .. } => "site",
            NavNodeKind::Building { .. } => "building",
            NavNodeKind::Floor { .. } => "floor",
            NavNodeKind::FloorArea { .. } => "floorArea",
            NavNodeKind::Room { .. } => "room",
        }
    }

    /// Which child kinds may be added under this node.
    pub fn allowed_children(&self) -> &'static [&'static str] {
        match self {
            NavNodeKind::Site { .. } => &["building"],
            NavNodeKind::Building { .. } => &["floor"],
            NavNodeKind::Floor { .. } => &["floorArea", "room"],
            NavNodeKind::FloorArea { .. } => &["room"],
            NavNodeKind::Room { .. } => &[],
        }
    }
}

/// A node in the navigation hierarchy (user-built).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NavNode {
    pub id: String,
    pub label: String,
    pub kind: NavNodeKind,
    pub children: Vec<NavNode>,
}

#[derive(Clone)]
pub struct AppState {
    /// Active site bundle (per-project stores, signals, shutdown).
    pub site: super::site_context::SiteContext,
    pub store: PointStore,
    pub node_store: NodeStore,
    pub event_bus: EventBus,
    pub loaded: LoadedScenario,
    pub project_meta: ProjectMeta,
    pub project_paths: ProjectPaths,
    pub active_view: Signal<ActiveView>,
    pub sidebar_tab: Signal<SidebarTab>,
    pub selected_device: Signal<Option<String>>,
    pub selected_point: Signal<Option<String>>,
    pub detail_open: Signal<bool>,
    pub store_version: Signal<u64>,
    pub node_version: Signal<u64>,
    pub nav_tree: Signal<Vec<NavNode>>,
    pub write_tx: UnboundedSender<WriteCommand>,
    pub write_error: Signal<Option<String>>,
    /// Counter for generating unique node IDs.
    pub next_node_id: Signal<u32>,
    /// History query handle (kept for downstream consumer access).
    pub history_store: HistoryStore,
    /// Entity store for Haystack semantic tagging.
    pub entity_store: EntityStore,
    /// Discovery store for device/point discovery.
    pub discovery_store: DiscoveryStore,
    /// Discovery service for scan + accept operations.
    pub discovery_service: Arc<DiscoveryService>,
    /// Protocol bridge registry — protocol-agnostic write routing + typed access via downcast.
    pub bridge_registry: Arc<BridgeRegistry>,
    /// Program store for logic engine.
    pub program_store: ProgramStore,
    /// MQTT config store for broker connections and topic patterns.
    pub mqtt_store: MqttStore,
    pub bridge_store: BridgeStore,
    /// Webhook subscription store for endpoint configs and delivery log.
    pub webhook_store: WebhookStore,
    /// Export store for database export connector configuration.
    pub export_store: bms_store_storage::store::export_store::ExportStore,
    /// Override store — tracks active manual writes.
    pub override_store: OverrideStore,
    /// Backup scheduler — manages project backups.
    pub backup_scheduler: Arc<std::sync::Mutex<BackupScheduler>>,
    /// API key store — manages programmatic access keys.
    pub api_key_store: Arc<ApiKeyStore>,
    /// Platform health registry — shared across all subsystems.
    pub health: bms_store_storage::health::HealthRegistry,
    /// Currently logged-in user.
    pub current_user: Signal<Option<User>>,
    /// User store for authentication and user management.
    pub user_store: UserStore,
    /// Per-role permission configuration.
    pub role_permissions: Signal<AllRolePermissions>,
    /// Audit trail store for logging user actions.
    pub audit_store: AuditStore,
    /// Saved naming rules store.
    pub naming_rule_store: NamingRuleStore,
    /// Theme configuration (colors, mode, custom logo).
    pub theme_config: Signal<ThemeConfig>,
    /// Requested config sub-tab (consumed by ConfigView on render).
    pub pending_config_section: Signal<Option<String>>,
    /// Whether sidebar is visible (for mobile responsive toggle).
    pub sidebar_visible: Signal<bool>,
    /// BAS Atlas taxonomy matcher — shared with DiscoveryService.
    /// Writing to this lock immediately affects the live DiscoveryService.
    pub atlas_lock: Arc<std::sync::RwLock<Option<Arc<bms_store_storage::atlas::matcher::AtlasMatcher>>>>,
    /// Operator-facing toast queue — populated by an `Event::Toast` subscriber
    /// in `app.rs` and rendered as a banner stack in the top-right.
    pub toasts: Signal<Vec<ToastMessage>>,
}

/// One operator-facing notification — rendered as a single toast banner.
#[derive(Debug, Clone, PartialEq)]
pub struct ToastMessage {
    pub id: u64,
    pub level: bms_core::ToastLevel,
    pub message: String,
    pub detail: Option<String>,
    pub source: String,
    pub created_ms: i64,
}

impl AppState {
    /// Get the BACnet bridge handle for protocol-specific operations.
    /// Lock the returned Arc, then downcast: `guard.as_any().downcast_ref::<BacnetNetworks>()`.
    pub fn bacnet_handle(&self) -> Arc<Mutex<Box<dyn ProtocolBridgeHandle>>> {
        self.bridge_registry
            .get("bacnet")
            .expect("bacnet bridge not registered")
    }

    /// Get the Modbus bridge handle for protocol-specific operations.
    /// Lock the returned Arc, then downcast: `guard.as_any().downcast_ref::<ModbusBridge>()`.
    pub fn modbus_handle(&self) -> Option<Arc<Mutex<Box<dyn ProtocolBridgeHandle>>>> {
        self.bridge_registry.get("modbus")
    }

    pub fn view_title(&self) -> String {
        match &*self.active_view.read() {
            ActiveView::Home => "Home".into(),
            ActiveView::Config => "Configuration".into(),
            ActiveView::Page(id) | ActiveView::Device { node_id: id, .. } => {
                find_node_label(&self.nav_tree.read(), id).unwrap_or_else(|| "Untitled".into())
            }
        }
    }

    pub fn alloc_node_id(&mut self) -> String {
        let id = *self.next_node_id.read();
        self.next_node_id.set(id + 1);
        format!("node-{id}")
    }

    /// Check if the current user has a specific permission based on role permissions config.
    pub fn has_permission(&self, perm: Permission) -> bool {
        let user = self.current_user.read();
        let perms = self.role_permissions.read();
        match user.as_ref() {
            Some(u) => bms_store_storage::auth::has_permission(u, perm, &perms),
            None => false,
        }
    }

    /// Persist nav tree to disk.
    pub fn save_layout(&self) {
        let state = SavedLayoutState {
            nav_tree: self.nav_tree.read().clone(),
            next_node_id: *self.next_node_id.read(),
        };
        save_layout(&self.project_paths, &state);
    }

    /// Log an audit entry for the current user. Fire-and-forget.
    pub fn audit(&self, builder: AuditEntryBuilder) {
        let user = self.current_user.read();
        let (uid, uname) = match user.as_ref() {
            Some(u) => (u.id.clone(), u.username.clone()),
            None => ("system".into(), "system".into()),
        };
        let store = self.audit_store.clone();
        spawn(async move {
            let _ = store.log_action(&uid, &uname, builder).await;
        });
    }
}

/// Insert a child node under the given parent ID in the nav tree.
pub fn insert_nav_child(nodes: &mut [NavNode], parent_id: &str, child: NavNode) -> bool {
    for node in nodes.iter_mut() {
        if node.id == parent_id {
            node.children.push(child);
            return true;
        }
        if insert_nav_child(&mut node.children, parent_id, child.clone()) {
            return true;
        }
    }
    false
}

/// Remove a node by ID from the nav tree.
pub fn remove_nav_node(nodes: &mut Vec<NavNode>, target_id: &str) -> bool {
    if let Some(pos) = nodes.iter().position(|n| n.id == target_id) {
        nodes.remove(pos);
        return true;
    }
    for node in nodes.iter_mut() {
        if remove_nav_node(&mut node.children, target_id) {
            return true;
        }
    }
    false
}

/// Update a node's label and kind by ID.
pub fn update_nav_node(
    nodes: &mut [NavNode],
    target_id: &str,
    label: String,
    kind: NavNodeKind,
) -> bool {
    for node in nodes.iter_mut() {
        if node.id == target_id {
            node.label = label;
            node.kind = kind;
            return true;
        }
        if update_nav_node(&mut node.children, target_id, label.clone(), kind.clone()) {
            return true;
        }
    }
    false
}

/// Check if a nav node ID corresponds to a Site node.
pub fn is_site_page(nodes: &[NavNode], node_id: &str) -> bool {
    for node in nodes {
        if node.id == node_id {
            return matches!(node.kind, NavNodeKind::Site { .. });
        }
        if is_site_page(&node.children, node_id) {
            return true;
        }
    }
    false
}

/// Find Building children of a given nav node.
pub fn find_building_children(nodes: &[NavNode], parent_id: &str) -> Vec<(String, String)> {
    for node in nodes {
        if node.id == parent_id {
            return node
                .children
                .iter()
                .filter_map(|c| match &c.kind {
                    NavNodeKind::Building { .. } => Some((c.id.clone(), c.label.clone())),
                    _ => None,
                })
                .collect();
        }
        let result = find_building_children(&node.children, parent_id);
        if !result.is_empty() {
            return result;
        }
    }
    Vec::new()
}

fn find_node_label(nodes: &[NavNode], node_id: &str) -> Option<String> {
    for node in nodes {
        if node.id == node_id {
            return Some(node.label.clone());
        }
        if let Some(label) = find_node_label(&node.children, node_id) {
            return Some(label);
        }
    }
    None
}

// ----------------------------------------------------------------
// Persistence: nav tree + page data
// ----------------------------------------------------------------

/// Combined state that gets saved to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedLayoutState {
    pub nav_tree: Vec<NavNode>,
    pub next_node_id: u32,
}

const LAYOUT_FILE: &str = "layout.json";

/// Save nav tree + pages to the project data directory.
pub fn save_layout(paths: &ProjectPaths, state: &SavedLayoutState) {
    let path = paths.data_dir.join(LAYOUT_FILE);
    if let Ok(json) = serde_json::to_string_pretty(state) {
        let _ = std::fs::write(path, json);
    }
}

/// Load nav tree + pages from the project data directory.
///
/// Tries the current schema first. On failure, walks the JSON tree and
/// strips legacy `NavNodeKind` variants (`Folder`, `Page`, `Device`) that
/// were removed when the Nav tab was scoped to physical hierarchy only.
/// This keeps existing projects loadable instead of silently returning an
/// empty tree on a schema rev.
pub fn load_layout(paths: &ProjectPaths) -> Option<SavedLayoutState> {
    let path = paths.data_dir.join(LAYOUT_FILE);
    let data = std::fs::read_to_string(path).ok()?;
    if let Ok(state) = serde_json::from_str::<SavedLayoutState>(&data) {
        return Some(state);
    }
    // Fallback: surgically drop legacy nodes from the raw JSON.
    let mut value: serde_json::Value = serde_json::from_str(&data).ok()?;
    if let Some(arr) = value
        .get_mut("nav_tree")
        .and_then(|t| t.as_array_mut())
    {
        prune_legacy_nodes(arr);
    }
    serde_json::from_value(value).ok()
}

/// Recursively drop nav-tree entries whose `kind` is a removed variant.
fn prune_legacy_nodes(arr: &mut Vec<serde_json::Value>) {
    arr.retain(|node| {
        let kind = node.get("kind");
        let removed = match kind {
            Some(serde_json::Value::String(s)) => {
                matches!(s.as_str(), "Folder" | "Page")
            }
            Some(serde_json::Value::Object(o)) => {
                // Tagged variants serialize as an object with a single key.
                o.contains_key("Device")
            }
            _ => false,
        };
        !removed
    });
    for node in arr.iter_mut() {
        if let Some(children) = node.get_mut("children").and_then(|c| c.as_array_mut()) {
            prune_legacy_nodes(children);
        }
    }
}
