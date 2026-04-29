use crate::store::alarm_store::{AlarmHistoryQuery, AlarmStore};
use crate::store::history_store::{HistoryQuery, HistoryStore};
use crate::store::node_store::NodeStore;
use crate::store::point_store::{PointKey, PointStore};
use crate::store::report_store::{
    ExecutionStatus, PointRef, PointSelector, ReportDefinition, ReportError, ReportStore,
    SectionType,
};

use super::renderer::{
    self, AlarmEventRow, CurrentValueRow, RenderedSection, RuntimeRow, SeverityCount, SummaryRow,
    TypeCount,
};

/// The report generation engine — queries data stores and produces rendered HTML.
pub struct ReportEngine {
    pub history_store: HistoryStore,
    pub alarm_store: AlarmStore,
    pub point_store: PointStore,
    pub node_store: NodeStore,
}

impl ReportEngine {
    pub fn new(
        history_store: HistoryStore,
        alarm_store: AlarmStore,
        point_store: PointStore,
        node_store: NodeStore,
    ) -> Self {
        Self {
            history_store,
            alarm_store,
            point_store,
            node_store,
        }
    }

    /// Generate a report and return rendered HTML.
    pub async fn generate(&self, definition: &ReportDefinition) -> Result<String, ReportError> {
        let (start_ms, end_ms) = definition.config.time_range.resolve();
        let mut sections = Vec::new();

        for section in &definition.config.sections {
            let points = self.resolve_points(&section.point_selector).await;
            let rendered = self
                .fetch_section_data(
                    &section.section_type,
                    &points,
                    start_ms,
                    end_ms,
                    &section.title,
                )
                .await?;
            sections.push(rendered);
        }

        renderer::render_report_html(
            &definition.name,
            start_ms,
            end_ms,
            &definition.config.time_range,
            &sections,
        )
    }

    /// Resolve a PointSelector to concrete PointRef list.
    async fn resolve_points(&self, selector: &PointSelector) -> Vec<PointRef> {
        match selector {
            PointSelector::Explicit(refs) => refs.clone(),
            PointSelector::ByTag(tag) => {
                if tag.is_empty() {
                    return vec![];
                }
                let nodes = self.node_store.find_by_tag(tag, None).await;
                nodes
                    .into_iter()
                    .filter(|n| n.node_type == "point" || n.node_type == "virtual_point")
                    .map(|n| {
                        let (device_id, point_id) = split_node_id(&n.id);
                        PointRef {
                            device_id,
                            point_id,
                            label: Some(n.dis.clone()),
                        }
                    })
                    .collect()
            }
            PointSelector::ByParentNode(parent_id) => {
                let nodes = self.node_store.get_hierarchy(Some(parent_id)).await;
                nodes
                    .into_iter()
                    .filter(|n| n.node_type == "point" || n.node_type == "virtual_point")
                    .map(|n| {
                        let (device_id, point_id) = split_node_id(&n.id);
                        PointRef {
                            device_id,
                            point_id,
                            label: Some(n.dis.clone()),
                        }
                    })
                    .collect()
            }
            PointSelector::ByDevices(device_ids) => {
                let all_points = self.node_store.list_nodes(Some("point"), None).await;
                all_points
                    .into_iter()
                    .filter(|n| {
                        let (dev, _) = split_node_id(&n.id);
                        device_ids.contains(&dev)
                    })
                    .map(|n| {
                        let (device_id, point_id) = split_node_id(&n.id);
                        PointRef {
                            device_id,
                            point_id,
                            label: Some(n.dis.clone()),
                        }
                    })
                    .collect()
            }
        }
    }

