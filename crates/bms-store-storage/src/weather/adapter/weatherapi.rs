use async_trait::async_trait;
use serde::Deserialize;

use super::{WeatherAdapter, WeatherError};
use crate::weather::model::*;

pub struct WeatherApiAdapter {
    pub api_key: String,
}

#[async_trait]
impl WeatherAdapter for WeatherApiAdapter {
    fn source(&self) -> WeatherSource {
        WeatherSource::WeatherApi
    }

    fn requires_api_key(&self) -> bool {
        true
    }

    async fn fetch(
        &self,
        client: &reqwest::Client,
        location: &WeatherLocation,
    ) -> Result<RawSourceData, WeatherError> {
        let url = format!(
            "https://api.weatherapi.com/v1/forecast.json?key={}&q={},{}&days=7&aqi=no&alerts=no",
            self.api_key, location.lat, location.lon
        );

        let resp: WaResponse = client
            .get(&url)
            .send()
            .await?
            .error_for_status()
            .map_err(|e| WeatherError::Api(format!("WeatherAPI: {e}")))?
            .json()
            .await?;

        let c = &resp.current;
        let now_ms = c.last_updated_epoch * 1000;

        let current = RawCurrentData {
            temperature_c: c.temp_c,
            humidity_pct: c.humidity,
            wind_speed_kmh: c.wind_kph,
            wind_direction_deg: c.wind_degree,
            pressure_hpa: c.pressure_mb,
            feels_like_c: c.feelslike_c,
            condition: wa_code_to_condition(c.condition.code),
            uv_index: Some(c.uv),
            timestamp_ms: now_ms,
        };

        let mut hourly = Vec::new();
        for day in &resp.forecast.forecastday {
            for h in &day.hour {
                hourly.push(RawHourlyData {
                    hour_ms: h.time_epoch * 1000,
                    temperature_c: h.temp_c,
                    humidity_pct: h.humidity,
                    wind_speed_kmh: h.wind_kph,
                    precip_probability_pct: h.chance_of_rain.max(h.chance_of_snow),
                    precip_mm: h.precip_mm,
                    condition: wa_code_to_condition(h.condition.code),
                });
                if hourly.len() >= 48 {
                    break;
                }
            }
            if hourly.len() >= 48 {
                break;
            }
        }

        let mut daily = Vec::new();
        for d in &resp.forecast.forecastday {
            daily.push(RawDailyData {
                date_ms: d.date_epoch * 1000,
                temp_high_c: d.day.maxtemp_c,
                temp_low_c: d.day.mintemp_c,
                humidity_pct: d.day.avghumidity,
                wind_speed_max_kmh: d.day.maxwind_kph,
                precip_probability_pct: d.day.daily_chance_of_rain.max(d.day.daily_chance_of_snow),
                precip_mm: d.day.totalprecip_mm,
                condition: wa_code_to_condition(d.day.condition.code),
                sunrise_ms: 0, // Would need time parsing; other sources provide this
                sunset_ms: 0,
            });
        }

        Ok(RawSourceData {
            source: WeatherSource::WeatherApi,
            current,
            hourly,
            daily,
        })
    }
}

fn wa_code_to_condition(code: i32) -> WeatherCondition {
    // WeatherAPI condition codes: https://www.weatherapi.com/docs/weather_conditions.json
    match code {
        1000 => WeatherCondition::Clear,
        1003 => WeatherCondition::PartlyCloudy,
        1006 | 1009 => WeatherCondition::Cloudy,
        1030 | 1135 | 1147 => WeatherCondition::Fog,
        1063 | 1150 | 1153 => WeatherCondition::Drizzle,
        1066 | 1210 | 1213 | 1216 | 1219 | 1222 | 1225 | 1255 | 1258 => WeatherCondition::Snow,
        1069 | 1072 | 1168 | 1171 | 1198 | 1201 | 1237 | 1249 | 1252 | 1261 | 1264 => {
            WeatherCondition::Sleet
        }
        1087 | 1273 | 1276 | 1279 | 1282 => WeatherCondition::Thunderstorm,
        1180 | 1183 | 1186 | 1189 | 1192 | 1195 | 1240 | 1243 | 1246 => WeatherCondition::Rain,
        _ => WeatherCondition::Unknown,
    }
}

// --- WeatherAPI response types ---

#[derive(Debug, Deserialize)]
struct WaResponse {
    current: WaCurrent,
    forecast: WaForecast,
}

#[derive(Debug, Deserialize)]
struct WaCurrent {
    last_updated_epoch: i64,
    temp_c: f64,
    condition: WaCondition,
    wind_kph: f64,
    wind_degree: f64,
    pressure_mb: f64,
    humidity: f64,
    feelslike_c: f64,
    uv: f64,
}

#[derive(Debug, Deserialize)]
struct WaCondition {
    code: i32,
}

#[derive(Debug, Deserialize)]
struct WaForecast {
    forecastday: Vec<WaForecastDay>,
}

#[derive(Debug, Deserialize)]
struct WaForecastDay {
    date_epoch: i64,
    day: WaDay,
    hour: Vec<WaHour>,
}

#[derive(Debug, Deserialize)]
struct WaDay {
    maxtemp_c: f64,
    mintemp_c: f64,
    avghumidity: f64,
    maxwind_kph: f64,
    totalprecip_mm: f64,
    daily_chance_of_rain: f64,
    daily_chance_of_snow: f64,
    condition: WaCondition,
}

#[derive(Debug, Deserialize)]
struct WaHour {
    time_epoch: i64,
    temp_c: f64,
    condition: WaCondition,
    wind_kph: f64,
    humidity: f64,
    precip_mm: f64,
    chance_of_rain: f64,
    chance_of_snow: f64,
}
