//! Re-export shim — the ontology now lives in `bms-haystack`.
//!
//! `crate::haystack::prototypes::find_equip_prototype` keeps working for
//! existing storage call sites.

pub mod prototypes {
    pub use bms_haystack::ontology::proto::*;
}
