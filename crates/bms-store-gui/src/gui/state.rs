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
use bms_store_storage::store::alarm_store::AlarmStore;
use bms_store_storage::store::audit_store::{AuditEntryBuilder, AuditStore};
use bms_store_storage::store::commissioning_store::CommissioningStore;
use bms_store_storage::store::discovery_store::DiscoveryStore;
use bms_store_storage::store::entity_store::EntityStore;
use bms_store_storage::store::history_store::HistoryStore;
use bms_store_storage::store::mqtt_store::MqttStore;
use bms_store_storage::store::node_store::NodeStore;
use bms_store_storage::store::notification_store::NotificationStore;
use bms_store_storage::store::point_store::PointStore;
use bms_store_storage::store::schedule_store::ScheduleStore;
use bms_store_storage::store::user_store::{User, UserStore};
use bms_store_storage::store::webhook_store::WebhookStore;
use bms_store_storage::weather::model::WeatherData;
use bms_store_storage::weather::service::WeatherService;


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
    Alarms,
    Schedules,
    History,
    Config,
    /// A graphic page canvas, keyed by node id.
    Page(String),
    /// A device view (point table), keyed by node id. Carries the device_id to look up.
    Device {
        node_id: String,
        device_id: String,
    },
    /// Weather view — current conditions, hourly/daily forecast.
    Weather,
}

// ----------------------------------------------------------------
// Trend dashboard data model
// ----------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrendRange {
    Hour1,
    Hour4,
    Hour24,
    Day7,
    Day30,
}

impl TrendRange {
    pub fn millis(&self) -> i64 {
        match self {
            TrendRange::Hour1 => 3_600_000,
            TrendRange::Hour4 => 14_400_000,
            TrendRange::Hour24 => 86_400_000,
            TrendRange::Day7 => 604_800_000,
            TrendRange::Day30 => 2_592_000_000,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            TrendRange::Hour1 => "1h",
            TrendRange::Hour4 => "4h",
            TrendRange::Hour24 => "24h",
            TrendRange::Day7 => "7d",
            TrendRange::Day30 => "30d",
        }
    }

    pub fn all() -> &'static [TrendRange] {
        &[
            TrendRange::Hour1,
            TrendRange::Hour4,
            TrendRange::Hour24,
            TrendRange::Day7,
            TrendRange::Day30,
        ]
    }
}

/// A data source for a dashboard widget — one device/point pair.
#[derive(Debug, Clone, PartialEq)]
pub struct WidgetSource {
    pub device_id: String,
    pub point_id: String,
    pub label: String,
    pub color: String,
}

/// What kind of visualization a widget renders.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WidgetKind {
    Chart,
    Gauge,
    Table,
    Value,
}

impl WidgetKind {
    pub fn all() -> &'static [WidgetKind] {
        &[Self::Chart, Self::Gauge, Self::Table, Self::Value]
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Chart => "Chart",
            Self::Gauge => "Gauge",
            Self::Table => "Table",
            Self::Value => "Value",
        }
    }

    pub fn icon_path(&self) -> &'static str {
        match self {
            // Trend line
            Self::Chart => "M3.5 18.5l6-6 4 4L22 6.92l-1.41-1.41-7.09 7.97-4-4L2 16.99z",
            // Speed gauge
            Self::Gauge => "M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm0 18c-4.42 0-8-3.58-8-8s3.58-8 8-8 8 3.58 8 8-3.58 8-8 8zm3.5-9c.83 0 1.5-.67 1.5-1.5S16.33 8 15.5 8 14 8.67 14 9.5s.67 1.5 1.5 1.5zm-7 0c.83 0 1.5-.67 1.5-1.5S9.33 8 8.5 8 7 8.67 7 9.5 7.67 11 8.5 11zm3.5 6.5c2.33 0 4.31-1.46 5.11-3.5H6.89c.8 2.04 2.78 3.5 5.11 3.5z",
            // Table grid
            Self::Table => "M3 3v18h18V3H3zm8 16H5v-6h6v6zm0-8H5V5h6v6zm8 8h-6v-6h6v6zm0-8h-6V5h6v6z",
            // Single number
            Self::Value => "M19 3H5c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zm-7 14H7v-2h5v2zm5-4H7v-2h10v2zm0-4H7V7h10v2z",
        }
    }
}

