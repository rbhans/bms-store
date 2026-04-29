mod db;
pub(crate) mod engine;
pub mod preview;
pub mod store;
pub mod templates;
pub(crate) mod time;
pub mod types;

// Re-export all public types so that `crate::store::schedule_store::X` still works
pub use preview::{compute_preview, PreviewBlock};
pub use store::{start_schedule_engine, start_schedule_engine_with_path, ScheduleStore};
pub use templates::*;
pub use types::*;

// Re-export time and engine internals needed by tests
#[cfg(test)]
use crate::store::point_store::{PointKey, PointStore};

#[cfg(test)]
mod tests;
