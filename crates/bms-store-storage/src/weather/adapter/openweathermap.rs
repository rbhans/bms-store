use async_trait::async_trait;
use serde::Deserialize;

use super::{WeatherAdapter, WeatherError};
use crate::weather::model::*;

pub struct OpenWeatherMapAdapter {
    pub api_key: String,
}

#[async_trait]
impl WeatherAdapter for OpenWeatherMapAdapter {
    fn source(&self) -> WeatherSource {
        WeatherSource::OpenWeatherMap
    }

    fn requires_api_key(&self) -> bool {
        true
    }

    async fn fetch(
        &self,
        client: &reqwest::Client,
        location: &WeatherLocation,
    ) -> Result<RawSourceData, WeatherError> {
        // OWM 3.0 One Call API (free tier with 1000 calls/day)
        let url = format!(
            "https://api.openweathermap.org/data/3.0/onecall?lat={}&lon={}&appid={}&units=metric&exclude=minutely,alerts",
            location.lat, location.lon, self.api_key
        );

        let resp: OwmResponse = client
            .get(&url)
            .send()
            .await?
            .error_for_status()
            .map_err(|e| WeatherError::Api(format!("OWM: {e}")))?
            .json()
            .await?;

        let c = &resp.current;
        let now_ms = c.dt * 1000;

        let current = RawCurrentData {
            temperature_c: c.temp,
            humidity_pct: c.humidity,
            wind_speed_kmh: c.wind_speed * 3.6, // m/s to km/h
            wind_direction_deg: c.wind_deg.unwrap_or(0.0),
            pressure_hpa: c.pressure,
            feels_like_c: c.feels_like,
            condition: owm_id_to_condition(c.weather.first().map(|w| w.id).unwrap_or(0)),
            uv_index: c.uvi,
            timestamp_ms: now_ms,
        };

        let mut hourly = Vec::new();
        for h in resp.hourly.iter().take(48) {
            hourly.push(RawHourlyData {
                hour_ms: h.dt * 1000,
                temperature_c: h.temp,
                humidity_pct: h.humidity,
                wind_speed_kmh: h.wind_speed * 3.6,
                precip_probability_pct: h.pop.unwrap_or(0.0) * 100.0,
                precip_mm: h.rain.as_ref().and_then(|r| r.one_h).unwrap_or(0.0)
                    + h.snow.as_ref().and_then(|s| s.one_h).unwrap_or(0.0),
                condition: owm_id_to_condition(h.weather.first().map(|w| w.id).unwrap_or(0)),
            });
        }

        let mut daily = Vec::new();
        for d in resp.daily.iter().take(7) {
            daily.push(RawDailyData {
                date_ms: d.dt * 1000,
                temp_high_c: d.temp.max,
                temp_low_c: d.temp.min,
                humidity_pct: d.humidity,
                wind_speed_max_kmh: d.wind_speed * 3.6,
                precip_probability_pct: d.pop.unwrap_or(0.0) * 100.0,
                precip_mm: d.rain.unwrap_or(0.0) + d.snow.unwrap_or(0.0),
                condition: owm_id_to_condition(d.weather.first().map(|w| w.id).unwrap_or(0)),
                sunrise_ms: d.sunrise * 1000,
                sunset_ms: d.sunset * 1000,
            });
        }

        Ok(RawSourceData {
            source: WeatherSource::OpenWeatherMap,
            current,
            hourly,
            daily,
        })
    }
}

fn owm_id_to_condition(id: i32) -> WeatherCondition {
    match id {
        200..=232 => WeatherCondition::Thunderstorm,
        300..=321 => WeatherCondition::Drizzle,
        500..=531 => WeatherCondition::Rain,
        611..=616 => WeatherCondition::Sleet,
        600..=622 => WeatherCondition::Snow,
        701 | 741 => WeatherCondition::Fog,
        721 | 731 | 751 | 761 | 762 => WeatherCondition::Fog,
        771 | 781 => WeatherCondition::Windy,
        800 => WeatherCondition::Clear,
        801..=802 => WeatherCondition::PartlyCloudy,
        803..=804 => WeatherCondition::Cloudy,
        _ => WeatherCondition::Unknown,
    }
}

// --- OWM response types ---

#[derive(Debug, Deserialize)]
struct OwmResponse {
    current: OwmCurrent,
    #[serde(default)]
    hourly: Vec<OwmHourly>,
    #[serde(default)]
    daily: Vec<OwmDaily>,
}

#[derive(Debug, Deserialize)]
struct OwmCurrent {
    dt: i64,
    temp: f64,
    feels_like: f64,
    pressure: f64,
    humidity: f64,
    uvi: Option<f64>,
    wind_speed: f64,
    wind_deg: Option<f64>,
    #[serde(default)]
    weather: Vec<OwmWeather>,
}

#[derive(Debug, Deserialize)]
struct OwmWeather {
    id: i32,
}

#[derive(Debug, Deserialize)]
struct OwmHourly {
    dt: i64,
    temp: f64,
    humidity: f64,
    wind_speed: f64,
    pop: Option<f64>,
    rain: Option<OwmPrecip>,
    snow: Option<OwmPrecip>,
    #[serde(default)]
    weather: Vec<OwmWeather>,
}

#[derive(Debug, Deserialize)]
struct OwmPrecip {
    #[serde(rename = "1h")]
    one_h: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct OwmDaily {
    dt: i64,
    sunrise: i64,
    sunset: i64,
    temp: OwmDailyTemp,
    humidity: f64,
    wind_speed: f64,
    pop: Option<f64>,
    rain: Option<f64>,
    snow: Option<f64>,
    #[serde(default)]
    weather: Vec<OwmWeather>,
}

#[derive(Debug, Deserialize)]
struct OwmDailyTemp {
    min: f64,
    max: f64,
}
