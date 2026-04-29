//! Platform health monitoring.
//!
//! Provides:
//! - [`HealthRegistry`] — tracks named subsystem statuses.
//! - [`spawn_supervised`] — spawns a tokio task that auto-restarts on panic,
//!   logging each failure and updating the health registry.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, RwLock};

/// Health status of a subsystem.
#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    /// Running normally.
    Healthy,
    /// Running but encountered issues (e.g., device reconnection).
    Degraded(String),
    /// Not running — failed and not yet recovered.
    Down(String),
    /// Not started yet.
    Unknown,
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthStatus::Healthy => write!(f, "healthy"),
            HealthStatus::Degraded(msg) => write!(f, "degraded: {msg}"),
            HealthStatus::Down(msg) => write!(f, "down: {msg}"),
            HealthStatus::Unknown => write!(f, "unknown"),
        }
    }
}

/// Tracks health status for named subsystems.
#[derive(Clone)]
pub struct HealthRegistry {
    inner: Arc<RwLock<HashMap<String, HealthStatus>>>,
}

impl Default for HealthRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set the health status of a subsystem.
    pub fn set(&self, name: &str, status: HealthStatus) {
        self.inner
            .write()
            .expect("health lock poisoned")
            .insert(name.to_string(), status);
    }

    /// Get the health status of a subsystem.
    pub fn get(&self, name: &str) -> HealthStatus {
        self.inner
            .read()
            .expect("health lock poisoned")
            .get(name)
            .cloned()
            .unwrap_or(HealthStatus::Unknown)
    }

    /// Returns true if all registered subsystems are Healthy.
    pub fn is_healthy(&self) -> bool {
        self.inner
            .read()
            .expect("health lock poisoned")
            .values()
            .all(|s| matches!(s, HealthStatus::Healthy))
    }

    /// Snapshot of all subsystem statuses.
    pub fn snapshot(&self) -> Vec<(String, HealthStatus)> {
        let map = self.inner.read().expect("health lock poisoned");
        let mut entries: Vec<_> = map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries
    }
}

/// Spawn a supervised async task that restarts on panic.
///
/// If the future panics, it is restarted after a short delay, up to `max_restarts`
/// times. Each restart logs an error and updates the health registry.
///
/// The task factory `make_task` is called each time the task needs to (re)start
/// so it can rebuild any state.
pub fn spawn_supervised<F, Fut>(
    name: &'static str,
    registry: HealthRegistry,
    max_restarts: u32,
    make_task: F,
) where
    F: Fn() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        let mut restarts = 0u32;
        loop {
            registry.set(name, HealthStatus::Healthy);

            let result = tokio::spawn(make_task()).await;

            match result {
                Ok(()) => {
                    // Task exited normally — don't restart.
                    tracing::info!(task = name, "supervised task exited normally");
                    return;
                }
                Err(e) => {
                    restarts += 1;
                    let msg = format!("panic #{restarts}: {e}");
                    tracing::error!(task = name, restarts, "supervised task panicked: {e}");
                    registry.set(name, HealthStatus::Down(msg));

                    if restarts > max_restarts {
                        tracing::error!(
                            task = name,
                            max_restarts,
                            "supervised task exceeded max restarts, giving up"
                        );
                        return;
                    }

                    // Exponential backoff: 1s, 2s, 4s, 8s, capped at 30s.
                    let delay = std::cmp::min(1u64 << (restarts - 1), 30);
                    tracing::warn!(
                        task = name,
                        delay_secs = delay,
                        "restarting supervised task"
                    );
                    tokio::time::sleep(tokio::time::Duration::from_secs(delay)).await;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_registry_basics() {
        let reg = HealthRegistry::new();
        assert_eq!(reg.get("foo"), HealthStatus::Unknown);
        assert!(reg.is_healthy()); // no subsystems = vacuously healthy

        reg.set("foo", HealthStatus::Healthy);
        assert!(reg.is_healthy());

        reg.set("bar", HealthStatus::Down("oops".into()));
        assert!(!reg.is_healthy());
        assert_eq!(reg.get("bar"), HealthStatus::Down("oops".into()));
    }

    #[test]
    fn snapshot_sorted() {
        let reg = HealthRegistry::new();
        reg.set("z", HealthStatus::Healthy);
        reg.set("a", HealthStatus::Healthy);
        let snap = reg.snapshot();
        assert_eq!(snap[0].0, "a");
        assert_eq!(snap[1].0, "z");
    }

    #[tokio::test]
    async fn supervised_task_normal_exit() {
        let reg = HealthRegistry::new();
        spawn_supervised("test-ok", reg.clone(), 3, || async {
            // exits immediately
        });
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        // After normal exit, status should still be Healthy (set before run).
        assert_eq!(reg.get("test-ok"), HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn supervised_task_restarts_on_panic() {
        use std::sync::atomic::{AtomicU32, Ordering};
        let counter = Arc::new(AtomicU32::new(0));
        let reg = HealthRegistry::new();

        let c = counter.clone();
        spawn_supervised("test-panic", reg.clone(), 2, move || {
            let c = c.clone();
            async move {
                let n = c.fetch_add(1, Ordering::SeqCst);
                if n < 2 {
                    panic!("intentional test panic #{n}");
                }
                // Third invocation succeeds.
            }
        });

        // Wait enough for restarts (1s + 2s delays plus margin).
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        assert!(
            counter.load(Ordering::SeqCst) >= 3,
            "should have been called 3+ times"
        );
    }
}
