//! Auto-tag preview modal — surfaces the tags `accept_device` would apply
//! along with the source (atlas vs heuristic) and a coarse confidence
//! score. Lets the operator confirm or skip before commit, instead of
//! accepting blind and re-editing tags after the fact.
//!
//! Calls [`bms_store_bridges::discovery::service::DiscoveryService::preview_device_tags`]
//! directly (no REST hop — this is the in-process desktop GUI).

use dioxus::prelude::*;

use bms_store_bridges::discovery::service::{DeviceTagPreview, TagSource};

use crate::gui::state::AppState;

#[component]
pub fn AutoTagPreviewModal(
    device_id: String,
    on_close: EventHandler<()>,
    on_accept: EventHandler<()>,
) -> Element {
    let state = use_context::<AppState>();
    let svc = state.discovery_service.clone();
    let mut preview: Signal<Option<Result<DeviceTagPreview, String>>> = use_signal(|| None);

    {
        let svc = svc.clone();
        let did = device_id.clone();
        let _ = use_resource(move || {
            let svc = svc.clone();
            let did = did.clone();
            async move {
                let res = svc.preview_device_tags(&did).await;
                preview.set(Some(res));
            }
        });
    }

    let svc_for_accept = svc.clone();
    let did_for_accept = device_id.clone();

    rsx! {
        div { class: "pt-bulk-rename-modal-overlay",
            onclick: move |_| on_close.call(()),
            div { class: "pt-bulk-rename-modal preview-modal",
                onclick: move |e| e.stop_propagation(),
                div { class: "pt-bulk-rename-header",
                    h4 { "Preview tags for `{device_id}`" }
                    button {
                        class: "pt-modal-close",
                        onclick: move |_| on_close.call(()),
                        "\u{00D7}"
                    }
                }
                div { class: "pt-bulk-rename-body preview-body",
                    match preview.read().as_ref() {
                        None => rsx! {
                            div { class: "preview-loading", "Loading preview…" }
                        },
                        Some(Err(e)) => rsx! {
                            div { class: "form-error", "Preview failed: {e}" }
                        },
                        Some(Ok(p)) => rsx! { PreviewBody { preview: p.clone() } },
                    }
                }
                div { class: "form-actions",
                    button {
                        class: "btn btn-primary",
                        onclick: move |_| {
                            let svc = svc_for_accept.clone();
                            let did = did_for_accept.clone();
                            let on_accept = on_accept;
                            spawn(async move {
                                let _ = svc.accept_device(&did).await;
                                on_accept.call(());
                            });
                        },
                        "Accept device with these tags"
                    }
                    button {
                        class: "btn",
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                }
            }
        }
    }
}

#[component]
fn PreviewBody(preview: DeviceTagPreview) -> Element {
    rsx! {
        section { class: "preview-section",
            h5 { "Equipment" }
            div { class: "preview-equip-row",
                span { class: "preview-equip-dis", "{preview.device_dis}" }
                SourceBadge {
                    source: preview.equip_source,
                    confidence: preview.equip_confidence,
                }
            }
            TagChips { tags: preview.equip_tags.clone() }
        }
        section { class: "preview-section",
            h5 { "Points ({preview.points.len()})" }
            div { class: "preview-points-scroll",
                table { class: "settings-table preview-points-table",
                    thead {
                        tr {
                            th { "Point" }
                            th { "Units" }
                            th { "Source" }
                            th { "Tags" }
                        }
                    }
                    tbody {
                        for pt in &preview.points {
                            {
                                let pt = pt.clone();
                                rsx! {
                                    tr { key: "{pt.point_id}",
                                        td {
                                            div { class: "preview-pt-dis", "{pt.point_dis}" }
                                            div { class: "preview-pt-id", "{pt.point_id}" }
                                        }
                                        td { "{pt.units.clone().unwrap_or_default()}" }
                                        td {
                                            SourceBadge {
                                                source: pt.source,
                                                confidence: pt.confidence,
                                            }
                                        }
                                        td { TagChips { tags: pt.tags.clone() } }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SourceBadge(source: TagSource, confidence: f32) -> Element {
    let (label, cls) = match source {
        TagSource::Atlas => ("atlas", "src-atlas"),
        TagSource::Heuristic => ("heuristic", "src-heuristic"),
    };
    let pct = (confidence * 100.0).round().clamp(0.0, 100.0) as i32;
    let conf_cls = if pct >= 80 {
        "conf-high"
    } else if pct >= 50 {
        "conf-mid"
    } else {
        "conf-low"
    };
    rsx! {
        span { class: "preview-src {cls}", "{label}" }
        span { class: "preview-conf {conf_cls}", "{pct}%" }
    }
}

#[component]
fn TagChips(tags: Vec<(String, Option<String>)>) -> Element {
    rsx! {
        div { class: "preview-chips",
            for (name, val) in tags {
                {
                    let label = match &val {
                        Some(v) if !v.is_empty() => format!("{name}: {v}"),
                        _ => name.clone(),
                    };
                    rsx! {
                        span { class: "preview-chip", "{label}" }
                    }
                }
            }
        }
    }
}
