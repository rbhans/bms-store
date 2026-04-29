use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::{mpsc, oneshot, watch};

use crate::discovery::model::{
    ConnStatus, DeviceState, DiscoveredDevice, DiscoveredPoint, PointKindHint, PROTOCOL_BACNET,
};
use crate::event::bus::{Event, EventBus};

// ----------------------------------------------------------------
// Error type
// ----------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("database error: {0}")]
    Db(String),
    #[error("channel closed")]
    ChannelClosed,
    #[error("not found: {0}")]
    NotFound(String),
}

// ----------------------------------------------------------------
// Commands
// ----------------------------------------------------------------

enum DiscoveryCmd {
    UpsertDevice {
        device: DiscoveredDevice,
        reply: oneshot::Sender<Result<(), DiscoveryError>>,
    },
    UpsertPoints {
        device_id: String,
        points: Vec<DiscoveredPoint>,
        reply: oneshot::Sender<Result<(), DiscoveryError>>,
    },
    ListDevices {
        state_filter: Option<DeviceState>,
        reply: oneshot::Sender<Vec<DiscoveredDevice>>,
    },
    GetDevice {
        id: String,
        reply: oneshot::Sender<Result<DiscoveredDevice, DiscoveryError>>,
    },
    GetPoints {
        device_id: String,
        reply: oneshot::Sender<Vec<DiscoveredPoint>>,
    },
    SetDeviceState {
        id: String,
        state: DeviceState,
        reply: oneshot::Sender<Result<(), DiscoveryError>>,
    },
    SetConnStatus {
        id: String,
        status: ConnStatus,
        reply: oneshot::Sender<Result<(), DiscoveryError>>,
    },
    RecordScan {
        protocol: String,
        reply: oneshot::Sender<i64>,
    },
    FinishScan {
        scan_id: i64,
        device_count: usize,
        reply: oneshot::Sender<()>,
    },
    ClearPending {
        reply: oneshot::Sender<usize>,
    },
    UpdateDeviceName {
        id: String,
        display_name: String,
        reply: oneshot::Sender<Result<(), DiscoveryError>>,
    },
    UpdatePoint {
        device_id: String,
        point_id: String,
        display_name: Option<String>,
        units: Option<String>,
        description: Option<String>,
        state_labels: Option<Option<std::collections::HashMap<String, String>>>,
        reply: oneshot::Sender<Result<(), DiscoveryError>>,
    },
    BulkUpdatePoints {
        device_id: String,
        point_ids: Vec<String>,
        units: Option<String>,
        description: Option<String>,
        reply: oneshot::Sender<Result<usize, DiscoveryError>>,
    },
    BulkRenameDevices {
        device_ids: Vec<String>,
        names: Vec<String>,
        reply: oneshot::Sender<Result<usize, DiscoveryError>>,
    },
    GetAllDevicePoints {
        reply: oneshot::Sender<Vec<(String, Vec<DiscoveredPoint>)>>,
    },
    SetObjectListStale {
        id: String,
        stale: bool,
        reply: oneshot::Sender<Result<(), DiscoveryError>>,
    },
}

// ----------------------------------------------------------------
// DiscoveryStore handle
// ----------------------------------------------------------------

#[derive(Clone)]
pub struct DiscoveryStore {
    cmd_tx: mpsc::UnboundedSender<DiscoveryCmd>,
    version_tx: watch::Sender<u64>,
    version_rx: watch::Receiver<u64>,
    event_bus: Option<EventBus>,
}

