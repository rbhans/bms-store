//! Override lifecycle tracking.
//!
//! Records active overrides (manual writes), supports expiry and relinquish,
//! and provides an override dashboard for operators.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot, watch};

use super::migration::{run_migrations, Migration};

// ── Public types ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverrideStatus {
    Active,
    Expired,
    Relinquished,
}

impl OverrideStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Expired => "expired",
            Self::Relinquished => "relinquished",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "expired" => Self::Expired,
            "relinquished" => Self::Relinquished,
            _ => Self::Active,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Override {
    pub id: i64,
    pub device_id: String,
    pub point_id: String,
    pub original_value: Option<serde_json::Value>,
    pub override_value: serde_json::Value,
    pub priority: Option<u8>,
    pub created_ms: i64,
    pub expires_ms: Option<i64>,
    pub created_by: String,
    pub status: OverrideStatus,
}

#[derive(Debug, thiserror::Error)]
pub enum OverrideError {
    #[error("override not found")]
    NotFound,
    #[error("database error: {0}")]
    Db(String),
    #[error("channel closed")]
    ChannelClosed,
}

// ── Commands ──

enum OverrideCmd {
    Record {
        device_id: String,
        point_id: String,
        original_value: Option<serde_json::Value>,
        override_value: serde_json::Value,
        priority: Option<u8>,
        expires_ms: Option<i64>,
        created_by: String,
        reply: oneshot::Sender<Result<i64, OverrideError>>,
    },
    ListActive {
        reply: oneshot::Sender<Vec<Override>>,
    },
    ListAll {
        limit: i64,
        reply: oneshot::Sender<Vec<Override>>,
    },
    UpdateExpiry {
        id: i64,
        expires_ms: Option<i64>,
        reply: oneshot::Sender<Result<(), OverrideError>>,
    },
    Relinquish {
        id: i64,
        reply: oneshot::Sender<Result<Override, OverrideError>>,
    },
    RelinquishByPoint {
        device_id: String,
        point_id: String,
        reply: oneshot::Sender<Result<Vec<Override>, OverrideError>>,
    },
    CheckExpired {
        reply: oneshot::Sender<Vec<Override>>,
    },
}

// ── Store handle ──

#[derive(Clone)]
pub struct OverrideStore {
    cmd_tx: mpsc::UnboundedSender<OverrideCmd>,
    #[allow(dead_code)]
    version_tx: watch::Sender<u64>,
    version_rx: watch::Receiver<u64>,
}

impl PartialEq for OverrideStore {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl OverrideStore {
    /// Record a new override after a successful write.
    pub async fn record(
        &self,
        device_id: &str,
        point_id: &str,
        original_value: Option<serde_json::Value>,
        override_value: serde_json::Value,
        priority: Option<u8>,
        expires_ms: Option<i64>,
        created_by: &str,
    ) -> Result<i64, OverrideError> {
        let (reply, rx) = oneshot::channel();
        self.cmd_tx
            .send(OverrideCmd::Record {
                device_id: device_id.to_string(),
                point_id: point_id.to_string(),
                original_value,
                override_value,
                priority,
                expires_ms,
                created_by: created_by.to_string(),
                reply,
            })
            .map_err(|_| OverrideError::ChannelClosed)?;
        rx.await.map_err(|_| OverrideError::ChannelClosed)?
    }

    /// List all active overrides.
    pub async fn list_active(&self) -> Vec<Override> {
        let (reply, rx) = oneshot::channel();
        let _ = self.cmd_tx.send(OverrideCmd::ListActive { reply });
        rx.await.unwrap_or_default()
    }

    /// List all overrides (including expired/relinquished).
    pub async fn list_all(&self, limit: i64) -> Vec<Override> {
        let (reply, rx) = oneshot::channel();
        let _ = self.cmd_tx.send(OverrideCmd::ListAll { limit, reply });
        rx.await.unwrap_or_default()
    }

