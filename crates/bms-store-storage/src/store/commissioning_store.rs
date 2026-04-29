use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

use super::migration::{run_migrations, Migration};

// ----------------------------------------------------------------
// Public types
// ----------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    NotStarted,
    InProgress,
    Completed,
    SignedOff,
}

impl SessionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NotStarted => "not_started",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::SignedOff => "signed_off",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "not_started" => Some(Self::NotStarted),
            "in_progress" => Some(Self::InProgress),
            "completed" => Some(Self::Completed),
            "signed_off" => Some(Self::SignedOff),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::NotStarted => "Not Started",
            Self::InProgress => "In Progress",
            Self::Completed => "Completed",
            Self::SignedOff => "Signed Off",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemType {
    ReadVerify,
    WriteVerify,
    AlarmVerify,
    ScheduleVerify,
}

impl ItemType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ReadVerify => "read_verify",
            Self::WriteVerify => "write_verify",
            Self::AlarmVerify => "alarm_verify",
            Self::ScheduleVerify => "schedule_verify",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "read_verify" => Some(Self::ReadVerify),
            "write_verify" => Some(Self::WriteVerify),
            "alarm_verify" => Some(Self::AlarmVerify),
            "schedule_verify" => Some(Self::ScheduleVerify),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::ReadVerify => "Read Verify",
            Self::WriteVerify => "Write Verify",
            Self::AlarmVerify => "Alarm Verify",
            Self::ScheduleVerify => "Schedule Verify",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemStatus {
    NotStarted,
    InProgress,
    Verified,
    Failed,
    Deferred,
}

