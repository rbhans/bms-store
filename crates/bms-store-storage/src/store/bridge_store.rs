//! BACnet network and Modbus bus configuration store.
//!
//! Persists bridge configurations that the GUI lets the operator add /
//! edit / delete at runtime, eliminating the need to hand-edit
//! `scenario.json` and restart for every new network or bus.
//!
//! Each row holds the configuration as a serialised JSON blob (the same
//! shape used in `scenario.json` — `BacnetNetworkConfig` /
//! `ModbusNetworkConfig`) so the schema does not need to be rev'd every
//! time the bridge config grows a field.
//!
//! **Hot-reload is not implemented yet.** Mutations emit no event; the
//! caller is expected to surface a "restart required to activate" toast.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot, watch};

use super::migration::{run_migrations, Migration};

// ----------------------------------------------------------------
// Public types
// ----------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredBacnetNetwork {
    pub id: i64,
    pub name: String,
    /// Serialised `BacnetNetworkConfig` — same shape as scenario.json.
    pub config_json: String,
    pub enabled: bool,
    pub created_ms: i64,
    pub updated_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredModbusBus {
    pub id: i64,
    pub name: String,
    /// Serialised `ModbusNetworkConfig` — same shape as scenario.json.
    pub config_json: String,
    pub enabled: bool,
    pub created_ms: i64,
    pub updated_ms: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum BridgeStoreError {
    #[error("database error: {0}")]
    Db(String),
    #[error("channel closed")]
    ChannelClosed,
    #[error("not found")]
    NotFound,
    #[error("name already exists")]
    Duplicate,
}

// ----------------------------------------------------------------
// Commands
// ----------------------------------------------------------------

enum BridgeCmd {
    // BACnet
    ListBacnet {
        reply: oneshot::Sender<Vec<StoredBacnetNetwork>>,
    },
    GetBacnet {
        id: i64,
        reply: oneshot::Sender<Option<StoredBacnetNetwork>>,
    },
    CreateBacnet {
        name: String,
        config_json: String,
        enabled: bool,
        reply: oneshot::Sender<Result<StoredBacnetNetwork, BridgeStoreError>>,
    },
    UpdateBacnet {
        id: i64,
        config_json: String,
        enabled: bool,
        reply: oneshot::Sender<Result<(), BridgeStoreError>>,
    },
    DeleteBacnet {
        id: i64,
        reply: oneshot::Sender<Result<(), BridgeStoreError>>,
    },

    // Modbus
    ListModbus {
        reply: oneshot::Sender<Vec<StoredModbusBus>>,
    },
    GetModbus {
        id: i64,
        reply: oneshot::Sender<Option<StoredModbusBus>>,
    },
    CreateModbus {
        name: String,
        config_json: String,
        enabled: bool,
        reply: oneshot::Sender<Result<StoredModbusBus, BridgeStoreError>>,
    },
    UpdateModbus {
        id: i64,
        config_json: String,
        enabled: bool,
        reply: oneshot::Sender<Result<(), BridgeStoreError>>,
    },
    DeleteModbus {
        id: i64,
        reply: oneshot::Sender<Result<(), BridgeStoreError>>,
    },
}

// ----------------------------------------------------------------
// Store handle
// ----------------------------------------------------------------

#[derive(Clone)]
pub struct BridgeStore {
    cmd_tx: mpsc::UnboundedSender<BridgeCmd>,
    version_tx: watch::Sender<u64>,
}

impl BridgeStore {
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.version_tx.subscribe()
    }

    fn bump(&self) {
        let _ = self
            .version_tx
            .send(self.version_tx.borrow().wrapping_add(1));
    }

    // ---- BACnet ----

    pub async fn list_bacnet_networks(&self) -> Vec<StoredBacnetNetwork> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(BridgeCmd::ListBacnet { reply: reply_tx });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn get_bacnet_network(&self, id: i64) -> Option<StoredBacnetNetwork> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(BridgeCmd::GetBacnet { id, reply: reply_tx });
        reply_rx.await.ok().flatten()
    }

    pub async fn create_bacnet_network(
        &self,
        name: &str,
        config_json: &str,
        enabled: bool,
    ) -> Result<StoredBacnetNetwork, BridgeStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(BridgeCmd::CreateBacnet {
                name: name.into(),
                config_json: config_json.into(),
                enabled,
                reply: reply_tx,
            })
            .map_err(|_| BridgeStoreError::ChannelClosed)?;
        let res = reply_rx.await.map_err(|_| BridgeStoreError::ChannelClosed)?;
        if res.is_ok() {
            self.bump();
        }
        res
    }

    pub async fn update_bacnet_network(
        &self,
        id: i64,
        config_json: &str,
        enabled: bool,
    ) -> Result<(), BridgeStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(BridgeCmd::UpdateBacnet {
                id,
                config_json: config_json.into(),
                enabled,
                reply: reply_tx,
            })
            .map_err(|_| BridgeStoreError::ChannelClosed)?;
        let res = reply_rx.await.map_err(|_| BridgeStoreError::ChannelClosed)?;
        if res.is_ok() {
            self.bump();
        }
        res
    }

    pub async fn delete_bacnet_network(&self, id: i64) -> Result<(), BridgeStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(BridgeCmd::DeleteBacnet { id, reply: reply_tx })
            .map_err(|_| BridgeStoreError::ChannelClosed)?;
        let res = reply_rx.await.map_err(|_| BridgeStoreError::ChannelClosed)?;
        if res.is_ok() {
            self.bump();
        }
        res
    }

    // ---- Modbus ----

    pub async fn list_modbus_buses(&self) -> Vec<StoredModbusBus> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(BridgeCmd::ListModbus { reply: reply_tx });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn get_modbus_bus(&self, id: i64) -> Option<StoredModbusBus> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(BridgeCmd::GetModbus { id, reply: reply_tx });
        reply_rx.await.ok().flatten()
    }

    pub async fn create_modbus_bus(
        &self,
        name: &str,
        config_json: &str,
        enabled: bool,
    ) -> Result<StoredModbusBus, BridgeStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(BridgeCmd::CreateModbus {
                name: name.into(),
                config_json: config_json.into(),
                enabled,
                reply: reply_tx,
            })
            .map_err(|_| BridgeStoreError::ChannelClosed)?;
        let res = reply_rx.await.map_err(|_| BridgeStoreError::ChannelClosed)?;
        if res.is_ok() {
            self.bump();
        }
        res
    }

    pub async fn update_modbus_bus(
        &self,
        id: i64,
        config_json: &str,
        enabled: bool,
    ) -> Result<(), BridgeStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(BridgeCmd::UpdateModbus {
                id,
                config_json: config_json.into(),
                enabled,
                reply: reply_tx,
            })
            .map_err(|_| BridgeStoreError::ChannelClosed)?;
        let res = reply_rx.await.map_err(|_| BridgeStoreError::ChannelClosed)?;
        if res.is_ok() {
            self.bump();
        }
        res
    }

    pub async fn delete_modbus_bus(&self, id: i64) -> Result<(), BridgeStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(BridgeCmd::DeleteModbus { id, reply: reply_tx })
            .map_err(|_| BridgeStoreError::ChannelClosed)?;
        let res = reply_rx.await.map_err(|_| BridgeStoreError::ChannelClosed)?;
        if res.is_ok() {
            self.bump();
        }
        res
    }
}

