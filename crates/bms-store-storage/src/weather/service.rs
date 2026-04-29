use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{watch, RwLock};
use tokio_util::sync::CancellationToken;

use super::adapter::nws::NwsAdapter;
use super::adapter::open_meteo::OpenMeteoAdapter;
use super::adapter::openweathermap::OpenWeatherMapAdapter;
use super::adapter::visual_crossing::VisualCrossingAdapter;
use super::adapter::weatherapi::WeatherApiAdapter;
use super::adapter::{WeatherAdapter, WeatherError};
use super::config::WeatherConfig;
use super::model::*;

pub struct WeatherService {
    config: Arc<RwLock<WeatherConfig>>,
    client: reqwest::Client,
    latest: Arc<RwLock<Option<WeatherData>>>,
    notify_tx: watch::Sender<u64>,
    notify_rx: watch::Receiver<u64>,
}

impl WeatherService {
    pub fn new(config: WeatherConfig) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("OpenCrate-BMS/0.1")
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .unwrap_or_default();

        let (notify_tx, notify_rx) = watch::channel(0u64);

        Self {
            config: Arc::new(RwLock::new(config)),
            client,
            latest: Arc::new(RwLock::new(None)),
            notify_tx,
            notify_rx,
        }
    }

    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.notify_rx.clone()
    }

    pub async fn latest(&self) -> Option<WeatherData> {
        self.latest.read().await.clone()
    }

    pub async fn config(&self) -> WeatherConfig {
        self.config.read().await.clone()
    }

    pub async fn update_config(&self, config: WeatherConfig) {
        *self.config.write().await = config;
    }

    /// Geocode a zip/postal code to lat/lon + place name via Open-Meteo.
    pub async fn geocode_zip(&self, zip: &str) -> Result<WeatherLocation, String> {
        geocode_zip(&self.client, zip).await
    }

    /// Fetch from all enabled adapters concurrently, aggregate, and store.
    pub async fn fetch_all(&self) {
        let config = self.config.read().await.clone();

        let location = match &config.location {
            Some(loc) => loc.clone(),
            None => return, // No location configured
        };

        let adapters = build_adapters(&config);
        if adapters.is_empty() {
            return;
        }

        let mut raw_results: Vec<RawSourceData> = Vec::new();
        let mut sources_available = Vec::new();
        let mut sources_failed: Vec<(WeatherSource, String)> = Vec::new();

        // Fan out to all adapters concurrently using tokio::spawn
        let mut handles = Vec::new();
        for adapter in adapters {
            let client = self.client.clone();
            let loc = location.clone();
            handles.push(tokio::spawn(async move {
                let source = adapter.source();
                let result = adapter.fetch(&client, &loc).await;
                (source, result)
            }));
        }

        let results = futures::future::join_all(handles).await;

        for join_result in results {
            let Ok((source, result)) = join_result else {
                continue;
            };
            match result {
                Ok(data) => {
                    sources_available.push(source);
                    raw_results.push(data);
                }
                Err(WeatherError::NotAvailable) => {
                    // Silently skip (e.g., NWS outside US)
                }
                Err(e) => {
                    sources_failed.push((source, e.to_string()));
                    tracing::warn!("Weather source {:?} failed: {}", source, e);
                }
            }
        }

        if raw_results.is_empty() {
            return;
        }

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let weather_data = aggregate(
            location,
            &raw_results,
            now_ms,
            sources_available,
            sources_failed,
        );
        *self.latest.write().await = Some(weather_data);

        // Bump version to notify subscribers
        let prev = *self.notify_rx.borrow();
        let _ = self.notify_tx.send(prev + 1);
    }

    /// Force a manual refresh.
    pub async fn force_refresh(&self) {
        self.fetch_all().await;
    }

    /// Start the background refresh loop. Returns a handle that can be aborted.
    pub fn start_refresh_loop(
        self: &Arc<Self>,
        cancel: CancellationToken,
    ) -> tokio::task::JoinHandle<()> {
        let svc = Arc::clone(self);
        tokio::spawn(async move {
            // Initial fetch
            svc.fetch_all().await;

            loop {
                let interval_secs = svc.config.read().await.refresh_interval_secs;
                let duration = std::time::Duration::from_secs(interval_secs);
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = tokio::time::sleep(duration) => {
                        svc.fetch_all().await;
                    }
                }
            }
        })
    }
}