impl ItemStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NotStarted => "not_started",
            Self::InProgress => "in_progress",
            Self::Verified => "verified",
            Self::Failed => "failed",
            Self::Deferred => "deferred",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "not_started" => Some(Self::NotStarted),
            "in_progress" => Some(Self::InProgress),
            "verified" => Some(Self::Verified),
            "failed" => Some(Self::Failed),
            "deferred" => Some(Self::Deferred),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::NotStarted => "Not Started",
            Self::InProgress => "In Progress",
            Self::Verified => "Verified",
            Self::Failed => "Failed",
            Self::Deferred => "Deferred",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommissionSession {
    pub id: i64,
    pub device_id: String,
    pub status: SessionStatus,
    pub created_ms: i64,
    pub updated_ms: i64,
    pub signed_off_by: Option<String>,
    pub signed_off_ms: Option<i64>,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommissionItem {
    pub id: i64,
    pub session_id: i64,
    pub device_id: String,
    pub point_id: String,
    pub item_type: ItemType,
    pub status: ItemStatus,
    pub verified_by: Option<String>,
    pub verified_ms: Option<i64>,
    pub expected_value: Option<String>,
    pub actual_value: Option<String>,
    pub notes: String,
}

/// Input for auto-generating checklist items during session creation.
#[derive(Debug, Clone)]
pub struct CommissionItemSeed {
    pub point_id: String,
    pub writable: bool,
    pub alarmable: bool,
    pub schedulable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommissionSummary {
    pub device_id: String,
    pub status: SessionStatus,
    pub total: usize,
    pub verified: usize,
    pub failed: usize,
    pub deferred: usize,
    pub not_started: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum CommissioningError {
    #[error("database error: {0}")]
    Db(String),
    #[error("channel closed")]
    ChannelClosed,
    #[error("not found")]
    NotFound,
}

// ----------------------------------------------------------------
// Commands sent to the SQLite thread
// ----------------------------------------------------------------

enum CommissionCmd {
    CreateSession {
        device_id: String,
        points: Vec<CommissionItemSeed>,
        reply: oneshot::Sender<Result<i64, CommissioningError>>,
    },
    GetSession {
        device_id: String,
        reply: oneshot::Sender<Option<CommissionSession>>,
    },
    ListSessions {
        reply: oneshot::Sender<Vec<CommissionSession>>,
    },
    ListItems {
        session_id: i64,
        reply: oneshot::Sender<Vec<CommissionItem>>,
    },
    UpdateItemStatus {
        item_id: i64,
        status: ItemStatus,
        verified_by: Option<String>,
        actual_value: Option<String>,
        notes: String,
        reply: oneshot::Sender<Result<(), CommissioningError>>,
    },
    UpdateSessionNotes {
        device_id: String,
        notes: String,
        reply: oneshot::Sender<Result<(), CommissioningError>>,
    },
    SignOffSession {
        device_id: String,
        user: String,
        reply: oneshot::Sender<Result<(), CommissioningError>>,
    },
    DeleteSession {
        device_id: String,
        reply: oneshot::Sender<Result<(), CommissioningError>>,
    },
    GetSummaries {
        reply: oneshot::Sender<Vec<CommissionSummary>>,
    },
}

// ----------------------------------------------------------------
// CommissioningStore — async handle to the SQLite thread
// ----------------------------------------------------------------

#[derive(Clone)]
pub struct CommissioningStore {
    cmd_tx: mpsc::UnboundedSender<CommissionCmd>,
}

impl CommissioningStore {
    pub async fn create_session(
        &self,
        device_id: &str,
        points: Vec<CommissionItemSeed>,
    ) -> Result<i64, CommissioningError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(CommissionCmd::CreateSession {
                device_id: device_id.to_string(),
                points,
                reply: reply_tx,
            })
            .map_err(|_| CommissioningError::ChannelClosed)?;
        reply_rx
            .await
            .map_err(|_| CommissioningError::ChannelClosed)?
    }

    pub async fn get_session(&self, device_id: &str) -> Option<CommissionSession> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(CommissionCmd::GetSession {
            device_id: device_id.to_string(),
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or(None)
    }

    pub async fn list_sessions(&self) -> Vec<CommissionSession> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .cmd_tx
            .send(CommissionCmd::ListSessions { reply: reply_tx });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn list_items(&self, session_id: i64) -> Vec<CommissionItem> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(CommissionCmd::ListItems {
            session_id,
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn update_item_status(
        &self,
        item_id: i64,
        status: ItemStatus,
        verified_by: Option<String>,
        actual_value: Option<String>,
        notes: String,
    ) -> Result<(), CommissioningError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(CommissionCmd::UpdateItemStatus {
                item_id,
                status,
                verified_by,
                actual_value,
                notes,
                reply: reply_tx,
            })
            .map_err(|_| CommissioningError::ChannelClosed)?;
        reply_rx
            .await
            .map_err(|_| CommissioningError::ChannelClosed)?
    }

    pub async fn update_session_notes(
        &self,
        device_id: &str,
        notes: String,
    ) -> Result<(), CommissioningError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(CommissionCmd::UpdateSessionNotes {
                device_id: device_id.to_string(),
                notes,
                reply: reply_tx,
            })
            .map_err(|_| CommissioningError::ChannelClosed)?;
        reply_rx
            .await
            .map_err(|_| CommissioningError::ChannelClosed)?
    }

    pub async fn sign_off_session(
        &self,
        device_id: &str,
        user: &str,
    ) -> Result<(), CommissioningError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(CommissionCmd::SignOffSession {
                device_id: device_id.to_string(),
                user: user.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| CommissioningError::ChannelClosed)?;
        reply_rx
            .await
            .map_err(|_| CommissioningError::ChannelClosed)?
    }

    pub async fn delete_session(&self, device_id: &str) -> Result<(), CommissioningError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(CommissionCmd::DeleteSession {
                device_id: device_id.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| CommissioningError::ChannelClosed)?;
        reply_rx
            .await
            .map_err(|_| CommissioningError::ChannelClosed)?
    }

    pub async fn get_summaries(&self) -> Vec<CommissionSummary> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .cmd_tx
            .send(CommissionCmd::GetSummaries { reply: reply_tx });
        reply_rx.await.unwrap_or_default()
    }
}

// ----------------------------------------------------------------
// SQLite schema & thread
// ----------------------------------------------------------------

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    label: "initial commissioning schema",
    sql: "
CREATE TABLE IF NOT EXISTS commission_session (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    device_id TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL DEFAULT 'not_started',
    created_ms INTEGER NOT NULL,
    updated_ms INTEGER NOT NULL,
    signed_off_by TEXT,
    signed_off_ms INTEGER,
    notes TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS commission_item (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id INTEGER NOT NULL REFERENCES commission_session(id) ON DELETE CASCADE,
    device_id TEXT NOT NULL,
    point_id TEXT NOT NULL,
    item_type TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'not_started',
    verified_by TEXT,
    verified_ms INTEGER,
    expected_value TEXT,
    actual_value TEXT,
    notes TEXT NOT NULL DEFAULT '',
    UNIQUE(session_id, point_id, item_type)
);
CREATE INDEX IF NOT EXISTS idx_ci_session ON commission_item(session_id);
CREATE INDEX IF NOT EXISTS idx_ci_status ON commission_item(status);
",
}];

fn run_sqlite_thread(db_path: &Path, mut cmd_rx: mpsc::UnboundedReceiver<CommissionCmd>) {
    let conn = rusqlite::Connection::open(db_path).expect("failed to open commissioning database");
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;",
    )
    .expect("failed to set pragmas");
    run_migrations(&conn, "commissioning", MIGRATIONS)
        .expect("commissioning: schema migration failed");

    while let Some(cmd) = cmd_rx.blocking_recv() {
        match cmd {
            CommissionCmd::CreateSession {
                device_id,
                points,
                reply,
            } => {
                let result = create_session_db(&conn, &device_id, &points);
                let _ = reply.send(result);
            }
            CommissionCmd::GetSession { device_id, reply } => {
                let _ = reply.send(get_session_db(&conn, &device_id));
            }
            CommissionCmd::ListSessions { reply } => {
                let _ = reply.send(list_sessions_db(&conn));
            }
            CommissionCmd::ListItems { session_id, reply } => {
                let _ = reply.send(list_items_db(&conn, session_id));
            }
            CommissionCmd::UpdateItemStatus {
                item_id,
                status,
                verified_by,
                actual_value,
                notes,
                reply,
            } => {
                let result = update_item_status_db(
                    &conn,
                    item_id,
                    status,
                    verified_by.as_deref(),
                    actual_value.as_deref(),
                    &notes,
                );
                let _ = reply.send(result);
            }
            CommissionCmd::UpdateSessionNotes {
                device_id,
                notes,
                reply,
            } => {
                let result = update_session_notes_db(&conn, &device_id, &notes);
                let _ = reply.send(result);
            }
            CommissionCmd::SignOffSession {
                device_id,
                user,
                reply,
            } => {
                let result = sign_off_session_db(&conn, &device_id, &user);
                let _ = reply.send(result);
            }
            CommissionCmd::DeleteSession { device_id, reply } => {
                let result = delete_session_db(&conn, &device_id);
                let _ = reply.send(result);
            }
            CommissionCmd::GetSummaries { reply } => {
                let _ = reply.send(get_summaries_db(&conn));
            }
        }
    }
}