// ----------------------------------------------------------------
// Schema + entry point
// ----------------------------------------------------------------

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    label: "bridge_v1_initial",
    sql: "
CREATE TABLE IF NOT EXISTS bridge_bacnet_network (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL UNIQUE,
    config_json TEXT NOT NULL,
    enabled     INTEGER NOT NULL DEFAULT 1,
    created_ms  INTEGER NOT NULL,
    updated_ms  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS bridge_modbus_bus (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL UNIQUE,
    config_json TEXT NOT NULL,
    enabled     INTEGER NOT NULL DEFAULT 1,
    created_ms  INTEGER NOT NULL,
    updated_ms  INTEGER NOT NULL
);
",
}];

pub fn start_bridge_store_with_path(db_path: &Path) -> BridgeStore {
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (version_tx, _) = watch::channel(0u64);
    let path = db_path.to_path_buf();
    std::thread::Builder::new()
        .name("bridge-sqlite".into())
        .spawn(move || run_sqlite_thread(&path, cmd_rx))
        .expect("failed to spawn bridge SQLite thread");
    BridgeStore { cmd_tx, version_tx }
}

fn run_sqlite_thread(db_path: &Path, mut cmd_rx: mpsc::UnboundedReceiver<BridgeCmd>) {
    let conn = rusqlite::Connection::open(db_path).expect("failed to open bridge database");
    run_migrations(&conn, "bridge_store", MIGRATIONS).expect("bridge migrations failed");

    while let Some(cmd) = cmd_rx.blocking_recv() {
        match cmd {
            BridgeCmd::ListBacnet { reply } => {
                let _ = reply.send(list_bacnet_db(&conn));
            }
            BridgeCmd::GetBacnet { id, reply } => {
                let _ = reply.send(get_bacnet_db(&conn, id).ok());
            }
            BridgeCmd::CreateBacnet {
                name,
                config_json,
                enabled,
                reply,
            } => {
                let _ = reply.send(create_bacnet_db(&conn, &name, &config_json, enabled));
            }
            BridgeCmd::UpdateBacnet {
                id,
                config_json,
                enabled,
                reply,
            } => {
                let _ = reply.send(update_bacnet_db(&conn, id, &config_json, enabled));
            }
            BridgeCmd::DeleteBacnet { id, reply } => {
                let _ = reply.send(delete_bacnet_db(&conn, id));
            }
            BridgeCmd::ListModbus { reply } => {
                let _ = reply.send(list_modbus_db(&conn));
            }
            BridgeCmd::GetModbus { id, reply } => {
                let _ = reply.send(get_modbus_db(&conn, id).ok());
            }
            BridgeCmd::CreateModbus {
                name,
                config_json,
                enabled,
                reply,
            } => {
                let _ = reply.send(create_modbus_db(&conn, &name, &config_json, enabled));
            }
            BridgeCmd::UpdateModbus {
                id,
                config_json,
                enabled,
                reply,
            } => {
                let _ = reply.send(update_modbus_db(&conn, id, &config_json, enabled));
            }
            BridgeCmd::DeleteModbus { id, reply } => {
                let _ = reply.send(delete_modbus_db(&conn, id));
            }
        }
    }
}

