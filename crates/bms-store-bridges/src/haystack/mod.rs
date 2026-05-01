//! Re-export shim — the ontology now lives in `bms-haystack`.
//!
//! Existing call sites under `crate::haystack::*` keep working unchanged.
//! `validation` stays in this crate for now because it depends on the storage
//! `Entity` type; it moves into `bms-haystack` once a generic `Dict` type lands.

pub use bms_haystack::auto_tag;

pub mod provider {
    pub use bms_haystack::ontology::{Haystack4Provider, Haystack5Provider, TagProvider};
}

pub mod tags {
    pub use bms_haystack::ontology::tags::*;
}

pub mod prototypes {
    pub use bms_haystack::ontology::proto::*;
}

pub mod validation;
