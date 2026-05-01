// SPDX-License-Identifier: MIT
//! Tag Coverage Report — gap evaluation Tier-1 #4.
//!
//! # Coverage Scoring Metric
//!
//! Each entity is assigned a score in the range 0–100:
//!
//! - **0%** — no tags at all (entity has zero tags).
//! - **50%** — has the base kind tag ("equip" or "point") but no Haystack
//!   semantic-measurement tags (e.g. `temp`, `sensor`, `cmd`).
//! - **75%** — has semantic tags but is missing *context* tags — specifically
//!   `equipRef` (for points) or `siteRef` / `spaceRef` (for equip), or
//!   validation issues include a Warning or Error.
//! - **100%** — passes `validate_tags` with no Errors or Warnings (Infos are OK).
//!
//! The score is deterministic: given the same tag set it always yields the same
//! score. The UI explains why each tier was chosen ("This point scores 75%
//! because it has validation warnings").

use std::collections::HashMap;

use dioxus::prelude::*;

use crate::gui::state::{ActiveView, AppState};
use bms_store_bridges::haystack::validation::{validate_tags, Severity};
use bms_store_storage::store::entity_store::Entity;

// ---------------------------------------------------------------------------
// Scoring
// ---------------------------------------------------------------------------

/// The four coverage tiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverageScore {
    /// 0% — no tags at all.
    None,
    /// 50% — base kind tag only, no semantic tags.
    Partial,
    /// 75% — semantic tags present but validation warnings/errors exist.
    HasWarnings,
    /// 100% — all required tags, passes validation (no Error/Warning).
    Full,
}

impl CoverageScore {
    pub fn pct(self) -> u32 {
        match self {
            Self::None => 0,
            Self::Partial => 50,
            Self::HasWarnings => 75,
            Self::Full => 100,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "Untagged",
            Self::Partial => "Partial (50%)",
            Self::HasWarnings => "Has Warnings (75%)",
            Self::Full => "Complete (100%)",
        }
    }

    pub fn css_class(self) -> &'static str {
        match self {
            Self::None => "cov-score-none",
            Self::Partial => "cov-score-partial",
            Self::HasWarnings => "cov-score-warnings",
            Self::Full => "cov-score-full",
        }
    }

    /// Explain why this score was assigned.
    pub fn explain(self) -> &'static str {
        match self {
            Self::None => "No tags applied — create an entity to start tagging.",
            Self::Partial => "Has entity type tag but missing Haystack semantic tags (e.g. temp, sensor).",
            Self::HasWarnings => "Has semantic tags but validation reported warnings or errors.",
            Self::Full => "All required tags present; no validation errors or warnings.",
        }
    }
}

/// Semantic marker tags that indicate "this entity is semantically tagged".
/// If any of these are present the entity is at least Partial+ .
const SEMANTIC_TAGS: &[&str] = &[
    "temp", "humidity", "co2", "pressure", "flow", "power", "energy", "current",
    "voltage", "freq", "speed", "level", "sensor", "cmd", "sp", "point",
    "air", "water", "elec", "gas", "hot", "chilled", "cool", "heat",
];

pub fn score_entity(entity: &Entity) -> CoverageScore {
    let tags = &entity.tags;
    if tags.is_empty() {
        return CoverageScore::None;
    }

    // Check for semantic tags.
    let has_semantic = SEMANTIC_TAGS.iter().any(|s| tags.contains_key(*s));

    if !has_semantic {
        return CoverageScore::Partial;
    }

    // Has semantic tags — run validation.
    let issues = validate_tags(&entity.entity_type, tags);
    let has_problem = issues
        .iter()
        .any(|i| i.severity == Severity::Error || i.severity == Severity::Warning);

    if has_problem {
        CoverageScore::HasWarnings
    } else {
        CoverageScore::Full
    }
}

// ---------------------------------------------------------------------------
// Per-entity row data
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct EntityRow {
    id: String,
    dis: String,
    entity_type: String,
    score: CoverageScore,
    /// First validation warning/error message, if any.
    top_issue: Option<String>,
}

// ---------------------------------------------------------------------------
// CoverageView component
// ---------------------------------------------------------------------------

