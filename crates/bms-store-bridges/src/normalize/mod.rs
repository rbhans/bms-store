//! Value normalization for BMS data points.
//!
//! Provides per-point value-map normalization (raw protocol values →
//! canonical display strings) and the storage convention for storing
//! those maps in the entity `enum` tag.

pub mod value_map;

pub use value_map::{BoolMap, NormalizedValue, ValueMap, build_entity_value_maps, value_map_for_entity};