// ----------------------------------------------------------------
// DB helpers
// ----------------------------------------------------------------

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

fn create_session_db(
    conn: &rusqlite::Connection,
    device_id: &str,
    points: &[CommissionItemSeed],
) -> Result<i64, CommissioningError> {
    let ts = now_ms();
    conn.execute(
        "INSERT INTO commission_session (device_id, status, created_ms, updated_ms, notes)
         VALUES (?1, 'not_started', ?2, ?3, '')",
        rusqlite::params![device_id, ts, ts],
    )
    .map_err(|e| CommissioningError::Db(e.to_string()))?;
    let session_id = conn.last_insert_rowid();

    for seed in points {
        // Always insert read_verify
        insert_item(
            conn,
            session_id,
            device_id,
            &seed.point_id,
            ItemType::ReadVerify,
        )?;
        if seed.writable {
            insert_item(
                conn,
                session_id,
                device_id,
                &seed.point_id,
                ItemType::WriteVerify,
            )?;
        }
        if seed.alarmable {
            insert_item(
                conn,
                session_id,
                device_id,
                &seed.point_id,
                ItemType::AlarmVerify,
            )?;
        }
        if seed.schedulable {
            insert_item(
                conn,
                session_id,
                device_id,
                &seed.point_id,
                ItemType::ScheduleVerify,
            )?;
        }
    }

    Ok(session_id)
}