    /// Update the expiry time for an override.
    pub async fn update_expiry(
        &self,
        id: i64,
        expires_ms: Option<i64>,
    ) -> Result<(), OverrideError> {
        let (reply, rx) = oneshot::channel();
        self.cmd_tx
            .send(OverrideCmd::UpdateExpiry {
                id,
                expires_ms,
                reply,
            })
            .map_err(|_| OverrideError::ChannelClosed)?;
        rx.await.map_err(|_| OverrideError::ChannelClosed)?
    }

    /// Mark an override as relinquished. Returns the override so the caller can
    /// send the relinquish command to the protocol bridge.
    pub async fn relinquish(&self, id: i64) -> Result<Override, OverrideError> {
        let (reply, rx) = oneshot::channel();
        self.cmd_tx
            .send(OverrideCmd::Relinquish { id, reply })
            .map_err(|_| OverrideError::ChannelClosed)?;
        rx.await.map_err(|_| OverrideError::ChannelClosed)?
    }

    /// Relinquish all active overrides for a specific point.
    pub async fn relinquish_by_point(
        &self,
        device_id: &str,
        point_id: &str,
    ) -> Result<Vec<Override>, OverrideError> {
        let (reply, rx) = oneshot::channel();
        self.cmd_tx
            .send(OverrideCmd::RelinquishByPoint {
                device_id: device_id.to_string(),
                point_id: point_id.to_string(),
                reply,
            })
            .map_err(|_| OverrideError::ChannelClosed)?;
        rx.await.map_err(|_| OverrideError::ChannelClosed)?
    }

    /// Check for expired overrides and mark them. Returns the list of newly expired
    /// overrides so the caller can send relinquish commands.
    pub async fn check_expired(&self) -> Vec<Override> {
        let (reply, rx) = oneshot::channel();
        let _ = self.cmd_tx.send(OverrideCmd::CheckExpired { reply });
        rx.await.unwrap_or_default()
    }

    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.version_rx.clone()
    }
}

// ── SQLite thread ──

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    label: "initial override schema",
    sql: "
CREATE TABLE IF NOT EXISTS overrides (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    device_id       TEXT NOT NULL,
    point_id        TEXT NOT NULL,
    original_value  TEXT,
    override_value  TEXT NOT NULL,
    priority        INTEGER,
    created_ms      INTEGER NOT NULL,
    expires_ms      INTEGER,
    created_by      TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'active'
);

CREATE INDEX IF NOT EXISTS idx_override_active ON overrides(status) WHERE status = 'active';
CREATE INDEX IF NOT EXISTS idx_override_point ON overrides(device_id, point_id, status);
CREATE INDEX IF NOT EXISTS idx_override_expires ON overrides(expires_ms) WHERE status = 'active' AND expires_ms IS NOT NULL;
",
}];

