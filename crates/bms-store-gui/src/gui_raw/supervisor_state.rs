//! `SupervisorState` — top-level state for the multi-site supervisor.
//!
//! In single-site mode this holds exactly one `SiteContext` and `mode == SingleSite`,
//! so existing UI surfaces (toolbar site picker, cross-site filter pills) hide
//! themselves and the user sees no change.
//!
//! In multi-site mode it holds N sites loaded by the `SupervisorGate`, plus the
//! authenticated supervisor user (if any), and a `view_site_filter` that
//! cross-cutting views (Alarms, Energy) consult to decide whether to render
//! their per-site or aggregated variant.

use dioxus::prelude::*;
use tokio_util::sync::CancellationToken;

use super::site_context::SiteContext;
use super::state::{ActiveView, SidebarTab};

/// Mode marker so cross-cutting views can branch without inspecting `sites.len()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SupervisorMode {
    /// One project loaded. Toolbar site picker and aggregation toggles are hidden.
    SingleSite,
    /// Multiple projects loaded by the supervisor flow. Aggregation views available.
    MultiSite,
}

/// Site selection scope for cross-cutting views.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SiteFilter {
    /// Aggregate across every loaded site.
    All,
    /// Show only the named site.
    Single(String),
    /// Show a chosen subset of loaded sites.
    Subset(Vec<String>),
}

impl Default for SiteFilter {
    fn default() -> Self {
        SiteFilter::All
    }
}

/// Top-level supervisor state. Provided as a Dioxus context alongside `AppState`.
#[derive(Clone)]
pub struct SupervisorState {
    /// Single-site or multi-site.
    pub mode: SupervisorMode,

    /// All loaded sites in load order. The first entry is the "primary" site
    /// in single-site mode.
    pub sites: Signal<Vec<SiteContext>>,

    /// Currently focused site for single-site views (Config, Schedules, Floor Plans,
    /// Point Detail). `None` only briefly during loading.
    pub active_site_id: Signal<Option<String>>,

    /// Cross-site scope for aggregation-aware views (Alarms, Energy).
    pub view_site_filter: Signal<SiteFilter>,

    /// Active main view (moved out of AppState — view selection is supervisor-global).
    pub active_view: Signal<ActiveView>,

    /// Active sidebar tab (also supervisor-global so it survives site switches).
    pub sidebar_tab: Signal<SidebarTab>,

    /// Top-level shutdown token. Per-site tokens are children of this token, so
    /// cancelling it stops every site's bridges, history collector, FDD engine, etc.
    pub shutdown: CancellationToken,
}

impl SupervisorState {
    /// Construct a single-site supervisor wrapping one already-built `SiteContext`.
    /// Used by `ProjectGate` to make the legacy single-project flow internally
    /// look like a 1-site supervisor.
    pub fn single_site(
        site: SiteContext,
        active_view: Signal<ActiveView>,
        sidebar_tab: Signal<SidebarTab>,
        shutdown: CancellationToken,
    ) -> Self {
        let site_id = site.site_id.clone();
        Self {
            mode: SupervisorMode::SingleSite,
            sites: Signal::new(vec![site]),
            active_site_id: Signal::new(Some(site_id)),
            view_site_filter: Signal::new(SiteFilter::default()),
            active_view,
            sidebar_tab,
            shutdown,
        }
    }

    /// True if this supervisor manages more than one loaded site.
    pub fn is_multi_site(&self) -> bool {
        matches!(self.mode, SupervisorMode::MultiSite)
    }

    /// Get the currently active site context, if any.
    pub fn active_site(&self) -> Option<SiteContext> {
        let active_id = self.active_site_id.read().clone()?;
        self.sites
            .read()
            .iter()
            .find(|s| s.site_id == active_id)
            .cloned()
    }

    /// Look up a site by id without requiring it to be active.
    pub fn site_by_id(&self, id: &str) -> Option<SiteContext> {
        self.sites.read().iter().find(|s| s.site_id == id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn site_filter_default_is_all() {
        let f = SiteFilter::default();
        assert_eq!(f, SiteFilter::All);
    }

    #[test]
    fn site_filter_variants_are_distinct() {
        let a = SiteFilter::Single("a".into());
        let b = SiteFilter::Single("b".into());
        let all = SiteFilter::All;
        let subset = SiteFilter::Subset(vec!["a".into(), "b".into()]);
        assert_ne!(a, b);
        assert_ne!(a, all);
        assert_ne!(a, subset);
    }

    #[test]
    fn supervisor_mode_helpers() {
        assert!(matches!(
            SupervisorMode::SingleSite,
            SupervisorMode::SingleSite
        ));
        assert_ne!(SupervisorMode::SingleSite, SupervisorMode::MultiSite);
    }
}
