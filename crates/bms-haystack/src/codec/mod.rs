//! Wire-format codecs.
//!
//! Each codec implements [`Codec`], converting between a [`crate::val::Grid`]
//! and a byte payload. The shipped codecs are:
//!
//! * [`hayson`] — JSON encoding (default for HTTP responses, `Content-Type:
//!   application/vnd.haystack+json` or `application/json`).
//! * `zinc` — text format (forthcoming).
//! * `trio` — record-oriented YAML-ish (forthcoming).
//! * `csv` — flat CSV (forthcoming).

pub mod hayson;

use thiserror::Error;

use crate::val::Grid;

#[derive(Debug, Error)]
pub enum CodecError {
    #[error("decode: {0}")]
    Decode(String),
    #[error("encode: {0}")]
    Encode(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

/// Trait implemented by every wire-format codec.
pub trait Codec {
    /// Standard MIME type advertised by this codec.
    const MIME: &'static str;
    fn encode_grid(grid: &Grid) -> Result<Vec<u8>, CodecError>;
    fn decode_grid(bytes: &[u8]) -> Result<Grid, CodecError>;
}

/// Content-type negotiation result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentType {
    Hayson,
    Zinc,
    Csv,
    PlainJson,
}

impl ContentType {
    pub fn mime(self) -> &'static str {
        match self {
            ContentType::Hayson => "application/vnd.haystack+json",
            ContentType::Zinc => "text/zinc",
            ContentType::Csv => "text/csv",
            ContentType::PlainJson => "application/json",
        }
    }

    /// Pick a content type from an `Accept` header value.
    /// Returns `Hayson` if no recognised type is present.
    pub fn from_accept(accept: &str) -> Self {
        for token in accept.split(',') {
            let head = token.split(';').next().unwrap_or("").trim();
            match head {
                "application/vnd.haystack+json" => return ContentType::Hayson,
                "text/zinc" => return ContentType::Zinc,
                "text/csv" => return ContentType::Csv,
                "application/json" => return ContentType::PlainJson,
                _ => {}
            }
        }
        ContentType::Hayson
    }
}
