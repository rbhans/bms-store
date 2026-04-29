use dioxus::prelude::*;

use crate::gui::state::{ActiveView, AppState, CloseAction, TrendDashboard};
use crate::gui::theme::BaseMode;

#[component]
pub fn Toolbar(on_close_project: EventHandler<CloseAction>) -> Element {
    let state = use_context::<AppState>();
    let active = state.active_view.read().clone();
    let title = state.view_title();
    let is_history = matches!(active, ActiveView::History);

    let has_dash = is_history && state.active_dashboard_id.read().is_some();
    let project_name = state.project_meta.name.clone();

    rsx! {
        div { class: "toolbar",
            div { class: "toolbar-left",
                // Sidebar toggle (hamburger) — visible on mobile
                button {
                    class: "toolbar-btn sidebar-toggle",
                    title: "Toggle sidebar",
                    onclick: move |_| {
                        let mut s = state.sidebar_visible;
                        s.toggle();
                    },
                    svg { view_box: "0 0 24 24", width: "18", height: "18",
                        path {
                            d: "M3 18h18v-2H3v2zm0-5h18v-2H3v2zm0-7v2h18V6H3z",
                            fill: "currentColor",
                        }
                    }
                }

                // OpenCrate logo / file menu
                FileMenu {
                    on_close_project: move |action: CloseAction| on_close_project.call(action),
                }

                // Divider
                span { class: "toolbar-divider" }

                // Home
                NavButton {
                    view: ActiveView::Home,
                    active_view: active.clone(),
                    label: "Home",
                    icon_path: "M10 20v-6h4v6h5v-8h3L12 3 2 12h3v8z",
                }

                // Alarms (with badge)
                AlarmNavButton { active_view: active.clone() }

                // Schedules
                NavButton {
                    view: ActiveView::Schedules,
                    active_view: active.clone(),
                    label: "Schedules",
                    icon_path: "M19 3h-1V1h-2v2H8V1H6v2H5c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zm0 16H5V8h14v11zM9 10H7v2h2v-2zm4 0h-2v2h2v-2zm4 0h-2v2h2v-2z",
                }

                // History/Trend
                NavButton {
                    view: ActiveView::History,
                    active_view: active.clone(),
                    label: "History",
                    icon_path: "M3.5 18.5l6-6 4 4L22 6.92l-1.41-1.41-7.09 7.97-4-4L2 16.99z",
                }

                // Weather
                NavButton {
                    view: ActiveView::Weather,
                    active_view: active.clone(),
                    label: "Weather",
                    icon_path: "M19.35 10.04C18.67 6.59 15.64 4 12 4 9.11 4 6.6 5.64 5.35 8.04 2.34 8.36 0 10.91 0 14c0 3.31 2.69 6 6 6h13c2.76 0 5-2.24 5-5 0-2.64-2.05-4.78-4.65-4.96z",
                }

                // Divider before Config
                span { class: "toolbar-divider" }

                // Config mode (gear icon)
                NavButton {
                    view: ActiveView::Config,
                    active_view: active.clone(),
                    label: "Config",
                    icon_path: "M19.14 12.94c.04-.3.06-.61.06-.94 0-.32-.02-.64-.07-.94l2.03-1.58a.49.49 0 00.12-.61l-1.92-3.32a.49.49 0 00-.59-.22l-2.39.96c-.5-.38-1.03-.7-1.62-.94l-.36-2.54a.484.484 0 00-.48-.41h-3.84c-.24 0-.43.17-.47.41l-.36 2.54c-.59.24-1.13.57-1.62.94l-2.39-.96a.49.49 0 00-.59.22L2.74 8.87c-.12.21-.08.47.12.61l2.03 1.58c-.05.3-.07.62-.07.94s.02.64.07.94l-2.03 1.58a.49.49 0 00-.12.61l1.92 3.32c.12.22.37.29.59.22l2.39-.96c.5.38 1.03.7 1.62.94l.36 2.54c.05.24.24.41.48.41h3.84c.24 0 .44-.17.47-.41l.36-2.54c.59-.24 1.13-.56 1.62-.94l2.39.96c.22.08.47 0 .59-.22l1.92-3.32c.12-.22.07-.47-.12-.61l-2.01-1.58zM12 15.6A3.6 3.6 0 1112 8.4a3.6 3.6 0 010 7.2z",
                }

                // Dashboard tabs (shown when History is active)
                if is_history {
                    span { class: "toolbar-divider" }
                    DashboardTabs {}
                }
            }
            if has_dash {
                // Spacer keeps toolbar-right pinned to the right edge
                div { class: "toolbar-center" }
            } else {
                div { class: "toolbar-center",
                    span { class: "toolbar-title", "{title}" }
                    span { class: "toolbar-project-name", "— {project_name}" }
                }
            }
            div { class: "toolbar-right",
                UserIndicator {}
            }
        }
    }
}

