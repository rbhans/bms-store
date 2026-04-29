//! Cross-site Energy view for supervisor mode.
//!
//! Phase 1 scope: a sortable table of per-site KPIs over the last 7 days —
//! total kWh, total cost, peak kW, load factor, meter count.
//!
//! Phase 2: each `LoadedSiteVariant` produces a trait-object source so
//! local + remote sites mix transparently. Failing sites show as a
//! "⚠ unreachable" row above the data table.

use std::sync::Arc;

use dioxus::prelude::*;

use crate::gui::aggregation::energy_aggregator::{
    AggregatedEnergyStore, SiteEnergyKpis, SiteEnergySource, SiteEnergyStore,
};

use super::supervisor_gate::{LoadedSiteVariant, SupervisorHandle};

const SEVEN_DAYS_MS: i64 = 7 * 24 * 60 * 60 * 1000;

#[component]
pub fn CrossSiteEnergyView(handle: SupervisorHandle) -> Element {
    let agg = use_hook(|| {
        let sources: Vec<SiteEnergySource> = handle
            .sites
            .iter()
            .map(|variant| match variant {
                LoadedSiteVariant::Local(s) => SiteEnergySource {
                    site_id: s.meta.id.clone(),
                    site_name: s.meta.name.clone(),
                    store: Arc::new(s.platform.energy_store.clone()) as Arc<dyn SiteEnergyStore>,
                },
                LoadedSiteVariant::Remote(r) => SiteEnergySource {
                    site_id: r.site_id.clone(),
                    site_name: r.name.clone(),
                    store: Arc::new(
                        crate::supervisor::remote::energy_store::RemoteEnergyStore::new(
                            r.client.clone(),
                        ),
                    ) as Arc<dyn SiteEnergyStore>,
                },
            })
            .collect();
        AggregatedEnergyStore::new(sources)
    });

    let site_count = agg.site_count();
    let agg_for_resource = agg.clone();
    let kpi_res = use_resource(move || {
        let agg = agg_for_resource.clone();
        async move {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;
            let start = now - SEVEN_DAYS_MS;
            agg.site_kpis(start, now).await
        }
    });

    let result = kpi_res.read().clone().unwrap_or_default();
    let kpis: Vec<SiteEnergyKpis> = result.kpis.clone();
    let total_kwh: f64 = kpis.iter().map(|k| k.total_kwh).sum();
    let total_cost: f64 = kpis.iter().map(|k| k.total_cost).sum();

    rsx! {
        div { class: "cross-site-energy-view",
            div { class: "cross-site-energy-header",
                h2 { "Energy — 7-day rollup across all sites" }
                p { class: "text-muted",
                    "{site_count} sites — {total_kwh:.0} kWh total, {total_cost:.2} total cost"
                }
            }
            if !result.site_errors.is_empty() {
                div { class: "cross-site-energy-errors",
                    for err in result.site_errors.iter() {
                        div { class: "site-error-row",
                            span { class: "warning-glyph", "⚠" }
                            strong { "{err.site_name}" }
                            span { class: "text-muted", " unreachable: {err.error}" }
                        }
                    }
                }
            }
            if kpis.is_empty() && result.site_errors.is_empty() {
                div { class: "text-muted", "No energy meters configured across any site." }
            } else if !kpis.is_empty() {
                table { class: "cross-site-energy-table",
                    thead {
                        tr {
                            th { "Site" }
                            th { "Meters" }
                            th { "Total kWh (7d)" }
                            th { "Total cost (7d)" }
                            th { "Peak kW" }
                            th { "Load factor" }
                        }
                    }
                    tbody {
                        for k in kpis.iter() {
                            {
                                let kwh = format!("{:.0}", k.total_kwh);
                                let cost = format!("{:.2}", k.total_cost);
                                let peak = format!("{:.1}", k.peak_kw);
                                let lf_pct = format!("{:.0}%", k.load_factor * 100.0);
                                let meter_count = k.meter_count;
                                rsx! {
                                    tr {
                                        td { "{k.site_name}" }
                                        td { "{meter_count}" }
                                        td { "{kwh}" }
                                        td { "{cost}" }
                                        td { "{peak}" }
                                        td { "{lf_pct}" }
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