    async fn fetch_section_data(
        &self,
        section_type: &SectionType,
        points: &[PointRef],
        start_ms: i64,
        end_ms: i64,
        title: &str,
    ) -> Result<RenderedSection, ReportError> {
        match section_type {
            SectionType::HistorySummary => {
                let mut rows = Vec::new();
                for pt in points {
                    let result = self
                        .history_store
                        .query(HistoryQuery {
                            device_id: pt.device_id.clone(),
                            point_id: pt.point_id.clone(),
                            start_ms,
                            end_ms,
                            max_results: None,
                        })
                        .await
                        .map_err(|e| ReportError::InvalidConfig(e.to_string()))?;

                    if result.samples.is_empty() {
                        continue;
                    }

                    let count = result.samples.len();
                    let sum: f64 = result.samples.iter().map(|s| s.value).sum();
                    let min = result
                        .samples
                        .iter()
                        .map(|s| s.value)
                        .fold(f64::INFINITY, f64::min);
                    let max = result
                        .samples
                        .iter()
                        .map(|s| s.value)
                        .fold(f64::NEG_INFINITY, f64::max);
                    let avg = sum / count as f64;

                    rows.push(SummaryRow {
                        label: pt
                            .label
                            .clone()
                            .unwrap_or_else(|| format!("{}/{}", pt.device_id, pt.point_id)),
                        device_id: pt.device_id.clone(),
                        point_id: pt.point_id.clone(),
                        min,
                        max,
                        avg,
                        sample_count: count,
                    });
                }
                Ok(RenderedSection::HistorySummary {
                    title: title.to_string(),
                    rows,
                })
            }

            SectionType::AlarmSummary => {
                // If points are resolved, query per-device; otherwise query all
                let events = self
                    .query_alarm_events(points, start_ms, end_ms, 10_000)
                    .await?;

                let total = events.len();
                let mut sev_counts: std::collections::HashMap<String, usize> =
                    std::collections::HashMap::new();
                let mut type_counts: std::collections::HashMap<String, usize> =
                    std::collections::HashMap::new();

                for ev in &events {
                    *sev_counts
                        .entry(ev.severity.label().to_string())
                        .or_default() += 1;
                    // Use to_state as event type indicator
                    *type_counts.entry(ev.to_state.clone()).or_default() += 1;
                }

                let mut by_severity: Vec<SeverityCount> = sev_counts
                    .into_iter()
                    .map(|(name, count)| SeverityCount { name, count })
                    .collect();
                by_severity.sort_by(|a, b| b.count.cmp(&a.count));

                let mut by_type: Vec<TypeCount> = type_counts
                    .into_iter()
                    .map(|(name, count)| TypeCount { name, count })
                    .collect();
                by_type.sort_by(|a, b| b.count.cmp(&a.count));

                Ok(RenderedSection::AlarmSummary {
                    title: title.to_string(),
                    by_severity,
                    by_type,
                    total,
                })
            }

            SectionType::AlarmList => {
                let events = self
                    .query_alarm_events(points, start_ms, end_ms, 500)
                    .await?;

                let rows: Vec<AlarmEventRow> = events
                    .into_iter()
                    .map(|ev| AlarmEventRow {
                        timestamp_ms: ev.timestamp_ms,
                        device_id: ev.device_id,
                        point_id: ev.point_id,
                        severity: ev.severity.label().to_string(),
                        from_state: ev.from_state,
                        to_state: ev.to_state,
                        value: ev.value,
                    })
                    .collect();

                Ok(RenderedSection::AlarmList {
                    title: title.to_string(),
                    events: rows,
                })
            }

            SectionType::CurrentValues => {
                let mut rows = Vec::new();
                for pt in points {
                    let key = PointKey {
                        device_instance_id: pt.device_id.clone(),
                        point_id: pt.point_id.clone(),
                    };
                    if let Some(tv) = self.point_store.get(&key) {
                        rows.push(CurrentValueRow {
                            label: pt
                                .label
                                .clone()
                                .unwrap_or_else(|| format!("{}/{}", pt.device_id, pt.point_id)),
                            device_id: pt.device_id.clone(),
                            point_id: pt.point_id.clone(),
                            value: tv.value.as_f64(),
                            status: format!("{:?}", tv.status),
                        });
                    }
                }
                Ok(RenderedSection::CurrentValues {
                    title: title.to_string(),
                    rows,
                })
            }

            SectionType::EnergyConsumption => {
                let mut rows = Vec::new();
                for pt in points {
                    let result = self
                        .history_store
                        .query(HistoryQuery {
                            device_id: pt.device_id.clone(),
                            point_id: pt.point_id.clone(),
                            start_ms,
                            end_ms,
                            max_results: Some(0),
                        })
                        .await
                        .map_err(|e| ReportError::InvalidConfig(e.to_string()))?;

                    if result.samples.is_empty() {
                        continue;
                    }

                    let kwh = crate::energy::consumption::integrate_power(&result.samples, None);
                    let (peak_kw, _peak_ts) =
                        crate::energy::demand::peak_demand(&result.samples, 15).unwrap_or((0.0, 0));
                    let hours = (end_ms - start_ms) as f64 / 3_600_000.0;
                    let avg_kw = if hours > 0.0 { kwh / hours } else { 0.0 };

                    rows.push(renderer::EnergyRow {
                        label: pt
                            .label
                            .clone()
                            .unwrap_or_else(|| format!("{}/{}", pt.device_id, pt.point_id)),
                        consumption_kwh: kwh,
                        peak_kw,
                        avg_kw,
                        cost: 0.0, // Cost requires meter rate assignment — not available at point level
                    });
                }
                Ok(RenderedSection::EnergyConsumption {
                    title: title.to_string(),
                    rows,
                })
            }

            SectionType::DemandSummary => {
                let mut rows = Vec::new();
                for pt in points {
                    let result = self
                        .history_store
                        .query(HistoryQuery {
                            device_id: pt.device_id.clone(),
                            point_id: pt.point_id.clone(),
                            start_ms,
                            end_ms,
                            max_results: Some(0),
                        })
                        .await
                        .map_err(|e| ReportError::InvalidConfig(e.to_string()))?;

                    if result.samples.is_empty() {
                        continue;
                    }

                    let kwh = crate::energy::consumption::integrate_power(&result.samples, None);
                    let (peak_kw, peak_ts) =
                        crate::energy::demand::peak_demand(&result.samples, 15).unwrap_or((0.0, 0));
                    let hours = (end_ms - start_ms) as f64 / 3_600_000.0;
                    let lf = crate::energy::demand::load_factor(kwh, peak_kw, hours);

                    rows.push(renderer::DemandRow {
                        label: pt
                            .label
                            .clone()
                            .unwrap_or_else(|| format!("{}/{}", pt.device_id, pt.point_id)),
                        peak_kw,
                        peak_time_ms: peak_ts,
                        load_factor: lf,
                    });
                }
                Ok(RenderedSection::DemandSummary {
                    title: title.to_string(),
                    rows,
                })
            }

            SectionType::RuntimeSummary => {
                let mut rows = Vec::new();
                let duration_hours = (end_ms - start_ms) as f64 / 3_600_000.0;

                for pt in points {
                    let result = self
                        .history_store
                        .query(HistoryQuery {
                            device_id: pt.device_id.clone(),
                            point_id: pt.point_id.clone(),
                            start_ms,
                            end_ms,
                            max_results: None,
                        })
                        .await
                        .map_err(|e| ReportError::InvalidConfig(e.to_string()))?;

                    if result.samples.is_empty() {
                        continue;
                    }

                    // Compute on-time: count samples where value > 0.5 (binary ON)
                    let on_count = result.samples.iter().filter(|s| s.value > 0.5).count();
                    let total_count = result.samples.len();
                    let on_percent = if total_count > 0 {
                        (on_count as f64 / total_count as f64) * 100.0
                    } else {
                        0.0
                    };
                    let on_hours = duration_hours * on_percent / 100.0;

                    rows.push(RuntimeRow {
                        label: pt
                            .label
                            .clone()
                            .unwrap_or_else(|| format!("{}/{}", pt.device_id, pt.point_id)),
                        device_id: pt.device_id.clone(),
                        point_id: pt.point_id.clone(),
                        on_percent,
                        total_hours: duration_hours,
                        on_hours,
                    });
                }
                Ok(RenderedSection::RuntimeSummary {
                    title: title.to_string(),
                    rows,
                })
            }
        }
    }

