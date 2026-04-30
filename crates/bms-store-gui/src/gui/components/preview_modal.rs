//! Shared dry-run preview modal for bulk operations.
//!
//! Renders a before/after diff table for N items. Designed to be reused
//! by any bulk operation (prototype apply, equipRef assign, bulk rename, etc.).

use dioxus::prelude::*;

// ----------------------------------------------------------------
// Public types
// ----------------------------------------------------------------

/// The nature of a change in a preview row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChangeKind {
    /// A new value is being added where none existed.
    Add,
    /// An existing value is being replaced.
    Modify,
    /// An existing value is being removed.
    Remove,
    /// No change will be made (included for completeness / visibility).
    NoOp,
}

impl ChangeKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Add => "Add",
            Self::Modify => "Modify",
            Self::Remove => "Remove",
            Self::NoOp => "No Change",
        }
    }

    pub fn css_class(&self) -> &'static str {
        match self {
            Self::Add => "preview-change-add",
            Self::Modify => "preview-change-modify",
            Self::Remove => "preview-change-remove",
            Self::NoOp => "preview-change-noop",
        }
    }
}

/// A single row in the preview table.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewRow {
    /// Stable key for this row (e.g. point UUID or "device/point_id").
    pub id: String,
    /// Human-readable item label (e.g. "VAV-101 / temp").
    pub label: String,
    /// Current value (shown in "Before" column).
    pub before: String,
    /// Proposed value (shown in "After" column).
    pub after: String,
    /// The kind of change this row represents.
    pub change_kind: ChangeKind,
}

// ----------------------------------------------------------------
// Component
// ----------------------------------------------------------------

/// A modal that shows a before/after diff for N items.
///
/// `on_confirm` is called when the user clicks "Apply".
/// `on_cancel` is called when the user clicks "Cancel" or the overlay.
#[component]
pub fn PreviewModal(
    title: String,
    rows: Vec<PreviewRow>,
    on_confirm: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let filter_add = use_signal(|| true);
    let filter_modify = use_signal(|| true);
    let filter_remove = use_signal(|| true);
    let filter_noop = use_signal(|| true);

    // Snapshot readable values before any mutable borrows below.
    let fa = *filter_add.read();
    let fm = *filter_modify.read();
    let fr = *filter_remove.read();
    let fn_ = *filter_noop.read();

    let total = rows.len();
    let actionable_count = rows
        .iter()
        .filter(|r| r.change_kind != ChangeKind::NoOp)
        .count();

    let apply_label = if actionable_count == 1 {
        "Apply 1 change".to_string()
    } else {
        format!("Apply {actionable_count} changes")
    };

    let summary_label = if actionable_count == 1 {
        format!("1 change of {total} items")
    } else {
        format!("{actionable_count} changes of {total} items")
    };

    let visible_rows: Vec<PreviewRow> = rows
        .iter()
        .filter(|r| match r.change_kind {
            ChangeKind::Add => fa,
            ChangeKind::Modify => fm,
            ChangeKind::Remove => fr,
            ChangeKind::NoOp => fn_,
        })
        .cloned()
        .collect();

    let apply_disabled = actionable_count == 0;

    let add_btn_class = filter_btn_class(fa, "preview-change-add");
    let modify_btn_class = filter_btn_class(fm, "preview-change-modify");
    let remove_btn_class = filter_btn_class(fr, "preview-change-remove");
    let noop_btn_class = filter_btn_class(fn_, "preview-change-noop");

    let apply_btn_class = if apply_disabled {
        "btn-primary btn-disabled"
    } else {
        "btn-primary"
    };

    rsx! {
        div {
            class: "preview-modal-overlay",
            onclick: move |_| on_cancel.call(()),

            div {
                class: "preview-modal",
                onclick: move |e| e.stop_propagation(),

                // ── Header ────────────────────────────────────────
                div { class: "preview-modal-header",
                    div { class: "preview-modal-title",
                        span { "{title}" }
                        span { class: "preview-modal-count", "{summary_label}" }
                    }
                    button {
                        class: "preview-modal-close",
                        onclick: move |_| on_cancel.call(()),
                        "x"
                    }
                }

                // ── Filter toggles ────────────────────────────────
                div { class: "preview-modal-filters",
                    button {
                        class: "{add_btn_class}",
                        onclick: move |_| { let v = *filter_add.read(); filter_add.clone().set(!v); },
                        "Add"
                    }
                    button {
                        class: "{modify_btn_class}",
                        onclick: move |_| { let v = *filter_modify.read(); filter_modify.clone().set(!v); },
                        "Modify"
                    }
                    button {
                        class: "{remove_btn_class}",
                        onclick: move |_| { let v = *filter_remove.read(); filter_remove.clone().set(!v); },
                        "Remove"
                    }
                    button {
                        class: "{noop_btn_class}",
                        onclick: move |_| { let v = *filter_noop.read(); filter_noop.clone().set(!v); },
                        "No Change"
                    }
                }

                // ── Scrollable table ─────────────────────────��────
                div { class: "preview-modal-body",
                    if visible_rows.is_empty() {
                        div { class: "preview-modal-empty", "No items match the current filters." }
                    } else {
                        table { class: "preview-modal-table",
                            thead {
                                tr {
                                    th { "Item" }
                                    th { "Before" }
                                    th { "After" }
                                    th { "Change" }
                                }
                            }
                            tbody {
                                for row in visible_rows {
                                    {
                                        let row_class = row.change_kind.css_class();
                                        let badge_class = format!("preview-badge {}", row.change_kind.css_class());
                                        let kind_label = row.change_kind.label();
                                        rsx! {
                                            tr {
                                                key: "{row.id}",
                                                class: "{row_class}",
                                                td { class: "preview-col-label", "{row.label}" }
                                                td { class: "preview-col-before",
                                                    span { class: "preview-value", "{row.before}" }
                                                }
                                                td { class: "preview-col-after",
                                                    span { class: "preview-value", "{row.after}" }
                                                }
                                                td { class: "preview-col-kind",
                                                    span { class: "{badge_class}", "{kind_label}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // ── Footer actions ─────────────────────────────���──
                div { class: "preview-modal-footer",
                    button {
                        class: "btn-secondary",
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }
                    button {
                        class: "{apply_btn_class}",
                        disabled: apply_disabled,
                        onclick: move |_| {
                            if !apply_disabled {
                                on_confirm.call(());
                            }
                        },
                        "{apply_label}"
                    }
                }
            }
        }
    }
}

fn filter_btn_class(active: bool, kind_class: &str) -> String {
    if active {
        format!("preview-filter-btn preview-filter-active {kind_class}")
    } else {
        format!("preview-filter-btn {kind_class}")
    }
}
