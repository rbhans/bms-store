//! Storage-side Haystack helpers.
//!
//! `prototypes` is a re-export shim — the canonical ontology lives in
//! the `bms-haystack` crate. `filter` is a storage-local filter helper
//! kept in this crate for now (uses storage's `Entity` type directly).

pub mod filter;

pub mod prototypes {
    pub use bms_haystack::ontology::proto::*;
}
