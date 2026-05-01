//! Hayson — JSON encoding for Haystack values.
//!
//! Spec: <https://hayson.io>. Scalars (Bool, Str, Number-without-unit) ride
//! as native JSON; richer kinds carry a `_kind` discriminator object.
//!
//! Encode/decode are total round-trips for the value kinds we model. Unknown
//! `_kind` strings decode to [`crate::codec::CodecError::Decode`] so callers
//! get a precise error instead of a silent fallback.

mod decode;
mod encode;

#[cfg(test)]
mod tests;

use crate::codec::{Codec, CodecError};
use crate::val::Grid;

/// Hayson codec. See module docs.
pub struct Hayson;

impl Codec for Hayson {
    const MIME: &'static str = "application/vnd.haystack+json";

    fn encode_grid(grid: &Grid) -> Result<Vec<u8>, CodecError> {
        let v = encode::grid_to_json(grid);
        Ok(serde_json::to_vec(&v)?)
    }

    fn decode_grid(bytes: &[u8]) -> Result<Grid, CodecError> {
        let v: serde_json::Value = serde_json::from_slice(bytes)?;
        decode::json_to_grid(&v)
    }
}

pub use decode::{json_to_dict, json_to_grid, json_to_value};
pub use encode::{dict_to_json, grid_to_json, value_to_json};
