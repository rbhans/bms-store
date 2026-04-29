//! Fan-out alarm queries across multiple sites.
//!
//! The supervisor holds N per-site stores, but a Phase 2 supervisor can also
//! hold N HTTP-backed remote stores in the same vector. To accommodate both,
//! aggregator sources are typed against the `SiteAlarmStore` trait — defined
//! in `crate::gui::aggregation::alarm` so it is reachable from the supervisor
//! module without depending on `gui` (which is feature-gated).
//!
//! Per-site errors are captured into a status map instead of failing the whole
//! aggregation, so a single unreachable remote site does not blank the
//! cross-site dashboard.

use std::sync::Arc;

use futures::future::join_all;

pub use crate::gui::aggregation::alarm::SiteAlarmStore;
pub use crate::gui::aggregation::types::{AggregatorError, SiteActiveAlarm, SiteAlarmEvent};
use bms_store_storage::store::alarm_store::AlarmHistoryQuery;

// ----------------------------------------------------------------
// Aggregator
// ----------------------------------------------------------------

/// One per-site alarm store paired with its identity for display/routing.
#[derive(Clone)]
pub struct SiteAlarmSource {
    pub site_id: String,
    pub site_name: String,
    pub store: Arc<dyn SiteAlarmStore>,
}

/// Active alarm annotated with its site of origin.
#[derive(Clone, Debug)]
pub struct SiteScopedActiveAlarm {
    pub site_id: String,
    pub site_name: String,
    pub alarm: SiteActiveAlarm,
}

/// History event annotated with its site of origin.
#[derive(Clone, Debug)]
pub struct SiteScopedAlarmEvent {
    pub site_id: String,
    pub site_name: String,
    pub event: SiteAlarmEvent,
}

/// Per-site failure record returned alongside successful aggregation rows.
#[derive(Clone, Debug)]
pub struct SiteAggregationError {
    pub site_id: String,
    pub site_name: String,
    pub error: String,
}

/// Result of a fan-out call: rows from successful sites + per-site errors for
/// failing sites. Cross-site views render the rows AND a "⚠ unreachable" badge
/// per failing site.
#[derive(Clone, Debug, Default)]
pub struct AggregatedActiveAlarms {
    pub rows: Vec<SiteScopedActiveAlarm>,
    pub site_errors: Vec<SiteAggregationError>,
}

#[derive(Clone, Debug, Default)]
pub struct AggregatedAlarmHistory {
    pub rows: Vec<SiteScopedAlarmEvent>,
    pub site_errors: Vec<SiteAggregationError>,
}

/// Aggregates alarm queries across all loaded sites.
#[derive(Clone)]
pub struct AggregatedAlarmStore {
    sources: Vec<SiteAlarmSource>,
}

impl AggregatedAlarmStore {
    pub fn new(sources: Vec<SiteAlarmSource>) -> Self {
        Self { sources }
    }

    /// Number of loaded sites contributing to the aggregation.
    pub fn site_count(&self) -> usize {
        self.sources.len()
    }

    /// Fan out `get_active_alarms()` across every site. A failing site is
    /// recorded in `site_errors` and skipped — successful sites still produce
    /// rows so one unreachable remote does not blank the dashboard.
    pub async fn get_active_alarms(&self) -> AggregatedActiveAlarms {
        let mut futures = Vec::with_capacity(self.sources.len());
        for src in &self.sources {
            let id = src.site_id.clone();
            let name = src.site_name.clone();
            let store = src.store.clone();
            futures.push(async move {
                let result = store.get_active_alarms().await;
                (id, name, result)
            });
        }
        let mut out = AggregatedActiveAlarms::default();
        for (site_id, site_name, result) in join_all(futures).await {
            match result {
                Ok(alarms) => {
                    for alarm in alarms {
                        out.rows.push(SiteScopedActiveAlarm {
                            site_id: site_id.clone(),
                            site_name: site_name.clone(),
                            alarm,
                        });
                    }
                }
                Err(e) => {
                    out.site_errors.push(SiteAggregationError {
                        site_id,
                        site_name,
                        error: e.to_string(),
                    });
                }
            }
        }
        out
    }

