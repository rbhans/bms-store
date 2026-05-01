use serde::{Deserialize, Serialize};

/// Geographic coordinate (latitude/longitude in degrees).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Coord {
    pub lat: f64,
    pub lng: f64,
}

impl Coord {
    pub const fn new(lat: f64, lng: f64) -> Self {
        Self { lat, lng }
    }
}
