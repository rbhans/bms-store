use serde::{Deserialize, Serialize};

use crate::store::history_store::{HistoryQuery, HistoryStore};

use super::consumption::integrate_power;
use super::cost::{calculate_cost, RateConfig};
use super::demand::peak_demand;

/// A computed rollup for a meter over a period (daily or monthly).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnergyRollup {
    pub meter_id: i64,
    pub period_type: String,
    pub period_start_ms: i64,
    pub consumption_kwh: f64,
    pub peak_demand_kw: f64,
    pub peak_demand_ms: i64,
    pub avg_kw: f64,
    pub cost: f64,
    pub hdd: f64,
    pub cdd: f64,
}

/// Compute a daily rollup for a single meter.
///
/// Queries the history store for the meter's power point, integrates consumption,
/// finds peak demand, and optionally calculates cost and degree-days.
pub async fn compute_daily_rollup(
    meter_node_id: &str,
    meter_id: i64,
    day_start_ms: i64,
    history: &HistoryStore,
    rate: Option<(&RateConfig, &str)>,
    hdd: f64,
    cdd: f64,
) -> EnergyRollup {
    let day_end_ms = day_start_ms + 86_400_000;

    // Parse device_id and point_id from node_id ("device/point").
    let (device_id, point_id) = split_node_id(meter_node_id);

    let result = history
        .query(HistoryQuery {
            device_id: device_id.to_string(),
            point_id: point_id.to_string(),
            start_ms: day_start_ms,
            end_ms: day_end_ms,
            max_results: Some(0), // uncapped
        })
        .await;

    let samples = match result {
        Ok(r) => r.samples,
        Err(_) => Vec::new(),
    };

    let consumption_kwh = integrate_power(&samples, None);
    let (peak_kw, peak_ts) = peak_demand(&samples, 15).unwrap_or((0.0, day_start_ms));
    let hours = 24.0;
    let avg_kw = if hours > 0.0 {
        consumption_kwh / hours
    } else {
        0.0
    };

    let cost = rate
        .map(|(r, currency)| calculate_cost(consumption_kwh, peak_kw, r, currency).total)
        .unwrap_or(0.0);

    EnergyRollup {
        meter_id,
        period_type: "daily".to_string(),
        period_start_ms: day_start_ms,
        consumption_kwh,
        peak_demand_kw: peak_kw,
        peak_demand_ms: peak_ts,
        avg_kw,
        cost,
        hdd,
        cdd,
    }
}

/// Aggregate daily rollups into a monthly rollup.
pub fn compute_monthly_rollup(
    meter_id: i64,
    month_start_ms: i64,
    daily_rollups: &[EnergyRollup],
) -> EnergyRollup {
    let consumption_kwh: f64 = daily_rollups.iter().map(|r| r.consumption_kwh).sum();
    let (peak_kw, peak_ts) = daily_rollups
        .iter()
        .max_by(|a, b| a.peak_demand_kw.partial_cmp(&b.peak_demand_kw).unwrap())
        .map(|r| (r.peak_demand_kw, r.peak_demand_ms))
        .unwrap_or((0.0, month_start_ms));
    let total_cost: f64 = daily_rollups.iter().map(|r| r.cost).sum();
    let total_hdd: f64 = daily_rollups.iter().map(|r| r.hdd).sum();
    let total_cdd: f64 = daily_rollups.iter().map(|r| r.cdd).sum();
    let total_hours = daily_rollups.len() as f64 * 24.0;
    let avg_kw = if total_hours > 0.0 {
        consumption_kwh / total_hours
    } else {
        0.0
    };

    EnergyRollup {
        meter_id,
        period_type: "monthly".to_string(),
        period_start_ms: month_start_ms,
        consumption_kwh,
        peak_demand_kw: peak_kw,
        peak_demand_ms: peak_ts,
        avg_kw,
        cost: total_cost,
        hdd: total_hdd,
        cdd: total_cdd,
    }
}

/// Split a node_id like "1234/analog-input-1" into ("1234", "analog-input-1").
/// If there's no slash, returns (node_id, "").
fn split_node_id(node_id: &str) -> (&str, &str) {
    match node_id.find('/') {
        Some(pos) => (&node_id[..pos], &node_id[pos + 1..]),
        None => (node_id, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_node_id_with_slash() {
        let (d, p) = split_node_id("1234/analog-input-1");
        assert_eq!(d, "1234");
        assert_eq!(p, "analog-input-1");
    }

    #[test]
    fn split_node_id_without_slash() {
        let (d, p) = split_node_id("1234");
        assert_eq!(d, "1234");
        assert_eq!(p, "");
    }

    #[test]
    fn monthly_rollup_aggregation() {
        let dailies = vec![
            EnergyRollup {
                meter_id: 1,
                period_type: "daily".into(),
                period_start_ms: 0,
                consumption_kwh: 100.0,
                peak_demand_kw: 20.0,
                peak_demand_ms: 1000,
                avg_kw: 4.17,
                cost: 12.0,
                hdd: 5.0,
                cdd: 0.0,
            },
            EnergyRollup {
                meter_id: 1,
                period_type: "daily".into(),
                period_start_ms: 86_400_000,
                consumption_kwh: 150.0,
                peak_demand_kw: 30.0,
                peak_demand_ms: 90_000_000,
                avg_kw: 6.25,
                cost: 18.0,
                hdd: 3.0,
                cdd: 0.0,
            },
        ];
        let monthly = compute_monthly_rollup(1, 0, &dailies);
        assert!((monthly.consumption_kwh - 250.0).abs() < 0.01);
        assert!((monthly.peak_demand_kw - 30.0).abs() < 0.01);
        assert!((monthly.cost - 30.0).abs() < 0.01);
        assert!((monthly.hdd - 8.0).abs() < 0.01);
    }
}
