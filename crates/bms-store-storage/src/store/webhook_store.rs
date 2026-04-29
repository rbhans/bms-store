use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::{mpsc, oneshot, watch};

use super::migration::{run_migrations, Migration};
use crate::webhook::model::{WebhookDelivery, WebhookEndpoint};

// ----------------------------------------------------------------
// Error type
// ----------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum WebhookStoreError {
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

enum WebhookCmd {
    // Endpoint CRUD
    CreateEndpoint {
        id: String,
        name: String,
        provider: String,
        url: String,
        headers: Option<String>,
        secret: Option<String>,
        on_alarm_raised: bool,
        on_alarm_cleared: bool,
        on_alarm_acknowledged: bool,
        on_device_down: bool,
        on_device_recovered: bool,
        on_fdd_fault_raised: bool,
        on_fdd_fault_cleared: bool,
        min_severity: String,
        tag_filters: Option<String>,
        reply: oneshot::Sender<Result<(), WebhookStoreError>>,
    },
    UpdateEndpoint {
        id: String,
        name: String,
        provider: String,
        url: String,
        headers: Option<String>,
        secret: Option<String>,
        enabled: bool,
        on_alarm_raised: bool,
        on_alarm_cleared: bool,
        on_alarm_acknowledged: bool,
        on_device_down: bool,
        on_device_recovered: bool,
        on_fdd_fault_raised: bool,
        on_fdd_fault_cleared: bool,
        min_severity: String,
        tag_filters: Option<String>,
        reply: oneshot::Sender<Result<(), WebhookStoreError>>,
    },
    DeleteEndpoint {
        id: String,
        reply: oneshot::Sender<Result<(), WebhookStoreError>>,
    },
    ListEndpoints {
        reply: oneshot::Sender<Vec<WebhookEndpoint>>,
    },
    GetEndpoint {
        id: String,
        reply: oneshot::Sender<Option<WebhookEndpoint>>,
    },
    ListEnabledEndpoints {
        reply: oneshot::Sender<Vec<WebhookEndpoint>>,
    },
    // Delivery log
    LogDelivery {
        endpoint_id: String,
        event_type: String,
        timestamp_ms: i64,
        status: String,
        http_status: Option<u16>,
        error: Option<String>,
        payload_preview: Option<String>,
        reply: oneshot::Sender<Result<i64, WebhookStoreError>>,
    },
    ListDeliveries {
        endpoint_id: Option<String>,
        status_filter: Option<String>,
        limit: u32,
        reply: oneshot::Sender<Vec<WebhookDelivery>>,
    },
    CountFailedDeliveries24h {
        reply: oneshot::Sender<u32>,
    },
    // Config (key-value)
    GetConfig {
        key: String,
        reply: oneshot::Sender<Option<String>>,
    },
    SetConfig {
        key: String,
        value: String,
        reply: oneshot::Sender<Result<(), WebhookStoreError>>,
    },
}

// ----------------------------------------------------------------
// WebhookStore — async handle to the SQLite thread
// ----------------------------------------------------------------

#[derive(Clone)]
pub struct WebhookStore {
    cmd_tx: mpsc::UnboundedSender<WebhookCmd>,
    version_tx: watch::Sender<u64>,
}