// ---- BACnet DB helpers ----

fn list_bacnet_db(conn: &rusqlite::Connection) -> Vec<StoredBacnetNetwork> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, config_json, enabled, created_ms, updated_ms
             FROM bridge_bacnet_network ORDER BY name",
        )
        .unwrap();
    stmt.query_map([], |row| {
        Ok(StoredBacnetNetwork {
            id: row.get(0)?,
            name: row.get(1)?,
            config_json: row.get(2)?,
            enabled: row.get::<_, i64>(3)? != 0,
            created_ms: row.get(4)?,
            updated_ms: row.get(5)?,
        })
    })
    .unwrap()
    .filter_map(|r| r.ok())
    .collect()
}

fn get_bacnet_db(
    conn: &rusqlite::Connection,
    id: i64,
) -> Result<StoredBacnetNetwork, BridgeStoreError> {
    conn.query_row(
        "SELECT id, name, config_json, enabled, created_ms, updated_ms
         FROM bridge_bacnet_network WHERE id = ?1",
        rusqlite::params![id],
        |row| {
            Ok(StoredBacnetNetwork {
                id: row.get(0)?,
                name: row.get(1)?,
                config_json: row.get(2)?,
                enabled: row.get::<_, i64>(3)? != 0,
                created_ms: row.get(4)?,
                updated_ms: row.get(5)?,
            })
        },
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => BridgeStoreError::NotFound,
        other => BridgeStoreError::Db(other.to_string()),
    })
}

