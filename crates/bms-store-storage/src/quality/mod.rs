//! Data quality management: stale detection and quality propagation.

pub mod stale_detector;

pub use stale_detector::{
    start_stale_detector, start_stale_detector_with_interval, DEFAULT_POLL_INTERVAL_SECS,
    DETECTOR_SWEEP_INTERVAL_SECS, STALE_TOLERANCE_FACTOR,
};