/// File menu dropdown triggered by the OpenCrate logo button.
#[component]
fn FileMenu(on_close_project: EventHandler<CloseAction>) -> Element {
    let mut menu_open = use_signal(|| false);
    let is_open = *menu_open.read();

    let logo_state = use_context::<AppState>();
    let custom_logo = logo_state.theme_config.read().custom_logo.clone();

    rsx! {
        div { class: "file-menu-anchor",
            button {
                class: if is_open { "toolbar-btn logo-btn active" } else { "toolbar-btn logo-btn" },
                title: "File",
                onclick: move |_| menu_open.toggle(),
                if let Some(ref logo_path) = custom_logo {
                    img {
                        src: "{logo_path}",
                        width: "20",
                        height: "20",
                    }
                } else {
                    img {
                        src: asset!("/assets/opencrate_icon.svg"),
                        width: "20",
                        height: "20",
                    }
                }
            }

            if is_open {
                // Invisible backdrop to close menu on outside click
                div {
                    class: "file-menu-backdrop",
                    onclick: move |_| menu_open.set(false),
                }

                div { class: "file-menu-dropdown",
                    // New Project
                    button {
                        class: "file-menu-item",
                        onclick: move |_| {
                            menu_open.set(false);
                            on_close_project.call(CloseAction::ToNewProject);
                        },
                        svg {
                            width: "14",
                            height: "14",
                            view_box: "0 0 24 24",
                            fill: "currentColor",
                            path { d: "M14 2H6c-1.1 0-2 .9-2 2v16c0 1.1.9 2 2 2h12c1.1 0 2-.9 2-2V8l-6-6zm2 14h-3v3h-2v-3H8v-2h3v-3h2v3h3v2zm-3-7V3.5L18.5 9H13z" }
                        }
                        span { "New Project" }
                    }

                    // Open Project
                    button {
                        class: "file-menu-item",
                        onclick: move |_| {
                            menu_open.set(false);
                            on_close_project.call(CloseAction::ToRecent);
                        },
                        svg {
                            width: "14",
                            height: "14",
                            view_box: "0 0 24 24",
                            fill: "currentColor",
                            path { d: "M20 6h-8l-2-2H4c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V8c0-1.1-.9-2-2-2zm0 12H4V8h16v10z" }
                        }
                        span { "Open Project" }
                    }

                    div { class: "file-menu-separator" }

                    // Close Project
                    button {
                        class: "file-menu-item",
                        onclick: move |_| {
                            menu_open.set(false);
                            on_close_project.call(CloseAction::ToRecent);
                        },
                        svg {
                            width: "14",
                            height: "14",
                            view_box: "0 0 24 24",
                            fill: "currentColor",
                            path { d: "M10.09 15.59L11.5 17l5-5-5-5-1.41 1.41L12.67 11H3v2h9.67l-2.58 2.59zM19 3H5c-1.11 0-2 .9-2 2v4h2V5h14v14H5v-4H3v4c0 1.1.89 2 2 2h14c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2z" }
                        }
                        span { "Close Project" }
                    }

                    div { class: "file-menu-separator" }

                    // Dark/Light toggle
                    {
                        let mut state3 = use_context::<AppState>();
                        let current_mode = state3.theme_config.read().base_mode;
                        let toggle_label = match current_mode {
                            BaseMode::Dark => "Switch to Light",
                            BaseMode::Light => "Switch to Dark",
                            BaseMode::System => "Switch to Dark",
                        };
                        let toggle_icon = match current_mode {
                            BaseMode::Dark => BaseMode::Light.icon_path(),
                            _ => BaseMode::Dark.icon_path(),
                        };
                        rsx! {
                            button {
                                class: "file-menu-item",
                                onclick: move |_| {
                                    menu_open.set(false);
                                    let mut cfg = state3.theme_config.read().clone();
                                    cfg.base_mode = match cfg.base_mode {
                                        BaseMode::Dark => BaseMode::Light,
                                        BaseMode::Light => BaseMode::Dark,
                                        BaseMode::System => BaseMode::Dark,
                                    };
                                    state3.theme_config.set(cfg);
                                },
                                svg {
                                    width: "14",
                                    height: "14",
                                    view_box: "0 0 24 24",
                                    fill: "currentColor",
                                    path { d: "{toggle_icon}" }
                                }
                                span { "{toggle_label}" }
                            }
                        }
                    }

                    div { class: "file-menu-separator" }

                    // Exit
                    button {
                        class: "file-menu-item",
                        onclick: move |_| {
                            let window = dioxus::desktop::window();
                            window.close();
                        },
                        svg {
                            width: "14",
                            height: "14",
                            view_box: "0 0 24 24",
                            fill: "currentColor",
                            path { d: "M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z" }
                        }
                        span { "Exit" }
                    }
                }
            }

        }
    }
}