fn build_adapters(config: &WeatherConfig) -> Vec<Box<dyn WeatherAdapter>> {
    let mut adapters: Vec<Box<dyn WeatherAdapter>> = Vec::new();

    for source in &config.enabled_sources {
        match source {
            WeatherSource::OpenMeteo => {
                adapters.push(Box::new(OpenMeteoAdapter));
            }
            WeatherSource::Nws => {
                adapters.push(Box::new(NwsAdapter));
            }
            WeatherSource::OpenWeatherMap => {
                if let Some(ref key) = config.openweathermap_api_key {
                    if !key.is_empty() {
                        adapters.push(Box::new(OpenWeatherMapAdapter {
                            api_key: key.clone(),
                        }));
                    }
                }
            }
            WeatherSource::WeatherApi => {
                if let Some(ref key) = config.weatherapi_api_key {
                    if !key.is_empty() {
                        adapters.push(Box::new(WeatherApiAdapter {
                            api_key: key.clone(),
                        }));
                    }
                }
            }
            WeatherSource::VisualCrossing => {
                if let Some(ref key) = config.visual_crossing_api_key {
                    if !key.is_empty() {
                        adapters.push(Box::new(VisualCrossingAdapter {
                            api_key: key.clone(),
                        }));
                    }
                }
            }
        }
    }

    adapters
}

/// Aggregate multiple raw source data into a single WeatherData.
fn aggregate(
    location: WeatherLocation,
    raw: &[RawSourceData],
    now_ms: i64,
    sources_available: Vec<WeatherSource>,
    sources_failed: Vec<(WeatherSource, String)>,
) -> WeatherData {
    // Aggregate current conditions
    let current = aggregate_current(raw, now_ms);

    // Aggregate hourly: align to nearest hour
    let hourly = aggregate_hourly(raw);

    // Aggregate daily: align to calendar date
    let daily = aggregate_daily(raw);

    WeatherData {
        location,
        current,
        hourly,
        daily,
        last_updated_ms: now_ms,
        sources_available,
        sources_failed,
    }
}

fn aggregate_current(raw: &[RawSourceData], now_ms: i64) -> CurrentConditions {
    let temps: Vec<f64> = raw.iter().map(|r| r.current.temperature_c).collect();
    let humidity: Vec<f64> = raw.iter().map(|r| r.current.humidity_pct).collect();
    let wind: Vec<f64> = raw.iter().map(|r| r.current.wind_speed_kmh).collect();
    let wind_dir: Vec<f64> = raw.iter().map(|r| r.current.wind_direction_deg).collect();
    let pressure: Vec<f64> = raw.iter().map(|r| r.current.pressure_hpa).collect();
    let feels: Vec<f64> = raw.iter().map(|r| r.current.feels_like_c).collect();
    let uv_vals: Vec<f64> = raw.iter().filter_map(|r| r.current.uv_index).collect();

    let conditions: Vec<WeatherCondition> = raw.iter().map(|r| r.current.condition).collect();

    CurrentConditions {
        temperature: AggregatedValue::from_values(&temps)
            .unwrap_or_else(|| AggregatedValue::from_single(0.0)),
        humidity: AggregatedValue::from_values(&humidity)
            .unwrap_or_else(|| AggregatedValue::from_single(0.0)),
        wind_speed: AggregatedValue::from_values(&wind)
            .unwrap_or_else(|| AggregatedValue::from_single(0.0)),
        wind_direction: AggregatedValue::from_values(&wind_dir)
            .unwrap_or_else(|| AggregatedValue::from_single(0.0)),
        pressure: AggregatedValue::from_values(&pressure)
            .unwrap_or_else(|| AggregatedValue::from_single(1013.25)),
        feels_like: AggregatedValue::from_values(&feels)
            .unwrap_or_else(|| AggregatedValue::from_single(0.0)),
        condition: majority_condition(&conditions),
        uv_index: AggregatedValue::from_values(&uv_vals),
        timestamp_ms: now_ms,
    }
}

