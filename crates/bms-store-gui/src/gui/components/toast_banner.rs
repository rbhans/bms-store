//! Toast banner stack — renders `Event::Toast` notifications fed in by the
//! subscriber in `app.rs`. Stacks in the top-right corner, auto-dismisses
//! after `AUTO_DISMISS_MS` (errors stay until clicked).

use dioxus::prelude::*;

use crate::gui::state::{AppState, ToastMessage};
use bms_core::ToastLevel;

const AUTO_DISMISS_MS: i64 = 6_000;

#[component]
pub fn ToastBanner() -> Element {
    let mut state = use_context::<AppState>();
    let toasts = state.toasts.read().clone();

    // Periodic sweep that drops auto-dismissable toasts past their TTL.
    use_hook(|| {
        spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(500));
            loop {
                interval.tick().await;
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                let current = state.toasts.read().clone();
                let kept: Vec<ToastMessage> = current
                    .into_iter()
                    .filter(|t| {
                        // Errors stick until manually dismissed; others time out.
                        if matches!(t.level, ToastLevel::Error) {
                            true
                        } else {
                            now - t.created_ms < AUTO_DISMISS_MS
                        }
                    })
                    .collect();
                state.toasts.set(kept);
            }
        });
    });

    if toasts.is_empty() {
        return rsx! {};
    }

    rsx! {
        div { class: "toast-stack",
            for t in toasts {
                ToastRow { key: "{t.id}", toast: t.clone() }
            }
        }
    }
}

#[component]
fn ToastRow(toast: ToastMessage) -> Element {
    let mut state = use_context::<AppState>();
    let level_class = match toast.level {
        ToastLevel::Info => "toast toast-info",
        ToastLevel::Warn => "toast toast-warn",
        ToastLevel::Error => "toast toast-error",
    };
    let level_label = match toast.level {
        ToastLevel::Info => "INFO",
        ToastLevel::Warn => "WARN",
        ToastLevel::Error => "ERROR",
    };
    let id = toast.id;
    let detail = toast.detail.clone();

    rsx! {
        div { class: "{level_class}",
            div { class: "toast-header",
                span { class: "toast-level", "{level_label}" }
                span { class: "toast-source", "{toast.source}" }
                button {
                    class: "toast-close",
                    title: "Dismiss",
                    onclick: move |_| {
                        let kept: Vec<ToastMessage> = state
                            .toasts
                            .read()
                            .iter()
                            .filter(|t| t.id != id)
                            .cloned()
                            .collect();
                        state.toasts.set(kept);
                    },
                    "×"
                }
            }
            div { class: "toast-message", "{toast.message}" }
            if let Some(d) = detail {
                details { class: "toast-detail",
                    summary { "Details" }
                    pre { "{d}" }
                }
            }
        }
    }
}