fn create_bacnet_db(
    conn: &rusqlite::Connection,
    name: &str,
    config_json: &str,
    enabled: bool,
) -> Result<StoredBacnetNetwork, BridgeStoreError> {
    let now = now_ms();
    conn.execute(
        "INSERT INTO bridge_bacnet_network (name, config_json, enabled, created_ms, updated_ms)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![name, config_json, enabled as i64, now, now],
    )
    .map_err(|e| match e {
        rusqlite::Error::SqliteFailure(_, Some(ref msg)) if msg.contains("UNIQUE") => {
            BridgeStoreError::Duplicate
        }
        other => BridgeStoreError::Db(other.to_string()),
    })?;
    let id = conn.last_insert_rowid();
    get_bacnet_db(conn, id)
}

fn update_bacnet_db(
    conn: &rusqlite::Connection,
    id: i64,
    config_json: &str,
    enabled: bool,
) -> Result<(), BridgeStoreError> {
    let now = now_ms();
    let n = conn
        .execute(
            "UPDATE bridge_bacnet_network
             SET config_json = ?1, enabled = ?2, updated_ms = ?3 WHERE id = ?4",
            rusqlite::params![config_json, enabled as i64, now, id],
        )
        .map_err(|e| BridgeStoreError::Db(e.to_string()))?;
    if n == 0 {
        return Err(BridgeStoreError::NotFound);
    }
    Ok(())
}

fn delete_bacnet_db(conn: &rusqlite::Connection, id: i64) -> Result<(), BridgeStoreError> {
    let n = conn
        .execute(
            "DELETE FROM bridge_bacnet_network WHERE id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| BridgeStoreError::Db(e.to_string()))?;
    if n == 0 {
        return Err(BridgeStoreError::NotFound);
    }
    Ok(())
}

// ---- Modbus DB helpers ----

fn list_modbus_db(conn: &rusqlite::Connection) -> Vec<StoredModbusBus> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, config_json, enabled, created_ms, updated_ms
             FROM bridge_modbus_bus ORDER BY name",
        )
        .unwrap();
    stmt.query_map([], |row| {
        Ok(StoredModbusBus {
            id: row.get(0)?,
            name: row.get(1)?,
            config_json: row.get(2)?,
            enabled: row.get::<_, i64>(3)? != 0,
            created_ms: row.get(4)?,
            updated_ms: row.get(5)?,
        })
    })
    .unwrap()
    .filter_map(|r| r.ok())
    .collect()
}

fn get_modbus_db(
    conn: &rusqlite::Connection,
    id: i64,
) -> Result<StoredModbusBus, BridgeStoreError> {
    conn.query_row(
        "SELECT id, name, config_json, enabled, created_ms, updated_ms
         FROM bridge_modbus_bus WHERE id = ?1",
        rusqlite::params![id],
        |row| {
            Ok(StoredModbusBus {
                id: row.get(0)?,
                name: row.get(1)?,
                config_json: row.get(2)?,
                enabled: row.get::<_, i64>(3)? != 0,
                created_ms: row.get(4)?,
                updated_ms: row.get(5)?,
            })
        },
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => BridgeStoreError::NotFound,
        other => BridgeStoreError::Db(other.to_string()),
    })
}

