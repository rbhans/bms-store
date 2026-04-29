use serde::{Deserialize, Serialize};

/// Breakdown of energy cost components.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CostBreakdown {
    pub energy_charge: f64,
    pub demand_charge: f64,
    pub total: f64,
    pub currency: String,
}

/// Utility rate configuration — stored as JSON in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RateConfig {
    Flat {
        energy_rate: f64, // $/kWh
        demand_rate: f64, // $/kW (0 if no demand charge)
    },
    Tou {
        periods: Vec<TouPeriod>,
        demand_rate: f64, // $/kW
    },
    Tiered {
        tiers: Vec<UsageTier>,
        demand_rate: f64, // $/kW
    },
    Demand {
        energy_rate: f64, // $/kWh
        demand_tiers: Vec<DemandTier>,
        ratchet_pct: f64, // % of past 12-month peak that sets minimum billing demand
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TouPeriod {
    pub name: String,           // "on_peak", "off_peak", "mid_peak"
    pub rate: f64,              // $/kWh
    pub weekday_start_hour: u8, // 0-23
    pub weekday_end_hour: u8,   // 0-23 (exclusive)
    pub weekend: bool,          // true if this period applies on weekends
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageTier {
    pub up_to_kwh: f64, // Tier ceiling (f64::MAX for final tier)
    pub rate: f64,      // $/kWh
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemandTier {
    pub up_to_kw: f64, // Tier ceiling
    pub rate: f64,     // $/kW
}

/// Calculate cost for a billing period.
///
/// - `consumption_kwh`: total energy consumed in the period.
/// - `peak_kw`: peak demand in the period.
/// - `rate`: the utility rate configuration.
/// - `currency`: e.g. "USD".
pub fn calculate_cost(
    consumption_kwh: f64,
    peak_kw: f64,
    rate: &RateConfig,
    currency: &str,
) -> CostBreakdown {
    let (energy_charge, demand_charge) = match rate {
        RateConfig::Flat {
            energy_rate,
            demand_rate,
        } => (consumption_kwh * energy_rate, peak_kw * demand_rate),

        RateConfig::Tou {
            periods,
            demand_rate,
        } => {
            // Without hourly consumption data, compute a weighted average rate
            // from the configured TOU periods based on their hour coverage.
            // For accurate per-period breakdown, use calculate_tou_cost() with hourly data.
            let avg_rate = weighted_average_tou_rate(periods);
            (consumption_kwh * avg_rate, peak_kw * demand_rate)
        }

        RateConfig::Tiered { tiers, demand_rate } => {
            let energy = calculate_tiered(consumption_kwh, tiers);
            (energy, peak_kw * demand_rate)
        }

        RateConfig::Demand {
            energy_rate,
            demand_tiers,
            ratchet_pct: _,
        } => {
            let energy = consumption_kwh * energy_rate;
            let demand = calculate_demand_tiered(peak_kw, demand_tiers);
            (energy, demand)
        }
    };

    CostBreakdown {
        energy_charge,
        demand_charge,
        total: energy_charge + demand_charge,
        currency: currency.to_string(),
    }
}

/// Calculate TOU energy cost with hourly consumption buckets.
///
/// `hourly_kwh`: 24 entries, one per hour (index 0 = midnight).
/// `is_weekend`: whether this day is a weekend.
pub fn calculate_tou_cost(hourly_kwh: &[f64; 24], is_weekend: bool, periods: &[TouPeriod]) -> f64 {
    let mut total = 0.0;
    for (hour, &kwh) in hourly_kwh.iter().enumerate() {
        let h = hour as u8;
        // Find matching period for this hour.
        let rate = periods
            .iter()
            .find(|p| {
                if is_weekend && !p.weekend {
                    return false;
                }
                if !is_weekend && p.weekend {
                    return false;
                }
                h >= p.weekday_start_hour && h < p.weekday_end_hour
            })
            .map(|p| p.rate)
            .unwrap_or(0.0);
        total += kwh * rate;
    }
    total
}

/// Compute a weighted average $/kWh from TOU periods based on hour coverage.
/// Weekday periods are weighted 5/7, weekend periods 2/7.
fn weighted_average_tou_rate(periods: &[TouPeriod]) -> f64 {
    if periods.is_empty() {
        return 0.0;
    }
    let mut total_weighted_rate = 0.0;
    let mut total_hours = 0.0;
    for p in periods {
        let hours = if p.weekday_end_hour > p.weekday_start_hour {
            (p.weekday_end_hour - p.weekday_start_hour) as f64
        } else {
            0.0
        };
        let weight = if p.weekend { 2.0 / 7.0 } else { 5.0 / 7.0 };
        total_weighted_rate += p.rate * hours * weight;
        total_hours += hours * weight;
    }
    if total_hours > 0.0 {
        total_weighted_rate / total_hours
    } else {
        // Fallback: simple average of period rates.
        periods.iter().map(|p| p.rate).sum::<f64>() / periods.len() as f64
    }
}

fn calculate_tiered(kwh: f64, tiers: &[UsageTier]) -> f64 {
    let mut remaining = kwh;
    let mut cost = 0.0;
    let mut prev_ceiling = 0.0;

    for tier in tiers {
        if remaining <= 0.0 {
            break;
        }
        let tier_width = tier.up_to_kwh - prev_ceiling;
        let tier_usage = remaining.min(tier_width);
        cost += tier_usage * tier.rate;
        remaining -= tier_usage;
        prev_ceiling = tier.up_to_kwh;
    }
    cost
}

fn calculate_demand_tiered(peak_kw: f64, tiers: &[DemandTier]) -> f64 {
    let mut remaining = peak_kw;
    let mut cost = 0.0;
    let mut prev_ceiling = 0.0;

    for tier in tiers {
        if remaining <= 0.0 {
            break;
        }
        let tier_width = tier.up_to_kw - prev_ceiling;
        let tier_usage = remaining.min(tier_width);
        cost += tier_usage * tier.rate;
        remaining -= tier_usage;
        prev_ceiling = tier.up_to_kw;
    }
    cost
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_rate_cost() {
        let rate = RateConfig::Flat {
            energy_rate: 0.12,
            demand_rate: 10.0,
        };
        let cost = calculate_cost(1000.0, 50.0, &rate, "USD");
        assert!((cost.energy_charge - 120.0).abs() < 0.01);
        assert!((cost.demand_charge - 500.0).abs() < 0.01);
        assert!((cost.total - 620.0).abs() < 0.01);
    }

    #[test]
    fn tiered_rate_cost() {
        let rate = RateConfig::Tiered {
            tiers: vec![
                UsageTier {
                    up_to_kwh: 500.0,
                    rate: 0.08,
                },
                UsageTier {
                    up_to_kwh: 1000.0,
                    rate: 0.12,
                },
                UsageTier {
                    up_to_kwh: f64::MAX,
                    rate: 0.18,
                },
            ],
            demand_rate: 0.0,
        };
        // 800 kWh: 500 * 0.08 + 300 * 0.12 = 40 + 36 = 76.
        let cost = calculate_cost(800.0, 0.0, &rate, "USD");
        assert!((cost.energy_charge - 76.0).abs() < 0.01);
    }

    #[test]
    fn demand_tiered_cost() {
        let rate = RateConfig::Demand {
            energy_rate: 0.10,
            demand_tiers: vec![
                DemandTier {
                    up_to_kw: 100.0,
                    rate: 8.0,
                },
                DemandTier {
                    up_to_kw: f64::MAX,
                    rate: 12.0,
                },
            ],
            ratchet_pct: 0.0,
        };
        // 1000 kWh, 150 kW peak.
        // Energy: 1000 * 0.10 = 100.
        // Demand: 100 * 8 + 50 * 12 = 800 + 600 = 1400.
        let cost = calculate_cost(1000.0, 150.0, &rate, "USD");
        assert!((cost.energy_charge - 100.0).abs() < 0.01);
        assert!((cost.demand_charge - 1400.0).abs() < 0.01);
    }

    #[test]
    fn tou_rate_uses_weighted_average() {
        let rate = RateConfig::Tou {
            periods: vec![
                TouPeriod {
                    name: "on_peak".into(),
                    rate: 0.20,
                    weekday_start_hour: 12,
                    weekday_end_hour: 20, // 8 hours
                    weekend: false,
                },
                TouPeriod {
                    name: "off_peak".into(),
                    rate: 0.08,
                    weekday_start_hour: 0,
                    weekday_end_hour: 12, // 12 hours
                    weekend: false,
                },
            ],
            demand_rate: 0.0,
        };
        let cost = calculate_cost(1000.0, 0.0, &rate, "USD");
        // Weighted avg: (0.20*8 + 0.08*12) / 20 * (5/7 each, same weight factor)
        // = (1.60 + 0.96) / 20 = 0.128
        // Energy: 1000 * 0.128 = 128.0
        assert!((cost.energy_charge - 128.0).abs() < 1.0);
        assert!(cost.energy_charge > 0.0); // Not the old hardcoded 0.10 * 1000 = 100
    }
}
