use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeatherLocation {
    pub lat: f64,
    pub lon: f64,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WeatherSource {
    OpenMeteo,
    Nws,
    OpenWeatherMap,
    WeatherApi,
    VisualCrossing,
}

impl WeatherSource {
    pub fn label(&self) -> &'static str {
        match self {
            Self::OpenMeteo => "Open-Meteo",
            Self::Nws => "NWS",
            Self::OpenWeatherMap => "OpenWeatherMap",
            Self::WeatherApi => "WeatherAPI",
            Self::VisualCrossing => "Visual Crossing",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WeatherCondition {
    Clear,
    PartlyCloudy,
    Cloudy,
    Rain,
    Snow,
    Thunderstorm,
    Fog,
    Drizzle,
    Sleet,
    Windy,
    Unknown,
}

impl WeatherCondition {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Clear => "Clear",
            Self::PartlyCloudy => "Partly Cloudy",
            Self::Cloudy => "Cloudy",
            Self::Rain => "Rain",
            Self::Snow => "Snow",
            Self::Thunderstorm => "Thunderstorm",
            Self::Fog => "Fog",
            Self::Drizzle => "Drizzle",
            Self::Sleet => "Sleet",
            Self::Windy => "Windy",
            Self::Unknown => "Unknown",
        }
    }

    /// SVG icon path for this condition (24x24 viewbox).
    pub fn icon_path(&self) -> &'static str {
        match self {
            Self::Clear => "M12 7c-2.76 0-5 2.24-5 5s2.24 5 5 5 5-2.24 5-5-2.24-5-5-5zM2 13h2c.55 0 1-.45 1-1s-.45-1-1-1H2c-.55 0-1 .45-1 1s.45 1 1 1zm18 0h2c.55 0 1-.45 1-1s-.45-1-1-1h-2c-.55 0-1 .45-1 1s.45 1 1 1zM11 2v2c0 .55.45 1 1 1s1-.45 1-1V2c0-.55-.45-1-1-1s-1 .45-1 1zm0 18v2c0 .55.45 1 1 1s1-.45 1-1v-2c0-.55-.45-1-1-1s-1 .45-1 1zM5.99 4.58a.996.996 0 00-1.41 0 .996.996 0 000 1.41l1.06 1.06c.39.39 1.03.39 1.41 0s.39-1.03 0-1.41L5.99 4.58zm12.37 12.37a.996.996 0 00-1.41 0 .996.996 0 000 1.41l1.06 1.06c.39.39 1.03.39 1.41 0a.996.996 0 000-1.41l-1.06-1.06zm1.06-10.96a.996.996 0 000-1.41.996.996 0 00-1.41 0l-1.06 1.06c-.39.39-.39 1.03 0 1.41s1.03.39 1.41 0l1.06-1.06zM7.05 18.36a.996.996 0 000-1.41.996.996 0 00-1.41 0l-1.06 1.06c-.39.39-.39 1.03 0 1.41s1.03.39 1.41 0l1.06-1.06z",
            Self::PartlyCloudy => "M12.74 5.47C15.1 6.5 16.35 9.03 15.92 11.46c.71.36 1.28.87 1.69 1.49 1.93.18 3.39 1.78 3.39 3.76 0 2.21-1.79 4-4 4H6c-2.76 0-5-2.24-5-5 0-2.64 2.05-4.78 4.65-4.96C7.14 8.1 9.82 6.5 12.74 5.47z",
            Self::Cloudy => "M19.35 10.04C18.67 6.59 15.64 4 12 4 9.11 4 6.6 5.64 5.35 8.04 2.34 8.36 0 10.91 0 14c0 3.31 2.69 6 6 6h13c2.76 0 5-2.24 5-5 0-2.64-2.05-4.78-4.65-4.96z",
            Self::Rain => "M19.35 10.04C18.67 6.59 15.64 4 12 4 9.11 4 6.6 5.64 5.35 8.04 2.34 8.36 0 10.91 0 14c0 3.31 2.69 6 6 6h13c2.76 0 5-2.24 5-5 0-2.64-2.05-4.78-4.65-4.96zM14.5 17l-2.5 3-2.5-3h5z",
            Self::Snow => "M19.35 10.04C18.67 6.59 15.64 4 12 4 9.11 4 6.6 5.64 5.35 8.04 2.34 8.36 0 10.91 0 14c0 3.31 2.69 6 6 6h13c2.76 0 5-2.24 5-5 0-2.64-2.05-4.78-4.65-4.96zM10 17l-1 1 1 1 1-1-1-1zm4 0l-1 1 1 1 1-1-1-1z",
            Self::Thunderstorm => "M19.35 10.04C18.67 6.59 15.64 4 12 4 9.11 4 6.6 5.64 5.35 8.04 2.34 8.36 0 10.91 0 14c0 3.31 2.69 6 6 6h13c2.76 0 5-2.24 5-5 0-2.64-2.05-4.78-4.65-4.96zM13 17l-2 5-1-3H8l2-5 1 3h2z",
            Self::Fog => "M3 15h18v-2H3v2zm0 4h18v-2H3v2zm0-8h18V9H3v2zM3 5v2h18V5H3z",
            Self::Drizzle => "M19.35 10.04C18.67 6.59 15.64 4 12 4 9.11 4 6.6 5.64 5.35 8.04 2.34 8.36 0 10.91 0 14c0 3.31 2.69 6 6 6h13c2.76 0 5-2.24 5-5 0-2.64-2.05-4.78-4.65-4.96zM14.5 17l-2.5 3-2.5-3h5z",
            Self::Sleet => "M19.35 10.04C18.67 6.59 15.64 4 12 4 9.11 4 6.6 5.64 5.35 8.04 2.34 8.36 0 10.91 0 14c0 3.31 2.69 6 6 6h13c2.76 0 5-2.24 5-5 0-2.64-2.05-4.78-4.65-4.96zM14.5 17l-2.5 3-2.5-3h5z",
            Self::Windy => "M14.5 17c0 1.65-1.35 3-3 3s-3-1.35-3-3h2c0 .55.45 1 1 1s1-.45 1-1-.45-1-1-1H2v-2h9.5c1.65 0 3 1.35 3 3zM19 6.5C19 4.57 17.43 3 15.5 3S12 4.57 12 6.5h2c0-.83.67-1.5 1.5-1.5s1.5.67 1.5 1.5S16.33 8 15.5 8H2v2h13.5c1.93 0 3.5-1.57 3.5-3.5zM18.5 11H2v2h16.5c.83 0 1.5.67 1.5 1.5s-.67 1.5-1.5 1.5-1.5-.67-1.5-1.5H15c0 1.93 1.57 3.5 3.5 3.5s3.5-1.57 3.5-3.5-1.57-3.5-3.5-3.5z",
            Self::Unknown => "M11 18h2v-2h-2v2zm1-16C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm0 18c-4.41 0-8-3.59-8-8s3.59-8 8-8 8 3.59 8 8-3.59 8-8 8zm0-14c-2.21 0-4 1.79-4 4h2c0-1.1.9-2 2-2s2 .9 2 2c0 2-3 1.75-3 5h2c0-2.25 3-2.5 3-5 0-2.21-1.79-4-4-4z",
        }
    }

    /// Specificity rank for tie-breaking in majority vote (higher = more specific).
    pub fn specificity(&self) -> u8 {
        match self {
            Self::Unknown => 0,
            Self::Clear => 1,
            Self::PartlyCloudy => 2,
            Self::Cloudy => 3,
            Self::Windy => 4,
            Self::Fog => 5,
            Self::Drizzle => 6,
            Self::Rain => 7,
            Self::Sleet => 8,
            Self::Snow => 9,
            Self::Thunderstorm => 10,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AggregatedValue {
    pub min: f64,
    pub max: f64,
    pub avg: f64,
    pub source_count: u8,
}

impl AggregatedValue {
    pub fn from_single(val: f64) -> Self {
        Self {
            min: val,
            max: val,
            avg: val,
            source_count: 1,
        }
    }

    pub fn from_values(values: &[f64]) -> Option<Self> {
        if values.is_empty() {
            return None;
        }
        let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let avg = values.iter().sum::<f64>() / values.len() as f64;
        Some(Self {
            min,
            max,
            avg,
            source_count: values.len() as u8,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CurrentConditions {
    pub temperature: AggregatedValue,
    pub humidity: AggregatedValue,
    pub wind_speed: AggregatedValue,
    pub wind_direction: AggregatedValue,
    pub pressure: AggregatedValue,
    pub feels_like: AggregatedValue,
    pub condition: WeatherCondition,
    pub uv_index: Option<AggregatedValue>,
    pub timestamp_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HourlyForecast {
    pub hour_ms: i64,
    pub temperature: AggregatedValue,
    pub humidity: AggregatedValue,
    pub wind_speed: AggregatedValue,
    pub precip_probability: AggregatedValue,
    pub precip_mm: AggregatedValue,
    pub condition: WeatherCondition,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DailyForecast {
    pub date_ms: i64,
    pub temp_high: AggregatedValue,
    pub temp_low: AggregatedValue,
    pub humidity: AggregatedValue,
    pub wind_speed_max: AggregatedValue,
    pub precip_probability: AggregatedValue,
    pub precip_mm: AggregatedValue,
    pub condition: WeatherCondition,
    pub sunrise_ms: i64,
    pub sunset_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeatherData {
    pub location: WeatherLocation,
    pub current: CurrentConditions,
    pub hourly: Vec<HourlyForecast>,
    pub daily: Vec<DailyForecast>,
    pub last_updated_ms: i64,
    pub sources_available: Vec<WeatherSource>,
    pub sources_failed: Vec<(WeatherSource, String)>,
}

/// Per-source intermediate format — same shape but raw f64 values, no aggregation.
#[derive(Debug, Clone)]
pub struct RawCurrentData {
    pub temperature_c: f64,
    pub humidity_pct: f64,
    pub wind_speed_kmh: f64,
    pub wind_direction_deg: f64,
    pub pressure_hpa: f64,
    pub feels_like_c: f64,
    pub condition: WeatherCondition,
    pub uv_index: Option<f64>,
    pub timestamp_ms: i64,
}

#[derive(Debug, Clone)]
pub struct RawHourlyData {
    pub hour_ms: i64,
    pub temperature_c: f64,
    pub humidity_pct: f64,
    pub wind_speed_kmh: f64,
    pub precip_probability_pct: f64,
    pub precip_mm: f64,
    pub condition: WeatherCondition,
}

#[derive(Debug, Clone)]
pub struct RawDailyData {
    pub date_ms: i64,
    pub temp_high_c: f64,
    pub temp_low_c: f64,
    pub humidity_pct: f64,
    pub wind_speed_max_kmh: f64,
    pub precip_probability_pct: f64,
    pub precip_mm: f64,
    pub condition: WeatherCondition,
    pub sunrise_ms: i64,
    pub sunset_ms: i64,
}

#[derive(Debug, Clone)]
pub struct RawSourceData {
    pub source: WeatherSource,
    pub current: RawCurrentData,
    pub hourly: Vec<RawHourlyData>,
    pub daily: Vec<RawDailyData>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregated_value_from_values() {
        let v = AggregatedValue::from_values(&[10.0, 20.0, 30.0]).unwrap();
        assert_eq!(v.min, 10.0);
        assert_eq!(v.max, 30.0);
        assert!((v.avg - 20.0).abs() < 0.001);
        assert_eq!(v.source_count, 3);
    }

    #[test]
    fn aggregated_value_empty() {
        assert!(AggregatedValue::from_values(&[]).is_none());
    }

    #[test]
    fn aggregated_value_single() {
        let v = AggregatedValue::from_single(42.0);
        assert_eq!(v.min, 42.0);
        assert_eq!(v.max, 42.0);
        assert_eq!(v.avg, 42.0);
        assert_eq!(v.source_count, 1);
    }
}