fn create_modbus_db(
    conn: &rusqlite::Connection,
    name: &str,
    config_json: &str,
    enabled: bool,
) -> Result<StoredModbusBus, BridgeStoreError> {
    let now = now_ms();
    conn.execute(
        "INSERT INTO bridge_modbus_bus (name, config_json, enabled, created_ms, updated_ms)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![name, config_json, enabled as i64, now, now],
    )
    .map_err(|e| match e {
        rusqlite::Error::SqliteFailure(_, Some(ref msg)) if msg.contains("UNIQUE") => {
            BridgeStoreError::Duplicate
        }
        other => BridgeStoreError::Db(other.to_string()),
    })?;
    let id = conn.last_insert_rowid();
    get_modbus_db(conn, id)
}

fn update_modbus_db(
    conn: &rusqlite::Connection,
    id: i64,
    config_json: &str,
    enabled: bool,
) -> Result<(), BridgeStoreError> {
    let now = now_ms();
    let n = conn
        .execute(
            "UPDATE bridge_modbus_bus
             SET config_json = ?1, enabled = ?2, updated_ms = ?3 WHERE id = ?4",
            rusqlite::params![config_json, enabled as i64, now, id],
        )
        .map_err(|e| BridgeStoreError::Db(e.to_string()))?;
    if n == 0 {
        return Err(BridgeStoreError::NotFound);
    }
    Ok(())
}

fn delete_modbus_db(conn: &rusqlite::Connection, id: i64) -> Result<(), BridgeStoreError> {
    let n = conn
        .execute(
            "DELETE FROM bridge_modbus_bus WHERE id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| BridgeStoreError::Db(e.to_string()))?;
    if n == 0 {
        return Err(BridgeStoreError::NotFound);
    }
    Ok(())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store(name: &str) -> BridgeStore {
        let path = std::path::PathBuf::from(format!("/tmp/test_{name}.db"));
        if path.exists() {
            std::fs::remove_file(&path).ok();
        }
        start_bridge_store_with_path(&path)
    }

    #[tokio::test]
    async fn bacnet_crud() {
        let s = test_store("bridge_bacnet_crud");
        assert!(s.list_bacnet_networks().await.is_empty());

        let cfg = r#"{"mode":"normal","bbmd_addr":"192.168.1.1:47808","ttl":60}"#;
        let n = s
            .create_bacnet_network("main", cfg, true)
            .await
            .unwrap();
        assert_eq!(n.name, "main");
        assert!(n.enabled);

        let list = s.list_bacnet_networks().await;
        assert_eq!(list.len(), 1);

        s.update_bacnet_network(n.id, cfg, false).await.unwrap();
        assert!(!s.get_bacnet_network(n.id).await.unwrap().enabled);

        s.delete_bacnet_network(n.id).await.unwrap();
        assert!(s.list_bacnet_networks().await.is_empty());

        std::fs::remove_file("/tmp/test_bridge_bacnet_crud.db").ok();
    }

    #[tokio::test]
    async fn duplicate_name_rejected() {
        let s = test_store("bridge_dup");
        s.create_bacnet_network("a", "{}", true).await.unwrap();
        let err = s
            .create_bacnet_network("a", "{}", true)
            .await
            .unwrap_err();
        assert!(matches!(err, BridgeStoreError::Duplicate));
        std::fs::remove_file("/tmp/test_bridge_dup.db").ok();
    }

    #[tokio::test]
    async fn modbus_crud() {
        let s = test_store("bridge_modbus_crud");
        let cfg = r#"{"mode":"rtu","serial_port":"/dev/ttyUSB0","baud_rate":9600}"#;
        let b = s.create_modbus_bus("rtu-bus", cfg, true).await.unwrap();
        assert_eq!(b.name, "rtu-bus");
        let list = s.list_modbus_buses().await;
        assert_eq!(list.len(), 1);
        s.delete_modbus_bus(b.id).await.unwrap();
        assert!(s.list_modbus_buses().await.is_empty());
        std::fs::remove_file("/tmp/test_bridge_modbus_crud.db").ok();
    }
}
