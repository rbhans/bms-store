#![allow(clippy::too_many_arguments)]

//! Project Haystack 5 / Xeto ontology + tagging for bms-store.
//!
//! Scope is intentionally narrow: tag tables (build-time generated from the
//! vendored xeto bundle) plus name-pattern auto-tag heuristics. Codecs,
//! HTTP ops, runtime xeto loading, and schema validation are deliberately
//! out of scope — bms-store is a data layer, not a Haystack server.

pub mod auto_tag;
pub mod ontology;
pub mod xeto;
