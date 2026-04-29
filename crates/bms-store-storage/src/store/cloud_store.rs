use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::{mpsc, oneshot, watch};

use super::migration::{run_migrations, Migration};
use crate::cloud::{CloudBridgeConfig, CloudBridgeStatus};

// ----------------------------------------------------------------
// Error type
// ----------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum CloudStoreError {
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

enum CloudCmd {
    // Bridge CRUD
    CreateBridge {
        id: String,
        name: String,
        provider: String,
        config: String,
        on_values: bool,
        on_alarms: bool,
        on_fdd: bool,
        on_device_status: bool,
        reply: oneshot::Sender<Result<(), CloudStoreError>>,
    },
    UpdateBridge {
        id: String,
        name: String,
        provider: String,
        config: String,
        enabled: bool,
        on_values: bool,
        on_alarms: bool,
        on_fdd: bool,
        on_device_status: bool,
        reply: oneshot::Sender<Result<(), CloudStoreError>>,
    },
    DeleteBridge {
        id: String,
        reply: oneshot::Sender<Result<(), CloudStoreError>>,
    },
    ListBridges {
        reply: oneshot::Sender<Vec<CloudBridgeConfig>>,
    },
    GetBridge {
        id: String,
        reply: oneshot::Sender<Option<CloudBridgeConfig>>,
    },
    ListEnabledBridges {
        reply: oneshot::Sender<Vec<CloudBridgeConfig>>,
    },
    // Status
    GetStatus {
        bridge_id: String,
        reply: oneshot::Sender<Option<CloudBridgeStatus>>,
    },
    ListStatuses {
        reply: oneshot::Sender<Vec<CloudBridgeStatus>>,
    },
    UpdateStatus {
        bridge_id: String,
        last_publish_ms: i64,
        messages_published: i64,
        last_error: Option<String>,
        state: String,
        reply: oneshot::Sender<Result<(), CloudStoreError>>,
    },
}

// ----------------------------------------------------------------
// CloudStore — async handle to the SQLite thread
// ----------------------------------------------------------------

#[derive(Clone)]
pub struct CloudStore {
    cmd_tx: mpsc::UnboundedSender<CloudCmd>,
    version_tx: watch::Sender<u64>,
}

impl CloudStore {
    /// Subscribe to version changes. The version is bumped on any bridge mutation.
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.version_tx.subscribe()
    }

    // ---- Bridge CRUD ----

    #[allow(clippy::too_many_arguments)]
    pub async fn create_bridge(
        &self,
        id: &str,
        name: &str,
        provider: &str,
        config: &str,
        on_values: bool,
        on_alarms: bool,
        on_fdd: bool,
        on_device_status: bool,
    ) -> Result<(), CloudStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(CloudCmd::CreateBridge {
                id: id.to_string(),
                name: name.to_string(),
                provider: provider.to_string(),
                config: config.to_string(),
                on_values,
                on_alarms,
                on_fdd,
                on_device_status,
                reply: reply_tx,
            })
            .map_err(|_| CloudStoreError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| CloudStoreError::ChannelClosed)?;
        if result.is_ok() {
            self.version_tx.send_modify(|v| *v += 1);
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_bridge(
        &self,
        id: &str,
        name: &str,
        provider: &str,
        config: &str,
        enabled: bool,
        on_values: bool,
        on_alarms: bool,
        on_fdd: bool,
        on_device_status: bool,
    ) -> Result<(), CloudStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(CloudCmd::UpdateBridge {
                id: id.to_string(),
                name: name.to_string(),
                provider: provider.to_string(),
                config: config.to_string(),
                enabled,
                on_values,
                on_alarms,
                on_fdd,
                on_device_status,
                reply: reply_tx,
            })
            .map_err(|_| CloudStoreError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| CloudStoreError::ChannelClosed)?;
        if result.is_ok() {
            self.version_tx.send_modify(|v| *v += 1);
        }
        result
    }

    pub async fn delete_bridge(&self, id: &str) -> Result<(), CloudStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(CloudCmd::DeleteBridge {
                id: id.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| CloudStoreError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| CloudStoreError::ChannelClosed)?;
        if result.is_ok() {
            self.version_tx.send_modify(|v| *v += 1);
        }
        result
    }

    pub async fn list_bridges(&self) -> Vec<CloudBridgeConfig> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(CloudCmd::ListBridges { reply: reply_tx });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn get_bridge(&self, id: &str) -> Option<CloudBridgeConfig> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(CloudCmd::GetBridge {
            id: id.to_string(),
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or(None)
    }

    pub async fn list_enabled_bridges(&self) -> Vec<CloudBridgeConfig> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .cmd_tx
            .send(CloudCmd::ListEnabledBridges { reply: reply_tx });
        reply_rx.await.unwrap_or_default()
    }

    // ---- Status ----

    pub async fn get_status(&self, bridge_id: &str) -> Option<CloudBridgeStatus> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(CloudCmd::GetStatus {
            bridge_id: bridge_id.to_string(),
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or(None)
    }

    pub async fn list_statuses(&self) -> Vec<CloudBridgeStatus> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(CloudCmd::ListStatuses { reply: reply_tx });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn update_status(
        &self,
        bridge_id: &str,
        last_publish_ms: i64,
        messages_published: i64,
        last_error: Option<&str>,
        state: &str,
    ) -> Result<(), CloudStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(CloudCmd::UpdateStatus {
                bridge_id: bridge_id.to_string(),
                last_publish_ms,
                messages_published,
                last_error: last_error.map(|s| s.to_string()),
                state: state.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| CloudStoreError::ChannelClosed)?;
        reply_rx.await.map_err(|_| CloudStoreError::ChannelClosed)?
    }
}