fn aggregate_hourly(raw: &[RawSourceData]) -> Vec<HourlyForecast> {
    // Collect all hourly data points, keyed by hour (rounded to nearest hour)
    let mut by_hour: HashMap<i64, Vec<&RawHourlyData>> = HashMap::new();

    for source in raw {
        for h in &source.hourly {
            let hour_key = round_to_hour(h.hour_ms);
            by_hour.entry(hour_key).or_default().push(h);
        }
    }

    let mut hours: Vec<i64> = by_hour.keys().copied().collect();
    hours.sort();

    // Limit to 48 hours
    hours.truncate(48);

    hours
        .into_iter()
        .map(|hour_ms| {
            let entries = &by_hour[&hour_ms];
            let temps: Vec<f64> = entries.iter().map(|e| e.temperature_c).collect();
            let humidity: Vec<f64> = entries.iter().map(|e| e.humidity_pct).collect();
            let wind: Vec<f64> = entries.iter().map(|e| e.wind_speed_kmh).collect();
            let precip_prob: Vec<f64> = entries.iter().map(|e| e.precip_probability_pct).collect();
            let precip_mm: Vec<f64> = entries.iter().map(|e| e.precip_mm).collect();
            let conditions: Vec<WeatherCondition> = entries.iter().map(|e| e.condition).collect();

            HourlyForecast {
                hour_ms,
                temperature: AggregatedValue::from_values(&temps)
                    .unwrap_or_else(|| AggregatedValue::from_single(0.0)),
                humidity: AggregatedValue::from_values(&humidity)
                    .unwrap_or_else(|| AggregatedValue::from_single(0.0)),
                wind_speed: AggregatedValue::from_values(&wind)
                    .unwrap_or_else(|| AggregatedValue::from_single(0.0)),
                precip_probability: AggregatedValue::from_values(&precip_prob)
                    .unwrap_or_else(|| AggregatedValue::from_single(0.0)),
                precip_mm: AggregatedValue::from_values(&precip_mm)
                    .unwrap_or_else(|| AggregatedValue::from_single(0.0)),
                condition: majority_condition(&conditions),
            }
        })
        .collect()
}

fn aggregate_daily(raw: &[RawSourceData]) -> Vec<DailyForecast> {
    let mut by_date: HashMap<i64, Vec<&RawDailyData>> = HashMap::new();

    for source in raw {
        for d in &source.daily {
            let date_key = round_to_day(d.date_ms);
            by_date.entry(date_key).or_default().push(d);
        }
    }

    let mut dates: Vec<i64> = by_date.keys().copied().collect();
    dates.sort();
    dates.truncate(7);

    dates
        .into_iter()
        .map(|date_ms| {
            let entries = &by_date[&date_ms];
            let highs: Vec<f64> = entries.iter().map(|e| e.temp_high_c).collect();
            let lows: Vec<f64> = entries.iter().map(|e| e.temp_low_c).collect();
            let humidity: Vec<f64> = entries
                .iter()
                .filter(|e| e.humidity_pct > 0.0)
                .map(|e| e.humidity_pct)
                .collect();
            let wind: Vec<f64> = entries.iter().map(|e| e.wind_speed_max_kmh).collect();
            let precip_prob: Vec<f64> = entries.iter().map(|e| e.precip_probability_pct).collect();
            let precip_mm: Vec<f64> = entries.iter().map(|e| e.precip_mm).collect();
            let conditions: Vec<WeatherCondition> = entries.iter().map(|e| e.condition).collect();

            // Take sunrise/sunset from first source that has them
            let sunrise = entries
                .iter()
                .map(|e| e.sunrise_ms)
                .find(|&ms| ms > 0)
                .unwrap_or(0);
            let sunset = entries
                .iter()
                .map(|e| e.sunset_ms)
                .find(|&ms| ms > 0)
                .unwrap_or(0);

            DailyForecast {
                date_ms,
                temp_high: AggregatedValue::from_values(&highs)
                    .unwrap_or_else(|| AggregatedValue::from_single(0.0)),
                temp_low: AggregatedValue::from_values(&lows)
                    .unwrap_or_else(|| AggregatedValue::from_single(0.0)),
                humidity: AggregatedValue::from_values(&humidity)
                    .unwrap_or_else(|| AggregatedValue::from_single(0.0)),
                wind_speed_max: AggregatedValue::from_values(&wind)
                    .unwrap_or_else(|| AggregatedValue::from_single(0.0)),
                precip_probability: AggregatedValue::from_values(&precip_prob)
                    .unwrap_or_else(|| AggregatedValue::from_single(0.0)),
                precip_mm: AggregatedValue::from_values(&precip_mm)
                    .unwrap_or_else(|| AggregatedValue::from_single(0.0)),
                condition: majority_condition(&conditions),
                sunrise_ms: sunrise,
                sunset_ms: sunset,
            }
        })
        .collect()
}

/// Round millisecond timestamp to nearest hour.
fn round_to_hour(ms: i64) -> i64 {
    let hour_ms = 3_600_000i64;
    ((ms + hour_ms / 2) / hour_ms) * hour_ms
}

/// Round millisecond timestamp to start of day (UTC).
fn round_to_day(ms: i64) -> i64 {
    let day_ms = 86_400_000i64;
    (ms / day_ms) * day_ms
}

