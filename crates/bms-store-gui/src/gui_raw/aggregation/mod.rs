//! Cross-site data aggregation helpers for supervisor mode.
//!
//! Each aggregator fans out queries across N per-site stores via
//! `futures::future::join_all`. In Phase 2 the per-site stores are accessed
//! through trait objects (`SiteAlarmStore`, `SiteEnergyStore`) so a single
//! aggregator can mix in-process local stores with HTTP-backed remote stores.
//! Per-site failures are captured into a status map instead of failing the
//! whole call so one unreachable remote does not blank cross-site dashboards.

pub mod alarm_aggregator;
pub mod energy_aggregator;
pub mod types;