#[component]
fn NavButton(
    view: ActiveView,
    active_view: ActiveView,
    label: &'static str,
    icon_path: &'static str,
) -> Element {
    let mut state = use_context::<AppState>();
    let is_active = active_view == view;

    rsx! {
        button {
            class: if is_active { "toolbar-btn nav-btn active" } else { "toolbar-btn nav-btn" },
            title: "{label}",
            onclick: move |_| {
                state.active_view.set(view.clone());
                state.selected_point.set(None);
                state.detail_open.set(false);
            },
            svg {
                width: "18",
                height: "18",
                view_box: "0 0 24 24",
                fill: "currentColor",
                path { d: "{icon_path}" }
            }
        }
    }
}

/// Dashboard tabs shown in the toolbar when History view is active.
#[component]
fn DashboardTabs() -> Element {
    let mut state = use_context::<AppState>();
    let dashboards = state.dashboards.read().clone();
    let active_id = state.active_dashboard_id.read().clone();

    rsx! {
        div { class: "dash-tabs-scroll",
            for dash in &dashboards {
                {
                    let is_active = active_id.as_deref() == Some(&dash.id);
                    let dash_id = dash.id.clone();
                    rsx! {
                        button {
                            class: if is_active { "toolbar-btn dash-tab active" } else { "toolbar-btn dash-tab" },
                            onclick: move |_| {
                                state.active_dashboard_id.set(Some(dash_id.clone()));
                                state.selected_widget.set(None);
                            },
                            "{dash.name}"
                        }
                    }
                }
            }

            // "+" button to create new dashboard
            button {
                class: "toolbar-btn dash-add-btn",
                title: "New Dashboard",
                onclick: move |_| {
                    let id = format!("dash-{}", state.dashboards.read().len() + 1);
                    let name = format!("Dashboard {}", state.dashboards.read().len() + 1);
                    let new_dash = TrendDashboard {
                        id: id.clone(),
                        name,
                        widgets: Vec::new(),
                    };
                    let mut dashes = state.dashboards.read().clone();
                    dashes.push(new_dash);
                    state.dashboards.set(dashes);
                    state.active_dashboard_id.set(Some(id));
                    state.selected_widget.set(None);
                },
                "+"
            }
        }
    }
}

