//! Cross-site data aggregation helpers for supervisor mode.
//!
//! In Phase 2 the per-site stores are accessed through trait objects so a
//! single aggregator can mix in-process local stores with HTTP-backed remote
//! stores. Consumer-app aggregators (alarm, energy) were removed in Phase 1.
