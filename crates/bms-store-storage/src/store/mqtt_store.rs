use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot, watch};

use super::migration::{run_migrations, Migration};

// ----------------------------------------------------------------
// Public types
// ----------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MqttEventType {
    Value,
    Alarm,
    Status,
}

impl MqttEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Value => "value",
            Self::Alarm => "alarm",
            Self::Status => "status",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "value" => Some(Self::Value),
            "alarm" => Some(Self::Alarm),
            "status" => Some(Self::Status),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MqttBrokerConfig {
    pub id: i64,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub client_id: String,
    pub username: String,
    pub password: String,
    pub use_tls: bool,
    pub clean_session: bool,
    pub keep_alive_secs: u16,
    pub enabled: bool,
    pub created_ms: i64,
    pub updated_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MqttTopicPattern {
    pub id: i64,
    pub broker_id: i64,
    pub event_type: MqttEventType,
    pub pattern: String,
    pub qos: u8,
    pub retain: bool,
    pub enabled: bool,
    pub node_filter: String,
}

#[derive(Debug, thiserror::Error)]
pub enum MqttStoreError {
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

enum MqttCmd {
    // Broker CRUD
    CreateBroker {
        name: String,
        host: String,
        port: u16,
        client_id: String,
        username: String,
        password: String,
        use_tls: bool,
        clean_session: bool,
        keep_alive_secs: u16,
        reply: oneshot::Sender<Result<i64, MqttStoreError>>,
    },
    UpdateBroker {
        id: i64,
        name: String,
        host: String,
        port: u16,
        client_id: String,
        username: String,
        password: String,
        use_tls: bool,
        clean_session: bool,
        keep_alive_secs: u16,
        enabled: bool,
        reply: oneshot::Sender<Result<(), MqttStoreError>>,
    },
    DeleteBroker {
        id: i64,
        reply: oneshot::Sender<Result<(), MqttStoreError>>,
    },
    ListBrokers {
        reply: oneshot::Sender<Vec<MqttBrokerConfig>>,
    },
    GetBroker {
        id: i64,
        reply: oneshot::Sender<Option<MqttBrokerConfig>>,
    },
    // Topic pattern CRUD
    CreateTopicPattern {
        broker_id: i64,
        event_type: MqttEventType,
        pattern: String,
        qos: u8,
        retain: bool,
        node_filter: String,
        reply: oneshot::Sender<Result<i64, MqttStoreError>>,
    },
    UpdateTopicPattern {
        id: i64,
        pattern: String,
        qos: u8,
        retain: bool,
        enabled: bool,
        node_filter: String,
        reply: oneshot::Sender<Result<(), MqttStoreError>>,
    },
    DeleteTopicPattern {
        id: i64,
        reply: oneshot::Sender<Result<(), MqttStoreError>>,
    },
    ListTopicPatterns {
        broker_id: i64,
        reply: oneshot::Sender<Vec<MqttTopicPattern>>,
    },
    // Composite query
    ListAllEnabledBrokers {
        reply: oneshot::Sender<Vec<(MqttBrokerConfig, Vec<MqttTopicPattern>)>>,
    },
}

// ----------------------------------------------------------------
// MqttStore — async handle to the SQLite thread
// ----------------------------------------------------------------

#[derive(Clone)]
pub struct MqttStore {
    cmd_tx: mpsc::UnboundedSender<MqttCmd>,
    version_tx: watch::Sender<u64>,
}

impl MqttStore {
    /// Subscribe to version changes. The version is bumped on any mutation.
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.version_tx.subscribe()
    }

    // ---- Broker CRUD ----

    pub async fn create_broker(
        &self,
        name: &str,
        host: &str,
        port: u16,
        client_id: &str,
        username: &str,
        password: &str,
        use_tls: bool,
        clean_session: bool,
        keep_alive_secs: u16,
    ) -> Result<i64, MqttStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(MqttCmd::CreateBroker {
                name: name.to_string(),
                host: host.to_string(),
                port,
                client_id: client_id.to_string(),
                username: username.to_string(),
                password: password.to_string(),
                use_tls,
                clean_session,
                keep_alive_secs,
                reply: reply_tx,
            })
            .map_err(|_| MqttStoreError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| MqttStoreError::ChannelClosed)?;
        if result.is_ok() {
            self.version_tx.send_modify(|v| *v += 1);
        }
        result
    }

    pub async fn update_broker(
        &self,
        id: i64,
        name: &str,
        host: &str,
        port: u16,
        client_id: &str,
        username: &str,
        password: &str,
        use_tls: bool,
        clean_session: bool,
        keep_alive_secs: u16,
        enabled: bool,
    ) -> Result<(), MqttStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(MqttCmd::UpdateBroker {
                id,
                name: name.to_string(),
                host: host.to_string(),
                port,
                client_id: client_id.to_string(),
                username: username.to_string(),
                password: password.to_string(),
                use_tls,
                clean_session,
                keep_alive_secs,
                enabled,
                reply: reply_tx,
            })
            .map_err(|_| MqttStoreError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| MqttStoreError::ChannelClosed)?;
        if result.is_ok() {
            self.version_tx.send_modify(|v| *v += 1);
        }
        result
    }

    pub async fn delete_broker(&self, id: i64) -> Result<(), MqttStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(MqttCmd::DeleteBroker {
                id,
                reply: reply_tx,
            })
            .map_err(|_| MqttStoreError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| MqttStoreError::ChannelClosed)?;
        if result.is_ok() {
            self.version_tx.send_modify(|v| *v += 1);
        }
        result
    }

    pub async fn list_brokers(&self) -> Vec<MqttBrokerConfig> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(MqttCmd::ListBrokers { reply: reply_tx });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn get_broker(&self, id: i64) -> Option<MqttBrokerConfig> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(MqttCmd::GetBroker {
            id,
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or(None)
    }

    // ---- Topic pattern CRUD ----

    pub async fn create_topic_pattern(
        &self,
        broker_id: i64,
        event_type: MqttEventType,
        pattern: &str,
        qos: u8,
        retain: bool,
        node_filter: &str,
    ) -> Result<i64, MqttStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(MqttCmd::CreateTopicPattern {
                broker_id,
                event_type,
                pattern: pattern.to_string(),
                qos,
                retain,
                node_filter: node_filter.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| MqttStoreError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| MqttStoreError::ChannelClosed)?;
        if result.is_ok() {
            self.version_tx.send_modify(|v| *v += 1);
        }
        result
    }

    pub async fn update_topic_pattern(
        &self,
        id: i64,
        pattern: &str,
        qos: u8,
        retain: bool,
        enabled: bool,
        node_filter: &str,
    ) -> Result<(), MqttStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(MqttCmd::UpdateTopicPattern {
                id,
                pattern: pattern.to_string(),
                qos,
                retain,
                enabled,
                node_filter: node_filter.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| MqttStoreError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| MqttStoreError::ChannelClosed)?;
        if result.is_ok() {
            self.version_tx.send_modify(|v| *v += 1);
        }
        result
    }

    pub async fn delete_topic_pattern(&self, id: i64) -> Result<(), MqttStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(MqttCmd::DeleteTopicPattern {
                id,
                reply: reply_tx,
            })
            .map_err(|_| MqttStoreError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| MqttStoreError::ChannelClosed)?;
        if result.is_ok() {
            self.version_tx.send_modify(|v| *v += 1);
        }
        result
    }

    pub async fn list_topic_patterns(&self, broker_id: i64) -> Vec<MqttTopicPattern> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(MqttCmd::ListTopicPatterns {
            broker_id,
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_default()
    }

    // ---- Composite query ----

    /// Returns all enabled brokers together with their topic patterns.
    /// Used by the MQTT publisher to load the full configuration.
    pub async fn list_all_enabled_brokers(&self) -> Vec<(MqttBrokerConfig, Vec<MqttTopicPattern>)> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .cmd_tx
            .send(MqttCmd::ListAllEnabledBrokers { reply: reply_tx });
        reply_rx.await.unwrap_or_default()
    }
}

