#![allow(clippy::too_many_arguments, clippy::should_implement_trait)]

//! Storage and background services for bms-store.

pub mod auth;
pub mod backup;
pub mod boot;
pub mod config;
pub mod discovery;
pub mod energy;
pub mod event;
pub mod export;
pub mod fdd;
pub mod haystack;
pub mod health;
pub mod logic;
pub mod mqtt;
pub mod node;
pub mod notification;
pub mod project;
pub mod protocol;
pub mod reporting;
pub mod store;
pub mod weather;
pub mod webhook;

#[cfg(feature = "atlas")]
pub mod atlas;

#[cfg(feature = "cloud")]
pub mod cloud;

pub use bms_core as core;
pub use bms_store_domain as domain;