fn run_sqlite_thread(
    db_path: &Path,
    rx: mpsc::UnboundedReceiver<OverrideCmd>,
    version_tx: watch::Sender<u64>,
) {
    let conn = rusqlite::Connection::open(db_path).expect("failed to open overrides DB");
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
        .expect("failed to set pragmas");
    run_migrations(&conn, "overrides", MIGRATIONS).expect("overrides: schema migration failed");

    let mut rx = rx;
    let mut version: u64 = 0;

    while let Some(cmd) = rx.blocking_recv() {
        match cmd {
            OverrideCmd::Record {
                device_id,
                point_id,
                original_value,
                override_value,
                priority,
                expires_ms,
                created_by,
                reply,
            } => {
                let now = now_ms();
                let orig_json = original_value.map(|v| v.to_string());
                let result = conn.execute(
                    "INSERT INTO overrides (device_id, point_id, original_value, override_value, priority, created_ms, expires_ms, created_by, status)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'active')",
                    rusqlite::params![
                        device_id,
                        point_id,
                        orig_json,
                        override_value.to_string(),
                        priority,
                        now,
                        expires_ms,
                        created_by,
                    ],
                );
                match result {
                    Ok(_) => {
                        let id = conn.last_insert_rowid();
                        version += 1;
                        let _ = version_tx.send(version);
                        let _ = reply.send(Ok(id));
                    }
                    Err(e) => {
                        let _ = reply.send(Err(OverrideError::Db(e.to_string())));
                    }
                }
            }
            OverrideCmd::ListActive { reply } => {
                let _ = reply.send(query_overrides(
                    &conn,
                    "SELECT id, device_id, point_id, original_value, override_value, priority, created_ms, expires_ms, created_by, status
                     FROM overrides WHERE status = 'active' ORDER BY created_ms DESC",
                    &[],
                ));
            }
            OverrideCmd::ListAll { limit, reply } => {
                let _ = reply.send(query_overrides(
                    &conn,
                    "SELECT id, device_id, point_id, original_value, override_value, priority, created_ms, expires_ms, created_by, status
                     FROM overrides ORDER BY created_ms DESC LIMIT ?1",
                    &[&limit as &dyn rusqlite::types::ToSql],
                ));
            }
            OverrideCmd::UpdateExpiry {
                id,
                expires_ms,
                reply,
            } => {
                let result = conn.execute(
                    "UPDATE overrides SET expires_ms = ?1 WHERE id = ?2 AND status = 'active'",
                    rusqlite::params![expires_ms, id],
                );
                match result {
                    Ok(0) => {
                        let _ = reply.send(Err(OverrideError::NotFound));
                    }
                    Ok(_) => {
                        version += 1;
                        let _ = version_tx.send(version);
                        let _ = reply.send(Ok(()));
                    }
                    Err(e) => {
                        let _ = reply.send(Err(OverrideError::Db(e.to_string())));
                    }
                }
            }
            OverrideCmd::Relinquish { id, reply } => {
                // First fetch the override
                let overrides = query_overrides(
                    &conn,
                    "SELECT id, device_id, point_id, original_value, override_value, priority, created_ms, expires_ms, created_by, status
                     FROM overrides WHERE id = ?1 AND status = 'active'",
                    &[&id as &dyn rusqlite::types::ToSql],
                );
                if overrides.is_empty() {
                    let _ = reply.send(Err(OverrideError::NotFound));
                    continue;
                }
                // Mark as relinquished
                let _ = conn.execute(
                    "UPDATE overrides SET status = 'relinquished' WHERE id = ?1",
                    [id],
                );
                version += 1;
                let _ = version_tx.send(version);
                let _ = reply.send(Ok(overrides.into_iter().next().unwrap()));
            }
            OverrideCmd::RelinquishByPoint {
                device_id,
                point_id,
                reply,
            } => {
                let overrides = query_overrides(
                    &conn,
                    "SELECT id, device_id, point_id, original_value, override_value, priority, created_ms, expires_ms, created_by, status
                     FROM overrides WHERE device_id = ?1 AND point_id = ?2 AND status = 'active'",
                    &[
                        &device_id as &dyn rusqlite::types::ToSql,
                        &point_id as &dyn rusqlite::types::ToSql,
                    ],
                );
                for ov in &overrides {
                    let _ = conn.execute(
                        "UPDATE overrides SET status = 'relinquished' WHERE id = ?1",
                        [ov.id],
                    );
                }
                if !overrides.is_empty() {
                    version += 1;
                    let _ = version_tx.send(version);
                }
                let _ = reply.send(Ok(overrides));
            }
            OverrideCmd::CheckExpired { reply } => {
                let now = now_ms();
                let expired = query_overrides(
                    &conn,
                    "SELECT id, device_id, point_id, original_value, override_value, priority, created_ms, expires_ms, created_by, status
                     FROM overrides WHERE status = 'active' AND expires_ms IS NOT NULL AND expires_ms <= ?1",
                    &[&now as &dyn rusqlite::types::ToSql],
                );
                for ov in &expired {
                    let _ = conn.execute(
                        "UPDATE overrides SET status = 'expired' WHERE id = ?1",
                        [ov.id],
                    );
                }
                if !expired.is_empty() {
                    version += 1;
                    let _ = version_tx.send(version);
                }
                let _ = reply.send(expired);
            }
        }
    }
}