impl DiscoveryStore {
    pub fn with_event_bus(mut self, bus: EventBus) -> Self {
        self.event_bus = Some(bus);
        self
    }

    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.version_rx.clone()
    }

    fn bump_version(&self) {
        self.version_tx.send_modify(|v| *v += 1);
    }

    pub async fn upsert_device(&self, device: DiscoveredDevice) -> Result<(), DiscoveryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(DiscoveryCmd::UpsertDevice {
                device,
                reply: reply_tx,
            })
            .map_err(|_| DiscoveryError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| DiscoveryError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_version();
        }
        result
    }

    pub async fn upsert_points(
        &self,
        device_id: &str,
        points: Vec<DiscoveredPoint>,
    ) -> Result<(), DiscoveryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(DiscoveryCmd::UpsertPoints {
                device_id: device_id.to_string(),
                points,
                reply: reply_tx,
            })
            .map_err(|_| DiscoveryError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| DiscoveryError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_version();
        }
        result
    }

    pub async fn list_devices(&self, state_filter: Option<DeviceState>) -> Vec<DiscoveredDevice> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(DiscoveryCmd::ListDevices {
            state_filter,
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn get_device(&self, id: &str) -> Result<DiscoveredDevice, DiscoveryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(DiscoveryCmd::GetDevice {
                id: id.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| DiscoveryError::ChannelClosed)?;
        reply_rx.await.map_err(|_| DiscoveryError::ChannelClosed)?
    }

    pub async fn get_points(&self, device_id: &str) -> Vec<DiscoveredPoint> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(DiscoveryCmd::GetPoints {
            device_id: device_id.to_string(),
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn set_device_state(
        &self,
        id: &str,
        state: DeviceState,
    ) -> Result<(), DiscoveryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(DiscoveryCmd::SetDeviceState {
                id: id.to_string(),
                state,
                reply: reply_tx,
            })
            .map_err(|_| DiscoveryError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| DiscoveryError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_version();
        }
        result
    }

    pub async fn set_conn_status(
        &self,
        id: &str,
        status: ConnStatus,
    ) -> Result<(), DiscoveryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(DiscoveryCmd::SetConnStatus {
                id: id.to_string(),
                status,
                reply: reply_tx,
            })
            .map_err(|_| DiscoveryError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| DiscoveryError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_version();
        }
        result
    }

    pub async fn record_scan(&self, protocol: &str) -> i64 {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(DiscoveryCmd::RecordScan {
            protocol: protocol.to_string(),
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or(0)
    }

    pub async fn finish_scan(&self, scan_id: i64, device_count: usize) {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(DiscoveryCmd::FinishScan {
            scan_id,
            device_count,
            reply: reply_tx,
        });
        let _ = reply_rx.await;
    }

    pub async fn update_device_name(
        &self,
        id: &str,
        display_name: &str,
    ) -> Result<(), DiscoveryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(DiscoveryCmd::UpdateDeviceName {
                id: id.to_string(),
                display_name: display_name.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| DiscoveryError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| DiscoveryError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_version();
        }
        result
    }

    pub async fn update_point(
        &self,
        device_id: &str,
        point_id: &str,
        display_name: Option<&str>,
        units: Option<&str>,
        description: Option<&str>,
        state_labels: Option<Option<&std::collections::HashMap<String, String>>>,
    ) -> Result<(), DiscoveryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(DiscoveryCmd::UpdatePoint {
                device_id: device_id.to_string(),
                point_id: point_id.to_string(),
                display_name: display_name.map(String::from),
                units: units.map(String::from),
                description: description.map(String::from),
                state_labels: state_labels.map(|opt| opt.cloned()),
                reply: reply_tx,
            })
            .map_err(|_| DiscoveryError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| DiscoveryError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_version();
        }
        result
    }

    pub async fn bulk_update_points(
        &self,
        device_id: &str,
        point_ids: &[String],
        units: Option<&str>,
        description: Option<&str>,
    ) -> Result<usize, DiscoveryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(DiscoveryCmd::BulkUpdatePoints {
                device_id: device_id.to_string(),
                point_ids: point_ids.to_vec(),
                units: units.map(String::from),
                description: description.map(String::from),
                reply: reply_tx,
            })
            .map_err(|_| DiscoveryError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| DiscoveryError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_version();
        }
        result
    }

    /// Rename multiple devices in one call. `ids` and `names` must be the same length.
    /// Returns the number of devices successfully renamed.
    pub async fn bulk_rename_devices(
        &self,
        ids: &[String],
        names: &[String],
    ) -> Result<usize, DiscoveryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(DiscoveryCmd::BulkRenameDevices {
                device_ids: ids.to_vec(),
                names: names.to_vec(),
                reply: reply_tx,
            })
            .map_err(|_| DiscoveryError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| DiscoveryError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_version();
        }
        result
    }

    /// Get all devices with their points in one query (avoids N+1).
    pub async fn get_all_device_points(&self) -> Vec<(String, Vec<DiscoveredPoint>)> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .cmd_tx
            .send(DiscoveryCmd::GetAllDevicePoints { reply: reply_tx });
        reply_rx.await.unwrap_or_default()
    }

    /// Mark a device's object list as stale (changed since last scan).
    pub async fn set_object_list_stale(&self, id: &str, stale: bool) -> Result<(), DiscoveryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(DiscoveryCmd::SetObjectListStale {
                id: id.to_string(),
                stale,
                reply: reply_tx,
            })
            .map_err(|_| DiscoveryError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| DiscoveryError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_version();
        }
        result
    }

    /// Remove all devices in Discovered (pending) state and their points.
    /// Returns the number of devices removed.
    pub async fn clear_pending(&self) -> usize {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .cmd_tx
            .send(DiscoveryCmd::ClearPending { reply: reply_tx });
        reply_rx.await.unwrap_or(0)
    }
}

// ----------------------------------------------------------------
// Schema
// ----------------------------------------------------------------

use super::migration::{run_migrations, Migration};

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        label: "initial discovery schema",
        sql: "
CREATE TABLE IF NOT EXISTS discovered_device (
    id TEXT PRIMARY KEY,
    protocol TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'discovered',
    conn_status TEXT NOT NULL DEFAULT 'unknown',
    display_name TEXT NOT NULL,
    vendor TEXT,
    model TEXT,
    address TEXT NOT NULL,
    point_count INTEGER NOT NULL DEFAULT 0,
    discovered_at INTEGER NOT NULL,
    accepted_at INTEGER,
    protocol_meta TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS discovered_point (
    id TEXT NOT NULL,
    device_id TEXT NOT NULL REFERENCES discovered_device(id) ON DELETE CASCADE,
    display_name TEXT NOT NULL,
    description TEXT,
    units TEXT,
    point_kind TEXT NOT NULL DEFAULT 'analog',
    writable INTEGER NOT NULL DEFAULT 0,
    binding_json TEXT NOT NULL,
    protocol_meta TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY (device_id, id)
);

CREATE TABLE IF NOT EXISTS discovery_scan (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    protocol TEXT NOT NULL,
    started_at INTEGER NOT NULL,
    ended_at INTEGER,
    device_count INTEGER DEFAULT 0,
    status TEXT DEFAULT 'running'
);
",
    },
    Migration {
        version: 2,
        label: "add state_labels to discovered_point",
        sql: "ALTER TABLE discovered_point ADD COLUMN state_labels TEXT;",
    },
    Migration {
        version: 3,
        label: "add network_id to discovered_device",
        sql: "ALTER TABLE discovered_device ADD COLUMN network_id TEXT NOT NULL DEFAULT '';",
    },
    Migration {
        version: 4,
        label: "add monitor columns to discovered_device",
        sql:
            "ALTER TABLE discovered_device ADD COLUMN object_list_stale INTEGER NOT NULL DEFAULT 0;
ALTER TABLE discovered_device ADD COLUMN last_monitor_ms INTEGER;",
    },
];