    /// Query alarm events, optionally filtered by resolved points.
    /// If points is empty, queries all alarms in the time range.
    /// If points is non-empty, queries per unique device_id and filters by point_id.
    async fn query_alarm_events(
        &self,
        points: &[PointRef],
        start_ms: i64,
        end_ms: i64,
        limit: i64,
    ) -> Result<Vec<crate::store::alarm_store::AlarmEvent>, ReportError> {
        if points.is_empty() {
            // No point filter — query all alarms
            return self
                .alarm_store
                .query_history(AlarmHistoryQuery {
                    start_ms: Some(start_ms),
                    end_ms: Some(end_ms),
                    limit: Some(limit),
                    ..Default::default()
                })
                .await
                .map_err(|e| ReportError::InvalidConfig(e.to_string()));
        }

        // Collect unique device IDs from resolved points
        let device_ids: std::collections::HashSet<&str> =
            points.iter().map(|p| p.device_id.as_str()).collect();
        let point_ids: std::collections::HashSet<(&str, &str)> = points
            .iter()
            .map(|p| (p.device_id.as_str(), p.point_id.as_str()))
            .collect();

        let mut all_events = Vec::new();
        for device_id in device_ids {
            let events = self
                .alarm_store
                .query_history(AlarmHistoryQuery {
                    device_id: Some(device_id.to_string()),
                    start_ms: Some(start_ms),
                    end_ms: Some(end_ms),
                    limit: Some(limit),
                    ..Default::default()
                })
                .await
                .map_err(|e| ReportError::InvalidConfig(e.to_string()))?;

            // Filter to only points in the selector
            for ev in events {
                if point_ids.contains(&(ev.device_id.as_str(), ev.point_id.as_str())) {
                    all_events.push(ev);
                }
            }
        }

        // Sort by timestamp and cap at limit
        all_events.sort_by_key(|e| e.timestamp_ms);
        all_events.truncate(limit as usize);
        Ok(all_events)
    }