/// A widget placed on a dashboard canvas (absolute pixel positioning).
#[derive(Debug, Clone, PartialEq)]
pub struct DashboardWidget {
    pub id: String,
    pub kind: WidgetKind,
    /// X position in pixels.
    pub x: f64,
    /// Y position in pixels.
    pub y: f64,
    /// Width in pixels.
    pub w: f64,
    /// Height in pixels.
    pub h: f64,
    /// Data sources (multiple device/point pairs).
    pub sources: Vec<WidgetSource>,
    /// Time range for chart widgets.
    pub range: TrendRange,
}

/// A saved trend dashboard.
#[derive(Debug, Clone, PartialEq)]
pub struct TrendDashboard {
    pub id: String,
    pub name: String,
    pub widgets: Vec<DashboardWidget>,
}

/// Grid snap size in pixels for dashboard widget positioning.
pub const GRID_SNAP: f64 = 20.0;

/// Snap a value to the nearest grid unit.
pub fn snap(val: f64) -> f64 {
    (val / GRID_SNAP).round() * GRID_SNAP
}

/// Active drag operation on a widget (all coordinates in page space).
#[derive(Debug, Clone, PartialEq)]
pub enum DragOp {
    /// Moving the widget — stores page coords at drag start + original widget position.
    Move {
        widget_id: String,
        start_page_x: f64,
        start_page_y: f64,
        orig_x: f64,
        orig_y: f64,
    },
    /// Resizing from bottom-right corner — stores page coords at drag start + original size.
    Resize {
        widget_id: String,
        start_page_x: f64,
        start_page_y: f64,
        orig_w: f64,
        orig_h: f64,
    },
}

/// What tool is active on the dashboard canvas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DashboardTool {
    Select,
    AddWidget(WidgetKind),
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

/// What kind of content a nav node represents.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NavNodeKind {
    /// Container only — just holds children, no content of its own.
    Folder,
    /// Blank graphic page / canvas.
    Page,
    /// Links to a device (shows its point table in main content).
    Device { device_id: String },
    /// Site / campus — creates NodeType::Site in NodeStore. Also acts as a page (canvas).
    Site { node_id: String },
    /// Physical building — creates NodeType::Space + tag "building". Also acts as a page.
    Building { node_id: String },
    /// Floor within a building — creates NodeType::Space + tag "floor". Also acts as a page.
    Floor { node_id: String },
    /// Room on a floor — creates NodeType::Space + tag "room". Also acts as a page.
    Room { node_id: String },
}

impl NavNodeKind {
    /// Returns true for Site/Building/Floor/Room (physical hierarchy nodes).
    pub fn is_spatial(&self) -> bool {
        matches!(
            self,
            NavNodeKind::Site { .. }
                | NavNodeKind::Building { .. }
                | NavNodeKind::Floor { .. }
                | NavNodeKind::Room { .. }
        )
    }