// ----------------------------------------------------------------
// Start functions
// ----------------------------------------------------------------

pub fn start_discovery_store() -> DiscoveryStore {
    start_discovery_store_with_path(&PathBuf::from("data/discovery.db"))
}

pub fn start_discovery_store_with_path(db_path: &Path) -> DiscoveryStore {
    let db_dir = db_path.parent().unwrap_or(Path::new("."));
    if !db_dir.exists() {
        std::fs::create_dir_all(db_dir).expect("failed to create data directory");
    }

    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (version_tx, version_rx) = watch::channel(0u64);

    let path_clone = db_path.to_path_buf();
    std::thread::Builder::new()
        .name("discovery-sqlite".into())
        .spawn(move || run_sqlite_thread(&path_clone, cmd_rx))
        .expect("failed to spawn discovery SQLite thread");

    DiscoveryStore {
        cmd_tx,
        version_tx,
        version_rx,
        event_bus: None,
    }
}

/// Spawn a background task that subscribes to the EventBus and updates
/// device connectivity status in the DiscoveryStore when DeviceDown or
/// DeviceDiscovered events are received.
pub fn start_conn_status_listener(
    store: DiscoveryStore,
    bus: EventBus,
    shutdown: tokio_util::sync::CancellationToken,
) {
    tokio::spawn(async move {
        let mut rx = bus.subscribe();
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                result = rx.recv() => {
                    match result {
                        Ok(event) => match event.as_ref() {
                            Event::DeviceDown { device_key, .. } => {
                                let _ = store.set_conn_status(device_key, ConnStatus::Offline).await;
                            }
                            Event::DeviceDiscovered { device_key, .. } => {
                                let _ = store.set_conn_status(device_key, ConnStatus::Online).await;
                            }
                            Event::ObjectListChanged { device_key, .. } => {
                                let _ = store.set_object_list_stale(device_key, true).await;
                            }
                            _ => {}
                        },
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(lagged = n, "Discovery conn_status listener lagged");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    });
}

// ----------------------------------------------------------------
// SQLite thread
// ----------------------------------------------------------------

fn run_sqlite_thread(db_path: &Path, mut cmd_rx: mpsc::UnboundedReceiver<DiscoveryCmd>) {
    let conn = rusqlite::Connection::open(db_path).expect("failed to open discovery database");
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;",
    )
    .expect("failed to set pragmas");
    run_migrations(&conn, "discovery", MIGRATIONS).expect("discovery: schema migration failed");

    while let Some(cmd) = cmd_rx.blocking_recv() {
        match cmd {
            DiscoveryCmd::UpsertDevice { device, reply } => {
                let result = upsert_device_db(&conn, &device);
                let _ = reply.send(result);
            }
            DiscoveryCmd::UpsertPoints {
                device_id,
                points,
                reply,
            } => {
                let result = upsert_points_db(&conn, &device_id, &points);
                let _ = reply.send(result);
            }
            DiscoveryCmd::ListDevices {
                state_filter,
                reply,
            } => {
                let result = list_devices_db(&conn, state_filter);
                let _ = reply.send(result);
            }
            DiscoveryCmd::GetDevice { id, reply } => {
                let result = get_device_db(&conn, &id);
                let _ = reply.send(result);
            }
            DiscoveryCmd::GetPoints { device_id, reply } => {
                let result = get_points_db(&conn, &device_id);
                let _ = reply.send(result);
            }
            DiscoveryCmd::SetDeviceState { id, state, reply } => {
                let result = set_device_state_db(&conn, &id, state);
                let _ = reply.send(result);
            }
            DiscoveryCmd::SetConnStatus { id, status, reply } => {
                let result = set_conn_status_db(&conn, &id, status);
                let _ = reply.send(result);
            }
            DiscoveryCmd::RecordScan { protocol, reply } => {
                let scan_id = record_scan_db(&conn, &protocol);
                let _ = reply.send(scan_id);
            }
            DiscoveryCmd::FinishScan {
                scan_id,
                device_count,
                reply,
            } => {
                finish_scan_db(&conn, scan_id, device_count);
                let _ = reply.send(());
            }
            DiscoveryCmd::ClearPending { reply } => {
                let count = clear_pending_db(&conn);
                let _ = reply.send(count);
            }
            DiscoveryCmd::UpdateDeviceName {
                id,
                display_name,
                reply,
            } => {
                let result = update_device_name_db(&conn, &id, &display_name);
                let _ = reply.send(result);
            }
            DiscoveryCmd::UpdatePoint {
                device_id,
                point_id,
                display_name,
                units,
                description,
                state_labels,
                reply,
            } => {
                let sl_ref = state_labels.as_ref().map(|opt| opt.as_ref());
                let result = update_point_db(
                    &conn,
                    &device_id,
                    &point_id,
                    display_name.as_deref(),
                    units.as_deref(),
                    description.as_deref(),
                    sl_ref,
                );
                let _ = reply.send(result);
            }
            DiscoveryCmd::BulkUpdatePoints {
                device_id,
                point_ids,
                units,
                description,
                reply,
            } => {
                let result = bulk_update_points_db(
                    &conn,
                    &device_id,
                    &point_ids,
                    units.as_deref(),
                    description.as_deref(),
                );
                let _ = reply.send(result);
            }
            DiscoveryCmd::BulkRenameDevices {
                device_ids,
                names,
                reply,
            } => {
                let result = bulk_rename_devices_db(&conn, &device_ids, &names);
                let _ = reply.send(result);
            }
            DiscoveryCmd::GetAllDevicePoints { reply } => {
                let result = get_all_device_points_db(&conn);
                let _ = reply.send(result);
            }
            DiscoveryCmd::SetObjectListStale { id, stale, reply } => {
                let result = set_object_list_stale_db(&conn, &id, stale);
                let _ = reply.send(result);
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
        .unwrap_or_default()
        .as_millis() as i64
}

fn upsert_device_db(
    conn: &rusqlite::Connection,
    device: &DiscoveredDevice,
) -> Result<(), DiscoveryError> {
    let meta_str = serde_json::to_string(&device.protocol_meta).unwrap_or_else(|_| "{}".into());

    // On re-discovery, preserve state and accepted_at — only update connectivity and metadata
    conn.execute(
        "INSERT INTO discovered_device (id, protocol, state, conn_status, display_name, vendor, model, address, point_count, discovered_at, accepted_at, protocol_meta, network_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
         ON CONFLICT(id) DO UPDATE SET
             conn_status = excluded.conn_status,
             display_name = excluded.display_name,
             vendor = COALESCE(excluded.vendor, discovered_device.vendor),
             model = COALESCE(excluded.model, discovered_device.model),
             address = excluded.address,
             point_count = excluded.point_count,
             protocol_meta = excluded.protocol_meta,
             network_id = excluded.network_id",
        rusqlite::params![
            device.id,
            &device.protocol,
            device.state.as_str(),
            device.conn_status.as_str(),
            device.display_name,
            device.vendor,
            device.model,
            device.address,
            device.point_count as i64,
            device.discovered_at_ms,
            device.accepted_at_ms,
            meta_str,
            device.network_id,
        ],
    )
    .map_err(|e| DiscoveryError::Db(e.to_string()))?;

    Ok(())
}

fn upsert_points_db(
    conn: &rusqlite::Connection,
    device_id: &str,
    points: &[DiscoveredPoint],
) -> Result<(), DiscoveryError> {
    // Remove points that no longer exist on the device
    if !points.is_empty() {
        let existing_ids: Vec<String> = points.iter().map(|p| p.id.clone()).collect();
        let placeholders: String = existing_ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 2))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "DELETE FROM discovered_point WHERE device_id = ?1 AND id NOT IN ({})",
            placeholders
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        params.push(Box::new(device_id.to_string()));
        for id in &existing_ids {
            params.push(Box::new(id.clone()));
        }
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        conn.execute(&sql, param_refs.as_slice())
            .map_err(|e| DiscoveryError::Db(e.to_string()))?;
    }

    for pt in points {
        let binding_json = serde_json::to_string(&pt.binding).unwrap_or_else(|_| "{}".into());
        let meta_str = serde_json::to_string(&pt.protocol_meta).unwrap_or_else(|_| "{}".into());
        let state_labels_str = pt
            .state_labels
            .as_ref()
            .and_then(|m| serde_json::to_string(m).ok());

        conn.execute(
            "INSERT INTO discovered_point (id, device_id, display_name, description, units, point_kind, writable, binding_json, protocol_meta, state_labels)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(device_id, id) DO UPDATE SET
                 point_kind = excluded.point_kind,
                 writable = excluded.writable,
                 binding_json = excluded.binding_json,
                 protocol_meta = excluded.protocol_meta",
            rusqlite::params![
                pt.id,
                device_id,
                pt.display_name,
                pt.description,
                pt.units,
                pt.point_kind.as_str(),
                pt.writable as i32,
                binding_json,
                meta_str,
                state_labels_str,
            ],
        )
        .map_err(|e| DiscoveryError::Db(e.to_string()))?;
    }

    Ok(())
}

fn list_devices_db(
    conn: &rusqlite::Connection,
    state_filter: Option<DeviceState>,
) -> Vec<DiscoveredDevice> {
    let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match state_filter {
        Some(state) => (
            "SELECT id, protocol, state, conn_status, display_name, vendor, model, address, point_count, discovered_at, accepted_at, protocol_meta, network_id FROM discovered_device WHERE state = ?1 ORDER BY display_name".into(),
            vec![Box::new(state.as_str().to_string())],
        ),
        None => (
            "SELECT id, protocol, state, conn_status, display_name, vendor, model, address, point_count, discovered_at, accepted_at, protocol_meta, network_id FROM discovered_device ORDER BY display_name".into(),
            vec![],
        ),
    };

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("list_devices query failed: {e}");
            return vec![];
        }
    };
    let rows = match stmt.query_map(param_refs.as_slice(), |row| Ok(row_to_device(row))) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("list_devices query_map failed: {e}");
            return vec![];
        }
    };

    rows.filter_map(|r| r.ok()).collect()
}

