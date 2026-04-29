use dioxus::prelude::*;

use crate::gui::state::{AppState, SidebarTab};

use super::building_tree::NavTree;
use super::device_tree::DeviceTree;

#[component]
pub fn Sidebar() -> Element {
    let mut state = use_context::<AppState>();
    let active_tab = *state.sidebar_tab.read();
    let mut search_query = use_signal(String::new);
    let query = search_query.read().clone();

    rsx! {
        div { class: "sidebar",
            div { class: "sidebar-tabs",
                button {
                    class: if active_tab == SidebarTab::Devices { "sidebar-tab active" } else { "sidebar-tab" },
                    onclick: move |_| state.sidebar_tab.set(SidebarTab::Devices),
                    "Devices"
                }
                button {
                    class: if active_tab == SidebarTab::Nav { "sidebar-tab active" } else { "sidebar-tab" },
                    onclick: move |_| state.sidebar_tab.set(SidebarTab::Nav),
                    "Nav"
                }
            }
            div { class: "sidebar-search",
                input {
                    class: "sidebar-search-input",
                    r#type: "text",
                    placeholder: "Search devices, points...",
                    value: "{query}",
                    oninput: move |evt| search_query.set(evt.value()),
                }
                if !query.is_empty() {
                    button {
                        class: "sidebar-search-clear",
                        onclick: move |_| search_query.set(String::new()),
                        "x"
                    }
                }
            }
            div { class: "sidebar-content",
                match active_tab {
                    SidebarTab::Devices => rsx! { DeviceTree { filter: query.clone() } },
                    SidebarTab::Nav => rsx! { NavTree {} },
                }
            }
        }
    }
}
