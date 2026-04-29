use serde::{Deserialize, Serialize};

use super::model::{WeatherLocation, WeatherSource};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TemperatureUnit {
    Celsius,
    Fahrenheit,
}

impl TemperatureUnit {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Celsius => "Celsius",
            Self::Fahrenheit => "Fahrenheit",
        }
    }

    pub fn suffix(&self) -> &'static str {
        match self {
            Self::Celsius => "\u{00B0}C",
            Self::Fahrenheit => "\u{00B0}F",
        }
    }

    pub fn convert(&self, celsius: f64) -> f64 {
        match self {
            Self::Celsius => celsius,
            Self::Fahrenheit => celsius * 9.0 / 5.0 + 32.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherConfig {
    pub location: Option<WeatherLocation>,
    /// User-entered zip/postal code (persisted so the field repopulates).
    #[serde(default)]
    pub zip_code: String,
    pub openweathermap_api_key: Option<String>,
    pub weatherapi_api_key: Option<String>,
    pub visual_crossing_api_key: Option<String>,
    pub enabled_sources: Vec<WeatherSource>,
    pub refresh_interval_secs: u64,
    pub temperature_unit: TemperatureUnit,
}

impl Default for WeatherConfig {
    fn default() -> Self {
        Self {
            location: None,
            zip_code: String::new(),
            openweathermap_api_key: None,
            weatherapi_api_key: None,
            visual_crossing_api_key: None,
            enabled_sources: vec![WeatherSource::OpenMeteo],
            refresh_interval_secs: 1800,
            temperature_unit: TemperatureUnit::Fahrenheit,
        }
    }
}

impl WeatherConfig {
    pub fn load(data_dir: &std::path::Path) -> Self {
        let path = data_dir.join("weather.json");
        match std::fs::read_to_string(&path) {
            Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, data_dir: &std::path::Path) {
        let path = data_dir.join("weather.json");
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temperature_conversion() {
        assert!((TemperatureUnit::Fahrenheit.convert(0.0) - 32.0).abs() < 0.01);
        assert!((TemperatureUnit::Fahrenheit.convert(100.0) - 212.0).abs() < 0.01);
        assert!((TemperatureUnit::Celsius.convert(25.0) - 25.0).abs() < 0.01);
    }

    #[test]
    fn default_config() {
        let cfg = WeatherConfig::default();
        assert!(cfg.location.is_none());
        assert_eq!(cfg.refresh_interval_secs, 1800);
        assert_eq!(cfg.enabled_sources, vec![WeatherSource::OpenMeteo]);
    }
}