    /// Fan out a history query across all sites. Per-site query limits are
    /// applied independently — total result length can exceed `query.limit`.
    pub async fn query_history(&self, query: AlarmHistoryQuery) -> AggregatedAlarmHistory {
        let mut futures = Vec::with_capacity(self.sources.len());
        for src in &self.sources {
            let id = src.site_id.clone();
            let name = src.site_name.clone();
            let store = src.store.clone();
            let q = query.clone();
            futures.push(async move {
                let result = store.query_history(q).await;
                (id, name, result)
            });
        }
        let mut out = AggregatedAlarmHistory::default();
        for (site_id, site_name, result) in join_all(futures).await {
            match result {
                Ok(events) => {
                    for event in events {
                        out.rows.push(SiteScopedAlarmEvent {
                            site_id: site_id.clone(),
                            site_name: site_name.clone(),
                            event,
                        });
                    }
                }
                Err(e) => {
                    out.site_errors.push(SiteAggregationError {
                        site_id,
                        site_name,
                        error: e.to_string(),
                    });
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bms_store_storage::store::alarm_store::{AlarmSeverity, AlarmStore};
    use async_trait::async_trait;

    /// Stub `SiteAlarmStore` that returns canned data so we can exercise the
    /// aggregator without spinning up a real `AlarmStore`.
    struct StubAlarmStore {
        active: Vec<SiteActiveAlarm>,
        history: Vec<SiteAlarmEvent>,
        fail: bool,
    }

    #[async_trait]
    impl SiteAlarmStore for StubAlarmStore {
        async fn get_active_alarms(&self) -> Result<Vec<SiteActiveAlarm>, AggregatorError> {
            if self.fail {
                Err(AggregatorError::Unreachable("stub".into()))
            } else {
                Ok(self.active.clone())
            }
        }

        async fn query_history(
            &self,
            _q: AlarmHistoryQuery,
        ) -> Result<Vec<SiteAlarmEvent>, AggregatorError> {
            if self.fail {
                Err(AggregatorError::Unreachable("stub".into()))
            } else {
                Ok(self.history.clone())
            }
        }
    }

    fn mk_alarm(device: &str) -> SiteActiveAlarm {
        SiteActiveAlarm {
            config_id: 1,
            device_id: device.into(),
            point_id: "pt".into(),
            severity: AlarmSeverity::Critical,
            trigger_value: 42.0,
            trigger_time_ms: 0,
            ack_time_ms: None,
        }
    }

    fn ok_source(id: &str, name: &str, alarms: Vec<SiteActiveAlarm>) -> SiteAlarmSource {
        SiteAlarmSource {
            site_id: id.into(),
            site_name: name.into(),
            store: Arc::new(StubAlarmStore {
                active: alarms,
                history: Vec::new(),
                fail: false,
            }),
        }
    }

    fn failing_source(id: &str, name: &str) -> SiteAlarmSource {
        SiteAlarmSource {
            site_id: id.into(),
            site_name: name.into(),
            store: Arc::new(StubAlarmStore {
                active: Vec::new(),
                history: Vec::new(),
                fail: true,
            }),
        }
    }

    #[tokio::test]
    async fn aggregate_merges_successful_sites() {
        let agg = AggregatedAlarmStore::new(vec![
            ok_source("a", "Site A", vec![mk_alarm("dev-a1"), mk_alarm("dev-a2")]),
            ok_source("b", "Site B", vec![mk_alarm("dev-b1")]),
        ]);
        let result = agg.get_active_alarms().await;
        assert_eq!(result.rows.len(), 3);
        assert!(result.site_errors.is_empty());
    }

    #[tokio::test]
    async fn aggregate_skips_failing_source() {
        let agg = AggregatedAlarmStore::new(vec![
            ok_source("a", "Site A", vec![mk_alarm("dev-a1")]),
            failing_source("b", "Site B"),
            ok_source("c", "Site C", vec![mk_alarm("dev-c1")]),
        ]);
        let result = agg.get_active_alarms().await;
        assert_eq!(result.rows.len(), 2);
        assert_eq!(result.site_errors.len(), 1);
        assert_eq!(result.site_errors[0].site_id, "b");
    }

    #[tokio::test]
    async fn history_aggregate_skips_failing_source() {
        let agg = AggregatedAlarmStore::new(vec![
            failing_source("a", "Site A"),
            ok_source("b", "Site B", vec![]),
        ]);
        let result = agg.query_history(AlarmHistoryQuery::default()).await;
        assert!(result.rows.is_empty());
        assert_eq!(result.site_errors.len(), 1);
    }

    #[test]
    fn local_alarm_store_implements_trait() {
        // Compile-only check that the blanket impl resolves.
        fn assert_impl<T: SiteAlarmStore>() {}
        assert_impl::<AlarmStore>();
    }
}
