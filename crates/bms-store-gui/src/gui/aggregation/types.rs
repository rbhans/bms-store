//! Re-export shim. The real DTOs live in `crate::gui::aggregation::types` so the
//! supervisor's HTTP-backed remote stores (which are not feature-gated behind
//! `desktop`) can produce them. This shim keeps the existing
//! `gui::aggregation::types::*` import paths working.

pub use crate::gui::aggregation::types::{
    AggregatorError, SiteActiveAlarm, SiteAlarmEvent, SiteDailyRollup, SiteMeter,
};