fn insert_item(
    conn: &rusqlite::Connection,
    session_id: i64,
    device_id: &str,
    point_id: &str,
    item_type: ItemType,
) -> Result<(), CommissioningError> {
    conn.execute(
        "INSERT INTO commission_item (session_id, device_id, point_id, item_type, status, notes)
         VALUES (?1, ?2, ?3, ?4, 'not_started', '')",
        rusqlite::params![session_id, device_id, point_id, item_type.as_str()],
    )
    .map_err(|e| CommissioningError::Db(e.to_string()))?;
    Ok(())
}

fn get_session_db(conn: &rusqlite::Connection, device_id: &str) -> Option<CommissionSession> {
    conn.query_row(
        "SELECT id, device_id, status, created_ms, updated_ms, signed_off_by, signed_off_ms, notes
         FROM commission_session WHERE device_id = ?1",
        rusqlite::params![device_id],
        parse_session_row,
    )
    .ok()
}

fn list_sessions_db(conn: &rusqlite::Connection) -> Vec<CommissionSession> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT id, device_id, status, created_ms, updated_ms, signed_off_by, signed_off_ms, notes
             FROM commission_session ORDER BY created_ms DESC",
        )
        .unwrap();
    let rows = stmt.query_map([], parse_session_row).unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

fn parse_session_row(row: &rusqlite::Row) -> rusqlite::Result<CommissionSession> {
    let status_str: String = row.get(2)?;
    Ok(CommissionSession {
        id: row.get(0)?,
        device_id: row.get(1)?,
        status: SessionStatus::from_str(&status_str).unwrap_or(SessionStatus::NotStarted),
        created_ms: row.get(3)?,
        updated_ms: row.get(4)?,
        signed_off_by: row.get(5)?,
        signed_off_ms: row.get(6)?,
        notes: row.get(7)?,
    })
}

fn list_items_db(conn: &rusqlite::Connection, session_id: i64) -> Vec<CommissionItem> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT id, session_id, device_id, point_id, item_type, status, verified_by, verified_ms, expected_value, actual_value, notes
             FROM commission_item WHERE session_id = ?1 ORDER BY id",
        )
        .unwrap();
    let rows = stmt
        .query_map(rusqlite::params![session_id], parse_item_row)
        .unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

fn parse_item_row(row: &rusqlite::Row) -> rusqlite::Result<CommissionItem> {
    let item_type_str: String = row.get(4)?;
    let status_str: String = row.get(5)?;
    Ok(CommissionItem {
        id: row.get(0)?,
        session_id: row.get(1)?,
        device_id: row.get(2)?,
        point_id: row.get(3)?,
        item_type: ItemType::from_str(&item_type_str).unwrap_or(ItemType::ReadVerify),
        status: ItemStatus::from_str(&status_str).unwrap_or(ItemStatus::NotStarted),
        verified_by: row.get(6)?,
        verified_ms: row.get(7)?,
        expected_value: row.get(8)?,
        actual_value: row.get(9)?,
        notes: row.get(10)?,
    })
}

fn update_item_status_db(
    conn: &rusqlite::Connection,
    item_id: i64,
    status: ItemStatus,
    verified_by: Option<&str>,
    actual_value: Option<&str>,
    notes: &str,
) -> Result<(), CommissioningError> {
    let ts = now_ms();
    let verified_ms: Option<i64> = if status == ItemStatus::Verified {
        Some(ts)
    } else {
        None
    };
    let rows = conn
        .execute(
            "UPDATE commission_item SET status = ?1, verified_by = ?2, verified_ms = ?3, actual_value = ?4, notes = ?5 WHERE id = ?6",
            rusqlite::params![status.as_str(), verified_by, verified_ms, actual_value, notes, item_id],
        )
        .map_err(|e| CommissioningError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(CommissioningError::NotFound);
    }

    // Auto-update session status based on item statuses
    let session_id: i64 = conn
        .query_row(
            "SELECT session_id FROM commission_item WHERE id = ?1",
            rusqlite::params![item_id],
            |row| row.get(0),
        )
        .map_err(|e| CommissioningError::Db(e.to_string()))?;

    recompute_session_status(conn, session_id, ts)?;

    Ok(())
}

