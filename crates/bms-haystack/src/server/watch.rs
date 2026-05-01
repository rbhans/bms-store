use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::val::{Dict, Ref};

/// Opaque watch identifier handed to clients.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WatchId(pub String);

impl WatchId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

/// Public projection of a watch session, returned to the caller of
/// `watchSub`.
#[derive(Debug, Clone)]
pub struct WatchHandle {
    pub id: WatchId,
    pub lease_ms: i64,
}

#[derive(Debug)]
struct Watch {
    subbed: HashSet<String>,
    last_snapshot: HashMap<String, Dict>,
    /// Wall-clock expiry. Stored as Unix-millis so suspend/resume on the
    /// host machine doesn't break lease arithmetic.
    expires_at_ms: i64,
}

/// In-memory watch session state.
///
/// Clients call `watchSub` to register a set of refs, then `watchPoll`
/// returns the dicts that have changed since the previous poll (or the
/// initial snapshot on the first poll). Lease expiry is wall-clock-based.
pub struct WatchState {
    watches: Mutex<HashMap<WatchId, Watch>>,
    default_lease: Duration,
}

impl Default for WatchState {
    fn default() -> Self {
        Self::new()
    }
}

impl WatchState {
    pub fn new() -> Self {
        Self {
            watches: Mutex::new(HashMap::new()),
            default_lease: Duration::from_secs(300),
        }
    }

    pub fn subscribe(
        &self,
        ids: impl IntoIterator<Item = Ref>,
        snapshot: HashMap<String, Dict>,
        lease: Option<Duration>,
    ) -> WatchHandle {
        let lease = lease.unwrap_or(self.default_lease);
        let id = WatchId::new();
        let subbed: HashSet<String> = ids.into_iter().map(|r| r.id).collect();
        let lease_ms = lease.as_millis() as i64;
        let expires_at_ms = now_ms() + lease_ms;
        let mut last_snapshot = HashMap::new();
        for sub in &subbed {
            if let Some(d) = snapshot.get(sub) {
                last_snapshot.insert(sub.clone(), d.clone());
            }
        }
        let watch = Watch {
            subbed,
            last_snapshot,
            expires_at_ms,
        };
        self.watches.lock().unwrap().insert(id.clone(), watch);
        WatchHandle { id, lease_ms }
    }

    pub fn unsubscribe(&self, id: &WatchId) {
        self.watches.lock().unwrap().remove(id);
    }

    /// Compute changed dicts vs the last poll snapshot, then update the
    /// snapshot. Returns `None` if the watch is unknown or expired.
    pub fn poll(&self, id: &WatchId, current: &HashMap<String, Dict>) -> Option<Vec<Dict>> {
        let mut guard = self.watches.lock().unwrap();
        let w = guard.get_mut(id)?;
        if now_ms() > w.expires_at_ms {
            guard.remove(id);
            return None;
        }
        let mut changed = Vec::new();
        for sub in &w.subbed {
            let cur = current.get(sub);
            let last = w.last_snapshot.get(sub);
            if cur != last {
                if let Some(c) = cur {
                    changed.push(c.clone());
                }
            }
        }
        // Refresh snapshot
        let mut new_snap = HashMap::new();
        for sub in &w.subbed {
            if let Some(c) = current.get(sub) {
                new_snap.insert(sub.clone(), c.clone());
            }
        }
        w.last_snapshot = new_snap;
        // Bump lease (renew on every successful poll)
        w.expires_at_ms = now_ms() + self.default_lease.as_millis() as i64;
        Some(changed)
    }

    /// Drop expired watches. Callers should run this on a tokio interval.
    pub fn prune_expired(&self) {
        let now = now_ms();
        self.watches
            .lock()
            .unwrap()
            .retain(|_, w| w.expires_at_ms > now);
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
