use async_trait::async_trait;
use serde::Deserialize;

use super::{WeatherAdapter, WeatherError};
use crate::weather::model::*;

pub struct OpenMeteoAdapter;

#[async_trait]
impl WeatherAdapter for OpenMeteoAdapter {
    fn source(&self) -> WeatherSource {
        WeatherSource::OpenMeteo
    }

    fn requires_api_key(&self) -> bool {
        false
    }

    async fn fetch(
        &self,
        client: &reqwest::Client,
        location: &WeatherLocation,
    ) -> Result<RawSourceData, WeatherError> {
        let url = format!(
            "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}\
             &current=temperature_2m,relative_humidity_2m,apparent_temperature,\
             surface_pressure,wind_speed_10m,wind_direction_10m,weather_code,uv_index\
             &hourly=temperature_2m,relative_humidity_2m,wind_speed_10m,\
             precipitation_probability,precipitation,weather_code\
             &daily=temperature_2m_max,temperature_2m_min,apparent_temperature_max,\
             apparent_temperature_min,precipitation_sum,precipitation_probability_max,\
             wind_speed_10m_max,weather_code,sunrise,sunset,uv_index_max\
             &timezone=auto&forecast_days=7&forecast_hours=48",
            location.lat, location.lon
        );

        let resp: OpenMeteoResponse = client
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let current = RawCurrentData {
            temperature_c: resp.current.temperature_2m,
            humidity_pct: resp.current.relative_humidity_2m,
            wind_speed_kmh: resp.current.wind_speed_10m,
            wind_direction_deg: resp.current.wind_direction_10m,
            pressure_hpa: resp.current.surface_pressure,
            feels_like_c: resp.current.apparent_temperature,
            condition: wmo_to_condition(resp.current.weather_code),
            uv_index: resp.current.uv_index,
            timestamp_ms: now_ms,
        };

        let hourly_len = resp.hourly.time.len();
        let mut hourly = Vec::with_capacity(hourly_len);
        for i in 0..hourly_len {
            let hour_ms = parse_iso_to_ms(&resp.hourly.time[i]);
            hourly.push(RawHourlyData {
                hour_ms,
                temperature_c: get_f64(&resp.hourly.temperature_2m, i),
                humidity_pct: get_f64(&resp.hourly.relative_humidity_2m, i),
                wind_speed_kmh: get_f64(&resp.hourly.wind_speed_10m, i),
                precip_probability_pct: get_f64(&resp.hourly.precipitation_probability, i),
                precip_mm: get_f64(&resp.hourly.precipitation, i),
                condition: wmo_to_condition(get_i32(&resp.hourly.weather_code, i)),
            });
        }

        let daily_len = resp.daily.time.len();
        let mut daily = Vec::with_capacity(daily_len);
        for i in 0..daily_len {
            let date_ms = parse_iso_to_ms(&resp.daily.time[i]);
            daily.push(RawDailyData {
                date_ms,
                temp_high_c: get_f64(&resp.daily.temperature_2m_max, i),
                temp_low_c: get_f64(&resp.daily.temperature_2m_min, i),
                humidity_pct: 0.0, // Open-Meteo daily doesn't provide avg humidity
                wind_speed_max_kmh: get_f64(&resp.daily.wind_speed_10m_max, i),
                precip_probability_pct: get_f64(&resp.daily.precipitation_probability_max, i),
                precip_mm: get_f64(&resp.daily.precipitation_sum, i),
                condition: wmo_to_condition(get_i32(&resp.daily.weather_code, i)),
                sunrise_ms: parse_iso_to_ms(
                    resp.daily
                        .sunrise
                        .as_ref()
                        .and_then(|v| v.get(i))
                        .map(|s| s.as_str())
                        .unwrap_or(""),
                ),
                sunset_ms: parse_iso_to_ms(
                    resp.daily
                        .sunset
                        .as_ref()
                        .and_then(|v| v.get(i))
                        .map(|s| s.as_str())
                        .unwrap_or(""),
                ),
            });
        }

        Ok(RawSourceData {
            source: WeatherSource::OpenMeteo,
            current,
            hourly,
            daily,
        })
    }
}

fn get_f64(v: &Option<Vec<f64>>, i: usize) -> f64 {
    v.as_ref()
        .and_then(|vec| vec.get(i).copied())
        .unwrap_or(0.0)
}

fn get_i32(v: &Option<Vec<i32>>, i: usize) -> i32 {
    v.as_ref().and_then(|vec| vec.get(i).copied()).unwrap_or(-1)
}

