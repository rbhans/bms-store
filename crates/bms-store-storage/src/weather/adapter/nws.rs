use async_trait::async_trait;
use serde::Deserialize;

use super::{WeatherAdapter, WeatherError};
use crate::weather::model::*;

pub struct NwsAdapter;

#[async_trait]
impl WeatherAdapter for NwsAdapter {
    fn source(&self) -> WeatherSource {
        WeatherSource::Nws
    }

    fn requires_api_key(&self) -> bool {
        false
    }

    async fn fetch(
        &self,
        client: &reqwest::Client,
        location: &WeatherLocation,
    ) -> Result<RawSourceData, WeatherError> {
        // NWS only covers US (roughly lat 24-50, lon -125 to -66)
        if location.lat < 24.0
            || location.lat > 50.0
            || location.lon < -125.0
            || location.lon > -66.0
        {
            return Err(WeatherError::NotAvailable);
        }

        // Step 1: Get grid coordinates from lat/lon
        let points_url = format!(
            "https://api.weather.gov/points/{:.4},{:.4}",
            location.lat, location.lon
        );
        let points_resp: NwsPointsResponse = client
            .get(&points_url)
            .send()
            .await?
            .error_for_status()
            .map_err(|_| WeatherError::NotAvailable)?
            .json()
            .await?;

        let forecast_url = &points_resp.properties.forecast_hourly;
        let station_url = &points_resp.properties.observation_stations;

        // Step 2: Get latest observation from nearest station
        let stations_resp: NwsStationsResponse = client
            .get(station_url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let current = if let Some(station) = stations_resp.features.first() {
            let obs_url = format!(
                "{}/observations/latest",
                station
                    .properties
                    .station_identifier_url
                    .as_deref()
                    .unwrap_or(&station.id)
            );
            match client.get(&obs_url).send().await {
                Ok(resp) => {
                    let obs: NwsObservationResponse = resp.json().await?;
                    let p = &obs.properties;
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as i64;
                    RawCurrentData {
                        temperature_c: p.temperature.value.unwrap_or(0.0),
                        humidity_pct: p.relative_humidity.value.unwrap_or(0.0),
                        wind_speed_kmh: p.wind_speed.value.unwrap_or(0.0),
                        wind_direction_deg: p.wind_direction.value.unwrap_or(0.0),
                        pressure_hpa: p.barometric_pressure.value.unwrap_or(0.0) / 100.0, // Pa to hPa
                        feels_like_c: p
                            .wind_chill
                            .value
                            .or(p.heat_index.value)
                            .unwrap_or(p.temperature.value.unwrap_or(0.0)),
                        condition: nws_icon_to_condition(p.icon.as_deref().unwrap_or("")),
                        uv_index: None,
                        timestamp_ms: now_ms,
                    }
                }
                Err(_) => default_current(),
            }
        } else {
            default_current()
        };

        // Step 3: Get hourly forecast
        let hourly_resp: NwsForecastResponse = client
            .get(forecast_url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let mut hourly = Vec::new();
        for period in &hourly_resp.properties.periods {
            let hour_ms = parse_nws_time(&period.start_time);
            let temp_c = if period.temperature_unit == "F" {
                (period.temperature - 32.0) * 5.0 / 9.0
            } else {
                period.temperature
            };
            let wind_kmh = parse_wind_speed(&period.wind_speed);
            hourly.push(RawHourlyData {
                hour_ms,
                temperature_c: temp_c,
                humidity_pct: period
                    .relative_humidity
                    .as_ref()
                    .and_then(|v| v.value)
                    .unwrap_or(0.0),
                wind_speed_kmh: wind_kmh,
                precip_probability_pct: period
                    .probability_of_precipitation
                    .as_ref()
                    .and_then(|v| v.value)
                    .unwrap_or(0.0),
                precip_mm: 0.0, // NWS doesn't provide hourly precip amounts
                condition: nws_forecast_to_condition(&period.short_forecast),
            });
            if hourly.len() >= 48 {
                break;
            }
        }

        // NWS doesn't provide a clean daily forecast via this endpoint, so we'll leave daily empty
        // (other sources will contribute daily data)
        let daily = Vec::new();

        Ok(RawSourceData {
            source: WeatherSource::Nws,
            current,
            hourly,
            daily,
        })
    }
}

fn default_current() -> RawCurrentData {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    RawCurrentData {
        temperature_c: 0.0,
        humidity_pct: 0.0,
        wind_speed_kmh: 0.0,
        wind_direction_deg: 0.0,
        pressure_hpa: 1013.25,
        feels_like_c: 0.0,
        condition: WeatherCondition::Unknown,
        uv_index: None,
        timestamp_ms: now_ms,
    }
}

fn parse_wind_speed(s: &str) -> f64 {
    // NWS gives "10 mph" or "5 to 10 mph"
    let parts: Vec<&str> = s.split_whitespace().collect();
    let mph = if parts.len() >= 3 && parts[1] == "to" {
        // Take the higher value
        parts[2].parse::<f64>().unwrap_or(0.0)
    } else {
        parts
            .first()
            .and_then(|p| p.parse::<f64>().ok())
            .unwrap_or(0.0)
    };
    mph * 1.60934 // mph to km/h
}

fn nws_icon_to_condition(icon_url: &str) -> WeatherCondition {
    // NWS icon URLs contain condition keywords like "skc", "few", "rain", etc.
    let lower = icon_url.to_lowercase();
    if lower.contains("tsra") || lower.contains("thunder") {
        WeatherCondition::Thunderstorm
    } else if lower.contains("snow") || lower.contains("blizzard") {
        WeatherCondition::Snow
    } else if lower.contains("rain") || lower.contains("shower") {
        WeatherCondition::Rain
    } else if lower.contains("sleet") || lower.contains("fzra") {
        WeatherCondition::Sleet
    } else if lower.contains("fog") {
        WeatherCondition::Fog
    } else if lower.contains("ovc") {
        WeatherCondition::Cloudy
    } else if lower.contains("bkn") || lower.contains("sct") || lower.contains("few") {
        WeatherCondition::PartlyCloudy
    } else if lower.contains("skc") || lower.contains("clear") {
        WeatherCondition::Clear
    } else if lower.contains("wind") {
        WeatherCondition::Windy
    } else {
        WeatherCondition::Unknown
    }
}

fn nws_forecast_to_condition(short_forecast: &str) -> WeatherCondition {
    let lower = short_forecast.to_lowercase();
    if lower.contains("thunder") {
        WeatherCondition::Thunderstorm
    } else if lower.contains("snow") || lower.contains("blizzard") {
        WeatherCondition::Snow
    } else if lower.contains("sleet") || lower.contains("freezing") {
        WeatherCondition::Sleet
    } else if lower.contains("drizzle") {
        WeatherCondition::Drizzle
    } else if lower.contains("rain") || lower.contains("shower") {
        WeatherCondition::Rain
    } else if lower.contains("fog") {
        WeatherCondition::Fog
    } else if lower.contains("cloudy") && lower.contains("partly") {
        WeatherCondition::PartlyCloudy
    } else if lower.contains("cloudy") || lower.contains("overcast") {
        WeatherCondition::Cloudy
    } else if lower.contains("sunny") || lower.contains("clear") {
        WeatherCondition::Clear
    } else if lower.contains("wind") {
        WeatherCondition::Windy
    } else {
        WeatherCondition::Unknown
    }
}

fn parse_nws_time(s: &str) -> i64 {
    // NWS uses ISO 8601: "2024-01-15T14:00:00-05:00"
    // Strip timezone offset for simple parsing
    let base = if let Some(pos) = s.rfind('+').or_else(|| {
        // Find last '-' that's part of timezone (after the T)
        let t_pos = s.find('T')?;
        s[t_pos..].rfind('-').map(|p| t_pos + p)
    }) {
        &s[..pos]
    } else {
        s.trim_end_matches('Z')
    };

    // Parse "2024-01-15T14:00:00"
    let parts: Vec<&str> = base.split('T').collect();
    if parts.len() < 2 {
        return 0;
    }
    let date_parts: Vec<i32> = parts[0].split('-').filter_map(|p| p.parse().ok()).collect();
    let time_parts: Vec<i32> = parts[1].split(':').filter_map(|p| p.parse().ok()).collect();
    if date_parts.len() < 3 {
        return 0;
    }
    let (year, month, day) = (date_parts[0], date_parts[1], date_parts[2]);
    let hour = time_parts.first().copied().unwrap_or(0);
    let minute = time_parts.get(1).copied().unwrap_or(0);

    let days = days_since_epoch(year, month, day);
    let secs = days as i64 * 86400 + hour as i64 * 3600 + minute as i64 * 60;
    secs * 1000
}

fn days_since_epoch(year: i32, month: i32, day: i32) -> i32 {
    let y = if month <= 2 { year - 1 } else { year };
    let m = if month <= 2 { month + 9 } else { month - 3 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

// --- NWS API response types ---

#[derive(Debug, Deserialize)]
struct NwsPointsResponse {
    properties: NwsPointsProperties,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NwsPointsProperties {
    forecast_hourly: String,
    observation_stations: String,
}

#[derive(Debug, Deserialize)]
struct NwsStationsResponse {
    features: Vec<NwsStationFeature>,
}

#[derive(Debug, Deserialize)]
struct NwsStationFeature {
    id: String,
    properties: NwsStationProperties,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NwsStationProperties {
    #[serde(rename = "@id")]
    station_identifier_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NwsObservationResponse {
    properties: NwsObservationProperties,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NwsObservationProperties {
    temperature: NwsValue,
    relative_humidity: NwsValue,
    wind_speed: NwsValue,
    wind_direction: NwsValue,
    barometric_pressure: NwsValue,
    wind_chill: NwsValue,
    heat_index: NwsValue,
    icon: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NwsValue {
    value: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct NwsForecastResponse {
    properties: NwsForecastProperties,
}

#[derive(Debug, Deserialize)]
struct NwsForecastProperties {
    periods: Vec<NwsForecastPeriod>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NwsForecastPeriod {
    start_time: String,
    temperature: f64,
    temperature_unit: String,
    wind_speed: String,
    short_forecast: String,
    relative_humidity: Option<NwsValue>,
    probability_of_precipitation: Option<NwsValue>,
}
