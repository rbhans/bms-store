#![allow(clippy::should_implement_trait)]

//! # BMS Core
//!
//! Shared domain model for BMS data, events, protocol bindings, RBAC, and
//! extension traits.
//!
//! This crate is intentionally IO-free except for the in-memory event bus. It is
//! the common dependency for `bms-store`, client SDKs, and UI applications.
//!
//! ## Plugin types
//!
//! - **Protocol bridges** — implement [`ProtocolPlugin`] + [`ProtocolBridgeHandle`]
//!   to add a new field protocol (KNX, DALI, EnOcean, etc.)
//! - **History backends** — implement [`HistoryBackend`] to store trend data
//!   in an external database (InfluxDB, TimescaleDB, etc.)
//! - **Alarm evaluators** — implement [`AlarmEvaluator`] for custom alarm logic
//! - **Logic engines** — implement [`LogicEnginePlugin`] for custom automation
//! - **Import/export** — implement [`ImportExportPlugin`] for data format support
//! - **Protocol drivers** — implement [`ProtocolDriver`] + [`ValueSink`] for the
//!   new push-based protocol abstraction
//!
//! ## Example: minimal protocol plugin
//!
//! ```rust
//! use bms_core::*;
//! use std::pin::Pin;
//!
//! struct MyPlugin;
//!
//! impl ProtocolPlugin for MyPlugin {
//!     fn protocol_id(&self) -> &str { "my-protocol" }
//!     fn display_name(&self) -> &str { "My Protocol" }
//! }
//! ```

pub mod alarm;
pub mod event;
pub mod node;
pub mod plugin;
pub mod protocol;
pub mod rbac;
pub mod types;

// Re-export everything at the crate root for convenience
pub use alarm::*;
pub use event::*;
pub use node::*;
pub use plugin::*;
pub use protocol::*;
pub use rbac::*;
pub use types::*;