// ----------------------------------------------------------------
// SQLite schema & thread
// ----------------------------------------------------------------

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    label: "initial mqtt schema",
    sql: "
CREATE TABLE IF NOT EXISTS mqtt_broker (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    name            TEXT NOT NULL,
    host            TEXT NOT NULL,
    port            INTEGER NOT NULL DEFAULT 1883,
    client_id       TEXT NOT NULL DEFAULT '',
    username        TEXT NOT NULL DEFAULT '',
    password        TEXT NOT NULL DEFAULT '',
    use_tls         INTEGER NOT NULL DEFAULT 0,
    clean_session   INTEGER NOT NULL DEFAULT 1,
    keep_alive_secs INTEGER NOT NULL DEFAULT 30,
    enabled         INTEGER NOT NULL DEFAULT 1,
    created_ms      INTEGER NOT NULL,
    updated_ms      INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS mqtt_topic_pattern (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    broker_id   INTEGER NOT NULL REFERENCES mqtt_broker(id) ON DELETE CASCADE,
    event_type  TEXT NOT NULL,
    pattern     TEXT NOT NULL,
    qos         INTEGER NOT NULL DEFAULT 0,
    retain      INTEGER NOT NULL DEFAULT 0,
    enabled     INTEGER NOT NULL DEFAULT 1,
    node_filter TEXT NOT NULL DEFAULT '',
    UNIQUE(broker_id, event_type)
);
",
}];