    /// Returns the NodeStore node_id for spatial kinds, None for others.
    pub fn node_store_id(&self) -> Option<&str> {
        match self {
            NavNodeKind::Site { node_id } => Some(node_id),
            NavNodeKind::Building { node_id } => Some(node_id),
            NavNodeKind::Floor { node_id } => Some(node_id),
            NavNodeKind::Room { node_id } => Some(node_id),
            _ => None,
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
    /// History query handle.
    pub history_store: HistoryStore,
    /// Saved dashboards.
    pub dashboards: Signal<Vec<TrendDashboard>>,
    /// ID of the currently active dashboard (None = no dashboard open).
    pub active_dashboard_id: Signal<Option<String>>,
    /// Currently selected widget ID on the dashboard.
    pub selected_widget: Signal<Option<String>>,
    /// Active dashboard tool.
    pub dashboard_tool: Signal<DashboardTool>,
    /// Counter for widget IDs.
    pub next_widget_id: Signal<u32>,
    /// Active drag operation.
    pub drag_op: Signal<Option<DragOp>>,
    /// Quick-trend: device + point shown inline on default history page.
    pub quick_trend_device: Signal<Option<String>>,
    pub quick_trend_point: Signal<Option<String>>,
    pub quick_trend_range: Signal<TrendRange>,
    /// Alarm system handle.
    pub alarm_store: AlarmStore,
    /// Schedule system handle.
    pub schedule_store: ScheduleStore,
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
    /// Notification store for alarm routing recipients, rules, shelving, and log.
    pub notification_store: NotificationStore,
    /// MQTT config store for broker connections and topic patterns.
    pub mqtt_store: MqttStore,
    /// Commissioning store for device verification checklists.
    pub commissioning_store: CommissioningStore,
    /// Report store for scheduled report definitions and executions.
    pub report_store: bms_store_storage::store::report_store::ReportStore,
    /// Energy analytics store for meters, rates, baselines, and rollups.
    pub energy_store: bms_store_storage::store::energy_store::EnergyStore,
    /// Webhook subscription store for endpoint configs and delivery log.
    pub webhook_store: WebhookStore,
    /// FDD store for fault detection rules, bindings, and active faults.
    pub fdd_store: bms_store_storage::store::fdd_store::FddStore,
    /// Export store for database export connector configuration.
    pub export_store: bms_store_storage::store::export_store::ExportStore,
    /// Cloud bridge store for cloud platform integration.
    #[cfg(feature = "cloud")]
    pub cloud_store: bms_store_storage::store::cloud_store::CloudStore,
    /// Platform health registry — shared across all subsystems.
    pub health: bms_store_storage::health::HealthRegistry,
    /// Live WASM plugin runtime — holds all loaded plugin instances.
    /// `None` when the `wasm-plugins` feature is disabled or init failed.
    #[cfg(feature = "wasm-plugins")]
    pub wasm_runtime: Option<std::sync::Arc<opencrate_plugin_wasm::WasmPluginRuntime>>,
    /// Currently logged-in user.
    pub current_user: Signal<Option<User>>,
    /// User store for authentication and user management.
    pub user_store: UserStore,
    /// Per-role permission configuration.
    pub role_permissions: Signal<AllRolePermissions>,
    /// Audit trail store for logging user actions.
    pub audit_store: AuditStore,
    /// Weather service for outdoor conditions and forecast.
    pub weather_service: Arc<WeatherService>,
    /// Cached weather data (updated via watch channel).
    pub weather_data: Signal<Option<WeatherData>>,
    /// Theme configuration (colors, mode, custom logo).
    pub theme_config: Signal<ThemeConfig>,
    /// Requested config sub-tab (consumed by ConfigView on render).
    pub pending_config_section: Signal<Option<String>>,
    /// Whether sidebar is visible (for mobile responsive toggle).
    pub sidebar_visible: Signal<bool>,
    /// BAS Atlas taxonomy matcher — shared with DiscoveryService.
    /// Writing to this lock immediately affects the live DiscoveryService.
    #[cfg(feature = "atlas")]
    pub atlas_lock: Arc<std::sync::RwLock<Option<Arc<bms_store_storage::atlas::matcher::AtlasMatcher>>>>,
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
            ActiveView::Alarms => "Alarms".into(),
            ActiveView::Schedules => "Schedules".into(),
            ActiveView::Config => "Configuration".into(),
            ActiveView::History => {
                if let Some(ref dash_id) = *self.active_dashboard_id.read() {
                    self.dashboards
                        .read()
                        .iter()
                        .find(|d| d.id == *dash_id)
                        .map(|d| d.name.clone())
                        .unwrap_or_else(|| "History".into())
                } else {
                    "History".into()
                }
            }
            ActiveView::Weather => "Weather".into(),
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
pub fn load_layout(paths: &ProjectPaths) -> Option<SavedLayoutState> {
    let path = paths.data_dir.join(LAYOUT_FILE);
    let data = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}