#[component]
pub fn CoverageView() -> Element {
    let mut state = use_context::<AppState>();
    let es = state.entity_store.clone();

    // Subscribe to entity store version for auto-refresh.
    let mut entity_version: Signal<u64> = use_signal(|| 0u64);
    use_future(move || {
        let store = es.clone();
        async move {
            let mut rx = store.subscribe();
            loop {
                if rx.changed().await.is_err() {
                    break;
                }
                entity_version.set(*rx.borrow());
            }
        }
    });

    // Fetch all entities.
    let es2 = state.entity_store.clone();
    let entities_res = use_resource(move || {
        let store = es2.clone();
        let _v = *entity_version.read();
        async move { store.list_entities(None, None).await }
    });

    let entities_read = entities_res.read();
    let entities = entities_read.as_deref().unwrap_or(&[]);

    // Compute per-entity rows.
    let mut rows: Vec<EntityRow> = entities
        .iter()
        .map(|e| {
            let score = score_entity(e);
            let issues = validate_tags(&e.entity_type, &e.tags);
            let top_issue = issues
                .iter()
                .find(|i| i.severity == Severity::Error || i.severity == Severity::Warning)
                .map(|i| i.message.clone());
            EntityRow {
                id: e.id.clone(),
                dis: if e.dis.is_empty() { e.id.clone() } else { e.dis.clone() },
                entity_type: e.entity_type.clone(),
                score,
                top_issue,
            }
        })
        .collect();

    let total = rows.len();

    // Project-wide stats.
    let full_count = rows.iter().filter(|r| r.score == CoverageScore::Full).count();
    let warn_count = rows.iter().filter(|r| r.score == CoverageScore::HasWarnings).count();
    let partial_count = rows.iter().filter(|r| r.score == CoverageScore::Partial).count();
    let none_count = rows.iter().filter(|r| r.score == CoverageScore::None).count();

    let overall_pct = if total == 0 {
        0u32
    } else {
        let sum: u32 = rows.iter().map(|r| r.score.pct()).sum();
        sum / total as u32
    };

    // Per-equipment-type breakdown.
    let mut by_equip_type: HashMap<String, (usize, usize)> = HashMap::new(); // type -> (full, total)
    for r in &rows {
        if r.entity_type == "equip" {
            let entry = by_equip_type.entry(
                // Derive a coarse type from entity id heuristic (first segment)
                r.id.split('-').next().unwrap_or("equip").to_uppercase()
            ).or_insert((0, 0));
            entry.1 += 1;
            if r.score == CoverageScore::Full {
                entry.0 += 1;
            }
        }
    }
    let mut equip_breakdown: Vec<(String, usize, usize)> = by_equip_type
        .into_iter()
        .map(|(k, (full, tot))| (k, full, tot))
        .collect();
    equip_breakdown.sort_by(|a, b| a.0.cmp(&b.0));

    // Untagged / partial list.
    let untagged_ids: Vec<String> = rows
        .iter()
        .filter(|r| r.score == CoverageScore::None || r.score == CoverageScore::Partial)
        .map(|r| r.id.clone())
        .collect();
    let untagged_count = untagged_ids.len();

    // Sort rows: None first, then Partial, then HasWarnings, then Full.
    rows.sort_by_key(|r| r.score.pct());

    // Drill-down filter state.
    let mut filter_score: Signal<Option<CoverageScore>> = use_signal(|| None);
    let active_filter = *filter_score.read();

    let visible_rows: Vec<EntityRow> = rows
        .iter()
        .filter(|r| active_filter.map(|f| r.score == f).unwrap_or(true))
        .take(200)
        .cloned()
        .collect();

    rsx! {
        div { class: "coverage-view",
            // Header
            div { class: "coverage-header",
                h2 { "Tag Coverage Report" }
                button {
                    class: "btn-secondary btn-sm",
                    onclick: move |_| {
                        // Force re-fetch by bumping a local signal
                        entity_version.set(entity_version() + 1);
                    },
                    "Refresh"
                }
            }

            // Project-wide score
            div { class: "coverage-summary",
                div { class: "coverage-score-banner",
                    span { class: "coverage-score-big", "{overall_pct}%" }
                    span { class: "coverage-score-sub",
                        "overall coverage ({full_count} of {total} entities fully tagged)"
                    }
                }
                div { class: "coverage-score-bar-bg",
                    div {
                        class: "coverage-score-bar-fill",
                        style: "width: {overall_pct}%;",
                    }
                }
            }

            // Tier breakdown
            div { class: "coverage-tiers",
                h3 { "Score Breakdown" }
                div { class: "coverage-tier-chips",
                    {
                        let cur = active_filter;
                        let is_active = cur == Some(CoverageScore::Full);
                        rsx! {
                            button {
                                class: if is_active { "cov-tier-chip cov-score-full active" } else { "cov-tier-chip cov-score-full" },
                                onclick: move |_| {
                                    filter_score.set(if cur == Some(CoverageScore::Full) { None } else { Some(CoverageScore::Full) });
                                },
                                span { class: "cov-tier-count", "{full_count}" }
                                span { class: "cov-tier-label", "Complete (100%)" }
                            }
                        }
                    }
                    {
                        let cur = active_filter;
                        let is_active = cur == Some(CoverageScore::HasWarnings);
                        rsx! {
                            button {
                                class: if is_active { "cov-tier-chip cov-score-warnings active" } else { "cov-tier-chip cov-score-warnings" },
                                onclick: move |_| {
                                    filter_score.set(if cur == Some(CoverageScore::HasWarnings) { None } else { Some(CoverageScore::HasWarnings) });
                                },
                                span { class: "cov-tier-count", "{warn_count}" }
                                span { class: "cov-tier-label", "Has Warnings (75%)" }
                            }
                        }
                    }
                    {
                        let cur = active_filter;
                        let is_active = cur == Some(CoverageScore::Partial);
                        rsx! {
                            button {
                                class: if is_active { "cov-tier-chip cov-score-partial active" } else { "cov-tier-chip cov-score-partial" },
                                onclick: move |_| {
                                    filter_score.set(if cur == Some(CoverageScore::Partial) { None } else { Some(CoverageScore::Partial) });
                                },
                                span { class: "cov-tier-count", "{partial_count}" }
                                span { class: "cov-tier-label", "Partial (50%)" }
                            }
                        }
                    }
                    {
                        let cur = active_filter;
                        let is_active = cur == Some(CoverageScore::None);
                        rsx! {
                            button {
                                class: if is_active { "cov-tier-chip cov-score-none active" } else { "cov-tier-chip cov-score-none" },
                                onclick: move |_| {
                                    filter_score.set(if cur == Some(CoverageScore::None) { None } else { Some(CoverageScore::None) });
                                },
                                span { class: "cov-tier-count", "{none_count}" }
                                span { class: "cov-tier-label", "Untagged (0%)" }
                            }
                        }
                    }
                }
            }

            // Per-equipment-type breakdown
            if !equip_breakdown.is_empty() {
                div { class: "coverage-equip-breakdown",
                    h3 { "Equipment Type Breakdown" }
                    table { class: "cov-breakdown-table",
                        thead {
                            tr {
                                th { "Type" }
                                th { "Fully Tagged" }
                                th { "Total" }
                                th { "Score" }
                            }
                        }
                        tbody {
                            for (etype, full, tot) in &equip_breakdown {
                                {
                                    let pct = if *tot == 0 { 0 } else { (*full * 100 / *tot) as u32 };
                                    let cls = if pct == 100 { "cov-score-full" } else if pct >= 75 { "cov-score-warnings" } else if pct >= 50 { "cov-score-partial" } else { "cov-score-none" };
                                    rsx! {
                                        tr {
                                            td { "{etype}" }
                                            td { "{full}" }
                                            td { "{tot}" }
                                            td {
                                                span { class: "cov-tier-pct {cls}", "{pct}%" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Untagged points bulk action
            if untagged_count > 0 {
                div { class: "coverage-untagged-actions",
                    span { class: "cov-untagged-count",
                        "{untagged_count} entities are untagged or partial."
                    }
                    button {
                        class: "btn-primary btn-sm",
                        title: "Switch to Haystack tab to tag these entities",
                        onclick: move |_| {
                            state.pending_config_section.set(Some("Haystack".to_string()));
                            state.active_view.set(ActiveView::Config);
                        },
                        "Open Haystack Tab"
                    }
                }
            }

            // Entity list
            div { class: "coverage-entity-list",
                h3 {
                    if let Some(f) = active_filter {
                        "Entities — {f.label()} (showing {visible_rows.len()})"
                    } else {
                        "All Entities (showing {visible_rows.len()} of {total})"
                    }
                    if active_filter.is_some() {
                        button {
                            class: "cov-clear-filter btn-sm",
                            onclick: move |_| filter_score.set(None),
                            "Clear filter"
                        }
                    }
                }
                if visible_rows.is_empty() {
                    p { class: "placeholder", "No entities match the selected filter." }
                } else {
                    table { class: "cov-entity-table",
                        thead {
                            tr {
                                th { "Entity" }
                                th { "Type" }
                                th { "Score" }
                                th { "Note" }
                            }
                        }
                        tbody {
                            for row in &visible_rows {
                                {
                                    let score_cls = row.score.css_class();
                                    let score_label = row.score.label();
                                    let explain = row.score.explain();
                                    let top = row.top_issue.as_deref().unwrap_or(explain);
                                    rsx! {
                                        tr { class: "cov-entity-row",
                                            td { class: "cov-entity-name", "{row.dis}" }
                                            td { class: "cov-entity-type",
                                                span { class: "config-type-badge config-type-{row.entity_type}",
                                                    match row.entity_type.as_str() {
                                                        "equip" => "E",
                                                        "point" => "P",
                                                        "site" => "S",
                                                        _ => "?",
                                                    }
                                                }
                                            }
                                            td {
                                                span {
                                                    class: "cov-score-badge {score_cls}",
                                                    title: "{explain}",
                                                    "{score_label}"
                                                }
                                            }
                                            td { class: "cov-entity-note", "{top}" }
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
}
