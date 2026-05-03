use dioxus::prelude::*;

use crate::gui::state::{ActiveView, AppState, CloseAction};
use crate::gui::theme::BaseMode;

#[component]
pub fn Toolbar(on_close_project: EventHandler<CloseAction>) -> Element {
    let state = use_context::<AppState>();
    let active = state.active_view.read().clone();
    let title = state.view_title();

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

                // Discovery shortcut — jumps to Config → Discovery in one click.
                DiscoveryShortcut {}

                // Divider before Config
                span { class: "toolbar-divider" }

                // Config mode (gear icon)
                NavButton {
                    view: ActiveView::Config,
                    active_view: active.clone(),
                    label: "Config",
                    icon_path: "M19.14 12.94c.04-.3.06-.61.06-.94 0-.32-.02-.64-.07-.94l2.03-1.58a.49.49 0 00.12-.61l-1.92-3.32a.49.49 0 00-.59-.22l-2.39.96c-.5-.38-1.03-.7-1.62-.94l-.36-2.54a.484.484 0 00-.48-.41h-3.84c-.24 0-.43.17-.47.41l-.36 2.54c-.59.24-1.13.57-1.62.94l-2.39-.96a.49.49 0 00-.59.22L2.74 8.87c-.12.21-.08.47.12.61l2.03 1.58c-.05.3-.07.62-.07.94s.02.64.07.94l-2.03 1.58a.49.49 0 00-.12.61l1.92 3.32c.12.22.37.29.59.22l2.39-.96c.5.38 1.03.7 1.62.94l.36 2.54c.05.24.24.41.48.41h3.84c.24 0 .44-.17.47-.41l.36-2.54c.59-.24 1.13-.56 1.62-.94l2.39.96c.22.08.47 0 .59-.22l1.92-3.32c.12-.22.07-.47-.12-.61l-2.01-1.58zM12 15.6A3.6 3.6 0 1112 8.4a3.6 3.6 0 010 7.2z",
                }
            }
            div { class: "toolbar-center",
                span { class: "toolbar-title", "{title}" }
                span { class: "toolbar-project-name", "— {project_name}" }
            }
            div { class: "toolbar-right",
                UserIndicator {}
            }
        }
    }
}

/// Toolbar shortcut that jumps to Config → Discovery in one click.
#[component]
fn DiscoveryShortcut() -> Element {
    let mut state = use_context::<AppState>();
    let active = state.active_view.read().clone();
    let pending = state.pending_config_section.read().clone();
    let is_active = matches!(active, ActiveView::Config)
        && pending.as_deref() == Some("Discovery");
    rsx! {
        button {
            class: if is_active { "toolbar-btn nav-btn active" } else { "toolbar-btn nav-btn" },
            title: "Discovery — scan + accept devices",
            onclick: move |_| {
                state.pending_config_section.set(Some("Discovery".into()));
                state.active_view.set(ActiveView::Config);
                state.selected_point.set(None);
                state.detail_open.set(false);
            },
            svg {
                width: "18",
                height: "18",
                view_box: "0 0 24 24",
                fill: "currentColor",
                path { d: "M15.5 14h-.79l-.28-.27A6.471 6.471 0 0 0 16 9.5 6.5 6.5 0 1 0 9.5 16c1.61 0 3.09-.59 4.23-1.57l.27.28v.79l5 4.99L20.49 19l-4.99-5zm-6 0C7.01 14 5 11.99 5 9.5S7.01 5 9.5 5 14 7.01 14 9.5 11.99 14 9.5 14z" }
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