    /// Run a report end-to-end: insert execution record, generate HTML, update result.
    /// Single entry point used by scheduler, API handler, and GUI.
    /// Returns (execution_id, status) so callers can log audit / show UI.
    pub async fn run_report(
        &self,
        report_store: &ReportStore,
        report_id: i64,
        schedule_id: Option<i64>,
        triggered_by: &str,
    ) -> Result<(i64, ExecutionStatus), ReportError> {
        let definition = report_store.get_definition(report_id).await?;

        let now = now_ms();
        let exec_id = report_store
            .insert_execution(report_id, schedule_id, triggered_by, now)
            .await?;

        let (status, html, error) = match self.generate(&definition).await {
            Ok(html) => (ExecutionStatus::Completed, Some(html), None),
            Err(e) => (ExecutionStatus::Failed, None, Some(e.to_string())),
        };

        let completed = now_ms();
        let _ = report_store
            .update_execution(exec_id, status.clone(), Some(completed), html, error, None)
            .await;

        Ok((exec_id, status))
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Split a node ID like "device_id/point_id" into (device_id, point_id).
/// If no "/" found, returns (id, "").
fn split_node_id(id: &str) -> (String, String) {
    match id.find('/') {
        Some(pos) => (id[..pos].to_string(), id[pos + 1..].to_string()),
        None => (id.to_string(), String::new()),
    }
}
