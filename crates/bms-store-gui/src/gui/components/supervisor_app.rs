//! `SupervisorApp` — multi-site shell.
//!
//! Holds N pre-initialized sites (from `SupervisorGate`) and renders the
//! existing `ProjectApp` for the currently active site, passing pre-built
//! handles so per-site SQLite threads aren't double-started.
//!
//! The active site is tracked via `active_site_id: Signal<Option<String>>`.
//! A toolbar site picker (Step 2b) lets the user switch. Re-keying on
//! `active_site_id` re-mounts `ProjectApp` so all per-site `use_hook`s re-run
//! cleanly for the new site.

use dioxus::prelude::*;

use bms_store_storage::auth::AllRolePermissions;
use crate::gui::state::CloseAction;
use crate::platform::SharedPlatform;
use bms_store_storage::store::supervisor_user_store::SiteGrant;

use super::cross_site_alarm_view::CrossSiteAlarmView;
use super::cross_site_energy_view::CrossSiteEnergyView;
use super::remote_site_view::RemoteSiteView;
use super::site_status_dashboard::SiteStatusDashboard;
use super::supervisor_gate::{
    synthesize_site_user, LoadedSite, LoadedSiteVariant, ProjectAppOverrides, RemoteLoadedSite,
    SupervisorHandle,
};

/// Top-level supervisor view mode.
#[derive(Clone, Copy, PartialEq)]
enum SupervisorTopView {
    /// Show the cross-site status dashboard (cards per site).
    Dashboard,
    /// Cross-site alarm aggregation.
    Alarms,
    /// Cross-site energy aggregation.
    Energy,
    /// Drill into the active site's ProjectApp.
    Site,
}