impl WebhookStore {
    /// Subscribe to version changes. The version is bumped on any mutation.
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.version_tx.subscribe()
    }

    // ---- Endpoint CRUD ----

    pub async fn create_endpoint(
        &self,
        id: &str,
        name: &str,
        provider: &str,
        url: &str,
        headers: Option<&str>,
        secret: Option<&str>,
        on_alarm_raised: bool,
        on_alarm_cleared: bool,
        on_alarm_acknowledged: bool,
        on_device_down: bool,
        on_device_recovered: bool,
        on_fdd_fault_raised: bool,
        on_fdd_fault_cleared: bool,
        min_severity: &str,
        tag_filters: Option<&str>,
    ) -> Result<(), WebhookStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(WebhookCmd::CreateEndpoint {
                id: id.to_string(),
                name: name.to_string(),
                provider: provider.to_string(),
                url: url.to_string(),
                headers: headers.map(|s| s.to_string()),
                secret: secret.map(|s| s.to_string()),
                on_alarm_raised,
                on_alarm_cleared,
                on_alarm_acknowledged,
                on_device_down,
                on_device_recovered,
                on_fdd_fault_raised,
                on_fdd_fault_cleared,
                min_severity: min_severity.to_string(),
                tag_filters: tag_filters.map(|s| s.to_string()),
                reply: reply_tx,
            })
            .map_err(|_| WebhookStoreError::ChannelClosed)?;
        let result = reply_rx
            .await
            .map_err(|_| WebhookStoreError::ChannelClosed)?;
        if result.is_ok() {
            self.version_tx.send_modify(|v| *v += 1);
        }
        result
    }

    pub async fn update_endpoint(
        &self,
        id: &str,
        name: &str,
        provider: &str,
        url: &str,
        headers: Option<&str>,
        secret: Option<&str>,
        enabled: bool,
        on_alarm_raised: bool,
        on_alarm_cleared: bool,
        on_alarm_acknowledged: bool,
        on_device_down: bool,
        on_device_recovered: bool,
        on_fdd_fault_raised: bool,
        on_fdd_fault_cleared: bool,
        min_severity: &str,
        tag_filters: Option<&str>,
    ) -> Result<(), WebhookStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(WebhookCmd::UpdateEndpoint {
                id: id.to_string(),
                name: name.to_string(),
                provider: provider.to_string(),
                url: url.to_string(),
                headers: headers.map(|s| s.to_string()),
                secret: secret.map(|s| s.to_string()),
                enabled,
                on_alarm_raised,
                on_alarm_cleared,
                on_alarm_acknowledged,
                on_device_down,
                on_device_recovered,
                on_fdd_fault_raised,
                on_fdd_fault_cleared,
                min_severity: min_severity.to_string(),
                tag_filters: tag_filters.map(|s| s.to_string()),
                reply: reply_tx,
            })
            .map_err(|_| WebhookStoreError::ChannelClosed)?;
        let result = reply_rx
            .await
            .map_err(|_| WebhookStoreError::ChannelClosed)?;
        if result.is_ok() {
            self.version_tx.send_modify(|v| *v += 1);
        }
        result
    }

    pub async fn delete_endpoint(&self, id: &str) -> Result<(), WebhookStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(WebhookCmd::DeleteEndpoint {
                id: id.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| WebhookStoreError::ChannelClosed)?;
        let result = reply_rx
            .await
            .map_err(|_| WebhookStoreError::ChannelClosed)?;
        if result.is_ok() {
            self.version_tx.send_modify(|v| *v += 1);
        }
        result
    }

    pub async fn list_endpoints(&self) -> Vec<WebhookEndpoint> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .cmd_tx
            .send(WebhookCmd::ListEndpoints { reply: reply_tx });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn get_endpoint(&self, id: &str) -> Option<WebhookEndpoint> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(WebhookCmd::GetEndpoint {
            id: id.to_string(),
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or(None)
    }

    pub async fn list_enabled_endpoints(&self) -> Vec<WebhookEndpoint> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .cmd_tx
            .send(WebhookCmd::ListEnabledEndpoints { reply: reply_tx });
        reply_rx.await.unwrap_or_default()
    }

    // ---- Delivery log ----

    pub async fn log_delivery(
        &self,
        endpoint_id: &str,
        event_type: &str,
        timestamp_ms: i64,
        status: &str,
        http_status: Option<u16>,
        error: Option<&str>,
        payload_preview: Option<&str>,
    ) -> Result<i64, WebhookStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(WebhookCmd::LogDelivery {
                endpoint_id: endpoint_id.to_string(),
                event_type: event_type.to_string(),
                timestamp_ms,
                status: status.to_string(),
                http_status,
                error: error.map(|s| s.to_string()),
                payload_preview: payload_preview.map(|s| s.to_string()),
                reply: reply_tx,
            })
            .map_err(|_| WebhookStoreError::ChannelClosed)?;
        reply_rx
            .await
            .map_err(|_| WebhookStoreError::ChannelClosed)?
    }

    pub async fn list_deliveries(
        &self,
        endpoint_id: Option<&str>,
        status_filter: Option<&str>,
        limit: u32,
    ) -> Vec<WebhookDelivery> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(WebhookCmd::ListDeliveries {
            endpoint_id: endpoint_id.map(|s| s.to_string()),
            status_filter: status_filter.map(|s| s.to_string()),
            limit,
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn count_failed_deliveries_24h(&self) -> u32 {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .cmd_tx
            .send(WebhookCmd::CountFailedDeliveries24h { reply: reply_tx });
        reply_rx.await.unwrap_or(0)
    }

    // ---- Config ----

    pub async fn get_config(&self, key: &str) -> Option<String> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(WebhookCmd::GetConfig {
            key: key.to_string(),
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or(None)
    }

    pub async fn set_config(&self, key: &str, value: &str) -> Result<(), WebhookStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(WebhookCmd::SetConfig {
                key: key.to_string(),
                value: value.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| WebhookStoreError::ChannelClosed)?;
        reply_rx
            .await
            .map_err(|_| WebhookStoreError::ChannelClosed)?
    }
}

// ----------------------------------------------------------------
// SQLite schema & thread
// ----------------------------------------------------------------

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        label: "initial webhook schema",
        sql: "
CREATE TABLE IF NOT EXISTS webhook_endpoint (
    id                      TEXT PRIMARY KEY,
    name                    TEXT NOT NULL,
    provider                TEXT NOT NULL DEFAULT 'generic',
    url                     TEXT NOT NULL,
    headers                 TEXT,
    secret                  TEXT,
    enabled                 INTEGER NOT NULL DEFAULT 1,
    on_alarm_raised         INTEGER NOT NULL DEFAULT 1,
    on_alarm_cleared        INTEGER NOT NULL DEFAULT 1,
    on_alarm_acknowledged   INTEGER NOT NULL DEFAULT 0,
    on_device_down          INTEGER NOT NULL DEFAULT 1,
    on_device_recovered     INTEGER NOT NULL DEFAULT 1,
    min_severity            TEXT NOT NULL DEFAULT 'info',
    tag_filters             TEXT,
    created_ms              INTEGER NOT NULL,
    updated_ms              INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS webhook_delivery (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    endpoint_id     TEXT NOT NULL,
    event_type      TEXT NOT NULL,
    timestamp_ms    INTEGER NOT NULL,
    status          TEXT NOT NULL,
    http_status     INTEGER,
    error           TEXT,
    payload_preview TEXT
);

CREATE INDEX IF NOT EXISTS idx_webhook_delivery_endpoint ON webhook_delivery(endpoint_id);
CREATE INDEX IF NOT EXISTS idx_webhook_delivery_timestamp ON webhook_delivery(timestamp_ms);

CREATE TABLE IF NOT EXISTS webhook_config (
    key     TEXT PRIMARY KEY,
    value   TEXT NOT NULL
);
",
    },
    Migration {
        version: 2,
        label: "add FDD fault event toggles",
        sql: "
ALTER TABLE webhook_endpoint ADD COLUMN on_fdd_fault_raised INTEGER NOT NULL DEFAULT 1;
ALTER TABLE webhook_endpoint ADD COLUMN on_fdd_fault_cleared INTEGER NOT NULL DEFAULT 1;
",
    },
];

fn run_sqlite_thread(db_path: &Path, mut cmd_rx: mpsc::UnboundedReceiver<WebhookCmd>) {
    let conn = rusqlite::Connection::open(db_path).expect("failed to open webhook database");
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;",
    )
    .expect("failed to set pragmas");
    run_migrations(&conn, "webhook", MIGRATIONS).expect("webhook: schema migration failed");

    while let Some(cmd) = cmd_rx.blocking_recv() {
        match cmd {
            // ---- Endpoints ----
            WebhookCmd::CreateEndpoint {
                id,
                name,
                provider,
                url,
                headers,
                secret,
                on_alarm_raised,
                on_alarm_cleared,
                on_alarm_acknowledged,
                on_device_down,
                on_device_recovered,
                on_fdd_fault_raised,
                on_fdd_fault_cleared,
                min_severity,
                tag_filters,
                reply,
            } => {
                let result = create_endpoint_db(
                    &conn,
                    &id,
                    &name,
                    &provider,
                    &url,
                    headers.as_deref(),
                    secret.as_deref(),
                    on_alarm_raised,
                    on_alarm_cleared,
                    on_alarm_acknowledged,
                    on_device_down,
                    on_device_recovered,
                    on_fdd_fault_raised,
                    on_fdd_fault_cleared,
                    &min_severity,
                    tag_filters.as_deref(),
                );
                let _ = reply.send(result);
            }
            WebhookCmd::UpdateEndpoint {
                id,
                name,
                provider,
                url,
                headers,
                secret,
                enabled,
                on_alarm_raised,
                on_alarm_cleared,
                on_alarm_acknowledged,
                on_device_down,
                on_device_recovered,
                on_fdd_fault_raised,
                on_fdd_fault_cleared,
                min_severity,
                tag_filters,
                reply,
            } => {
                let result = update_endpoint_db(
                    &conn,
                    &id,
                    &name,
                    &provider,
                    &url,
                    headers.as_deref(),
                    secret.as_deref(),
                    enabled,
                    on_alarm_raised,
                    on_alarm_cleared,
                    on_alarm_acknowledged,
                    on_device_down,
                    on_device_recovered,
                    on_fdd_fault_raised,
                    on_fdd_fault_cleared,
                    &min_severity,
                    tag_filters.as_deref(),
                );
                let _ = reply.send(result);
            }
            WebhookCmd::DeleteEndpoint { id, reply } => {
                let result = delete_endpoint_db(&conn, &id);
                let _ = reply.send(result);
            }
            WebhookCmd::ListEndpoints { reply } => {
                let _ = reply.send(list_endpoints_db(&conn));
            }
            WebhookCmd::GetEndpoint { id, reply } => {
                let _ = reply.send(get_endpoint_db(&conn, &id));
            }
            WebhookCmd::ListEnabledEndpoints { reply } => {
                let _ = reply.send(list_enabled_endpoints_db(&conn));
            }
            // ---- Delivery log ----
            WebhookCmd::LogDelivery {
                endpoint_id,
                event_type,
                timestamp_ms,
                status,
                http_status,
                error,
                payload_preview,
                reply,
            } => {
                let result = log_delivery_db(
                    &conn,
                    &endpoint_id,
                    &event_type,
                    timestamp_ms,
                    &status,
                    http_status,
                    error.as_deref(),
                    payload_preview.as_deref(),
                );
                let _ = reply.send(result);
            }
            WebhookCmd::ListDeliveries {
                endpoint_id,
                status_filter,
                limit,
                reply,
            } => {
                let _ = reply.send(list_deliveries_db(
                    &conn,
                    endpoint_id.as_deref(),
                    status_filter.as_deref(),
                    limit,
                ));
            }
            WebhookCmd::CountFailedDeliveries24h { reply } => {
                let _ = reply.send(count_failed_24h_db(&conn));
            }
            // ---- Config ----
            WebhookCmd::GetConfig { key, reply } => {
                let _ = reply.send(get_config_db(&conn, &key));
            }
            WebhookCmd::SetConfig { key, value, reply } => {
                let _ = reply.send(set_config_db(&conn, &key, &value));
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

// ---- Endpoints ----

#[allow(clippy::too_many_arguments)]
fn create_endpoint_db(
    conn: &rusqlite::Connection,
    id: &str,
    name: &str,
    provider: &str,
    url: &str,
    headers: Option<&str>,
    secret: Option<&str>,
    on_alarm_raised: bool,
    on_alarm_cleared: bool,
    on_alarm_acknowledged: bool,
    on_device_down: bool,
    on_device_recovered: bool,
    on_fdd_fault_raised: bool,
    on_fdd_fault_cleared: bool,
    min_severity: &str,
    tag_filters: Option<&str>,
) -> Result<(), WebhookStoreError> {
    let ts = now_ms();
    conn.execute(
        "INSERT INTO webhook_endpoint (id, name, provider, url, headers, secret, enabled, on_alarm_raised, on_alarm_cleared, on_alarm_acknowledged, on_device_down, on_device_recovered, on_fdd_fault_raised, on_fdd_fault_cleared, min_severity, tag_filters, created_ms, updated_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
        rusqlite::params![
            id,
            name,
            provider,
            url,
            headers,
            secret,
            on_alarm_raised as i32,
            on_alarm_cleared as i32,
            on_alarm_acknowledged as i32,
            on_device_down as i32,
            on_device_recovered as i32,
            on_fdd_fault_raised as i32,
            on_fdd_fault_cleared as i32,
            min_severity,
            tag_filters,
            ts,
            ts,
        ],
    )
    .map_err(|e| WebhookStoreError::Db(e.to_string()))?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn update_endpoint_db(
    conn: &rusqlite::Connection,
    id: &str,
    name: &str,
    provider: &str,
    url: &str,
    headers: Option<&str>,
    secret: Option<&str>,
    enabled: bool,
    on_alarm_raised: bool,
    on_alarm_cleared: bool,
    on_alarm_acknowledged: bool,
    on_device_down: bool,
    on_device_recovered: bool,
    on_fdd_fault_raised: bool,
    on_fdd_fault_cleared: bool,
    min_severity: &str,
    tag_filters: Option<&str>,
) -> Result<(), WebhookStoreError> {
    let ts = now_ms();
    let rows = conn
        .execute(
            "UPDATE webhook_endpoint SET name = ?1, provider = ?2, url = ?3, headers = ?4, secret = ?5, enabled = ?6, on_alarm_raised = ?7, on_alarm_cleared = ?8, on_alarm_acknowledged = ?9, on_device_down = ?10, on_device_recovered = ?11, on_fdd_fault_raised = ?12, on_fdd_fault_cleared = ?13, min_severity = ?14, tag_filters = ?15, updated_ms = ?16 WHERE id = ?17",
            rusqlite::params![
                name,
                provider,
                url,
                headers,
                secret,
                enabled as i32,
                on_alarm_raised as i32,
                on_alarm_cleared as i32,
                on_alarm_acknowledged as i32,
                on_device_down as i32,
                on_device_recovered as i32,
                on_fdd_fault_raised as i32,
                on_fdd_fault_cleared as i32,
                min_severity,
                tag_filters,
                ts,
                id,
            ],
        )
        .map_err(|e| WebhookStoreError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(WebhookStoreError::NotFound);
    }
    Ok(())
}

fn delete_endpoint_db(conn: &rusqlite::Connection, id: &str) -> Result<(), WebhookStoreError> {
    // Also delete delivery logs for this endpoint
    let _ = conn.execute(
        "DELETE FROM webhook_delivery WHERE endpoint_id = ?1",
        rusqlite::params![id],
    );
    let rows = conn
        .execute(
            "DELETE FROM webhook_endpoint WHERE id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| WebhookStoreError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(WebhookStoreError::NotFound);
    }
    Ok(())
}

fn parse_endpoint_row(row: &rusqlite::Row) -> rusqlite::Result<WebhookEndpoint> {
    Ok(WebhookEndpoint {
        id: row.get(0)?,
        name: row.get(1)?,
        provider: row.get(2)?,
        url: row.get(3)?,
        headers: row.get(4)?,
        secret: row.get(5)?,
        enabled: row.get::<_, i32>(6)? != 0,
        on_alarm_raised: row.get::<_, i32>(7)? != 0,
        on_alarm_cleared: row.get::<_, i32>(8)? != 0,
        on_alarm_acknowledged: row.get::<_, i32>(9)? != 0,
        on_device_down: row.get::<_, i32>(10)? != 0,
        on_device_recovered: row.get::<_, i32>(11)? != 0,
        on_fdd_fault_raised: row.get::<_, i32>(12)? != 0,
        on_fdd_fault_cleared: row.get::<_, i32>(13)? != 0,
        min_severity: row.get(14)?,
        tag_filters: row.get(15)?,
        created_ms: row.get(16)?,
        updated_ms: row.get(17)?,
    })
}

const ENDPOINT_COLS: &str = "id, name, provider, url, headers, secret, enabled, on_alarm_raised, on_alarm_cleared, on_alarm_acknowledged, on_device_down, on_device_recovered, on_fdd_fault_raised, on_fdd_fault_cleared, min_severity, tag_filters, created_ms, updated_ms";

fn list_endpoints_db(conn: &rusqlite::Connection) -> Vec<WebhookEndpoint> {
    let sql = format!(
        "SELECT {} FROM webhook_endpoint ORDER BY created_ms",
        ENDPOINT_COLS
    );
    let mut stmt = conn.prepare_cached(&sql).unwrap();
    let rows = stmt.query_map([], parse_endpoint_row).unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

fn get_endpoint_db(conn: &rusqlite::Connection, id: &str) -> Option<WebhookEndpoint> {
    let sql = format!(
        "SELECT {} FROM webhook_endpoint WHERE id = ?1",
        ENDPOINT_COLS
    );
    let mut stmt = conn.prepare_cached(&sql).unwrap();
    stmt.query_row(rusqlite::params![id], parse_endpoint_row)
        .ok()
}

fn list_enabled_endpoints_db(conn: &rusqlite::Connection) -> Vec<WebhookEndpoint> {
    let sql = format!(
        "SELECT {} FROM webhook_endpoint WHERE enabled = 1 ORDER BY created_ms",
        ENDPOINT_COLS
    );
    let mut stmt = conn.prepare_cached(&sql).unwrap();
    let rows = stmt.query_map([], parse_endpoint_row).unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

// ---- Delivery log ----

#[allow(clippy::too_many_arguments)]
fn log_delivery_db(
    conn: &rusqlite::Connection,
    endpoint_id: &str,
    event_type: &str,
    timestamp_ms: i64,
    status: &str,
    http_status: Option<u16>,
    error: Option<&str>,
    payload_preview: Option<&str>,
) -> Result<i64, WebhookStoreError> {
    conn.execute(
        "INSERT INTO webhook_delivery (endpoint_id, event_type, timestamp_ms, status, http_status, error, payload_preview)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            endpoint_id,
            event_type,
            timestamp_ms,
            status,
            http_status.map(|s| s as i32),
            error,
            payload_preview,
        ],
    )
    .map_err(|e| WebhookStoreError::Db(e.to_string()))?;

    // Auto-prune old deliveries (keep last 1000 per endpoint)
    let _ = conn.execute(
        "DELETE FROM webhook_delivery WHERE endpoint_id = ?1 AND id NOT IN (
            SELECT id FROM webhook_delivery WHERE endpoint_id = ?1 ORDER BY id DESC LIMIT 1000
        )",
        rusqlite::params![endpoint_id],
    );

    Ok(conn.last_insert_rowid())
}

fn parse_delivery_row(row: &rusqlite::Row) -> rusqlite::Result<WebhookDelivery> {
    Ok(WebhookDelivery {
        id: row.get(0)?,
        endpoint_id: row.get(1)?,
        event_type: row.get(2)?,
        timestamp_ms: row.get(3)?,
        status: row.get(4)?,
        http_status: row.get::<_, Option<i32>>(5)?.map(|s| s as u16),
        error: row.get(6)?,
        payload_preview: row.get(7)?,
    })
}

fn list_deliveries_db(
    conn: &rusqlite::Connection,
    endpoint_id: Option<&str>,
    status_filter: Option<&str>,
    limit: u32,
) -> Vec<WebhookDelivery> {
    let mut sql = "SELECT id, endpoint_id, event_type, timestamp_ms, status, http_status, error, payload_preview FROM webhook_delivery WHERE 1=1".to_string();
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(eid) = endpoint_id {
        params.push(Box::new(eid.to_string()));
        sql.push_str(&format!(" AND endpoint_id = ?{}", params.len()));
    }
    if let Some(sf) = status_filter {
        params.push(Box::new(sf.to_string()));
        sql.push_str(&format!(" AND status = ?{}", params.len()));
    }
    sql.push_str(&format!(" ORDER BY id DESC LIMIT {}", limit));

    let mut stmt = conn.prepare(&sql).unwrap();
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows = stmt
        .query_map(param_refs.as_slice(), parse_delivery_row)
        .unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

fn count_failed_24h_db(conn: &rusqlite::Connection) -> u32 {
    let cutoff = now_ms() - 86_400_000;
    let mut stmt = conn
        .prepare_cached(
            "SELECT COUNT(*) FROM webhook_delivery WHERE status = 'failed' AND timestamp_ms > ?1",
        )
        .unwrap();
    stmt.query_row(rusqlite::params![cutoff], |row| row.get::<_, u32>(0))
        .unwrap_or(0)
}

// ---- Config ----

fn get_config_db(conn: &rusqlite::Connection, key: &str) -> Option<String> {
    let mut stmt = conn
        .prepare_cached("SELECT value FROM webhook_config WHERE key = ?1")
        .unwrap();
    stmt.query_row(rusqlite::params![key], |row| row.get(0))
        .ok()
}

fn set_config_db(
    conn: &rusqlite::Connection,
    key: &str,
    value: &str,
) -> Result<(), WebhookStoreError> {
    conn.execute(
        "INSERT INTO webhook_config (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
        rusqlite::params![key, value],
    )
    .map_err(|e| WebhookStoreError::Db(e.to_string()))?;
    Ok(())
}

// ----------------------------------------------------------------
// Constructor
// ----------------------------------------------------------------

pub fn start_webhook_store_with_path(db_path: &Path) -> WebhookStore {
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (version_tx, _version_rx) = watch::channel(0u64);
    let path_clone = db_path.to_path_buf();
    std::thread::Builder::new()
        .name("webhook-sqlite".into())
        .spawn(move || run_sqlite_thread(&path_clone, cmd_rx))
        .expect("failed to spawn webhook SQLite thread");
    WebhookStore { cmd_tx, version_tx }
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> WebhookStore {
        let path = std::env::temp_dir().join(format!(
            "opencrate_test_webhooks_{}.db",
            uuid::Uuid::new_v4()
        ));
        start_webhook_store_with_path(&path)
    }

    #[tokio::test]
    async fn create_and_list_endpoints() {
        let store = temp_store();
        store
            .create_endpoint(
                "ep-1",
                "Slack Alerts",
                "slack",
                "https://hooks.slack.com/test",
                None,
                None,
                true,
                true,
                false,
                true,
                true,
                true,
                true,
                "warning",
                None,
            )
            .await
            .unwrap();

        let eps = store.list_endpoints().await;
        assert_eq!(eps.len(), 1);
        assert_eq!(eps[0].id, "ep-1");
        assert_eq!(eps[0].name, "Slack Alerts");
        assert_eq!(eps[0].provider, "slack");
        assert!(eps[0].enabled);
        assert!(eps[0].on_alarm_raised);
        assert!(!eps[0].on_alarm_acknowledged);
        assert!(eps[0].on_fdd_fault_raised);
        assert!(eps[0].on_fdd_fault_cleared);
    }

    #[tokio::test]
    async fn update_endpoint() {
        let store = temp_store();
        store
            .create_endpoint(
                "ep-1",
                "Test",
                "generic",
                "https://example.com",
                None,
                None,
                true,
                true,
                false,
                true,
                true,
                true,
                true,
                "info",
                None,
            )
            .await
            .unwrap();

        store
            .update_endpoint(
                "ep-1",
                "Updated",
                "slack",
                "https://new-url.com",
                None,
                Some("secret123"),
                false, // disabled
                true,
                true,
                true,
                false,
                false,
                false,
                true,
                "critical",
                None,
            )
            .await
            .unwrap();

        let ep = store.get_endpoint("ep-1").await.unwrap();
        assert_eq!(ep.name, "Updated");
        assert_eq!(ep.provider, "slack");
        assert!(!ep.enabled);
        assert_eq!(ep.min_severity, "critical");
        assert_eq!(ep.secret.as_deref(), Some("secret123"));
        assert!(!ep.on_fdd_fault_raised);
        assert!(ep.on_fdd_fault_cleared);
    }

    #[tokio::test]
    async fn delete_endpoint_and_deliveries() {
        let store = temp_store();
        store
            .create_endpoint(
                "ep-1",
                "Test",
                "generic",
                "https://example.com",
                None,
                None,
                true,
                true,
                false,
                true,
                true,
                true,
                true,
                "info",
                None,
            )
            .await
            .unwrap();

        store
            .log_delivery(
                "ep-1",
                "alarm_raised",
                1000,
                "delivered",
                Some(200),
                None,
                None,
            )
            .await
            .unwrap();

        store.delete_endpoint("ep-1").await.unwrap();
        assert!(store.get_endpoint("ep-1").await.is_none());
        let deliveries = store.list_deliveries(Some("ep-1"), None, 100).await;
        assert!(deliveries.is_empty());
    }

    #[tokio::test]
    async fn enabled_filter() {
        let store = temp_store();
        store
            .create_endpoint(
                "ep-1",
                "Enabled",
                "generic",
                "https://a.com",
                None,
                None,
                true,
                true,
                false,
                true,
                true,
                true,
                true,
                "info",
                None,
            )
            .await
            .unwrap();
        store
            .create_endpoint(
                "ep-2",
                "Disabled",
                "generic",
                "https://b.com",
                None,
                None,
                true,
                true,
                false,
                true,
                true,
                true,
                true,
                "info",
                None,
            )
            .await
            .unwrap();
        store
            .update_endpoint(
                "ep-2",
                "Disabled",
                "generic",
                "https://b.com",
                None,
                None,
                false,
                true,
                true,
                false,
                true,
                true,
                true,
                true,
                "info",
                None,
            )
            .await
            .unwrap();

        let enabled = store.list_enabled_endpoints().await;
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].id, "ep-1");
    }

    #[tokio::test]
    async fn delivery_log_and_count() {
        let store = temp_store();
        store
            .create_endpoint(
                "ep-1",
                "Test",
                "generic",
                "https://example.com",
                None,
                None,
                true,
                true,
                false,
                true,
                true,
                true,
                true,
                "info",
                None,
            )
            .await
            .unwrap();

        let now = now_ms();
        store
            .log_delivery(
                "ep-1",
                "alarm_raised",
                now,
                "delivered",
                Some(200),
                None,
                Some("{\"test\":true}"),
            )
            .await
            .unwrap();
        store
            .log_delivery(
                "ep-1",
                "device_down",
                now,
                "failed",
                Some(500),
                Some("server error"),
                None,
            )
            .await
            .unwrap();

        let all = store.list_deliveries(None, None, 100).await;
        assert_eq!(all.len(), 2);

        let failed = store.list_deliveries(None, Some("failed"), 100).await;
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].status, "failed");

        let count = store.count_failed_deliveries_24h().await;
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn config_get_set() {
        let store = temp_store();
        assert_eq!(store.get_config("paused").await, None);

        store.set_config("paused", "true").await.unwrap();
        assert_eq!(store.get_config("paused").await, Some("true".into()));

        store.set_config("paused", "false").await.unwrap();
        assert_eq!(store.get_config("paused").await, Some("false".into()));
    }
}
