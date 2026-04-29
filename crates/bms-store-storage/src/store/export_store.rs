use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::{mpsc, oneshot, watch};

use super::migration::{run_migrations, Migration};
use crate::export::{ExportConnectorConfig, ExportStatus};

// ----------------------------------------------------------------
// Error type
// ----------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum ExportStoreError {
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

enum ExportCmd {
    // Connector CRUD
    CreateConnector {
        id: String,
        name: String,
        connector_type: String,
        config: String,
        on_values: bool,
        on_alarms: bool,
        on_fdd: bool,
        reply: oneshot::Sender<Result<(), ExportStoreError>>,
    },
    UpdateConnector {
        id: String,
        name: String,
        connector_type: String,
        config: String,
        enabled: bool,
        on_values: bool,
        on_alarms: bool,
        on_fdd: bool,
        reply: oneshot::Sender<Result<(), ExportStoreError>>,
    },
    DeleteConnector {
        id: String,
        reply: oneshot::Sender<Result<(), ExportStoreError>>,
    },
    ListConnectors {
        reply: oneshot::Sender<Vec<ExportConnectorConfig>>,
    },
    GetConnector {
        id: String,
        reply: oneshot::Sender<Option<ExportConnectorConfig>>,
    },
    ListEnabledConnectors {
        reply: oneshot::Sender<Vec<ExportConnectorConfig>>,
    },
    // Status
    GetStatus {
        connector_id: String,
        reply: oneshot::Sender<Option<ExportStatus>>,
    },
    ListStatuses {
        reply: oneshot::Sender<Vec<ExportStatus>>,
    },
    UpdateStatus {
        connector_id: String,
        last_sync_ms: i64,
        rows_exported: i64,
        last_error: Option<String>,
        state: String,
        reply: oneshot::Sender<Result<(), ExportStoreError>>,
    },
}

// ----------------------------------------------------------------
// ExportStore — async handle to the SQLite thread
// ----------------------------------------------------------------

#[derive(Clone)]
pub struct ExportStore {
    cmd_tx: mpsc::UnboundedSender<ExportCmd>,
    version_tx: watch::Sender<u64>,
}

impl ExportStore {
    /// Subscribe to version changes. The version is bumped on any connector mutation.
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.version_tx.subscribe()
    }

    // ---- Connector CRUD ----

    #[allow(clippy::too_many_arguments)]
    pub async fn create_connector(
        &self,
        id: &str,
        name: &str,
        connector_type: &str,
        config: &str,
        on_values: bool,
        on_alarms: bool,
        on_fdd: bool,
    ) -> Result<(), ExportStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(ExportCmd::CreateConnector {
                id: id.to_string(),
                name: name.to_string(),
                connector_type: connector_type.to_string(),
                config: config.to_string(),
                on_values,
                on_alarms,
                on_fdd,
                reply: reply_tx,
            })
            .map_err(|_| ExportStoreError::ChannelClosed)?;
        let result = reply_rx
            .await
            .map_err(|_| ExportStoreError::ChannelClosed)?;
        if result.is_ok() {
            self.version_tx.send_modify(|v| *v += 1);
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_connector(
        &self,
        id: &str,
        name: &str,
        connector_type: &str,
        config: &str,
        enabled: bool,
        on_values: bool,
        on_alarms: bool,
        on_fdd: bool,
    ) -> Result<(), ExportStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(ExportCmd::UpdateConnector {
                id: id.to_string(),
                name: name.to_string(),
                connector_type: connector_type.to_string(),
                config: config.to_string(),
                enabled,
                on_values,
                on_alarms,
                on_fdd,
                reply: reply_tx,
            })
            .map_err(|_| ExportStoreError::ChannelClosed)?;
        let result = reply_rx
            .await
            .map_err(|_| ExportStoreError::ChannelClosed)?;
        if result.is_ok() {
            self.version_tx.send_modify(|v| *v += 1);
        }
        result
    }

    pub async fn delete_connector(&self, id: &str) -> Result<(), ExportStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(ExportCmd::DeleteConnector {
                id: id.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| ExportStoreError::ChannelClosed)?;
        let result = reply_rx
            .await
            .map_err(|_| ExportStoreError::ChannelClosed)?;
        if result.is_ok() {
            self.version_tx.send_modify(|v| *v += 1);
        }
        result
    }

    pub async fn list_connectors(&self) -> Vec<ExportConnectorConfig> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .cmd_tx
            .send(ExportCmd::ListConnectors { reply: reply_tx });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn get_connector(&self, id: &str) -> Option<ExportConnectorConfig> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(ExportCmd::GetConnector {
            id: id.to_string(),
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or(None)
    }

    pub async fn list_enabled_connectors(&self) -> Vec<ExportConnectorConfig> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .cmd_tx
            .send(ExportCmd::ListEnabledConnectors { reply: reply_tx });
        reply_rx.await.unwrap_or_default()
    }

    // ---- Status ----

    pub async fn get_status(&self, connector_id: &str) -> Option<ExportStatus> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(ExportCmd::GetStatus {
            connector_id: connector_id.to_string(),
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or(None)
    }

    pub async fn list_statuses(&self) -> Vec<ExportStatus> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .cmd_tx
            .send(ExportCmd::ListStatuses { reply: reply_tx });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn update_status(
        &self,
        connector_id: &str,
        last_sync_ms: i64,
        rows_exported: i64,
        last_error: Option<&str>,
        state: &str,
    ) -> Result<(), ExportStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(ExportCmd::UpdateStatus {
                connector_id: connector_id.to_string(),
                last_sync_ms,
                rows_exported,
                last_error: last_error.map(|s| s.to_string()),
                state: state.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| ExportStoreError::ChannelClosed)?;
        reply_rx
            .await
            .map_err(|_| ExportStoreError::ChannelClosed)?
    }
}