/// Alarm nav button with unacknowledged count badge.
#[component]
fn AlarmNavButton(active_view: ActiveView) -> Element {
    let mut state = use_context::<AppState>();
    let is_active = matches!(active_view, ActiveView::Alarms);

    // Poll active alarm count + failed notifications periodically
    let mut alarm_count = use_signal(|| 0u32);
    let mut notif_fail_count = use_signal(|| 0i64);
    let alarm_store = state.alarm_store.clone();
    let notif_store = state.notification_store.clone();
    use_future(move || {
        let store = alarm_store.clone();
        let ns = notif_store.clone();
        async move {
            loop {
                let alarms = store.get_active_alarms().await;
                let unacked = alarms
                    .iter()
                    .filter(|a| a.state == bms_store_storage::store::alarm_store::AlarmState::Offnormal)
                    .count() as u32;
                alarm_count.set(unacked);
                // Count failed notifications in last 24h
                let day_ago = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as i64
                    - 86_400_000;
                notif_fail_count.set(ns.count_failed_recent(day_ago).await);
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            }
        }
    });

    let count = *alarm_count.read();
    let has_alarms = count > 0;
    let fail_count = *notif_fail_count.read();

    rsx! {
        button {
            class: if is_active { "toolbar-btn nav-btn active" } else { "toolbar-btn nav-btn" },
            title: "Alarms",
            onclick: move |_| {
                state.active_view.set(ActiveView::Alarms);
                state.selected_point.set(None);
                state.detail_open.set(false);
            },
            div { class: "alarm-btn-wrap",
                svg {
                    width: "18",
                    height: "18",
                    view_box: "0 0 24 24",
                    fill: "currentColor",
                    path { d: "M12 22c1.1 0 2-.9 2-2h-4c0 1.1.9 2 2 2zm6-6v-5c0-3.07-1.63-5.64-4.5-6.32V4c0-.83-.67-1.5-1.5-1.5s-1.5.67-1.5 1.5v.68C7.64 5.36 6 7.92 6 11v5l-2 2v1h16v-1l-2-2z" }
                }
                if has_alarms {
                    span { class: "alarm-badge", "{count}" }
                }
                if fail_count > 0 {
                    span { class: "alarm-badge notif-fail-badge", title: "Failed notifications (24h)", "{fail_count}" }
                }
            }
        }
    }
}

/// User indicator with logout dropdown in toolbar-right.
#[component]
fn UserIndicator() -> Element {
    let mut state = use_context::<AppState>();
    let mut dropdown_open = use_signal(|| false);

    let user = state.current_user.read().clone();
    let Some(user) = user else {
        return rsx! {};
    };

    let role_class = format!("role-{}", user.role.to_string().to_lowercase());

    rsx! {
        div { class: "user-indicator",
            button {
                class: "user-indicator-btn",
                onclick: move |_| dropdown_open.toggle(),
                span { class: "user-indicator-name", "{user.display_name}" }
                span { class: "user-role-badge {role_class}", "{user.role.label()}" }
            }

            if *dropdown_open.read() {
                div {
                    class: "file-menu-backdrop",
                    onclick: move |_| dropdown_open.set(false),
                }
                div { class: "user-dropdown",
                    button {
                        class: "file-menu-item",
                        onclick: move |_| {
                            dropdown_open.set(false);
                            state.audit(
                                bms_store_storage::store::audit_store::AuditEntryBuilder::new(
                                    bms_store_storage::store::audit_store::AuditAction::Logout, "session",
                                ),
                            );
                            state.current_user.set(None);
                        },
                        svg {
                            width: "14",
                            height: "14",
                            view_box: "0 0 24 24",
                            fill: "currentColor",
                            path { d: "M10.09 15.59L11.5 17l5-5-5-5-1.41 1.41L12.67 11H3v2h9.67l-2.58 2.59zM19 3H5c-1.11 0-2 .9-2 2v4h2V5h14v14H5v-4H3v4c0 1.1.89 2 2 2h14c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2z" }
                        }
                        span { "Log Out" }
                    }
                }
            }
        }
    }
}