fn recompute_session_status(
    conn: &rusqlite::Connection,
    session_id: i64,
    ts: i64,
) -> Result<(), CommissioningError> {
    let mut stmt = conn
        .prepare_cached("SELECT status FROM commission_item WHERE session_id = ?1")
        .map_err(|e| CommissioningError::Db(e.to_string()))?;
    let statuses: Vec<String> = stmt
        .query_map(rusqlite::params![session_id], |row| row.get(0))
        .map_err(|e| CommissioningError::Db(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    let all_done = statuses.iter().all(|s| s == "verified" || s == "deferred");
    let any_started = statuses.iter().any(|s| s != "not_started");

    let new_status = if all_done && !statuses.is_empty() {
        "completed"
    } else if any_started {
        "in_progress"
    } else {
        "not_started"
    };

    // Don't demote a signed_off session
    let current_status: String = conn
        .query_row(
            "SELECT status FROM commission_session WHERE id = ?1",
            rusqlite::params![session_id],
            |row| row.get(0),
        )
        .map_err(|e| CommissioningError::Db(e.to_string()))?;
    if current_status == "signed_off" {
        return Ok(());
    }

    conn.execute(
        "UPDATE commission_session SET status = ?1, updated_ms = ?2 WHERE id = ?3",
        rusqlite::params![new_status, ts, session_id],
    )
    .map_err(|e| CommissioningError::Db(e.to_string()))?;

    Ok(())
}

fn update_session_notes_db(
    conn: &rusqlite::Connection,
    device_id: &str,
    notes: &str,
) -> Result<(), CommissioningError> {
    let ts = now_ms();
    let rows = conn
        .execute(
            "UPDATE commission_session SET notes = ?1, updated_ms = ?2 WHERE device_id = ?3",
            rusqlite::params![notes, ts, device_id],
        )
        .map_err(|e| CommissioningError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(CommissioningError::NotFound);
    }
    Ok(())
}

fn sign_off_session_db(
    conn: &rusqlite::Connection,
    device_id: &str,
    user: &str,
) -> Result<(), CommissioningError> {
    let session = get_session_db(conn, device_id).ok_or(CommissioningError::NotFound)?;
    if session.status != SessionStatus::Completed {
        return Err(CommissioningError::Db("Not all items verified".to_string()));
    }
    let ts = now_ms();
    conn.execute(
        "UPDATE commission_session SET status = 'signed_off', signed_off_by = ?1, signed_off_ms = ?2, updated_ms = ?3 WHERE device_id = ?4",
        rusqlite::params![user, ts, ts, device_id],
    )
    .map_err(|e| CommissioningError::Db(e.to_string()))?;
    Ok(())
}

fn delete_session_db(
    conn: &rusqlite::Connection,
    device_id: &str,
) -> Result<(), CommissioningError> {
    let rows = conn
        .execute(
            "DELETE FROM commission_session WHERE device_id = ?1",
            rusqlite::params![device_id],
        )
        .map_err(|e| CommissioningError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(CommissioningError::NotFound);
    }
    Ok(())
}

fn get_summaries_db(conn: &rusqlite::Connection) -> Vec<CommissionSummary> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT s.device_id, s.status,
                COUNT(*) as total,
                SUM(CASE WHEN i.status = 'verified' THEN 1 ELSE 0 END) as verified,
                SUM(CASE WHEN i.status = 'failed' THEN 1 ELSE 0 END) as failed,
                SUM(CASE WHEN i.status = 'deferred' THEN 1 ELSE 0 END) as deferred,
                SUM(CASE WHEN i.status = 'not_started' THEN 1 ELSE 0 END) as not_started
             FROM commission_session s
             LEFT JOIN commission_item i ON i.session_id = s.id
             GROUP BY s.id
             ORDER BY s.created_ms DESC",
        )
        .unwrap();
    let rows = stmt
        .query_map([], |row| {
            let status_str: String = row.get(1)?;
            Ok(CommissionSummary {
                device_id: row.get(0)?,
                status: SessionStatus::from_str(&status_str).unwrap_or(SessionStatus::NotStarted),
                total: row.get::<_, i64>(2)? as usize,
                verified: row.get::<_, i64>(3)? as usize,
                failed: row.get::<_, i64>(4)? as usize,
                deferred: row.get::<_, i64>(5)? as usize,
                not_started: row.get::<_, i64>(6)? as usize,
            })
        })
        .unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

// ----------------------------------------------------------------
// Constructor
// ----------------------------------------------------------------

pub fn start_commissioning_store_with_path(db_path: &Path) -> CommissioningStore {
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let path_clone = db_path.to_path_buf();
    std::thread::Builder::new()
        .name("commissioning-sqlite".into())
        .spawn(move || run_sqlite_thread(&path_clone, cmd_rx))
        .expect("failed to spawn commissioning SQLite thread");
    CommissioningStore { cmd_tx }
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn test_store(name: &str) -> CommissioningStore {
        let n = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("opencrate_test_commission_{name}_{n}"));
        let _ = std::fs::create_dir_all(&dir);
        let db = dir.join("commissioning.db");
        let _ = std::fs::remove_file(&db);
        let _ = std::fs::remove_file(db.with_extension("db-wal"));
        let _ = std::fs::remove_file(db.with_extension("db-shm"));
        start_commissioning_store_with_path(&db)
    }

    fn sample_seeds() -> Vec<CommissionItemSeed> {
        vec![
            CommissionItemSeed {
                point_id: "temp-sensor".to_string(),
                writable: true,
                alarmable: true,
                schedulable: false,
            },
            CommissionItemSeed {
                point_id: "fan-status".to_string(),
                writable: false,
                alarmable: false,
                schedulable: false,
            },
        ]
    }

    #[tokio::test]
    async fn create_session_generates_items() {
        let store = test_store("gen_items");
        let seeds = sample_seeds();
        // seed 0: read_verify + write_verify + alarm_verify = 3
        // seed 1: read_verify = 1
        // total = 4
        let session_id = store.create_session("ahu-1", seeds).await.unwrap();
        assert!(session_id > 0);

        let items = store.list_items(session_id).await;
        assert_eq!(items.len(), 4);

        // Verify types
        let types: Vec<&str> = items.iter().map(|i| i.item_type.as_str()).collect();
        assert!(types.contains(&"read_verify"));
        assert!(types.contains(&"write_verify"));
        assert!(types.contains(&"alarm_verify"));

        // All should be not_started
        assert!(items.iter().all(|i| i.status == ItemStatus::NotStarted));

        // Session should exist
        let session = store.get_session("ahu-1").await.unwrap();
        assert_eq!(session.status, SessionStatus::NotStarted);
        assert_eq!(session.device_id, "ahu-1");
    }

    #[tokio::test]
    async fn update_item_auto_promotes_session() {
        let store = test_store("auto_promote");
        let seeds = vec![
            CommissionItemSeed {
                point_id: "p1".to_string(),
                writable: false,
                alarmable: false,
                schedulable: false,
            },
            CommissionItemSeed {
                point_id: "p2".to_string(),
                writable: false,
                alarmable: false,
                schedulable: false,
            },
        ];
        let session_id = store.create_session("dev-1", seeds).await.unwrap();
        let items = store.list_items(session_id).await;
        assert_eq!(items.len(), 2);

        // Verify first item → session should become in_progress
        store
            .update_item_status(
                items[0].id,
                ItemStatus::Verified,
                Some("admin".to_string()),
                None,
                String::new(),
            )
            .await
            .unwrap();
        let session = store.get_session("dev-1").await.unwrap();
        assert_eq!(session.status, SessionStatus::InProgress);

        // Verify second item → session should become completed
        store
            .update_item_status(
                items[1].id,
                ItemStatus::Verified,
                Some("admin".to_string()),
                None,
                String::new(),
            )
            .await
            .unwrap();
        let session = store.get_session("dev-1").await.unwrap();
        assert_eq!(session.status, SessionStatus::Completed);
    }

    #[tokio::test]
    async fn sign_off_requires_completed() {
        let store = test_store("signoff_req");
        let seeds = vec![CommissionItemSeed {
            point_id: "p1".to_string(),
            writable: false,
            alarmable: false,
            schedulable: false,
        }];
        store.create_session("dev-2", seeds).await.unwrap();

        // Session is not_started — sign off should fail
        let result = store.sign_off_session("dev-2", "admin").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn sign_off_succeeds() {
        let store = test_store("signoff_ok");
        let seeds = vec![CommissionItemSeed {
            point_id: "p1".to_string(),
            writable: false,
            alarmable: false,
            schedulable: false,
        }];
        let session_id = store.create_session("dev-3", seeds).await.unwrap();
        let items = store.list_items(session_id).await;

        // Complete all items
        store
            .update_item_status(
                items[0].id,
                ItemStatus::Verified,
                Some("tech".to_string()),
                Some("72.5".to_string()),
                "looks good".to_string(),
            )
            .await
            .unwrap();

        // Sign off
        store.sign_off_session("dev-3", "manager").await.unwrap();
        let session = store.get_session("dev-3").await.unwrap();
        assert_eq!(session.status, SessionStatus::SignedOff);
        assert_eq!(session.signed_off_by, Some("manager".to_string()));
        assert!(session.signed_off_ms.is_some());
    }

    #[tokio::test]
    async fn delete_session_cascades() {
        let store = test_store("delete_cascade");
        let seeds = sample_seeds();
        let session_id = store.create_session("dev-4", seeds).await.unwrap();

        // Verify items exist
        let items = store.list_items(session_id).await;
        assert!(!items.is_empty());

        // Delete
        store.delete_session("dev-4").await.unwrap();

        // Session gone
        assert!(store.get_session("dev-4").await.is_none());

        // Items should be gone (CASCADE)
        let items = store.list_items(session_id).await;
        assert!(items.is_empty());

        // Delete non-existent
        let result = store.delete_session("dev-4").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn get_summaries() {
        let store = test_store("summaries");

        // Session 1: 2 seeds → 4 items (one writable+alarmable, one read-only)
        let seeds1 = sample_seeds();
        let sid1 = store.create_session("dev-a", seeds1).await.unwrap();
        let items1 = store.list_items(sid1).await;

        // Verify one item in session 1
        store
            .update_item_status(
                items1[0].id,
                ItemStatus::Verified,
                Some("tech".to_string()),
                None,
                String::new(),
            )
            .await
            .unwrap();
        // Fail one item
        store
            .update_item_status(
                items1[1].id,
                ItemStatus::Failed,
                None,
                None,
                "out of range".to_string(),
            )
            .await
            .unwrap();

        // Session 2: 1 seed → 1 item, all not_started
        let seeds2 = vec![CommissionItemSeed {
            point_id: "sensor-1".to_string(),
            writable: false,
            alarmable: false,
            schedulable: false,
        }];
        store.create_session("dev-b", seeds2).await.unwrap();

        let summaries = store.get_summaries().await;
        assert_eq!(summaries.len(), 2);

        // Find dev-a summary
        let sa = summaries.iter().find(|s| s.device_id == "dev-a").unwrap();
        assert_eq!(sa.total, 4);
        assert_eq!(sa.verified, 1);
        assert_eq!(sa.failed, 1);
        assert_eq!(sa.not_started, 2);
        assert_eq!(sa.status, SessionStatus::InProgress);

        // Find dev-b summary
        let sb = summaries.iter().find(|s| s.device_id == "dev-b").unwrap();
        assert_eq!(sb.total, 1);
        assert_eq!(sb.not_started, 1);
        assert_eq!(sb.status, SessionStatus::NotStarted);
    }
}