#[component]
pub fn SupervisorApp(handle: SupervisorHandle, on_close: EventHandler<CloseAction>) -> Element {
    let sites = use_hook(|| handle.sites.clone());
    let supervisor_shutdown = use_hook(|| handle.shutdown.clone());
    // Authenticated supervisor user + grants, used to synthesize per-site
    // current_user. Cloned into the child SiteShell on mount.
    let sup_user = use_hook(|| handle.supervisor_user.clone());
    let grants = use_hook(|| handle.grants.clone());
    let mut active_idx = use_signal(|| 0usize);
    let mut top_view = use_signal(|| SupervisorTopView::Dashboard);

    // Top-level shutdown. Cancelled when the component unmounts so every site's
    // background tasks (child tokens) wind down together.
    {
        let token = supervisor_shutdown.clone();
        use_drop(move || {
            token.cancel();
        });
    }

    let idx = *active_idx.read();
    let idx = idx.min(sites.len().saturating_sub(1));
    let active = sites.get(idx).cloned();

    let Some(active) = active else {
        return rsx! {
            div { class: "login-backdrop",
                div { class: "login-card",
                    h3 { "No sites loaded" }
                    button {
                        class: "btn btn-primary",
                        onclick: move |_| on_close.call(CloseAction::ToSupervisor),
                        "Back to Launcher"
                    }
                }
            }
        };
    };

    let current_top = *top_view.read();
    let sites_for_picker = sites.clone();
    let sites_for_picker_lookup = sites.clone();
    let sites_for_dashboard = sites.clone();
    let shutdown_for_handle = supervisor_shutdown.clone();
    let sup_user_for_handle = sup_user.clone();
    let grants_for_handle = grants.clone();

    rsx! {
        div { class: "supervisor-shell",
            div { class: "supervisor-banner",
                strong { "Supervisor" }
                span { class: "text-muted", " — {sites.len()} sites loaded" }
                // Top-view toggle: Dashboard ↔ Active site
                div { class: "supervisor-view-toggle",
                    button {
                        class: if current_top == SupervisorTopView::Dashboard { "btn btn-sm active" } else { "btn btn-sm" },
                        onclick: move |_| top_view.set(SupervisorTopView::Dashboard),
                        "Dashboard"
                    }
                    button {
                        class: if current_top == SupervisorTopView::Alarms { "btn btn-sm active" } else { "btn btn-sm" },
                        onclick: move |_| top_view.set(SupervisorTopView::Alarms),
                        "Alarms"
                    }
                    button {
                        class: if current_top == SupervisorTopView::Energy { "btn btn-sm active" } else { "btn btn-sm" },
                        onclick: move |_| top_view.set(SupervisorTopView::Energy),
                        "Energy"
                    }
                    button {
                        class: if current_top == SupervisorTopView::Site { "btn btn-sm active" } else { "btn btn-sm" },
                        onclick: move |_| top_view.set(SupervisorTopView::Site),
                        "Site view"
                    }
                }
                // Site picker (only meaningful in Site view). Remote sites
                // get a 🌐 glyph to distinguish them from local sites.
                if current_top == SupervisorTopView::Site {
                    span { class: "supervisor-site-picker",
                        select {
                            onchange: move |e| {
                                if let Ok(new_idx) = e.value().parse::<usize>() {
                                    active_idx.set(new_idx);
                                }
                            },
                            value: "{idx}",
                            for (i, s) in sites_for_picker.iter().enumerate() {
                                option {
                                    value: "{i}",
                                    selected: i == idx,
                                    {
                                        if s.is_remote() {
                                            format!("🌐 {}", s.name())
                                        } else {
                                            s.name().to_string()
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                button {
                    class: "btn btn-sm",
                    onclick: move |_| on_close.call(CloseAction::ToSupervisor),
                    "Close Supervisor"
                }
            }

            {
                let make_handle = || SupervisorHandle {
                    sites: sites_for_dashboard.clone(),
                    shutdown: shutdown_for_handle.clone(),
                    supervisor_user: sup_user_for_handle.clone(),
                    grants: grants_for_handle.clone(),
                };
                match current_top {
                    SupervisorTopView::Dashboard => rsx! {
                        SiteStatusDashboard {
                            handle: make_handle(),
                            on_open_site: move |site_id: String| {
                                // Find the site by display key and switch to it.
                                if let Some(new_idx) = sites_for_picker_lookup.iter().position(|s| s.display_key() == site_id) {
                                    active_idx.set(new_idx);
                                }
                                top_view.set(SupervisorTopView::Site);
                            },
                        }
                    },
                    SupervisorTopView::Alarms => rsx! {
                        CrossSiteAlarmView {
                            handle: make_handle(),
                        }
                    },
                    SupervisorTopView::Energy => rsx! {
                        CrossSiteEnergyView {
                            handle: make_handle(),
                        }
                    },
                    SupervisorTopView::Site => match active.clone() {
                        LoadedSiteVariant::Local(local_site) => rsx! {
                            SupervisorSiteShell {
                                key: "{local_site.paths.root.display()}",
                                site: local_site,
                                sup_user: sup_user.clone(),
                                grants: grants.clone(),
                                on_close: move |action: CloseAction| on_close.call(action),
                            }
                        },
                        LoadedSiteVariant::Remote(remote_site) => rsx! {
                            RemoteSiteShell {
                                key: "remote::{remote_site.config_id}",
                                remote: remote_site,
                            }
                        },
                    },
                }
            }
        }
    }
}

/// Per-active-site wrapper: freshly constructs the platform signal + synthesized
/// per-site `current_user` on every mount, so when the supervisor switches the
/// active site the re-keyed remount carries the right data. Solves the stale
/// `platform_signal` and `current_user = None` bugs at once.
#[component]
fn SupervisorSiteShell(
    site: LoadedSite,
    sup_user: Option<bms_store_storage::store::supervisor_user_store::SupervisorUser>,
    grants: Vec<SiteGrant>,
    on_close: EventHandler<CloseAction>,
) -> Element {
    // Fresh platform signal for this site (re-run per mount).
    let platform_signal = use_signal(|| Option::<SharedPlatform>::Some(site.platform.clone()));

    // Synthesize a per-site User from the supervisor identity + grants.
    // Uses the project UUID as the site id for grant lookup. Falls back to
    // a viewer-level user when no supervisor user is available (shouldn't
    // happen in practice because SupervisorGate gates init on login).
    let synthesized = sup_user
        .as_ref()
        .map(|u| synthesize_site_user(u, &grants, &site.meta.id));
    let current_user = use_signal(|| synthesized.clone());
    let role_permissions = use_signal(AllRolePermissions::default);

    rsx! {
        div {
            class: "supervisor-active-site",
            super::super::app::ProjectApp {
                paths: site.paths.clone(),
                on_close: move |action: CloseAction| on_close.call(action),
                user_store: site.user_store.clone(),
                current_user,
                role_permissions,
                platform_data: platform_signal,
                supervisor_overrides: Some(ProjectAppOverrides {
                    audit_store: site.audit_store.clone(),
                    weather_service: site.weather_service.clone(),
                    shutdown: site.shutdown.clone(),
                }),
            }
        }
    }
}

/// Drill-in shell for a remote site. The remote site does not have an
/// in-process platform — instead we render a slim REST-backed view (alarms,
/// energy KPIs, "open in browser" link to the remote site's own web UI for
/// deep edits).
#[component]
fn RemoteSiteShell(remote: RemoteLoadedSite) -> Element {
    rsx! {
        div { class: "supervisor-active-site",
            RemoteSiteView { remote: remote }
        }
    }
}
