use std::time::{Duration, Instant};

pub const BACKOFF_BASE_SECS: u64 = 2;
pub const BACKOFF_MAX_SECS: u64 = 300; // 5 minutes
pub const DEVICE_DOWN_THRESHOLD: u32 = 5;

pub struct DeviceBackoff {
    pub failures: u32,
    pub next_retry: Instant,
    pub was_down: bool,
}

impl Default for DeviceBackoff {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceBackoff {
    pub fn new() -> Self {
        Self {
            failures: 0,
            next_retry: Instant::now(),
            was_down: false,
        }
    }

    pub fn record_success(&mut self) {
        self.failures = 0;
        self.next_retry = Instant::now();
    }

    pub fn record_failure(&mut self) {
        self.failures = self.failures.saturating_add(1);
        let delay_secs = std::cmp::min(
            BACKOFF_BASE_SECS.saturating_pow(self.failures),
            BACKOFF_MAX_SECS,
        );
        self.next_retry = Instant::now() + Duration::from_secs(delay_secs);
    }

    pub fn should_skip(&self) -> bool {
        Instant::now() < self.next_retry
    }

    pub fn is_down(&self) -> bool {
        self.failures >= DEVICE_DOWN_THRESHOLD
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_backoff_is_not_down() {
        let b = DeviceBackoff::new();
        assert_eq!(b.failures, 0);
        assert!(!b.is_down());
        assert!(!b.should_skip());
        assert!(!b.was_down);
    }

    #[test]
    fn record_failure_increments() {
        let mut b = DeviceBackoff::new();
        b.record_failure();
        assert_eq!(b.failures, 1);
        assert!(!b.is_down());

        for _ in 1..DEVICE_DOWN_THRESHOLD {
            b.record_failure();
        }
        assert!(b.is_down());
    }

    #[test]
    fn record_success_resets() {
        let mut b = DeviceBackoff::new();
        for _ in 0..DEVICE_DOWN_THRESHOLD {
            b.record_failure();
        }
        assert!(b.is_down());

        b.record_success();
        assert_eq!(b.failures, 0);
        assert!(!b.is_down());
        assert!(!b.should_skip());
    }

    #[test]
    fn backoff_delay_capped() {
        let mut b = DeviceBackoff::new();
        // Many failures should not exceed max
        for _ in 0..100 {
            b.record_failure();
        }
        // Next retry should be at most BACKOFF_MAX_SECS from now
        let max_future = Instant::now() + Duration::from_secs(BACKOFF_MAX_SECS + 1);
        assert!(b.next_retry <= max_future);
    }
}