// ----------------------------------------------------------------
// SQLite schema & thread
// ----------------------------------------------------------------

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    label: "initial export schema",
    sql: "
CREATE TABLE IF NOT EXISTS export_connector (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    connector_type  TEXT NOT NULL,
    config          TEXT NOT NULL,
    enabled         INTEGER NOT NULL DEFAULT 1,
    on_values       INTEGER NOT NULL DEFAULT 1,
    on_alarms       INTEGER NOT NULL DEFAULT 1,
    on_fdd          INTEGER NOT NULL DEFAULT 1,
    created_ms      INTEGER NOT NULL,
    updated_ms      INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS export_status (
    connector_id    TEXT PRIMARY KEY,
    last_sync_ms    INTEGER NOT NULL DEFAULT 0,
    rows_exported   INTEGER NOT NULL DEFAULT 0,
    last_error      TEXT,
    state           TEXT NOT NULL DEFAULT 'idle',
    FOREIGN KEY (connector_id) REFERENCES export_connector(id) ON DELETE CASCADE
);
",
}];

fn run_sqlite_thread(db_path: &Path, mut cmd_rx: mpsc::UnboundedReceiver<ExportCmd>) {
    let conn = rusqlite::Connection::open(db_path).expect("failed to open export database");
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;",
    )
    .expect("failed to set pragmas");
    run_migrations(&conn, "export", MIGRATIONS).expect("export: schema migration failed");

    while let Some(cmd) = cmd_rx.blocking_recv() {
        match cmd {
            // ---- Connectors ----
            ExportCmd::CreateConnector {
                id,
                name,
                connector_type,
                config,
                on_values,
                on_alarms,
                on_fdd,
                reply,
            } => {
                let result = create_connector_db(
                    &conn,
                    &id,
                    &name,
                    &connector_type,
                    &config,
                    on_values,
                    on_alarms,
                    on_fdd,
                );
                let _ = reply.send(result);
            }
            ExportCmd::UpdateConnector {
                id,
                name,
                connector_type,
                config,
                enabled,
                on_values,
                on_alarms,
                on_fdd,
                reply,
            } => {
                let result = update_connector_db(
                    &conn,
                    &id,
                    &name,
                    &connector_type,
                    &config,
                    enabled,
                    on_values,
                    on_alarms,
                    on_fdd,
                );
                let _ = reply.send(result);
            }
            ExportCmd::DeleteConnector { id, reply } => {
                let _ = reply.send(delete_connector_db(&conn, &id));
            }
            ExportCmd::ListConnectors { reply } => {
                let _ = reply.send(list_connectors_db(&conn));
            }
            ExportCmd::GetConnector { id, reply } => {
                let _ = reply.send(get_connector_db(&conn, &id));
            }
            ExportCmd::ListEnabledConnectors { reply } => {
                let _ = reply.send(list_enabled_connectors_db(&conn));
            }
            // ---- Status ----
            ExportCmd::GetStatus {
                connector_id,
                reply,
            } => {
                let _ = reply.send(get_status_db(&conn, &connector_id));
            }
            ExportCmd::ListStatuses { reply } => {
                let _ = reply.send(list_statuses_db(&conn));
            }
            ExportCmd::UpdateStatus {
                connector_id,
                last_sync_ms,
                rows_exported,
                last_error,
                state,
                reply,
            } => {
                let _ = reply.send(update_status_db(
                    &conn,
                    &connector_id,
                    last_sync_ms,
                    rows_exported,
                    last_error.as_deref(),
                    &state,
                ));
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

// ---- Connectors ----

#[allow(clippy::too_many_arguments)]
fn create_connector_db(
    conn: &rusqlite::Connection,
    id: &str,
    name: &str,
    connector_type: &str,
    config: &str,
    on_values: bool,
    on_alarms: bool,
    on_fdd: bool,
) -> Result<(), ExportStoreError> {
    let ts = now_ms();
    conn.execute(
        "INSERT INTO export_connector (id, name, connector_type, config, enabled, on_values, on_alarms, on_fdd, created_ms, updated_ms)
         VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            id,
            name,
            connector_type,
            config,
            on_values as i32,
            on_alarms as i32,
            on_fdd as i32,
            ts,
            ts,
        ],
    )
    .map_err(|e| ExportStoreError::Db(e.to_string()))?;

    // Initialize status row
    conn.execute(
        "INSERT INTO export_status (connector_id, last_sync_ms, rows_exported, state)
         VALUES (?1, 0, 0, 'idle')",
        rusqlite::params![id],
    )
    .map_err(|e| ExportStoreError::Db(e.to_string()))?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn update_connector_db(
    conn: &rusqlite::Connection,
    id: &str,
    name: &str,
    connector_type: &str,
    config: &str,
    enabled: bool,
    on_values: bool,
    on_alarms: bool,
    on_fdd: bool,
) -> Result<(), ExportStoreError> {
    let ts = now_ms();
    let rows = conn
        .execute(
            "UPDATE export_connector SET name = ?1, connector_type = ?2, config = ?3, enabled = ?4, on_values = ?5, on_alarms = ?6, on_fdd = ?7, updated_ms = ?8 WHERE id = ?9",
            rusqlite::params![
                name,
                connector_type,
                config,
                enabled as i32,
                on_values as i32,
                on_alarms as i32,
                on_fdd as i32,
                ts,
                id,
            ],
        )
        .map_err(|e| ExportStoreError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(ExportStoreError::NotFound);
    }
    Ok(())
}

fn delete_connector_db(conn: &rusqlite::Connection, id: &str) -> Result<(), ExportStoreError> {
    // Status row is cascade-deleted via FK
    let rows = conn
        .execute(
            "DELETE FROM export_connector WHERE id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| ExportStoreError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(ExportStoreError::NotFound);
    }
    Ok(())
}

const CONNECTOR_COLS: &str =
    "id, name, connector_type, config, enabled, on_values, on_alarms, on_fdd, created_ms, updated_ms";

fn parse_connector_row(row: &rusqlite::Row) -> rusqlite::Result<ExportConnectorConfig> {
    Ok(ExportConnectorConfig {
        id: row.get(0)?,
        name: row.get(1)?,
        connector_type: row.get(2)?,
        config: row.get(3)?,
        enabled: row.get::<_, i32>(4)? != 0,
        on_values: row.get::<_, i32>(5)? != 0,
        on_alarms: row.get::<_, i32>(6)? != 0,
        on_fdd: row.get::<_, i32>(7)? != 0,
        created_ms: row.get(8)?,
        updated_ms: row.get(9)?,
    })
}

fn list_connectors_db(conn: &rusqlite::Connection) -> Vec<ExportConnectorConfig> {
    let sql = format!(
        "SELECT {} FROM export_connector ORDER BY created_ms",
        CONNECTOR_COLS
    );
    let mut stmt = conn.prepare_cached(&sql).unwrap();
    let rows = stmt.query_map([], parse_connector_row).unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

fn get_connector_db(conn: &rusqlite::Connection, id: &str) -> Option<ExportConnectorConfig> {
    let sql = format!(
        "SELECT {} FROM export_connector WHERE id = ?1",
        CONNECTOR_COLS
    );
    let mut stmt = conn.prepare_cached(&sql).unwrap();
    stmt.query_row(rusqlite::params![id], parse_connector_row)
        .ok()
}

fn list_enabled_connectors_db(conn: &rusqlite::Connection) -> Vec<ExportConnectorConfig> {
    let sql = format!(
        "SELECT {} FROM export_connector WHERE enabled = 1 ORDER BY created_ms",
        CONNECTOR_COLS
    );
    let mut stmt = conn.prepare_cached(&sql).unwrap();
    let rows = stmt.query_map([], parse_connector_row).unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

// ---- Status ----

fn parse_status_row(row: &rusqlite::Row) -> rusqlite::Result<ExportStatus> {
    Ok(ExportStatus {
        connector_id: row.get(0)?,
        last_sync_ms: row.get(1)?,
        rows_exported: row.get(2)?,
        last_error: row.get(3)?,
        state: row.get(4)?,
    })
}

fn get_status_db(conn: &rusqlite::Connection, connector_id: &str) -> Option<ExportStatus> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT connector_id, last_sync_ms, rows_exported, last_error, state FROM export_status WHERE connector_id = ?1",
        )
        .unwrap();
    stmt.query_row(rusqlite::params![connector_id], parse_status_row)
        .ok()
}

fn list_statuses_db(conn: &rusqlite::Connection) -> Vec<ExportStatus> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT connector_id, last_sync_ms, rows_exported, last_error, state FROM export_status",
        )
        .unwrap();
    let rows = stmt.query_map([], parse_status_row).unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

fn update_status_db(
    conn: &rusqlite::Connection,
    connector_id: &str,
    last_sync_ms: i64,
    rows_exported: i64,
    last_error: Option<&str>,
    state: &str,
) -> Result<(), ExportStoreError> {
    conn.execute(
        "INSERT INTO export_status (connector_id, last_sync_ms, rows_exported, last_error, state)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(connector_id) DO UPDATE SET last_sync_ms = ?2, rows_exported = ?3, last_error = ?4, state = ?5",
        rusqlite::params![connector_id, last_sync_ms, rows_exported, last_error, state],
    )
    .map_err(|e| ExportStoreError::Db(e.to_string()))?;
    Ok(())
}

// ----------------------------------------------------------------
// Constructor
// ----------------------------------------------------------------

pub fn start_export_store_with_path(db_path: &Path) -> ExportStore {
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (version_tx, _version_rx) = watch::channel(0u64);
    let path_clone = db_path.to_path_buf();
    std::thread::Builder::new()
        .name("export-sqlite".into())
        .spawn(move || run_sqlite_thread(&path_clone, cmd_rx))
        .expect("failed to spawn export SQLite thread");
    ExportStore { cmd_tx, version_tx }
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> ExportStore {
        let path =
            std::env::temp_dir().join(format!("opencrate_test_export_{}.db", uuid::Uuid::new_v4()));
        start_export_store_with_path(&path)
    }

    #[tokio::test]
    async fn create_and_list_connectors() {
        let store = temp_store();
        let config =
            r#"{"url":"http://localhost:8086","token":"tok","org":"myorg","bucket":"bms"}"#;
        store
            .create_connector(
                "exp-1",
                "InfluxDB Prod",
                "influxdb",
                config,
                true,
                true,
                true,
            )
            .await
            .unwrap();

        let connectors = store.list_connectors().await;
        assert_eq!(connectors.len(), 1);
        assert_eq!(connectors[0].id, "exp-1");
        assert_eq!(connectors[0].name, "InfluxDB Prod");
        assert_eq!(connectors[0].connector_type, "influxdb");
        assert!(connectors[0].enabled);
        assert!(connectors[0].on_values);
        assert!(connectors[0].on_alarms);
        assert!(connectors[0].on_fdd);
    }

    #[tokio::test]
    async fn update_connector() {
        let store = temp_store();
        let config =
            r#"{"url":"http://localhost:8086","token":"tok","org":"myorg","bucket":"bms"}"#;
        store
            .create_connector("exp-1", "Test", "influxdb", config, true, true, true)
            .await
            .unwrap();

        let new_config =
            r#"{"url":"http://influx:8086","token":"newtok","org":"myorg","bucket":"bms2"}"#;
        store
            .update_connector(
                "exp-1", "Updated", "influxdb", new_config, false, true, false, true,
            )
            .await
            .unwrap();

        let c = store.get_connector("exp-1").await.unwrap();
        assert_eq!(c.name, "Updated");
        assert!(!c.enabled);
        assert!(!c.on_alarms);
        assert!(c.on_fdd);
    }

    #[tokio::test]
    async fn delete_connector_cascades_status() {
        let store = temp_store();
        let config = r#"{"url":"http://localhost:8086","token":"tok","org":"o","bucket":"b"}"#;
        store
            .create_connector("exp-1", "Test", "influxdb", config, true, true, true)
            .await
            .unwrap();

        // Status row should exist after create
        let status = store.get_status("exp-1").await;
        assert!(status.is_some());

        store.delete_connector("exp-1").await.unwrap();
        assert!(store.get_connector("exp-1").await.is_none());
        // Status should be cascade-deleted
        let status = store.get_status("exp-1").await;
        assert!(status.is_none());
    }

    #[tokio::test]
    async fn enabled_filter() {
        let store = temp_store();
        let config = r#"{"url":"http://a:8086","token":"t","org":"o","bucket":"b"}"#;
        store
            .create_connector("exp-1", "Enabled", "influxdb", config, true, true, true)
            .await
            .unwrap();
        store
            .create_connector("exp-2", "Disabled", "influxdb", config, true, true, true)
            .await
            .unwrap();
        store
            .update_connector(
                "exp-2", "Disabled", "influxdb", config, false, true, true, true,
            )
            .await
            .unwrap();

        let enabled = store.list_enabled_connectors().await;
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].id, "exp-1");
    }

    #[tokio::test]
    async fn status_update_and_read() {
        let store = temp_store();
        let config = r#"{"url":"http://a:8086","token":"t","org":"o","bucket":"b"}"#;
        store
            .create_connector("exp-1", "Test", "influxdb", config, true, true, true)
            .await
            .unwrap();

        // Initial status
        let status = store.get_status("exp-1").await.unwrap();
        assert_eq!(status.state, "idle");
        assert_eq!(status.rows_exported, 0);

        // Update status
        store
            .update_status("exp-1", 1000, 500, None, "syncing")
            .await
            .unwrap();

        let status = store.get_status("exp-1").await.unwrap();
        assert_eq!(status.state, "syncing");
        assert_eq!(status.rows_exported, 500);
        assert_eq!(status.last_sync_ms, 1000);
        assert!(status.last_error.is_none());

        // Update with error
        store
            .update_status("exp-1", 2000, 500, Some("connection refused"), "error")
            .await
            .unwrap();

        let status = store.get_status("exp-1").await.unwrap();
        assert_eq!(status.state, "error");
        assert_eq!(status.last_error.as_deref(), Some("connection refused"));
    }

    #[tokio::test]
    async fn list_statuses() {
        let store = temp_store();
        let config = r#"{"url":"http://a:8086","token":"t","org":"o","bucket":"b"}"#;
        store
            .create_connector("exp-1", "A", "influxdb", config, true, true, true)
            .await
            .unwrap();
        store
            .create_connector("exp-2", "B", "influxdb", config, true, true, true)
            .await
            .unwrap();

        let statuses = store.list_statuses().await;
        assert_eq!(statuses.len(), 2);
    }
}
