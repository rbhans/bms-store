//! History retention configuration view.
//!
//! The history tiering system uses four fixed retention windows: Hot, Warm, Cold,
//! and Archive. These values are currently compile-time constants in
//! `bms-store-storage/src/store/history_store.rs` — there is no runtime API to
//! change them. This view is therefore informational only.
//!
//! Future work: expose a `RetentionConfig` struct persisted per-project so that
//! operators can adjust windows at runtime without recompiling.

use dioxus::prelude::*;

/// Hard-coded tier definitions mirroring the constants in history_store.rs.
struct TierInfo {
    name: &'static str,
    resolution: &'static str,
    retention: &'static str,
    rollup_schedule: &'static str,
    description: &'static str,
}

const TIERS: &[TierInfo] = &[
    TierInfo {
        name: "Hot",
        resolution: "Full COV (every change)",
        retention: "48 hours",
        rollup_schedule: "Rolls down to Warm every hour",
        description: "Raw change-of-value samples. Highest resolution, shortest retention.",
    },
    TierInfo {
        name: "Warm",
        resolution: "1-minute rollups",
        retention: "90 days",
        rollup_schedule: "Rolls down to Cold every 24 hours",
        description: "1-minute min/mean/max averages. Good balance of resolution and storage.",
    },
    TierInfo {
        name: "Cold",
        resolution: "15-minute rollups",
        retention: "2 years",
        rollup_schedule: "Rolls down to Archive every 7 days",
        description: "15-minute summaries. Suitable for long-term trend analysis.",
    },
    TierInfo {
        name: "Archive",
        resolution: "1-hour rollups",
        retention: "Indefinite",
        rollup_schedule: "Never removed automatically",
        description: "Hourly summaries kept forever. Minimal storage footprint.",
    },
];

#[component]
pub fn RetentionView() -> Element {
    rsx! {
        div { class: "retention-section",
            div { class: "retention-header",
                h2 { "History Retention" }
                span { class: "retention-info-badge",
                    "Retention is currently fixed; runtime configuration coming in a future release.
                    Edit history_store.rs to adjust values for now."
                }
            }

            p { class: "retention-intro",
                "The history engine automatically downsample raw samples through four tiers.
                Older data is progressively aggregated to save storage while preserving long-term trends."
            }

            div { class: "retention-tier-grid",
                for tier in TIERS {
                    div { class: "retention-tier-card",
                        div { class: "retention-tier-header",
                            span { class: "retention-tier-name", "{tier.name}" }
                            span { class: "retention-tier-resolution", "{tier.resolution}" }
                        }
                        div { class: "retention-tier-body",
                            div { class: "retention-tier-row",
                                span { class: "retention-label", "Retention:" }
                                span { class: "retention-value", "{tier.retention}" }
                            }
                            div { class: "retention-tier-row",
                                span { class: "retention-label", "Schedule:" }
                                span { class: "retention-value", "{tier.rollup_schedule}" }
                            }
                            p { class: "retention-tier-desc", "{tier.description}" }
                        }
                    }
                }
            }

            div { class: "retention-flow",
                h3 { "Data Flow" }
                div { class: "retention-flow-row",
                    span { class: "retention-flow-step", "COV" }
                    span { class: "retention-flow-arrow", "→ 1h →" }
                    span { class: "retention-flow-step", "Hot (48h)" }
                    span { class: "retention-flow-arrow", "→ 24h →" }
                    span { class: "retention-flow-step", "Warm (90d)" }
                    span { class: "retention-flow-arrow", "→ 7d →" }
                    span { class: "retention-flow-step", "Cold (2y)" }
                    span { class: "retention-flow-arrow", "→ 7d →" }
                    span { class: "retention-flow-step", "Archive (∞)" }
                }
            }

            div { class: "retention-notice",
                h3 { "Configuration" }
                p {
                    "Retention windows are currently compile-time constants in "
                    code { "bms-store-storage/src/store/history_store.rs" }
                    ". To change them, update the "
                    code { "HOT_RETENTION_MS" }
                    ", "
                    code { "WARM_RETENTION_MS" }
                    ", and "
                    code { "COLD_RETENTION_MS" }
                    " constants and rebuild."
                }
                p {
                    "A future release will expose these as a per-project "
                    code { "RetentionConfig" }
                    " that can be edited here without recompiling."
                }
            }
        }
    }
}
