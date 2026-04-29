use async_trait::async_trait;
use serde::Deserialize;

use super::{WeatherAdapter, WeatherError};
use crate::weather::model::*;

pub struct VisualCrossingAdapter {
    pub api_key: String,
}

#[async_trait]
impl WeatherAdapter for VisualCrossingAdapter {
    fn source(&self) -> WeatherSource {
        WeatherSource::VisualCrossing
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
            "https://weather.visualcrossing.com/VisualCrossingWebServices/rest/services/timeline/{},{}?unitGroup=metric&key={}&include=current,hours,days&contentType=json",
            location.lat, location.lon, self.api_key
        );

        let resp: VcResponse = client
            .get(&url)
            .send()
            .await?
            .error_for_status()
            .map_err(|e| WeatherError::Api(format!("VisualCrossing: {e}")))?
            .json()
            .await?;

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let current = if let Some(c) = &resp.current_conditions {
            RawCurrentData {
                temperature_c: c.temp.unwrap_or(0.0),
                humidity_pct: c.humidity.unwrap_or(0.0),
                wind_speed_kmh: c.windspeed.unwrap_or(0.0),
                wind_direction_deg: c.winddir.unwrap_or(0.0),
                pressure_hpa: c.pressure.unwrap_or(1013.25),
                feels_like_c: c.feelslike.unwrap_or(c.temp.unwrap_or(0.0)),
                condition: vc_icon_to_condition(c.icon.as_deref().unwrap_or("")),
                uv_index: c.uvindex,
                timestamp_ms: c.datetime_epoch.map(|e| e * 1000).unwrap_or(now_ms),
            }
        } else {
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
        };

        let mut hourly = Vec::new();
        for day in &resp.days {
            for h in day.hours.as_deref().unwrap_or(&[]) {
                let hour_ms = h.datetime_epoch.map(|e| e * 1000).unwrap_or(0);
                hourly.push(RawHourlyData {
                    hour_ms,
                    temperature_c: h.temp.unwrap_or(0.0),
                    humidity_pct: h.humidity.unwrap_or(0.0),
                    wind_speed_kmh: h.windspeed.unwrap_or(0.0),
                    precip_probability_pct: h.precipprob.unwrap_or(0.0),
                    precip_mm: h.precip.unwrap_or(0.0),
                    condition: vc_icon_to_condition(h.icon.as_deref().unwrap_or("")),
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
        for d in resp.days.iter().take(7) {
            daily.push(RawDailyData {
                date_ms: d.datetime_epoch.map(|e| e * 1000).unwrap_or(0),
                temp_high_c: d.tempmax.unwrap_or(0.0),
                temp_low_c: d.tempmin.unwrap_or(0.0),
                humidity_pct: d.humidity.unwrap_or(0.0),
                wind_speed_max_kmh: d.windspeed.unwrap_or(0.0),
                precip_probability_pct: d.precipprob.unwrap_or(0.0),
                precip_mm: d.precip.unwrap_or(0.0),
                condition: vc_icon_to_condition(d.icon.as_deref().unwrap_or("")),
                sunrise_ms: d.sunrise_epoch.map(|e| e * 1000).unwrap_or(0),
                sunset_ms: d.sunset_epoch.map(|e| e * 1000).unwrap_or(0),
            });
        }

        Ok(RawSourceData {
            source: WeatherSource::VisualCrossing,
            current,
            hourly,
            daily,
        })
    }
}

fn vc_icon_to_condition(icon: &str) -> WeatherCondition {
    match icon {
        "clear-day" | "clear-night" => WeatherCondition::Clear,
        "partly-cloudy-day" | "partly-cloudy-night" => WeatherCondition::PartlyCloudy,
        "cloudy" => WeatherCondition::Cloudy,
        "rain" => WeatherCondition::Rain,
        "snow" => WeatherCondition::Snow,
        "sleet" => WeatherCondition::Sleet,
        "wind" => WeatherCondition::Windy,
        "fog" => WeatherCondition::Fog,
        "thunder-rain" | "thunder-showers-day" | "thunder-showers-night" => {
            WeatherCondition::Thunderstorm
        }
        "showers-day" | "showers-night" => WeatherCondition::Rain,
        "snow-showers-day" | "snow-showers-night" => WeatherCondition::Snow,
        _ => WeatherCondition::Unknown,
    }
}

// --- Visual Crossing response types ---

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VcResponse {
    current_conditions: Option<VcCurrent>,
    days: Vec<VcDay>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VcCurrent {
    temp: Option<f64>,
    humidity: Option<f64>,
    windspeed: Option<f64>,
    winddir: Option<f64>,
    pressure: Option<f64>,
    feelslike: Option<f64>,
    uvindex: Option<f64>,
    icon: Option<String>,
    datetime_epoch: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VcDay {
    datetime_epoch: Option<i64>,
    tempmax: Option<f64>,
    tempmin: Option<f64>,
    humidity: Option<f64>,
    windspeed: Option<f64>,
    precip: Option<f64>,
    precipprob: Option<f64>,
    icon: Option<String>,
    sunrise_epoch: Option<i64>,
    sunset_epoch: Option<i64>,
    hours: Option<Vec<VcHour>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VcHour {
    datetime_epoch: Option<i64>,
    temp: Option<f64>,
    humidity: Option<f64>,
    windspeed: Option<f64>,
    precip: Option<f64>,
    precipprob: Option<f64>,
    icon: Option<String>,
}
