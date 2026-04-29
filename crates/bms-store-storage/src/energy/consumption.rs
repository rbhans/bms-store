use crate::store::history_store::HistorySample;

/// Integrate power samples (kW) to energy (kWh) using the trapezoidal rule.
///
/// Samples must be sorted by timestamp. Gaps larger than `max_gap_ms`
/// (default 15 minutes) are skipped to avoid phantom energy from missing data.
pub fn integrate_power(samples: &[HistorySample], max_gap_ms: Option<i64>) -> f64 {
    let max_gap = max_gap_ms.unwrap_or(15 * 60 * 1000);
    if samples.len() < 2 {
        return 0.0;
    }
    let mut kwh = 0.0;
    for pair in samples.windows(2) {
        let dt_ms = pair[1].timestamp_ms - pair[0].timestamp_ms;
        if dt_ms <= 0 || dt_ms > max_gap {
            continue;
        }
        let dt_hours = dt_ms as f64 / 3_600_000.0;
        let avg_kw = (pair[0].value + pair[1].value) / 2.0;
        if avg_kw >= 0.0 {
            kwh += avg_kw * dt_hours;
        }
    }
    kwh
}

/// Round a timestamp down to the start of its UTC day (midnight).
pub fn day_start_ms(ts_ms: i64) -> i64 {
    let secs = ts_ms / 1000;
    let day_secs = secs - (secs % 86400);
    day_secs * 1000
}

/// Round a timestamp down to the start of its UTC month.
pub fn month_start_ms(ts_ms: i64) -> i64 {
    // Simple: compute year/month from days since epoch.
    let days = ts_ms / 86_400_000;
    // Approximate: use chrono-free calculation.
    // Days since 1970-01-01.
    let (y, m, _d) = days_to_ymd(days);
    ymd_to_ms(y, m, 1)
}

/// Convert days since epoch to (year, month, day).
fn days_to_ymd(mut days: i64) -> (i32, u32, u32) {
    // Civil days algorithm (Howard Hinnant).
    days += 719468;
    let era = if days >= 0 { days } else { days - 146096 } / 146097;
    let doe = (days - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

/// Convert (year, month, day) to milliseconds since epoch.
fn ymd_to_ms(y: i32, m: u32, d: u32) -> i64 {
    // Inverse of days_to_ymd.
    let y = if m <= 2 { y as i64 - 1 } else { y as i64 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let m_adj = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * m_adj + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe as i64 - 719468;
    days * 86_400_000
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
    fn integrate_constant_power() {
        // 10 kW for exactly 1 hour = 10 kWh.
        // Use minute-resolution samples so no single gap exceeds 15min.
        let samples: Vec<_> = (0..=60).map(|i| sample(i * 60_000, 10.0)).collect();
        let kwh = integrate_power(&samples, None);
        assert!((kwh - 10.0).abs() < 0.001);
    }

    #[test]
    fn integrate_linear_ramp() {
        // 0 kW → 10 kW over 1 hour = 5 kWh (triangle).
        let samples: Vec<_> = (0..=60)
            .map(|i| sample(i * 60_000, i as f64 / 6.0))
            .collect();
        let kwh = integrate_power(&samples, None);
        assert!((kwh - 5.0).abs() < 0.1);
    }

    #[test]
    fn integrate_skips_gaps() {
        // 10 kW for 10min, then a 20-min gap (exceeds 15min default), then 10 kW for 10min.
        let mut samples = Vec::new();
        // First segment: 0–10 min at 1-min intervals.
        for i in 0..=10 {
            samples.push(sample(i * 60_000, 10.0));
        }
        // Gap: 20 minutes (no samples from 10min to 30min).
        // Second segment: 30–40 min at 1-min intervals.
        for i in 30..=40 {
            samples.push(sample(i * 60_000, 10.0));
        }
        let kwh = integrate_power(&samples, None);
        // Two 10-min segments of 10kW each: 2 * (10 * 10/60) = 3.33 kWh.
        assert!((kwh - 3.333).abs() < 0.1);
    }

    #[test]
    fn integrate_empty_and_single() {
        assert_eq!(integrate_power(&[], None), 0.0);
        assert_eq!(integrate_power(&[sample(0, 10.0)], None), 0.0);
    }

    #[test]
    fn day_start_is_midnight() {
        // 2024-01-15 14:30:00 UTC = 1705325400000
        let ts = 1705325400000i64;
        let start = day_start_ms(ts);
        // 2024-01-15 00:00:00 UTC = 1705276800000
        assert_eq!(start, 1705276800000);
    }

    #[test]
    fn month_start_calculation() {
        // 2024-03-15 = some ms → should give 2024-03-01 00:00 UTC.
        let ts = 1710504000000i64; // ~2024-03-15
        let start = month_start_ms(ts);
        // 2024-03-01 00:00:00 UTC = 1709251200000
        assert_eq!(start, 1709251200000);
    }
}