fn get_device_db(
    conn: &rusqlite::Connection,
    id: &str,
) -> Result<DiscoveredDevice, DiscoveryError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, protocol, state, conn_status, display_name, vendor, model, address, point_count, discovered_at, accepted_at, protocol_meta, network_id FROM discovered_device WHERE id = ?1",
        )
        .map_err(|e| DiscoveryError::Db(e.to_string()))?;

    stmt.query_row(rusqlite::params![id], |row| Ok(row_to_device(row)))
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => DiscoveryError::NotFound(id.to_string()),
            other => DiscoveryError::Db(other.to_string()),
        })
}

fn row_to_device(row: &rusqlite::Row) -> DiscoveredDevice {
    let protocol_str: String = row.get(1).unwrap_or_default();
    let state_str: String = row.get(2).unwrap_or_default();
    let conn_str: String = row.get(3).unwrap_or_default();
    let meta_str: String = row.get(11).unwrap_or_default();
    let network_id: String = row.get(12).unwrap_or_default();

    DiscoveredDevice {
        id: row.get(0).unwrap_or_default(),
        protocol: if protocol_str.is_empty() {
            PROTOCOL_BACNET.into()
        } else {
            protocol_str
        },
        state: DeviceState::from_str(&state_str).unwrap_or(DeviceState::Discovered),
        conn_status: ConnStatus::from_str(&conn_str).unwrap_or(ConnStatus::Unknown),
        display_name: row.get(4).unwrap_or_default(),
        vendor: row.get(5).unwrap_or_default(),
        model: row.get(6).unwrap_or_default(),
        address: row.get(7).unwrap_or_default(),
        point_count: row.get::<_, i64>(8).unwrap_or(0) as usize,
        discovered_at_ms: row.get(9).unwrap_or(0),
        accepted_at_ms: row.get(10).unwrap_or(None),
        protocol_meta: serde_json::from_str(&meta_str)
            .unwrap_or(serde_json::Value::Object(Default::default())),
        network_id,
    }
}

