//! `SiteContext` — per-site bundle of platform handles, GUI state, and a
//! per-site shutdown token.
//!
//! In single-site mode this wraps the one open project. In multi-site supervisor
//! mode the `SupervisorState` holds one of these per loaded project.

use std::sync::Arc;

use dioxus::prelude::*;
use tokio_util::sync::CancellationToken;

use crate::auth::AllRolePermissions;
use crate::platform::{BridgeStartReport, SharedPlatform};
use crate::project::{ProjectMeta, ProjectPaths};
use crate::store::audit_store::AuditStore;
use crate::store::user_store::{User, UserStore};
use crate::weather::model::WeatherData;
use crate::weather::service::WeatherService;

use super::theme::ThemeConfig;

/// Everything that's currently per-project: identity, the SharedPlatform, per-site
/// signals (for reactivity), and a per-site `CancellationToken`.
///
/// Cheap to clone — every field is already `Clone` (channels, Arcs, Signals).
#[derive(Clone)]
pub struct SiteContext {
    // -- Identity --
    /// Stable site identifier (the project UUID from `project.json`).
    pub site_id: String,
    pub project_meta: ProjectMeta,
    pub project_paths: ProjectPaths,

    // -- Platform stack (data layer + automation engines + bridges) --
    pub platform: SharedPlatform,
    /// Status of protocol bridge starts captured during `init_platform`.
    pub bridge_report: BridgeStartReport,

    // -- Per-site GUI handles that are not part of `SharedPlatform` --
    pub audit_store: AuditStore,
    pub user_store: UserStore,
    pub current_user: Signal<Option<User>>,
    pub role_permissions: Signal<AllRolePermissions>,
    pub weather_service: Arc<WeatherService>,
    pub weather_data: Signal<Option<WeatherData>>,
    pub theme_config: Signal<ThemeConfig>,

    // -- Reactive store version watchers (driven by background tasks) --
    pub store_version: Signal<u64>,
    pub node_version: Signal<u64>,

    // -- Per-site shutdown — child of the supervisor token in multi-site mode --
    pub shutdown: CancellationToken,
}

impl SiteContext {
    /// Returns the site display name (project name).
    pub fn name(&self) -> &str {
        &self.project_meta.name
    }
}