// ----------------------------------------------------------------
// SQLite schema & thread
// ----------------------------------------------------------------

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    label: "initial cloud bridge schema",
    sql: "
CREATE TABLE IF NOT EXISTS cloud_bridge (
    id               TEXT PRIMARY KEY,
    name             TEXT NOT NULL,
    provider         TEXT NOT NULL,
    config           TEXT NOT NULL,
    enabled          INTEGER NOT NULL DEFAULT 1,
    on_values        INTEGER NOT NULL DEFAULT 1,
    on_alarms        INTEGER NOT NULL DEFAULT 1,
    on_fdd           INTEGER NOT NULL DEFAULT 1,
    on_device_status INTEGER NOT NULL DEFAULT 0,
    created_ms       INTEGER NOT NULL,
    updated_ms       INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS cloud_bridge_status (
    bridge_id          TEXT PRIMARY KEY,
    last_publish_ms    INTEGER NOT NULL DEFAULT 0,
    messages_published INTEGER NOT NULL DEFAULT 0,
    last_error         TEXT,
    state              TEXT NOT NULL DEFAULT 'idle',
    FOREIGN KEY (bridge_id) REFERENCES cloud_bridge(id) ON DELETE CASCADE
);
",
}];

fn run_sqlite_thread(db_path: &Path, mut cmd_rx: mpsc::UnboundedReceiver<CloudCmd>) {
    let conn = rusqlite::Connection::open(db_path).expect("failed to open cloud bridge database");
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;",
    )
    .expect("failed to set pragmas");
    run_migrations(&conn, "cloud_bridge", MIGRATIONS)
        .expect("cloud_bridge: schema migration failed");

    while let Some(cmd) = cmd_rx.blocking_recv() {
        match cmd {
            // ---- Bridges ----
            CloudCmd::CreateBridge {
                id,
                name,
                provider,
                config,
                on_values,
                on_alarms,
                on_fdd,
                on_device_status,
                reply,
            } => {
                let result = create_bridge_db(
                    &conn,
                    &id,
                    &name,
                    &provider,
                    &config,
                    on_values,
                    on_alarms,
                    on_fdd,
                    on_device_status,
                );
                let _ = reply.send(result);
            }
            CloudCmd::UpdateBridge {
                id,
                name,
                provider,
                config,
                enabled,
                on_values,
                on_alarms,
                on_fdd,
                on_device_status,
                reply,
            } => {
                let result = update_bridge_db(
                    &conn,
                    &id,
                    &name,
                    &provider,
                    &config,
                    enabled,
                    on_values,
                    on_alarms,
                    on_fdd,
                    on_device_status,
                );
                let _ = reply.send(result);
            }
            CloudCmd::DeleteBridge { id, reply } => {
                let _ = reply.send(delete_bridge_db(&conn, &id));
            }
            CloudCmd::ListBridges { reply } => {
                let _ = reply.send(list_bridges_db(&conn));
            }
            CloudCmd::GetBridge { id, reply } => {
                let _ = reply.send(get_bridge_db(&conn, &id));
            }
            CloudCmd::ListEnabledBridges { reply } => {
                let _ = reply.send(list_enabled_bridges_db(&conn));
            }
            // ---- Status ----
            CloudCmd::GetStatus { bridge_id, reply } => {
                let _ = reply.send(get_status_db(&conn, &bridge_id));
            }
            CloudCmd::ListStatuses { reply } => {
                let _ = reply.send(list_statuses_db(&conn));
            }
            CloudCmd::UpdateStatus {
                bridge_id,
                last_publish_ms,
                messages_published,
                last_error,
                state,
                reply,
            } => {
                let _ = reply.send(update_status_db(
                    &conn,
                    &bridge_id,
                    last_publish_ms,
                    messages_published,
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

// ---- Bridges ----

#[allow(clippy::too_many_arguments)]
fn create_bridge_db(
    conn: &rusqlite::Connection,
    id: &str,
    name: &str,
    provider: &str,
    config: &str,
    on_values: bool,
    on_alarms: bool,
    on_fdd: bool,
    on_device_status: bool,
) -> Result<(), CloudStoreError> {
    let ts = now_ms();
    conn.execute(
        "INSERT INTO cloud_bridge (id, name, provider, config, enabled, on_values, on_alarms, on_fdd, on_device_status, created_ms, updated_ms)
         VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![
            id,
            name,
            provider,
            config,
            on_values as i32,
            on_alarms as i32,
            on_fdd as i32,
            on_device_status as i32,
            ts,
            ts,
        ],
    )
    .map_err(|e| CloudStoreError::Db(e.to_string()))?;

    // Initialize status row
    conn.execute(
        "INSERT INTO cloud_bridge_status (bridge_id, last_publish_ms, messages_published, state)
         VALUES (?1, 0, 0, 'idle')",
        rusqlite::params![id],
    )
    .map_err(|e| CloudStoreError::Db(e.to_string()))?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn update_bridge_db(
    conn: &rusqlite::Connection,
    id: &str,
    name: &str,
    provider: &str,
    config: &str,
    enabled: bool,
    on_values: bool,
    on_alarms: bool,
    on_fdd: bool,
    on_device_status: bool,
) -> Result<(), CloudStoreError> {
    let ts = now_ms();
    let rows = conn
        .execute(
            "UPDATE cloud_bridge SET name = ?1, provider = ?2, config = ?3, enabled = ?4, on_values = ?5, on_alarms = ?6, on_fdd = ?7, on_device_status = ?8, updated_ms = ?9 WHERE id = ?10",
            rusqlite::params![
                name,
                provider,
                config,
                enabled as i32,
                on_values as i32,
                on_alarms as i32,
                on_fdd as i32,
                on_device_status as i32,
                ts,
                id,
            ],
        )
        .map_err(|e| CloudStoreError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(CloudStoreError::NotFound);
    }
    Ok(())
}

fn delete_bridge_db(conn: &rusqlite::Connection, id: &str) -> Result<(), CloudStoreError> {
    let rows = conn
        .execute(
            "DELETE FROM cloud_bridge WHERE id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| CloudStoreError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(CloudStoreError::NotFound);
    }
    Ok(())
}

const BRIDGE_COLS: &str =
    "id, name, provider, config, enabled, on_values, on_alarms, on_fdd, on_device_status, created_ms, updated_ms";

fn parse_bridge_row(row: &rusqlite::Row) -> rusqlite::Result<CloudBridgeConfig> {
    Ok(CloudBridgeConfig {
        id: row.get(0)?,
        name: row.get(1)?,
        provider: row.get(2)?,
        config: row.get(3)?,
        enabled: row.get::<_, i32>(4)? != 0,
        on_values: row.get::<_, i32>(5)? != 0,
        on_alarms: row.get::<_, i32>(6)? != 0,
        on_fdd: row.get::<_, i32>(7)? != 0,
        on_device_status: row.get::<_, i32>(8)? != 0,
        created_ms: row.get(9)?,
        updated_ms: row.get(10)?,
    })
}

fn list_bridges_db(conn: &rusqlite::Connection) -> Vec<CloudBridgeConfig> {
    let sql = format!(
        "SELECT {} FROM cloud_bridge ORDER BY created_ms",
        BRIDGE_COLS
    );
    let mut stmt = conn.prepare_cached(&sql).unwrap();
    let rows = stmt.query_map([], parse_bridge_row).unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

fn get_bridge_db(conn: &rusqlite::Connection, id: &str) -> Option<CloudBridgeConfig> {
    let sql = format!("SELECT {} FROM cloud_bridge WHERE id = ?1", BRIDGE_COLS);
    let mut stmt = conn.prepare_cached(&sql).unwrap();
    stmt.query_row(rusqlite::params![id], parse_bridge_row).ok()
}

fn list_enabled_bridges_db(conn: &rusqlite::Connection) -> Vec<CloudBridgeConfig> {
    let sql = format!(
        "SELECT {} FROM cloud_bridge WHERE enabled = 1 ORDER BY created_ms",
        BRIDGE_COLS
    );
    let mut stmt = conn.prepare_cached(&sql).unwrap();
    let rows = stmt.query_map([], parse_bridge_row).unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

// ---- Status ----

fn parse_status_row(row: &rusqlite::Row) -> rusqlite::Result<CloudBridgeStatus> {
    Ok(CloudBridgeStatus {
        bridge_id: row.get(0)?,
        last_publish_ms: row.get(1)?,
        messages_published: row.get(2)?,
        last_error: row.get(3)?,
        state: row.get(4)?,
    })
}

fn get_status_db(conn: &rusqlite::Connection, bridge_id: &str) -> Option<CloudBridgeStatus> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT bridge_id, last_publish_ms, messages_published, last_error, state FROM cloud_bridge_status WHERE bridge_id = ?1",
        )
        .unwrap();
    stmt.query_row(rusqlite::params![bridge_id], parse_status_row)
        .ok()
}

fn list_statuses_db(conn: &rusqlite::Connection) -> Vec<CloudBridgeStatus> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT bridge_id, last_publish_ms, messages_published, last_error, state FROM cloud_bridge_status",
        )
        .unwrap();
    let rows = stmt.query_map([], parse_status_row).unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

fn update_status_db(
    conn: &rusqlite::Connection,
    bridge_id: &str,
    last_publish_ms: i64,
    messages_published: i64,
    last_error: Option<&str>,
    state: &str,
) -> Result<(), CloudStoreError> {
    conn.execute(
        "INSERT INTO cloud_bridge_status (bridge_id, last_publish_ms, messages_published, last_error, state)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(bridge_id) DO UPDATE SET last_publish_ms = ?2, messages_published = ?3, last_error = ?4, state = ?5",
        rusqlite::params![bridge_id, last_publish_ms, messages_published, last_error, state],
    )
    .map_err(|e| CloudStoreError::Db(e.to_string()))?;
    Ok(())
}

// ----------------------------------------------------------------
// Constructor
// ----------------------------------------------------------------

pub fn start_cloud_store_with_path(db_path: &Path) -> CloudStore {
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (version_tx, _version_rx) = watch::channel(0u64);
    let path_clone = db_path.to_path_buf();
    std::thread::Builder::new()
        .name("cloud-sqlite".into())
        .spawn(move || run_sqlite_thread(&path_clone, cmd_rx))
        .expect("failed to spawn cloud bridge SQLite thread");
    CloudStore { cmd_tx, version_tx }
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> CloudStore {
        let path =
            std::env::temp_dir().join(format!("opencrate_test_cloud_{}.db", uuid::Uuid::new_v4()));
        start_cloud_store_with_path(&path)
    }

    #[tokio::test]
    async fn create_and_list_bridges() {
        let store = temp_store();
        let config = r#"{"endpoint":"a1b2c3.iot.us-east-1.amazonaws.com","client_id":"bms-gw","thing_name":"bms-gateway","cert_pem_path":"/certs/cert.pem","key_pem_path":"/certs/key.pem","topic_prefix":"opencrate"}"#;
        store
            .create_bridge(
                "cloud-1",
                "AWS Prod",
                "aws_iot_core",
                config,
                true,
                true,
                true,
                false,
            )
            .await
            .unwrap();

        let bridges = store.list_bridges().await;
        assert_eq!(bridges.len(), 1);
        assert_eq!(bridges[0].id, "cloud-1");
        assert_eq!(bridges[0].name, "AWS Prod");
        assert_eq!(bridges[0].provider, "aws_iot_core");
        assert!(bridges[0].enabled);
        assert!(bridges[0].on_values);
        assert!(bridges[0].on_alarms);
        assert!(bridges[0].on_fdd);
        assert!(!bridges[0].on_device_status);
    }

    #[tokio::test]
    async fn update_bridge() {
        let store = temp_store();
        let config = r#"{"endpoint":"test.iot.us-east-1.amazonaws.com"}"#;
        store
            .create_bridge(
                "cloud-1",
                "Test",
                "aws_iot_core",
                config,
                true,
                true,
                true,
                false,
            )
            .await
            .unwrap();

        let new_config = r#"{"endpoint":"prod.iot.us-east-1.amazonaws.com"}"#;
        store
            .update_bridge(
                "cloud-1",
                "Updated",
                "aws_iot_core",
                new_config,
                false,
                true,
                false,
                true,
                true,
            )
            .await
            .unwrap();

        let c = store.get_bridge("cloud-1").await.unwrap();
        assert_eq!(c.name, "Updated");
        assert!(!c.enabled);
        assert!(!c.on_alarms);
        assert!(c.on_fdd);
        assert!(c.on_device_status);
    }

    #[tokio::test]
    async fn delete_bridge_cascades_status() {
        let store = temp_store();
        let config = r#"{"endpoint":"test"}"#;
        store
            .create_bridge(
                "cloud-1",
                "Test",
                "aws_iot_core",
                config,
                true,
                true,
                true,
                false,
            )
            .await
            .unwrap();

        let status = store.get_status("cloud-1").await;
        assert!(status.is_some());

        store.delete_bridge("cloud-1").await.unwrap();
        assert!(store.get_bridge("cloud-1").await.is_none());
        let status = store.get_status("cloud-1").await;
        assert!(status.is_none());
    }

    #[tokio::test]
    async fn enabled_filter() {
        let store = temp_store();
        let config = r#"{"endpoint":"test"}"#;
        store
            .create_bridge(
                "cloud-1",
                "Enabled",
                "aws_iot_core",
                config,
                true,
                true,
                true,
                false,
            )
            .await
            .unwrap();
        store
            .create_bridge(
                "cloud-2",
                "Disabled",
                "azure_iot_hub",
                config,
                true,
                true,
                true,
                false,
            )
            .await
            .unwrap();
        store
            .update_bridge(
                "cloud-2",
                "Disabled",
                "azure_iot_hub",
                config,
                false,
                true,
                true,
                true,
                false,
            )
            .await
            .unwrap();

        let enabled = store.list_enabled_bridges().await;
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].id, "cloud-1");
    }

    #[tokio::test]
    async fn status_update_and_read() {
        let store = temp_store();
        let config = r#"{"endpoint":"test"}"#;
        store
            .create_bridge(
                "cloud-1",
                "Test",
                "aws_iot_core",
                config,
                true,
                true,
                true,
                false,
            )
            .await
            .unwrap();

        let status = store.get_status("cloud-1").await.unwrap();
        assert_eq!(status.state, "idle");
        assert_eq!(status.messages_published, 0);

        store
            .update_status("cloud-1", 1000, 500, None, "publishing")
            .await
            .unwrap();

        let status = store.get_status("cloud-1").await.unwrap();
        assert_eq!(status.state, "publishing");
        assert_eq!(status.messages_published, 500);
        assert_eq!(status.last_publish_ms, 1000);
        assert!(status.last_error.is_none());

        store
            .update_status("cloud-1", 2000, 500, Some("connection refused"), "error")
            .await
            .unwrap();

        let status = store.get_status("cloud-1").await.unwrap();
        assert_eq!(status.state, "error");
        assert_eq!(status.last_error.as_deref(), Some("connection refused"));
    }

    #[tokio::test]
    async fn list_statuses() {
        let store = temp_store();
        let config = r#"{"endpoint":"test"}"#;
        store
            .create_bridge(
                "cloud-1",
                "A",
                "aws_iot_core",
                config,
                true,
                true,
                true,
                false,
            )
            .await
            .unwrap();
        store
            .create_bridge(
                "cloud-2",
                "B",
                "azure_iot_hub",
                config,
                true,
                true,
                true,
                false,
            )
            .await
            .unwrap();

        let statuses = store.list_statuses().await;
        assert_eq!(statuses.len(), 2);
    }
}