fn query_overrides(
    conn: &rusqlite::Connection,
    sql: &str,
    params: &[&dyn rusqlite::types::ToSql],
) -> Vec<Override> {
    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows = match stmt.query_map(params, |row| {
        let orig_str: Option<String> = row.get(3)?;
        let orig_val = orig_str.and_then(|s| serde_json::from_str(&s).ok());
        let val_str: String = row.get(4)?;
        let val = serde_json::from_str(&val_str).unwrap_or(serde_json::Value::Null);
        let status_str: String = row.get(9)?;

        Ok(Override {
            id: row.get(0)?,
            device_id: row.get(1)?,
            point_id: row.get(2)?,
            original_value: orig_val,
            override_value: val,
            priority: row.get(5)?,
            created_ms: row.get(6)?,
            expires_ms: row.get(7)?,
            created_by: row.get(8)?,
            status: OverrideStatus::from_str(&status_str),
        })
    }) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    rows.filter_map(|r| r.ok()).collect()
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

// ── Constructor ──

pub fn start_override_store_with_path(db_path: &Path) -> OverrideStore {
    let path_clone = db_path.to_path_buf();
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (version_tx, version_rx) = watch::channel(0u64);
    let vtx = version_tx.clone();

    std::thread::Builder::new()
        .name("override-sqlite".into())
        .spawn(move || run_sqlite_thread(&path_clone, cmd_rx, vtx))
        .expect("failed to spawn override SQLite thread");

    OverrideStore {
        cmd_tx,
        version_tx,
        version_rx,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store(name: &str) -> OverrideStore {
        let db_path = std::path::PathBuf::from(format!("/tmp/test_overrides_{name}.db"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).ok();
        }
        start_override_store_with_path(&db_path)
    }

    #[tokio::test]
    async fn record_and_list() {
        let store = test_store("record_list");

        let id = store
            .record(
                "dev1",
                "temp",
                Some(serde_json::json!(72.0)),
                serde_json::json!(74.0),
                Some(8),
                None,
                "admin",
            )
            .await
            .unwrap();
        assert!(id > 0);

        let active = store.list_active().await;
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].device_id, "dev1");
        assert_eq!(active[0].status, OverrideStatus::Active);
    }

    #[tokio::test]
    async fn relinquish_marks_inactive() {
        let store = test_store("relinquish");

        let id = store
            .record(
                "dev1",
                "temp",
                None,
                serde_json::json!(74.0),
                Some(8),
                None,
                "admin",
            )
            .await
            .unwrap();

        let ov = store.relinquish(id).await.unwrap();
        assert_eq!(ov.device_id, "dev1");

        let active = store.list_active().await;
        assert!(active.is_empty());
    }

    #[tokio::test]
    async fn expiry_check() {
        let store = test_store("expiry");

        // Create an override that expired 1 second ago
        let past = now_ms() - 1000;
        let _id = store
            .record(
                "dev1",
                "temp",
                None,
                serde_json::json!(74.0),
                Some(8),
                Some(past),
                "admin",
            )
            .await
            .unwrap();

        let expired = store.check_expired().await;
        assert_eq!(expired.len(), 1);

        let active = store.list_active().await;
        assert!(active.is_empty());
    }

    #[tokio::test]
    async fn update_expiry() {
        let store = test_store("update_expiry");

        let id = store
            .record(
                "dev1",
                "temp",
                None,
                serde_json::json!(74.0),
                Some(8),
                None,
                "admin",
            )
            .await
            .unwrap();

        let future = now_ms() + 3_600_000;
        store.update_expiry(id, Some(future)).await.unwrap();

        let active = store.list_active().await;
        assert_eq!(active[0].expires_ms, Some(future));
    }
}
