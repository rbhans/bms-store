pub mod nws;
pub mod open_meteo;
pub mod openweathermap;
pub mod visual_crossing;
pub mod weatherapi;

use async_trait::async_trait;
use thiserror::Error;

use super::model::{RawSourceData, WeatherLocation, WeatherSource};

#[derive(Debug, Error)]
pub enum WeatherError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error: {0}")]
    Api(String),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Not available for this location")]
    NotAvailable,
    #[error("API key required but not configured")]
    ApiKeyMissing,
}

#[async_trait]
pub trait WeatherAdapter: Send + Sync {
    fn source(&self) -> WeatherSource;
    fn requires_api_key(&self) -> bool;
    async fn fetch(
        &self,
        client: &reqwest::Client,
        location: &WeatherLocation,
    ) -> Result<RawSourceData, WeatherError>;
}