fn get_points_db(conn: &rusqlite::Connection, device_id: &str) -> Vec<DiscoveredPoint> {
    let mut stmt = match conn.prepare(
        "SELECT id, device_id, display_name, description, units, point_kind, writable, binding_json, protocol_meta, state_labels FROM discovered_point WHERE device_id = ?1 ORDER BY display_name",
    ) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("get_points query failed: {e}");
            return vec![];
        }
    };

    let rows = match stmt.query_map(rusqlite::params![device_id], |row| {
        let kind_str: String = row.get(5).unwrap_or_default();
        let writable_int: i32 = row.get(6).unwrap_or(0);
        let binding_str: String = row.get(7).unwrap_or_default();
        let meta_str: String = row.get(8).unwrap_or_default();
        let state_labels_str: Option<String> = row.get(9).unwrap_or(None);

        let state_labels = state_labels_str.and_then(|s| serde_json::from_str(&s).ok());

        Ok(DiscoveredPoint {
            id: row.get(0).unwrap_or_default(),
            device_id: row.get(1).unwrap_or_default(),
            display_name: row.get(2).unwrap_or_default(),
            description: row.get(3).unwrap_or_default(),
            units: row.get(4).unwrap_or_default(),
            point_kind: PointKindHint::from_str(&kind_str).unwrap_or(PointKindHint::Analog),
            writable: writable_int != 0,
            binding: serde_json::from_str(&binding_str)
                .unwrap_or_else(|_| crate::node::ProtocolBinding::virtual_binding()),
            protocol_meta: serde_json::from_str(&meta_str)
                .unwrap_or(serde_json::Value::Object(Default::default())),
            state_labels,
        })
    }) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("get_points query_map failed: {e}");
            return vec![];
        }
    };

    rows.filter_map(|r| r.ok()).collect()
}

fn set_device_state_db(
    conn: &rusqlite::Connection,
    id: &str,
    state: DeviceState,
) -> Result<(), DiscoveryError> {
    let accepted_at = if state == DeviceState::Accepted {
        Some(now_ms())
    } else {
        None
    };

    let rows = conn
        .execute(
            "UPDATE discovered_device SET state = ?1, accepted_at = COALESCE(?2, accepted_at) WHERE id = ?3",
            rusqlite::params![state.as_str(), accepted_at, id],
        )
        .map_err(|e| DiscoveryError::Db(e.to_string()))?;

    if rows == 0 {
        return Err(DiscoveryError::NotFound(id.to_string()));
    }
    Ok(())
}

fn set_conn_status_db(
    conn: &rusqlite::Connection,
    id: &str,
    status: ConnStatus,
) -> Result<(), DiscoveryError> {
    let rows = conn
        .execute(
            "UPDATE discovered_device SET conn_status = ?1 WHERE id = ?2",
            rusqlite::params![status.as_str(), id],
        )
        .map_err(|e| DiscoveryError::Db(e.to_string()))?;

    if rows == 0 {
        return Err(DiscoveryError::NotFound(id.to_string()));
    }
    Ok(())
}

fn record_scan_db(conn: &rusqlite::Connection, protocol: &str) -> i64 {
    let now = now_ms();
    conn.execute(
        "INSERT INTO discovery_scan (protocol, started_at, status) VALUES (?1, ?2, 'running')",
        rusqlite::params![protocol, now],
    )
    .unwrap_or(0);
    conn.last_insert_rowid()
}

fn finish_scan_db(conn: &rusqlite::Connection, scan_id: i64, device_count: usize) {
    let now = now_ms();
    let _ = conn.execute(
        "UPDATE discovery_scan SET ended_at = ?1, device_count = ?2, status = 'complete' WHERE id = ?3",
        rusqlite::params![now, device_count as i64, scan_id],
    );
}

/// Delete all devices (and their points) that are still in Discovered state.
/// Returns the number of devices removed.
fn update_device_name_db(
    conn: &rusqlite::Connection,
    id: &str,
    display_name: &str,
) -> Result<(), DiscoveryError> {
    let rows = conn
        .execute(
            "UPDATE discovered_device SET display_name = ?1 WHERE id = ?2",
            rusqlite::params![display_name, id],
        )
        .map_err(|e| DiscoveryError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(DiscoveryError::NotFound(id.to_string()));
    }
    Ok(())
}

