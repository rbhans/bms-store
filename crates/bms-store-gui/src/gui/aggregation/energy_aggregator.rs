//! Fan-out energy queries across multiple sites.
//!
//! The `SiteEnergyStore` trait + blanket impl for `EnergyStore` lives in
//! `crate::gui::aggregation::energy` so the supervisor's HTTP-backed remote store
//! can implement it without depending on `gui`.
//!
//! Each per-site KPI computation issues N+1 calls (1 list_meters + N
//! query_daily_rollups). For remote sites this means N+1 HTTPs over the wire;
//! that's fine for the meter counts (single digits) we expect during the MVP.
//! Future optimization: a `/api/energy/site-kpis` endpoint that does the math
//! server-side.

use std::sync::Arc;

use futures::future::join_all;

pub use crate::gui::aggregation::energy::SiteEnergyStore;
pub use crate::gui::aggregation::types::{AggregatorError, SiteDailyRollup, SiteMeter};

// ----------------------------------------------------------------
// Aggregator
// ----------------------------------------------------------------

/// One per-site `SiteEnergyStore` paired with its identity.
#[derive(Clone)]
pub struct SiteEnergySource {
    pub site_id: String,
    pub site_name: String,
    pub store: Arc<dyn SiteEnergyStore>,
}

/// Per-site energy KPIs over a configurable window.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SiteEnergyKpis {
    pub site_id: String,
    pub site_name: String,
    pub total_kwh: f64,
    pub total_cost: f64,
    pub peak_kw: f64,
    pub meter_count: usize,
    /// Load factor = average demand / peak demand (0..1). 0 if peak is 0.
    pub load_factor: f64,
}

#[derive(Clone, Debug)]
pub struct SiteEnergyError {
    pub site_id: String,
    pub site_name: String,
    pub error: String,
}

#[derive(Clone, Debug, Default)]
pub struct AggregatedSiteKpis {
    pub kpis: Vec<SiteEnergyKpis>,
    pub site_errors: Vec<SiteEnergyError>,
}

#[derive(Clone)]
pub struct AggregatedEnergyStore {
    sources: Vec<SiteEnergySource>,
}

impl AggregatedEnergyStore {
    pub fn new(sources: Vec<SiteEnergySource>) -> Self {
        Self { sources }
    }

    pub fn site_count(&self) -> usize {
        self.sources.len()
    }

    /// Compute per-site KPIs over the given ms range by summing all meters'
    /// daily rollups. Failing sites are recorded in `site_errors` and skipped.
    pub async fn site_kpis(&self, start_ms: i64, end_ms: i64) -> AggregatedSiteKpis {
        let mut futs = Vec::with_capacity(self.sources.len());
        for src in &self.sources {
            let id = src.site_id.clone();
            let name = src.site_name.clone();
            let store = src.store.clone();
            futs.push(async move {
                let result = compute_site_kpis(&id, &name, store, start_ms, end_ms).await;
                (id, name, result)
            });
        }
        let mut out = AggregatedSiteKpis::default();
        for (site_id, site_name, result) in join_all(futs).await {
            match result {
                Ok(kpis) => out.kpis.push(kpis),
                Err(e) => out.site_errors.push(SiteEnergyError {
                    site_id,
                    site_name,
                    error: e.to_string(),
                }),
            }
        }
        out
    }
}

async fn compute_site_kpis(
    site_id: &str,
    site_name: &str,
    store: Arc<dyn SiteEnergyStore>,
    start_ms: i64,
    end_ms: i64,
) -> Result<SiteEnergyKpis, AggregatorError> {
    let meters = store.list_meters().await?;
    let meter_count = meters.len();
    let mut total_kwh = 0.0f64;
    let mut total_cost = 0.0f64;
    let mut peak_kw = 0.0f64;
    let mut weighted_avg = 0.0f64;
    for m in &meters {
        let rollups = store.query_daily_rollups(m.id, start_ms, end_ms).await?;
        for r in &rollups {
            total_kwh += r.consumption_kwh;
            total_cost += r.cost;
            if r.peak_demand_kw > peak_kw {
                peak_kw = r.peak_demand_kw;
            }
            weighted_avg += r.avg_kw;
        }
    }
    let load_factor = if peak_kw > 0.0 && !meters.is_empty() {
        (weighted_avg / (meters.len() as f64)) / peak_kw
    } else {
        0.0
    };
    Ok(SiteEnergyKpis {
        site_id: site_id.into(),
        site_name: site_name.into(),
        total_kwh,
        total_cost,
        peak_kw,
        meter_count,
        load_factor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bms_store_storage::store::energy_store::EnergyStore;
    use async_trait::async_trait;

    struct StubEnergyStore {
        meters: Vec<SiteMeter>,
        rollups: Vec<SiteDailyRollup>,
        fail: bool,
    }

    #[async_trait]
    impl SiteEnergyStore for StubEnergyStore {
        async fn list_meters(&self) -> Result<Vec<SiteMeter>, AggregatorError> {
            if self.fail {
                Err(AggregatorError::Unreachable("stub".into()))
            } else {
                Ok(self.meters.clone())
            }
        }
        async fn query_daily_rollups(
            &self,
            _meter_id: i64,
            _start_ms: i64,
            _end_ms: i64,
        ) -> Result<Vec<SiteDailyRollup>, AggregatorError> {
            if self.fail {
                Err(AggregatorError::Unreachable("stub".into()))
            } else {
                Ok(self.rollups.clone())
            }
        }
    }

    fn ok_source(id: &str) -> SiteEnergySource {
        SiteEnergySource {
            site_id: id.into(),
            site_name: format!("Site {id}"),
            store: Arc::new(StubEnergyStore {
                meters: vec![SiteMeter {
                    id: 1,
                    name: "Main".into(),
                }],
                rollups: vec![SiteDailyRollup {
                    period_start_ms: 0,
                    consumption_kwh: 100.0,
                    peak_demand_kw: 50.0,
                    avg_kw: 25.0,
                    cost: 12.50,
                }],
                fail: false,
            }),
        }
    }

    fn failing_source(id: &str) -> SiteEnergySource {
        SiteEnergySource {
            site_id: id.into(),
            site_name: format!("Site {id}"),
            store: Arc::new(StubEnergyStore {
                meters: Vec::new(),
                rollups: Vec::new(),
                fail: true,
            }),
        }
    }

    #[test]
    fn site_kpis_default_is_zero() {
        let k = SiteEnergyKpis::default();
        assert_eq!(k.total_kwh, 0.0);
        assert_eq!(k.meter_count, 0);
    }

    #[tokio::test]
    async fn aggregate_merges_successful_sites() {
        let agg = AggregatedEnergyStore::new(vec![ok_source("a"), ok_source("b")]);
        let result = agg.site_kpis(0, 1).await;
        assert_eq!(result.kpis.len(), 2);
        assert!(result.site_errors.is_empty());
        assert_eq!(result.kpis[0].total_kwh, 100.0);
        assert_eq!(result.kpis[0].peak_kw, 50.0);
    }

    #[tokio::test]
    async fn aggregate_skips_failing_source() {
        let agg = AggregatedEnergyStore::new(vec![ok_source("a"), failing_source("b")]);
        let result = agg.site_kpis(0, 1).await;
        assert_eq!(result.kpis.len(), 1);
        assert_eq!(result.site_errors.len(), 1);
        assert_eq!(result.site_errors[0].site_id, "b");
    }

    #[test]
    fn local_energy_store_implements_trait() {
        fn assert_impl<T: SiteEnergyStore>() {}
        assert_impl::<EnergyStore>();
    }
}