fn run_sqlite_thread(db_path: &Path, mut cmd_rx: mpsc::UnboundedReceiver<MqttCmd>) {
    let conn = rusqlite::Connection::open(db_path).expect("failed to open mqtt database");
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;",
    )
    .expect("failed to set pragmas");
    run_migrations(&conn, "mqtt", MIGRATIONS).expect("mqtt: schema migration failed");

    while let Some(cmd) = cmd_rx.blocking_recv() {
        match cmd {
            // ---- Brokers ----
            MqttCmd::CreateBroker {
                name,
                host,
                port,
                client_id,
                username,
                password,
                use_tls,
                clean_session,
                keep_alive_secs,
                reply,
            } => {
                let result = create_broker_db(
                    &conn,
                    &name,
                    &host,
                    port,
                    &client_id,
                    &username,
                    &password,
                    use_tls,
                    clean_session,
                    keep_alive_secs,
                );
                let _ = reply.send(result);
            }
            MqttCmd::UpdateBroker {
                id,
                name,
                host,
                port,
                client_id,
                username,
                password,
                use_tls,
                clean_session,
                keep_alive_secs,
                enabled,
                reply,
            } => {
                let result = update_broker_db(
                    &conn,
                    id,
                    &name,
                    &host,
                    port,
                    &client_id,
                    &username,
                    &password,
                    use_tls,
                    clean_session,
                    keep_alive_secs,
                    enabled,
                );
                let _ = reply.send(result);
            }
            MqttCmd::DeleteBroker { id, reply } => {
                let result = delete_broker_db(&conn, id);
                let _ = reply.send(result);
            }
            MqttCmd::ListBrokers { reply } => {
                let _ = reply.send(list_brokers_db(&conn));
            }
            MqttCmd::GetBroker { id, reply } => {
                let _ = reply.send(get_broker_db(&conn, id));
            }
            // ---- Topic patterns ----
            MqttCmd::CreateTopicPattern {
                broker_id,
                event_type,
                pattern,
                qos,
                retain,
                node_filter,
                reply,
            } => {
                let result = create_topic_pattern_db(
                    &conn,
                    broker_id,
                    &event_type,
                    &pattern,
                    qos,
                    retain,
                    &node_filter,
                );
                let _ = reply.send(result);
            }
            MqttCmd::UpdateTopicPattern {
                id,
                pattern,
                qos,
                retain,
                enabled,
                node_filter,
                reply,
            } => {
                let result = update_topic_pattern_db(
                    &conn,
                    id,
                    &pattern,
                    qos,
                    retain,
                    enabled,
                    &node_filter,
                );
                let _ = reply.send(result);
            }
            MqttCmd::DeleteTopicPattern { id, reply } => {
                let result = delete_topic_pattern_db(&conn, id);
                let _ = reply.send(result);
            }
            MqttCmd::ListTopicPatterns { broker_id, reply } => {
                let _ = reply.send(list_topic_patterns_db(&conn, broker_id));
            }
            // ---- Composite ----
            MqttCmd::ListAllEnabledBrokers { reply } => {
                let _ = reply.send(list_all_enabled_brokers_db(&conn));
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

// ---- Brokers ----

fn create_broker_db(
    conn: &rusqlite::Connection,
    name: &str,
    host: &str,
    port: u16,
    client_id: &str,
    username: &str,
    password: &str,
    use_tls: bool,
    clean_session: bool,
    keep_alive_secs: u16,
) -> Result<i64, MqttStoreError> {
    let ts = now_ms();
    conn.execute(
        "INSERT INTO mqtt_broker (name, host, port, client_id, username, password, use_tls, clean_session, keep_alive_secs, enabled, created_ms, updated_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1, ?10, ?11)",
        rusqlite::params![
            name,
            host,
            port as i32,
            client_id,
            username,
            password,
            use_tls as i32,
            clean_session as i32,
            keep_alive_secs as i32,
            ts,
            ts,
        ],
    )
    .map_err(|e| MqttStoreError::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

fn update_broker_db(
    conn: &rusqlite::Connection,
    id: i64,
    name: &str,
    host: &str,
    port: u16,
    client_id: &str,
    username: &str,
    password: &str,
    use_tls: bool,
    clean_session: bool,
    keep_alive_secs: u16,
    enabled: bool,
) -> Result<(), MqttStoreError> {
    let ts = now_ms();
    let rows = conn
        .execute(
            "UPDATE mqtt_broker SET name = ?1, host = ?2, port = ?3, client_id = ?4, username = ?5, password = ?6, use_tls = ?7, clean_session = ?8, keep_alive_secs = ?9, enabled = ?10, updated_ms = ?11 WHERE id = ?12",
            rusqlite::params![
                name,
                host,
                port as i32,
                client_id,
                username,
                password,
                use_tls as i32,
                clean_session as i32,
                keep_alive_secs as i32,
                enabled as i32,
                ts,
                id,
            ],
        )
        .map_err(|e| MqttStoreError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(MqttStoreError::NotFound);
    }
    Ok(())
}

fn delete_broker_db(conn: &rusqlite::Connection, id: i64) -> Result<(), MqttStoreError> {
    let rows = conn
        .execute(
            "DELETE FROM mqtt_broker WHERE id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| MqttStoreError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(MqttStoreError::NotFound);
    }
    Ok(())
}

fn parse_broker_row(row: &rusqlite::Row) -> rusqlite::Result<MqttBrokerConfig> {
    Ok(MqttBrokerConfig {
        id: row.get(0)?,
        name: row.get(1)?,
        host: row.get(2)?,
        port: row.get::<_, i32>(3)? as u16,
        client_id: row.get(4)?,
        username: row.get(5)?,
        password: row.get(6)?,
        use_tls: row.get::<_, i32>(7)? != 0,
        clean_session: row.get::<_, i32>(8)? != 0,
        keep_alive_secs: row.get::<_, i32>(9)? as u16,
        enabled: row.get::<_, i32>(10)? != 0,
        created_ms: row.get(11)?,
        updated_ms: row.get(12)?,
    })
}

fn list_brokers_db(conn: &rusqlite::Connection) -> Vec<MqttBrokerConfig> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT id, name, host, port, client_id, username, password, use_tls, clean_session, keep_alive_secs, enabled, created_ms, updated_ms FROM mqtt_broker ORDER BY id",
        )
        .unwrap();
    let rows = stmt.query_map([], parse_broker_row).unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

fn get_broker_db(conn: &rusqlite::Connection, id: i64) -> Option<MqttBrokerConfig> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT id, name, host, port, client_id, username, password, use_tls, clean_session, keep_alive_secs, enabled, created_ms, updated_ms FROM mqtt_broker WHERE id = ?1",
        )
        .unwrap();
    stmt.query_row(rusqlite::params![id], parse_broker_row).ok()
}

// ---- Topic patterns ----

fn create_topic_pattern_db(
    conn: &rusqlite::Connection,
    broker_id: i64,
    event_type: &MqttEventType,
    pattern: &str,
    qos: u8,
    retain: bool,
    node_filter: &str,
) -> Result<i64, MqttStoreError> {
    conn.execute(
        "INSERT INTO mqtt_topic_pattern (broker_id, event_type, pattern, qos, retain, enabled, node_filter)
         VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6)",
        rusqlite::params![
            broker_id,
            event_type.as_str(),
            pattern,
            qos as i32,
            retain as i32,
            node_filter,
        ],
    )
    .map_err(|e| MqttStoreError::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

fn update_topic_pattern_db(
    conn: &rusqlite::Connection,
    id: i64,
    pattern: &str,
    qos: u8,
    retain: bool,
    enabled: bool,
    node_filter: &str,
) -> Result<(), MqttStoreError> {
    let rows = conn
        .execute(
            "UPDATE mqtt_topic_pattern SET pattern = ?1, qos = ?2, retain = ?3, enabled = ?4, node_filter = ?5 WHERE id = ?6",
            rusqlite::params![
                pattern,
                qos as i32,
                retain as i32,
                enabled as i32,
                node_filter,
                id,
            ],
        )
        .map_err(|e| MqttStoreError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(MqttStoreError::NotFound);
    }
    Ok(())
}

fn delete_topic_pattern_db(conn: &rusqlite::Connection, id: i64) -> Result<(), MqttStoreError> {
    let rows = conn
        .execute(
            "DELETE FROM mqtt_topic_pattern WHERE id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| MqttStoreError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(MqttStoreError::NotFound);
    }
    Ok(())
}

fn parse_topic_pattern_row(row: &rusqlite::Row) -> rusqlite::Result<MqttTopicPattern> {
    let et_str: String = row.get(2)?;
    Ok(MqttTopicPattern {
        id: row.get(0)?,
        broker_id: row.get(1)?,
        event_type: MqttEventType::from_str(&et_str).unwrap_or(MqttEventType::Value),
        pattern: row.get(3)?,
        qos: row.get::<_, i32>(4)? as u8,
        retain: row.get::<_, i32>(5)? != 0,
        enabled: row.get::<_, i32>(6)? != 0,
        node_filter: row.get(7)?,
    })
}

fn list_topic_patterns_db(conn: &rusqlite::Connection, broker_id: i64) -> Vec<MqttTopicPattern> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT id, broker_id, event_type, pattern, qos, retain, enabled, node_filter FROM mqtt_topic_pattern WHERE broker_id = ?1 ORDER BY id",
        )
        .unwrap();
    let rows = stmt
        .query_map(rusqlite::params![broker_id], parse_topic_pattern_row)
        .unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

// ---- Composite ----

fn list_all_enabled_brokers_db(
    conn: &rusqlite::Connection,
) -> Vec<(MqttBrokerConfig, Vec<MqttTopicPattern>)> {
    let brokers = {
        let mut stmt = conn
            .prepare_cached(
                "SELECT id, name, host, port, client_id, username, password, use_tls, clean_session, keep_alive_secs, enabled, created_ms, updated_ms FROM mqtt_broker WHERE enabled = 1 ORDER BY id",
            )
            .unwrap();
        let rows = stmt.query_map([], parse_broker_row).unwrap();
        rows.filter_map(|r| r.ok()).collect::<Vec<_>>()
    };

    brokers
        .into_iter()
        .map(|broker| {
            let patterns = list_topic_patterns_db(conn, broker.id);
            (broker, patterns)
        })
        .collect()
}

// ----------------------------------------------------------------
// Constructor
// ----------------------------------------------------------------

pub fn start_mqtt_store_with_path(db_path: &Path) -> MqttStore {
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (version_tx, _version_rx) = watch::channel(0u64);
    let path_clone = db_path.to_path_buf();
    std::thread::Builder::new()
        .name("mqtt-sqlite".into())
        .spawn(move || run_sqlite_thread(&path_clone, cmd_rx))
        .expect("failed to spawn mqtt SQLite thread");
    MqttStore { cmd_tx, version_tx }
}
