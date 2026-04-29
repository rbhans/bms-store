use crate::store::history_store::HistorySample;

/// A demand bucket representing average power over a fixed interval.
#[derive(Debug, Clone)]
pub struct DemandBucket {
    pub start_ms: i64,
    pub end_ms: i64,
    pub avg_kw: f64,
    pub max_kw: f64,
    pub sample_count: u32,
}

/// Compute peak demand using a sliding window of `interval_minutes`.
///
/// Returns (peak_kw, peak_timestamp_ms). The peak is the highest
/// time-weighted average power over any window of the given duration.
/// Standard utility billing uses 15-minute intervals.
///
/// Time-weighting: each sample's value is weighted by the duration until
/// the next sample (trapezoidal midpoint), making the result correct for
/// COV or irregularly-spaced data.
pub fn peak_demand(samples: &[HistorySample], interval_minutes: u32) -> Option<(f64, i64)> {
    if samples.len() < 2 {
        return samples.first().map(|s| (s.value, s.timestamp_ms));
    }
    let interval_ms = interval_minutes as i64 * 60_000;
    let mut best_avg = f64::NEG_INFINITY;
    let mut best_ts = samples[0].timestamp_ms;

    // For each possible window start, compute time-weighted average.
    // Use a two-pointer approach: left scans start positions, right tracks window end.
    let mut left = 0usize;
    for right in 1..samples.len() {
        // Advance left so the window fits within interval_ms.
        while left < right && samples[right].timestamp_ms - samples[left].timestamp_ms > interval_ms
        {
            left += 1;
        }
        if left >= right {
            continue;
        }
        // Compute time-weighted average over [left..right].
        let avg = time_weighted_avg(&samples[left..=right]);
        if avg > best_avg {
            best_avg = avg;
            best_ts = samples[left].timestamp_ms;
        }
    }

    if best_avg == f64::NEG_INFINITY {
        None
    } else {
        Some((best_avg, best_ts))
    }
}

/// Compute time-weighted average of samples using trapezoidal rule.
/// Each pair contributes (v0 + v1) / 2 * dt to the integral.
fn time_weighted_avg(samples: &[HistorySample]) -> f64 {
    if samples.len() < 2 {
        return samples.first().map(|s| s.value).unwrap_or(0.0);
    }
    let mut integral = 0.0;
    let mut total_dt = 0.0;
    for pair in samples.windows(2) {
        let dt = (pair[1].timestamp_ms - pair[0].timestamp_ms) as f64;
        if dt > 0.0 {
            integral += (pair[0].value + pair[1].value) / 2.0 * dt;
            total_dt += dt;
        }
    }
    if total_dt > 0.0 {
        integral / total_dt
    } else {
        samples[0].value
    }
}

/// Build a demand profile with fixed-size buckets (typically 15 minutes).
///
/// Each bucket contains the time-weighted average and max kW over that interval.
pub fn demand_profile(samples: &[HistorySample], interval_minutes: u32) -> Vec<DemandBucket> {
    if samples.is_empty() {
        return Vec::new();
    }
    let interval_ms = interval_minutes as i64 * 60_000;
    let start = samples[0].timestamp_ms;
    let end = samples.last().unwrap().timestamp_ms;

    let mut buckets = Vec::new();
    let mut bucket_start = start - (start % interval_ms);
    let mut idx = 0;

    while bucket_start <= end {
        let bucket_end = bucket_start + interval_ms;
        let mut max_kw = f64::NEG_INFINITY;
        let mut count = 0u32;
        let bucket_first = idx;

        while idx < samples.len() && samples[idx].timestamp_ms < bucket_end {
            if samples[idx].timestamp_ms >= bucket_start {
                if samples[idx].value > max_kw {
                    max_kw = samples[idx].value;
                }
                count += 1;
            }
            idx += 1;
        }

        if count > 0 {
            // Time-weighted average for this bucket's samples.
            let bucket_samples = &samples[bucket_first..idx];
            let avg = time_weighted_avg(bucket_samples);
            buckets.push(DemandBucket {
                start_ms: bucket_start,
                end_ms: bucket_end,
                avg_kw: avg,
                max_kw,
                sample_count: count,
            });
        }
        // Rewind to not skip boundary samples.
        while idx > 0 && samples[idx - 1].timestamp_ms >= bucket_end {
            idx -= 1;
        }
        bucket_start = bucket_end;
    }

    buckets
}

/// Load factor: ratio of average demand to peak demand.
///
/// A load factor of 1.0 means perfectly flat consumption.
/// Low load factors indicate spiky demand (expensive for utilities).
pub fn load_factor(consumption_kwh: f64, peak_kw: f64, hours: f64) -> f64 {
    if peak_kw <= 0.0 || hours <= 0.0 {
        return 0.0;
    }
    let avg_kw = consumption_kwh / hours;
    avg_kw / peak_kw
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(ts_ms: i64, value: f64) -> HistorySample {
        HistorySample {
            timestamp_ms: ts_ms,
            value,
        }
    }

    #[test]
    fn peak_demand_constant() {
        let samples: Vec<_> = (0..60).map(|i| sample(i * 60_000, 100.0)).collect();
        let (peak, _ts) = peak_demand(&samples, 15).unwrap();
        assert!((peak - 100.0).abs() < 0.01);
    }

    #[test]
    fn peak_demand_spike() {
        // Mostly 10 kW, brief spike to 100 kW.
        let mut samples: Vec<_> = (0..60).map(|i| sample(i * 60_000, 10.0)).collect();
        // Spike at minutes 20-24.
        for s in samples[20..25].iter_mut() {
            s.value = 100.0;
        }
        let (peak, _ts) = peak_demand(&samples, 15).unwrap();
        // The 15-min window containing the spike should show elevated demand.
        assert!(peak > 10.0);
        assert!(peak < 100.0);
    }

    #[test]
    fn demand_profile_buckets() {
        // 1 hour of data at 1-minute intervals, 15-min buckets → 4 buckets.
        let samples: Vec<_> = (0..60).map(|i| sample(i * 60_000, 50.0)).collect();
        let profile = demand_profile(&samples, 15);
        assert_eq!(profile.len(), 4);
        for bucket in &profile {
            assert!((bucket.avg_kw - 50.0).abs() < 0.01);
        }
    }

    #[test]
    fn load_factor_flat() {
        // 100 kWh over 10 hours with 10 kW peak = LF 1.0.
        assert!((load_factor(100.0, 10.0, 10.0) - 1.0).abs() < 0.001);
    }

    #[test]
    fn load_factor_spiky() {
        // 50 kWh over 10 hours with 100 kW peak = LF 0.05.
        assert!((load_factor(50.0, 100.0, 10.0) - 0.05).abs() < 0.001);
    }
}
