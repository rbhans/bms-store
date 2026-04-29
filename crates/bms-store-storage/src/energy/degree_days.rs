/// Compute heating degree days from hourly temperature readings.
///
/// HDD = sum of max(0, base_temp - hourly_temp) / 24.
/// Each entry is (timestamp_ms, temp_fahrenheit_or_celsius).
pub fn heating_degree_days(hourly_temps: &[(i64, f64)], base_temp: f64) -> f64 {
    if hourly_temps.is_empty() {
        return 0.0;
    }
    let sum: f64 = hourly_temps
        .iter()
        .map(|(_, t)| (base_temp - t).max(0.0))
        .sum();
    sum / 24.0
}

/// Compute cooling degree days from hourly temperature readings.
///
/// CDD = sum of max(0, hourly_temp - base_temp) / 24.
pub fn cooling_degree_days(hourly_temps: &[(i64, f64)], base_temp: f64) -> f64 {
    if hourly_temps.is_empty() {
        return 0.0;
    }
    let sum: f64 = hourly_temps
        .iter()
        .map(|(_, t)| (t - base_temp).max(0.0))
        .sum();
    sum / 24.0
}

/// Weather-normalize consumption using degree-day ratio.
///
/// normalized = consumption * (baseline_dd / actual_dd).
/// If actual_dd is zero (no heating/cooling needed), returns consumption as-is.
pub fn weather_normalize(consumption: f64, actual_dd: f64, baseline_dd: f64) -> f64 {
    if actual_dd <= 0.001 {
        return consumption;
    }
    consumption * (baseline_dd / actual_dd)
}

/// Linear regression: y = slope * x + intercept.
///
/// Input: pairs of (x, y), e.g., (degree_days, consumption_kwh).
/// Returns (slope, intercept). Used for baseline model building.
///
/// Returns (0, 0) if fewer than 2 data points.
pub fn linear_regression(data: &[(f64, f64)]) -> (f64, f64) {
    let n = data.len() as f64;
    if n < 2.0 {
        return (0.0, 0.0);
    }
    let sum_x: f64 = data.iter().map(|(x, _)| x).sum();
    let sum_y: f64 = data.iter().map(|(_, y)| y).sum();
    let sum_xy: f64 = data.iter().map(|(x, y)| x * y).sum();
    let sum_xx: f64 = data.iter().map(|(x, _)| x * x).sum();

    let denom = n * sum_xx - sum_x * sum_x;
    if denom.abs() < 1e-12 {
        return (0.0, sum_y / n);
    }
    let slope = (n * sum_xy - sum_x * sum_y) / denom;
    let intercept = (sum_y - slope * sum_x) / n;
    (slope, intercept)
}

/// Predict consumption from degree days using a linear model.
pub fn predict(dd: f64, slope: f64, intercept: f64) -> f64 {
    (slope * dd + intercept).max(0.0)
}

/// Coefficient of Variation of the Root Mean Square Error (CV-RMSE).
///
/// Used in IPMVP M&V to evaluate baseline model accuracy.
/// CV-RMSE < 25% (monthly) or < 30% (daily) is acceptable.
pub fn cv_rmse(actuals: &[f64], predictions: &[f64]) -> f64 {
    if actuals.is_empty() || actuals.len() != predictions.len() {
        return 0.0;
    }
    let n = actuals.len() as f64;
    let mean_actual: f64 = actuals.iter().sum::<f64>() / n;
    if mean_actual.abs() < 1e-12 {
        return 0.0;
    }
    let mse: f64 = actuals
        .iter()
        .zip(predictions.iter())
        .map(|(a, p)| (a - p).powi(2))
        .sum::<f64>()
        / n;
    let rmse = mse.sqrt();
    (rmse / mean_actual) * 100.0
}

/// Normalized Mean Bias Error (NMBE) as a percentage.
///
/// Positive NMBE means the model over-predicts; negative means under-predicts.
/// NMBE within +/-10% is acceptable per IPMVP.
pub fn nmbe(actuals: &[f64], predictions: &[f64]) -> f64 {
    if actuals.is_empty() || actuals.len() != predictions.len() {
        return 0.0;
    }
    let n = actuals.len() as f64;
    let mean_actual: f64 = actuals.iter().sum::<f64>() / n;
    if mean_actual.abs() < 1e-12 {
        return 0.0;
    }
    let sum_error: f64 = predictions
        .iter()
        .zip(actuals.iter())
        .map(|(p, a)| p - a)
        .sum();
    (sum_error / (n * mean_actual)) * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hdd_below_base() {
        // 24 hours at 55°F, base 65°F → HDD = 10.
        let temps: Vec<_> = (0..24).map(|i| (i as i64 * 3_600_000, 55.0)).collect();
        let hdd = heating_degree_days(&temps, 65.0);
        assert!((hdd - 10.0).abs() < 0.01);
    }

    #[test]
    fn hdd_above_base_is_zero() {
        let temps: Vec<_> = (0..24).map(|i| (i as i64 * 3_600_000, 75.0)).collect();
        let hdd = heating_degree_days(&temps, 65.0);
        assert!(hdd.abs() < 0.01);
    }

    #[test]
    fn cdd_above_base() {
        // 24 hours at 85°F, base 65°F → CDD = 20.
        let temps: Vec<_> = (0..24).map(|i| (i as i64 * 3_600_000, 85.0)).collect();
        let cdd = cooling_degree_days(&temps, 65.0);
        assert!((cdd - 20.0).abs() < 0.01);
    }

    #[test]
    fn regression_perfect_fit() {
        // y = 2x + 10.
        let data = vec![(1.0, 12.0), (2.0, 14.0), (3.0, 16.0), (4.0, 18.0)];
        let (slope, intercept) = linear_regression(&data);
        assert!((slope - 2.0).abs() < 0.001);
        assert!((intercept - 10.0).abs() < 0.001);
    }

    #[test]
    fn weather_normalize_basic() {
        // Consumed 1000 kWh at 30 HDD, baseline is 25 HDD.
        let normalized = weather_normalize(1000.0, 30.0, 25.0);
        assert!((normalized - 833.33).abs() < 1.0);
    }

    #[test]
    fn cv_rmse_perfect() {
        let actual = vec![100.0, 200.0, 300.0];
        let predicted = vec![100.0, 200.0, 300.0];
        assert!(cv_rmse(&actual, &predicted) < 0.01);
    }

    #[test]
    fn nmbe_over_prediction() {
        let actual = vec![100.0, 100.0, 100.0];
        let predicted = vec![110.0, 110.0, 110.0];
        let bias = nmbe(&actual, &predicted);
        assert!((bias - 10.0).abs() < 0.01);
    }
}