fn update_point_db(
    conn: &rusqlite::Connection,
    device_id: &str,
    point_id: &str,
    display_name: Option<&str>,
    units: Option<&str>,
    description: Option<&str>,
    state_labels: Option<Option<&std::collections::HashMap<String, String>>>,
) -> Result<(), DiscoveryError> {
    let mut sets = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1;

    if let Some(v) = display_name {
        sets.push(format!("display_name = ?{idx}"));
        params.push(Box::new(v.to_string()));
        idx += 1;
    }
    if let Some(v) = units {
        sets.push(format!("units = ?{idx}"));
        params.push(Box::new(v.to_string()));
        idx += 1;
    }
    if let Some(v) = description {
        sets.push(format!("description = ?{idx}"));
        params.push(Box::new(v.to_string()));
        idx += 1;
    }
    if let Some(labels_opt) = state_labels {
        sets.push(format!("state_labels = ?{idx}"));
        let json_str: Option<String> = labels_opt.and_then(|m| serde_json::to_string(m).ok());
        params.push(Box::new(json_str));
        idx += 1;
    }

    if sets.is_empty() {
        return Ok(());
    }

    let sql = format!(
        "UPDATE discovered_point SET {} WHERE device_id = ?{} AND id = ?{}",
        sets.join(", "),
        idx,
        idx + 1
    );
    params.push(Box::new(device_id.to_string()));
    params.push(Box::new(point_id.to_string()));

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows = conn
        .execute(&sql, param_refs.as_slice())
        .map_err(|e| DiscoveryError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(DiscoveryError::NotFound(format!("{device_id}/{point_id}")));
    }
    Ok(())
}

fn bulk_update_points_db(
    conn: &rusqlite::Connection,
    device_id: &str,
    point_ids: &[String],
    units: Option<&str>,
    description: Option<&str>,
) -> Result<usize, DiscoveryError> {
    if point_ids.is_empty() {
        return Ok(0);
    }

    let mut sets = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1;

    if let Some(v) = units {
        sets.push(format!("units = ?{idx}"));
        params.push(Box::new(v.to_string()));
        idx += 1;
    }
    if let Some(v) = description {
        sets.push(format!("description = ?{idx}"));
        params.push(Box::new(v.to_string()));
        idx += 1;
    }

    if sets.is_empty() {
        return Ok(0);
    }

    // device_id param
    let device_param_idx = idx;
    params.push(Box::new(device_id.to_string()));
    idx += 1;

    // Build IN clause placeholders
    let placeholders: Vec<String> = point_ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("?{}", idx + i))
        .collect();
    for pid in point_ids {
        params.push(Box::new(pid.clone()));
    }

    let sql = format!(
        "UPDATE discovered_point SET {} WHERE device_id = ?{} AND id IN ({})",
        sets.join(", "),
        device_param_idx,
        placeholders.join(", ")
    );

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let count = conn
        .execute(&sql, param_refs.as_slice())
        .map_err(|e| DiscoveryError::Db(e.to_string()))?;
    Ok(count)
}

fn bulk_rename_devices_db(
    conn: &rusqlite::Connection,
    device_ids: &[String],
    names: &[String],
) -> Result<usize, DiscoveryError> {
    if device_ids.len() != names.len() {
        return Err(DiscoveryError::Db(
            "device_ids and names must have the same length".into(),
        ));
    }
    let mut count = 0;
    for (id, name) in device_ids.iter().zip(names.iter()) {
        let rows = conn
            .execute(
                "UPDATE discovered_device SET display_name = ?1 WHERE id = ?2",
                rusqlite::params![name, id],
            )
            .map_err(|e| DiscoveryError::Db(e.to_string()))?;
        count += rows;
    }
    Ok(count)
}

fn get_all_device_points_db(conn: &rusqlite::Connection) -> Vec<(String, Vec<DiscoveredPoint>)> {
    let mut stmt = match conn.prepare(
        "SELECT p.id, p.device_id, p.display_name, p.description, p.units, p.point_kind, p.writable, p.binding_json, p.protocol_meta, p.state_labels
         FROM discovered_point p
         INNER JOIN discovered_device d ON p.device_id = d.id
         ORDER BY d.display_name, p.display_name"
    ) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("get_all_device_points query failed: {e}");
            return vec![];
        }
    };

    let rows = match stmt.query_map([], |row| {
        let kind_str: String = row.get(5).unwrap_or_default();
        let writable_int: i32 = row.get(6).unwrap_or(0);
        let binding_str: String = row.get(7).unwrap_or_default();
        let meta_str: String = row.get(8).unwrap_or_default();
        let state_labels_str: Option<String> = row.get(9).unwrap_or(None);
        let state_labels = state_labels_str.and_then(|s| serde_json::from_str(&s).ok());

        Ok(DiscoveredPoint {
            id: row.get(0).unwrap_or_default(),
            device_id: row.get(1).unwrap_or_default(),
            display_name: row.get(2).unwrap_or_default(),
            description: row.get(3).unwrap_or_default(),
            units: row.get(4).unwrap_or_default(),
            point_kind: PointKindHint::from_str(&kind_str).unwrap_or(PointKindHint::Analog),
            writable: writable_int != 0,
            binding: serde_json::from_str(&binding_str)
                .unwrap_or_else(|_| crate::node::ProtocolBinding::virtual_binding()),
            protocol_meta: serde_json::from_str(&meta_str)
                .unwrap_or(serde_json::Value::Object(Default::default())),
            state_labels,
        })
    }) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("get_all_device_points query_map failed: {e}");
            return vec![];
        }
    };

    // Group by device_id
    let mut map: std::collections::HashMap<String, Vec<DiscoveredPoint>> =
        std::collections::HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for pt in rows.flatten() {
        let did = pt.device_id.clone();
        if !map.contains_key(&did) {
            order.push(did.clone());
        }
        map.entry(did).or_default().push(pt);
    }
    order
        .into_iter()
        .filter_map(|id| map.remove(&id).map(|pts| (id, pts)))
        .collect()
}