/// Majority vote for condition with specificity tie-break.
fn majority_condition(conditions: &[WeatherCondition]) -> WeatherCondition {
    if conditions.is_empty() {
        return WeatherCondition::Unknown;
    }

    let mut counts: HashMap<WeatherCondition, usize> = HashMap::new();
    for c in conditions {
        *counts.entry(*c).or_insert(0) += 1;
    }

    let max_count = *counts.values().max().unwrap();
    let mut candidates: Vec<WeatherCondition> = counts
        .into_iter()
        .filter(|(_, count)| *count == max_count)
        .map(|(cond, _)| cond)
        .collect();

    // Tie-break by specificity (higher = more specific = preferred)
    candidates.sort_by_key(|b| std::cmp::Reverse(b.specificity()));
    candidates[0]
}

/// Geocode a zip/postal code to lat/lon using the Open-Meteo geocoding API.
async fn geocode_zip(client: &reqwest::Client, zip: &str) -> Result<WeatherLocation, String> {
    // Open-Meteo geocoding: free, no key, works with zip codes and city names
    let url = format!(
        "https://geocoding-api.open-meteo.com/v1/search?name={}&count=1&language=en&format=json",
        zip
    );

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Geocoding request failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Geocoding API error: {e}"))?;

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse geocoding response: {e}"))?;

    let results = body["results"]
        .as_array()
        .ok_or_else(|| "No results found for that zip code".to_string())?;

    let first = results
        .first()
        .ok_or_else(|| "No results found for that zip code".to_string())?;

    let lat = first["latitude"]
        .as_f64()
        .ok_or("Missing latitude in response")?;
    let lon = first["longitude"]
        .as_f64()
        .ok_or("Missing longitude in response")?;

    let name = first["name"].as_str().map(String::from);
    let admin1 = first["admin1"].as_str();
    let country = first["country_code"].as_str();

    let display_name = match (name.as_deref(), admin1, country) {
        (Some(n), Some(state), Some(cc)) => Some(format!("{n}, {state}, {cc}")),
        (Some(n), None, Some(cc)) => Some(format!("{n}, {cc}")),
        (Some(n), _, _) => Some(n.to_string()),
        _ => None,
    };

    Ok(WeatherLocation {
        lat,
        lon,
        name: display_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_majority_condition_clear_winner() {
        let conditions = vec![
            WeatherCondition::Rain,
            WeatherCondition::Rain,
            WeatherCondition::Clear,
        ];
        assert_eq!(majority_condition(&conditions), WeatherCondition::Rain);
    }

    #[test]
    fn test_majority_condition_tie_specificity() {
        let conditions = vec![WeatherCondition::Clear, WeatherCondition::Rain];
        // Tie: 1 each. Rain is more specific.
        assert_eq!(majority_condition(&conditions), WeatherCondition::Rain);
    }

    #[test]
    fn test_majority_condition_empty() {
        assert_eq!(majority_condition(&[]), WeatherCondition::Unknown);
    }

    #[test]
    fn test_round_to_hour() {
        // 14:29 should round to 14:00
        let ms = 14 * 3_600_000 + 29 * 60_000;
        assert_eq!(round_to_hour(ms), 14 * 3_600_000);

        // 14:31 should round to 15:00
        let ms = 14 * 3_600_000 + 31 * 60_000;
        assert_eq!(round_to_hour(ms), 15 * 3_600_000);
    }

    #[test]
    fn test_aggregate_current() {
        let raw = vec![
            RawSourceData {
                source: WeatherSource::OpenMeteo,
                current: RawCurrentData {
                    temperature_c: 20.0,
                    humidity_pct: 50.0,
                    wind_speed_kmh: 10.0,
                    wind_direction_deg: 180.0,
                    pressure_hpa: 1013.0,
                    feels_like_c: 19.0,
                    condition: WeatherCondition::Clear,
                    uv_index: Some(5.0),
                    timestamp_ms: 1000,
                },
                hourly: vec![],
                daily: vec![],
            },
            RawSourceData {
                source: WeatherSource::Nws,
                current: RawCurrentData {
                    temperature_c: 22.0,
                    humidity_pct: 55.0,
                    wind_speed_kmh: 12.0,
                    wind_direction_deg: 190.0,
                    pressure_hpa: 1015.0,
                    feels_like_c: 21.0,
                    condition: WeatherCondition::PartlyCloudy,
                    uv_index: None,
                    timestamp_ms: 1000,
                },
                hourly: vec![],
                daily: vec![],
            },
        ];

        let current = aggregate_current(&raw, 1000);
        assert_eq!(current.temperature.source_count, 2);
        assert!((current.temperature.avg - 21.0).abs() < 0.01);
        assert_eq!(current.temperature.min, 20.0);
        assert_eq!(current.temperature.max, 22.0);
    }
}
