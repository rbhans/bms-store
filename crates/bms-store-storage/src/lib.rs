#![allow(clippy::too_many_arguments, clippy::should_implement_trait)]

//! Storage and background services for bms-store.

pub mod auth;
pub mod backup;
pub mod boot;
pub mod config;
pub mod discovery;
pub mod event;
pub mod export;
pub mod haystack;
pub mod health;
pub mod logic;
pub mod mqtt;
pub mod node;
pub mod project;
pub mod protocol;
pub mod store;
pub mod webhook;

#[cfg(feature = "atlas")]
pub mod atlas;

pub use bms_core as core;
pub use bms_store_domain as domain;
