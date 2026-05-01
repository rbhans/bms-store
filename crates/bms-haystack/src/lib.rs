#![allow(clippy::too_many_arguments)]

//! Haystack 5 / Xeto types, ontology, codecs, and HTTP ops for bms-store.
//!
//! Step 1 of the Haystack-5-native migration: this crate hosts the unified
//! ontology layer (tags, prototypes, provider, auto-tag heuristics) that was
//! previously duplicated across `bms-store-bridges` and `bms-store-storage`.
//! Codec, filter, xeto runtime, and HTTP ops modules land in later steps.

pub mod auto_tag;
pub mod codec;
pub mod filter;
pub mod ontology;
#[cfg(feature = "server")]
pub mod server;
pub mod val;
pub mod validation;
pub mod xeto;