fn parse_iso_to_ms(s: &str) -> i64 {
    // Open-Meteo returns "2024-01-15T14:00" or "2024-01-15"
    // Simple parser: split into date + optional time parts
    if s.is_empty() {
        return 0;
    }
    let parts: Vec<&str> = s.split('T').collect();
    let date_parts: Vec<i32> = parts[0].split('-').filter_map(|p| p.parse().ok()).collect();
    if date_parts.len() < 3 {
        return 0;
    }
    let (year, month, day) = (date_parts[0], date_parts[1], date_parts[2]);
    let (hour, minute) = if parts.len() > 1 {
        let time_parts: Vec<i32> = parts[1].split(':').filter_map(|p| p.parse().ok()).collect();
        (
            time_parts.first().copied().unwrap_or(0),
            time_parts.get(1).copied().unwrap_or(0),
        )
    } else {
        (0, 0)
    };

    // Calculate days since epoch (simplified, adequate for weather)
    let days = days_since_epoch(year, month, day);
    let secs = days as i64 * 86400 + hour as i64 * 3600 + minute as i64 * 60;
    secs * 1000
}

fn days_since_epoch(year: i32, month: i32, day: i32) -> i32 {
    // Compute days from 1970-01-01 using a well-known formula
    let y = if month <= 2 { year - 1 } else { year };
    let m = if month <= 2 { month + 9 } else { month - 3 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Map WMO weather interpretation codes to WeatherCondition.
/// See: https://open-meteo.com/en/docs
fn wmo_to_condition(code: i32) -> WeatherCondition {
    match code {
        0 => WeatherCondition::Clear,
        1..=2 => WeatherCondition::PartlyCloudy,
        3 => WeatherCondition::Cloudy,
        45 | 48 => WeatherCondition::Fog,
        51 | 53 | 55 => WeatherCondition::Drizzle,
        56 | 57 => WeatherCondition::Sleet,
        61 | 63 | 65 => WeatherCondition::Rain,
        66 | 67 => WeatherCondition::Sleet,
        71 | 73 | 75 | 77 => WeatherCondition::Snow,
        80..=82 => WeatherCondition::Rain,
        85 | 86 => WeatherCondition::Snow,
        95 => WeatherCondition::Thunderstorm,
        96 | 99 => WeatherCondition::Thunderstorm,
        _ => WeatherCondition::Unknown,
    }
}

#[derive(Debug, Deserialize)]
struct OpenMeteoResponse {
    current: OpenMeteoCurrent,
    hourly: OpenMeteoHourly,
    daily: OpenMeteoDaily,
}

#[derive(Debug, Deserialize)]
struct OpenMeteoCurrent {
    temperature_2m: f64,
    relative_humidity_2m: f64,
    apparent_temperature: f64,
    surface_pressure: f64,
    wind_speed_10m: f64,
    wind_direction_10m: f64,
    weather_code: i32,
    uv_index: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct OpenMeteoHourly {
    time: Vec<String>,
    temperature_2m: Option<Vec<f64>>,
    relative_humidity_2m: Option<Vec<f64>>,
    wind_speed_10m: Option<Vec<f64>>,
    precipitation_probability: Option<Vec<f64>>,
    precipitation: Option<Vec<f64>>,
    weather_code: Option<Vec<i32>>,
}

#[derive(Debug, Deserialize)]
struct OpenMeteoDaily {
    time: Vec<String>,
    temperature_2m_max: Option<Vec<f64>>,
    temperature_2m_min: Option<Vec<f64>>,
    wind_speed_10m_max: Option<Vec<f64>>,
    precipitation_probability_max: Option<Vec<f64>>,
    precipitation_sum: Option<Vec<f64>>,
    weather_code: Option<Vec<i32>>,
    sunrise: Option<Vec<String>>,
    sunset: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wmo_codes() {
        assert_eq!(wmo_to_condition(0), WeatherCondition::Clear);
        assert_eq!(wmo_to_condition(3), WeatherCondition::Cloudy);
        assert_eq!(wmo_to_condition(61), WeatherCondition::Rain);
        assert_eq!(wmo_to_condition(71), WeatherCondition::Snow);
        assert_eq!(wmo_to_condition(95), WeatherCondition::Thunderstorm);
    }

    #[test]
    fn iso_parse() {
        let ms = parse_iso_to_ms("2024-01-15T14:00");
        assert!(ms > 0);
        // 2024-01-15 14:00 UTC should be around 1705327200000
        let expected = 1705327200000i64;
        assert!(
            (ms - expected).abs() < 86400000,
            "parsed {ms} vs expected {expected}"
        );
    }

    #[test]
    fn iso_parse_date_only() {
        let ms = parse_iso_to_ms("2024-01-15");
        assert!(ms > 0);
    }

    #[test]
    fn iso_parse_empty() {
        assert_eq!(parse_iso_to_ms(""), 0);
    }
}
