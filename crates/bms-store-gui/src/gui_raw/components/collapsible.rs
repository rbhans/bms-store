use dioxus::prelude::*;

#[component]
pub fn CollapsibleSection(
    title: String,
    badge: Option<String>,
    initially_open: Option<bool>,
    children: Element,
) -> Element {
    let mut expanded = use_signal(|| initially_open.unwrap_or(false));
    let chevron = if *expanded.read() { "▾" } else { "▸" };

    rsx! {
        div { class: "collapsible-section",
            div {
                class: "collapsible-header",
                onclick: move |_| {
                    let v = *expanded.read();
                    expanded.set(!v);
                },
                span { class: "collapsible-chevron", "{chevron}" }
                span { class: "collapsible-title", "{title}" }
                if let Some(ref b) = badge {
                    span { class: "collapsible-badge", "{b}" }
                }
            }
            if *expanded.read() {
                div { class: "collapsible-body",
                    {children}
                }
            }
        }
    }
}