fn clear_pending_db(conn: &rusqlite::Connection) -> usize {
    let mut stmt = match conn.prepare("SELECT id FROM discovered_device WHERE state = 'discovered'")
    {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("clear_pending query failed: {e}");
            return 0;
        }
    };
    let ids: Vec<String> = match stmt.query_map([], |row| row.get(0)) {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
        Err(e) => {
            tracing::warn!("clear_pending query_map failed: {e}");
            return 0;
        }
    };

    for id in &ids {
        let _ = conn.execute("DELETE FROM discovered_point WHERE device_id = ?1", [id]);
    }
    let _ = conn.execute(
        "DELETE FROM discovered_device WHERE state = 'discovered'",
        [],
    );
    ids.len()
}

fn set_object_list_stale_db(
    conn: &rusqlite::Connection,
    id: &str,
    stale: bool,
) -> Result<(), DiscoveryError> {
    let changed = conn
        .execute(
            "UPDATE discovered_device SET object_list_stale = ?1 WHERE id = ?2",
            rusqlite::params![stale as i32, id],
        )
        .map_err(|e| DiscoveryError::Db(e.to_string()))?;
    if changed == 0 {
        return Err(DiscoveryError::NotFound(id.to_string()));
    }
    Ok(())
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::ProtocolBinding;

    fn test_store(path: &str) -> DiscoveryStore {
        let db_path = PathBuf::from(path);
        if db_path.exists() {
            std::fs::remove_file(&db_path).ok();
        }
        start_discovery_store_with_path(&db_path)
    }

    fn sample_device(id: &str) -> DiscoveredDevice {
        DiscoveredDevice {
            id: id.to_string(),
            protocol: PROTOCOL_BACNET.into(),
            state: DeviceState::Discovered,
            conn_status: ConnStatus::Online,
            display_name: format!("BACnet Device {id}"),
            vendor: None,
            model: None,
            address: "192.168.1.100:47808".into(),
            point_count: 2,
            discovered_at_ms: 1000,
            accepted_at_ms: None,
            protocol_meta: serde_json::json!({}),
            network_id: String::new(),
        }
    }

    fn sample_points(device_id: &str) -> Vec<DiscoveredPoint> {
        vec![
            DiscoveredPoint {
                id: "dat".into(),
                device_id: device_id.to_string(),
                display_name: "Discharge Air Temp".into(),
                description: Some("DAT sensor".into()),
                units: Some("°F".into()),
                point_kind: PointKindHint::Analog,
                writable: false,
                binding: ProtocolBinding::bacnet(1000, "AnalogInput", 1),
                protocol_meta: serde_json::json!({}),
                state_labels: None,
            },
            DiscoveredPoint {
                id: "fan-cmd".into(),
                device_id: device_id.to_string(),
                display_name: "Fan Run Command".into(),
                description: None,
                units: None,
                point_kind: PointKindHint::Binary,
                writable: true,
                binding: ProtocolBinding::bacnet(1000, "BinaryOutput", 2),
                protocol_meta: serde_json::json!({}),
                state_labels: None,
            },
        ]
    }

    #[tokio::test]
    async fn upsert_and_get_device() {
        let store = test_store("/tmp/test_discovery_upsert.db");
        let dev = sample_device("bacnet-1000");

        store.upsert_device(dev).await.unwrap();

        let fetched = store.get_device("bacnet-1000").await.unwrap();
        assert_eq!(fetched.id, "bacnet-1000");
        assert_eq!(fetched.protocol, PROTOCOL_BACNET);
        assert_eq!(fetched.state, DeviceState::Discovered);
        assert_eq!(fetched.conn_status, ConnStatus::Online);

        std::fs::remove_file("/tmp/test_discovery_upsert.db").ok();
    }

    #[tokio::test]
    async fn upsert_preserves_state_on_rediscovery() {
        let store = test_store("/tmp/test_discovery_preserve.db");
        let dev = sample_device("bacnet-1000");

        store.upsert_device(dev).await.unwrap();
        store
            .set_device_state("bacnet-1000", DeviceState::Accepted)
            .await
            .unwrap();

        // Re-discover same device
        let dev2 = sample_device("bacnet-1000");
        store.upsert_device(dev2).await.unwrap();

        // State should still be accepted (upsert preserves state)
        let fetched = store.get_device("bacnet-1000").await.unwrap();
        assert_eq!(fetched.state, DeviceState::Accepted);

        std::fs::remove_file("/tmp/test_discovery_preserve.db").ok();
    }

    #[tokio::test]
    async fn list_devices_with_filter() {
        let store = test_store("/tmp/test_discovery_filter.db");

        store
            .upsert_device(sample_device("bacnet-1000"))
            .await
            .unwrap();
        store
            .upsert_device(sample_device("bacnet-2000"))
            .await
            .unwrap();
        store
            .set_device_state("bacnet-1000", DeviceState::Accepted)
            .await
            .unwrap();

        let all = store.list_devices(None).await;
        assert_eq!(all.len(), 2);

        let accepted = store.list_devices(Some(DeviceState::Accepted)).await;
        assert_eq!(accepted.len(), 1);
        assert_eq!(accepted[0].id, "bacnet-1000");

        let discovered = store.list_devices(Some(DeviceState::Discovered)).await;
        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0].id, "bacnet-2000");

        std::fs::remove_file("/tmp/test_discovery_filter.db").ok();
    }

    #[tokio::test]
    async fn points_crud() {
        let store = test_store("/tmp/test_discovery_points.db");

        store
            .upsert_device(sample_device("bacnet-1000"))
            .await
            .unwrap();
        store
            .upsert_points("bacnet-1000", sample_points("bacnet-1000"))
            .await
            .unwrap();

        let points = store.get_points("bacnet-1000").await;
        assert_eq!(points.len(), 2);

        // Verify first point
        let dat = points.iter().find(|p| p.id == "dat").unwrap();
        assert_eq!(dat.display_name, "Discharge Air Temp");
        assert_eq!(dat.units.as_deref(), Some("°F"));
        assert_eq!(dat.point_kind, PointKindHint::Analog);
        assert!(!dat.writable);

        // Verify second point
        let fan = points.iter().find(|p| p.id == "fan-cmd").unwrap();
        assert_eq!(fan.point_kind, PointKindHint::Binary);
        assert!(fan.writable);

        std::fs::remove_file("/tmp/test_discovery_points.db").ok();
    }

    #[tokio::test]
    async fn state_transitions() {
        let store = test_store("/tmp/test_discovery_states.db");
        store
            .upsert_device(sample_device("bacnet-1000"))
            .await
            .unwrap();

        // Discovered → Ignored
        store
            .set_device_state("bacnet-1000", DeviceState::Ignored)
            .await
            .unwrap();
        let dev = store.get_device("bacnet-1000").await.unwrap();
        assert_eq!(dev.state, DeviceState::Ignored);

        // Ignored → Discovered (un-ignore)
        store
            .set_device_state("bacnet-1000", DeviceState::Discovered)
            .await
            .unwrap();
        let dev = store.get_device("bacnet-1000").await.unwrap();
        assert_eq!(dev.state, DeviceState::Discovered);

        // Discovered → Accepted
        store
            .set_device_state("bacnet-1000", DeviceState::Accepted)
            .await
            .unwrap();
        let dev = store.get_device("bacnet-1000").await.unwrap();
        assert_eq!(dev.state, DeviceState::Accepted);
        assert!(dev.accepted_at_ms.is_some());

        std::fs::remove_file("/tmp/test_discovery_states.db").ok();
    }

    #[tokio::test]
    async fn conn_status_update() {
        let store = test_store("/tmp/test_discovery_conn.db");
        store
            .upsert_device(sample_device("bacnet-1000"))
            .await
            .unwrap();

        store
            .set_conn_status("bacnet-1000", ConnStatus::Offline)
            .await
            .unwrap();
        let dev = store.get_device("bacnet-1000").await.unwrap();
        assert_eq!(dev.conn_status, ConnStatus::Offline);

        std::fs::remove_file("/tmp/test_discovery_conn.db").ok();
    }

    #[tokio::test]
    async fn scan_tracking() {
        let store = test_store("/tmp/test_discovery_scan.db");

        let scan_id = store.record_scan("bacnet").await;
        assert!(scan_id > 0);

        store.finish_scan(scan_id, 3).await;
        // No assertion needed — just verifying it doesn't panic

        std::fs::remove_file("/tmp/test_discovery_scan.db").ok();
    }

    #[tokio::test]
    async fn update_device_name() {
        let store = test_store("/tmp/test_discovery_rename.db");
        store
            .upsert_device(sample_device("bacnet-1000"))
            .await
            .unwrap();

        store
            .update_device_name("bacnet-1000", "My AHU")
            .await
            .unwrap();

        let fetched = store.get_device("bacnet-1000").await.unwrap();
        assert_eq!(fetched.display_name, "My AHU");

        std::fs::remove_file("/tmp/test_discovery_rename.db").ok();
    }

    #[tokio::test]
    async fn update_point_properties() {
        let store = test_store("/tmp/test_discovery_point_edit.db");
        store
            .upsert_device(sample_device("bacnet-1000"))
            .await
            .unwrap();
        store
            .upsert_points("bacnet-1000", sample_points("bacnet-1000"))
            .await
            .unwrap();

        store
            .update_point(
                "bacnet-1000",
                "dat",
                Some("Supply Temp"),
                Some("°C"),
                None,
                None,
            )
            .await
            .unwrap();

        let pts = store.get_points("bacnet-1000").await;
        let dat = pts.iter().find(|p| p.id == "dat").unwrap();
        assert_eq!(dat.display_name, "Supply Temp");
        assert_eq!(dat.units.as_deref(), Some("°C"));
        // Description unchanged
        assert_eq!(dat.description.as_deref(), Some("DAT sensor"));

        std::fs::remove_file("/tmp/test_discovery_point_edit.db").ok();
    }

    #[tokio::test]
    async fn bulk_update_points_units() {
        let store = test_store("/tmp/test_discovery_bulk.db");
        store
            .upsert_device(sample_device("bacnet-1000"))
            .await
            .unwrap();
        store
            .upsert_points("bacnet-1000", sample_points("bacnet-1000"))
            .await
            .unwrap();

        let ids = vec!["dat".to_string(), "fan-cmd".to_string()];
        let count = store
            .bulk_update_points("bacnet-1000", &ids, Some("kPa"), None)
            .await
            .unwrap();
        assert_eq!(count, 2);

        let pts = store.get_points("bacnet-1000").await;
        assert!(pts.iter().all(|p| p.units.as_deref() == Some("kPa")));

        std::fs::remove_file("/tmp/test_discovery_bulk.db").ok();
    }
}
