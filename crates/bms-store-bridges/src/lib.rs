#![allow(clippy::too_many_arguments, clippy::should_implement_trait)]

//! Protocol bridges and discovery adapters for bms-store.

pub mod boot;
pub mod bridge;
pub mod discovery;
pub mod haystack;
pub mod normalize;
pub mod plugin;

pub mod config {
    pub use bms_store_storage::config::*;
}

pub mod event {
    pub use bms_store_storage::event::*;
}

pub mod health {
    pub use bms_store_storage::health::*;
}

pub mod node {
    pub use bms_store_storage::node::*;
}

pub mod protocol {
    pub use bms_store_storage::protocol::*;
}

pub mod store {
    pub use bms_store_storage::store::*;
}

#[cfg(feature = "atlas")]
pub mod atlas {
    pub use bms_store_storage::atlas::*;
}

pub use bms_core as core;
pub use bms_store_domain as domain;
pub use bms_store_storage as storage;
