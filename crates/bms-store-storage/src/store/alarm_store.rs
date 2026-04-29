use std::collections::HashMap;
use std::path::Path;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use tokio::sync::{mpsc, oneshot, watch};

use crate::config::loader::LoadedScenario;
use crate::event::bus::{Event, EventBus};
use crate::store::point_store::{PointKey, PointStore};

// ----------------------------------------------------------------
// Public types — re-exported from the bms-core crate
// ----------------------------------------------------------------

pub use bms_core::{AlarmConfig, AlarmConfigId, AlarmParams, AlarmSeverity, AlarmState, AlarmType};

#[derive(Debug, Clone, PartialEq)]
pub struct ActiveAlarm {
    pub config_id: AlarmConfigId,
    pub device_id: String,
    pub point_id: String,
    pub alarm_type: AlarmType,
    pub severity: AlarmSeverity,
    pub state: AlarmState,
    pub trigger_value: f64,
    pub trigger_time_ms: i64,
    pub ack_time_ms: Option<i64>,
    pub context_snapshot: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlarmEvent {
    pub id: i64,
    pub config_id: AlarmConfigId,
    pub device_id: String,
    pub point_id: String,
    pub severity: AlarmSeverity,
    pub from_state: String,
    pub to_state: String,
    pub value: f64,
    pub timestamp_ms: i64,
    pub context_snapshot: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AlarmHistoryQuery {
    pub device_id: Option<String>,
    pub point_id: Option<String>,
    pub severity: Option<AlarmSeverity>,
    pub from_state: Option<String>,
    pub to_state: Option<String>,
    pub search: Option<String>,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Debug, thiserror::Error)]
pub enum AlarmError {
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

enum AlarmCmd {
    CreateConfig {
        device_id: String,
        point_id: String,
        severity: AlarmSeverity,
        params: AlarmParams,
        reply: oneshot::Sender<Result<AlarmConfigId, AlarmError>>,
    },
    UpdateConfig {
        id: AlarmConfigId,
        severity: AlarmSeverity,
        enabled: bool,
        params: AlarmParams,
        reply: oneshot::Sender<Result<(), AlarmError>>,
    },
    DeleteConfig {
        id: AlarmConfigId,
        reply: oneshot::Sender<Result<(), AlarmError>>,
    },
    ListConfigs {
        reply: oneshot::Sender<Vec<AlarmConfig>>,
    },
    GetConfigsForPoint {
        device_id: String,
        point_id: String,
        reply: oneshot::Sender<Vec<AlarmConfig>>,
    },
    GetActiveAlarms {
        reply: oneshot::Sender<Vec<ActiveAlarm>>,
    },
    Acknowledge {
        config_id: AlarmConfigId,
        reply: oneshot::Sender<Result<(), AlarmError>>,
    },
    AcknowledgeAll {
        reply: oneshot::Sender<Result<u32, AlarmError>>,
    },
    QueryHistory {
        query: AlarmHistoryQuery,
        reply: oneshot::Sender<Result<Vec<AlarmEvent>, AlarmError>>,
    },
    CountHistory {
        query: AlarmHistoryQuery,
        reply: oneshot::Sender<Result<i64, AlarmError>>,
    },
    CreateConfigBatch {
        entries: Vec<(String, String)>,
        severity: AlarmSeverity,
        params: AlarmParams,
        reply: oneshot::Sender<Result<Vec<AlarmConfigId>, AlarmError>>,
    },
    // Internal — used by the engine
    UpsertActive {
        alarm: ActiveAlarm,
    },
    RemoveActive {
        config_id: AlarmConfigId,
    },
    InsertEvent {
        config_id: AlarmConfigId,
        device_id: String,
        point_id: String,
        severity: AlarmSeverity,
        from_state: String,
        to_state: String,
        value: f64,
        timestamp_ms: i64,
        context_json: Option<String>,
    },
}

// ----------------------------------------------------------------
// AlarmStore — async handle to the SQLite thread
// ----------------------------------------------------------------

#[derive(Clone)]
pub struct AlarmStore {
    cmd_tx: mpsc::UnboundedSender<AlarmCmd>,
    config_version_tx: watch::Sender<u64>,
    config_version_rx: watch::Receiver<u64>,
    event_bus: Option<EventBus>,
}

impl AlarmStore {
    pub fn with_event_bus(mut self, bus: EventBus) -> Self {
        self.event_bus = Some(bus);
        self
    }

    pub async fn create_config(
        &self,
        device_id: &str,
        point_id: &str,
        severity: AlarmSeverity,
        params: AlarmParams,
    ) -> Result<AlarmConfigId, AlarmError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(AlarmCmd::CreateConfig {
                device_id: device_id.to_string(),
                point_id: point_id.to_string(),
                severity,
                params,
                reply: reply_tx,
            })
            .map_err(|_| AlarmError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| AlarmError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_config_version();
        }
        result
    }

    pub async fn create_configs_batch(
        &self,
        entries: &[(String, String)],
        severity: AlarmSeverity,
        params: AlarmParams,
    ) -> Result<Vec<AlarmConfigId>, AlarmError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(AlarmCmd::CreateConfigBatch {
                entries: entries.to_vec(),
                severity,
                params,
                reply: reply_tx,
            })
            .map_err(|_| AlarmError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| AlarmError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_config_version();
        }
        result
    }

    pub async fn update_config(
        &self,
        id: AlarmConfigId,
        severity: AlarmSeverity,
        enabled: bool,
        params: AlarmParams,
    ) -> Result<(), AlarmError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(AlarmCmd::UpdateConfig {
                id,
                severity,
                enabled,
                params,
                reply: reply_tx,
            })
            .map_err(|_| AlarmError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| AlarmError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_config_version();
        }
        result
    }

    pub async fn delete_config(&self, id: AlarmConfigId) -> Result<(), AlarmError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(AlarmCmd::DeleteConfig {
                id,
                reply: reply_tx,
            })
            .map_err(|_| AlarmError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| AlarmError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_config_version();
        }
        result
    }

    pub async fn list_configs(&self) -> Vec<AlarmConfig> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(AlarmCmd::ListConfigs { reply: reply_tx });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn get_configs_for_point(&self, device_id: &str, point_id: &str) -> Vec<AlarmConfig> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(AlarmCmd::GetConfigsForPoint {
            device_id: device_id.to_string(),
            point_id: point_id.to_string(),
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn get_active_alarms(&self) -> Vec<ActiveAlarm> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .cmd_tx
            .send(AlarmCmd::GetActiveAlarms { reply: reply_tx });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn acknowledge(&self, config_id: AlarmConfigId) -> Result<(), AlarmError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(AlarmCmd::Acknowledge {
                config_id,
                reply: reply_tx,
            })
            .map_err(|_| AlarmError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| AlarmError::ChannelClosed)?;
        if result.is_ok() {
            if let Some(ref bus) = self.event_bus {
                bus.publish(Event::AlarmAcknowledged {
                    alarm_id: config_id,
                });
            }
        }
        result
    }

    pub async fn acknowledge_all(&self) -> Result<u32, AlarmError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(AlarmCmd::AcknowledgeAll { reply: reply_tx })
            .map_err(|_| AlarmError::ChannelClosed)?;
        reply_rx.await.map_err(|_| AlarmError::ChannelClosed)?
    }

    pub async fn query_history(
        &self,
        query: AlarmHistoryQuery,
    ) -> Result<Vec<AlarmEvent>, AlarmError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(AlarmCmd::QueryHistory {
                query,
                reply: reply_tx,
            })
            .map_err(|_| AlarmError::ChannelClosed)?;
        reply_rx.await.map_err(|_| AlarmError::ChannelClosed)?
    }

    pub async fn count_history(&self, query: AlarmHistoryQuery) -> Result<i64, AlarmError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(AlarmCmd::CountHistory {
                query,
                reply: reply_tx,
            })
            .map_err(|_| AlarmError::ChannelClosed)?;
        reply_rx.await.map_err(|_| AlarmError::ChannelClosed)?
    }

    pub fn subscribe_config_changes(&self) -> watch::Receiver<u64> {
        self.config_version_rx.clone()
    }

    fn bump_config_version(&self) {
        let current = *self.config_version_rx.borrow();
        let _ = self.config_version_tx.send(current + 1);
    }

    // Internal methods for the engine
    fn upsert_active(&self, alarm: ActiveAlarm) {
        if let Some(ref bus) = self.event_bus {
            bus.publish(Event::AlarmRaised {
                alarm_id: alarm.config_id,
                node_id: format!("{}/{}", alarm.device_id, alarm.point_id),
            });
        }
        let _ = self.cmd_tx.send(AlarmCmd::UpsertActive { alarm });
    }

    fn remove_active(&self, config_id: AlarmConfigId, device_id: &str, point_id: &str) {
        if let Some(ref bus) = self.event_bus {
            bus.publish(Event::AlarmCleared {
                alarm_id: config_id,
                node_id: format!("{}/{}", device_id, point_id),
            });
        }
        let _ = self.cmd_tx.send(AlarmCmd::RemoveActive { config_id });
    }

    fn insert_event(
        &self,
        config_id: AlarmConfigId,
        device_id: &str,
        point_id: &str,
        severity: AlarmSeverity,
        from_state: &str,
        to_state: &str,
        value: f64,
        timestamp_ms: i64,
        context_json: Option<String>,
    ) {
        let _ = self.cmd_tx.send(AlarmCmd::InsertEvent {
            config_id,
            device_id: device_id.to_string(),
            point_id: point_id.to_string(),
            severity,
            from_state: from_state.to_string(),
            to_state: to_state.to_string(),
            value,
            timestamp_ms,
            context_json,
        });
    }
}

// ----------------------------------------------------------------
// SQLite schema & thread
// ----------------------------------------------------------------

use super::migration::{run_migrations, Migration};

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        label: "initial alarm schema",
        sql: "
CREATE TABLE IF NOT EXISTS alarm_config (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    device_id   TEXT NOT NULL,
    point_id    TEXT NOT NULL,
    alarm_type  TEXT NOT NULL,
    severity    TEXT NOT NULL,
    enabled     INTEGER NOT NULL DEFAULT 1,
    params_json TEXT NOT NULL,
    created_ms  INTEGER NOT NULL,
    updated_ms  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_alarm_config_point ON alarm_config(device_id, point_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_alarm_config_unique ON alarm_config(device_id, point_id, alarm_type);

CREATE TABLE IF NOT EXISTS alarm_active (
    config_id       INTEGER PRIMARY KEY REFERENCES alarm_config(id),
    device_id       TEXT NOT NULL,
    point_id        TEXT NOT NULL,
    alarm_type      TEXT NOT NULL,
    severity        TEXT NOT NULL,
    state           TEXT NOT NULL,
    trigger_value   REAL NOT NULL,
    trigger_time_ms INTEGER NOT NULL,
    ack_time_ms     INTEGER,
    context_json    TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS alarm_history (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    config_id    INTEGER NOT NULL,
    device_id    TEXT NOT NULL,
    point_id     TEXT NOT NULL,
    severity     TEXT NOT NULL,
    from_state   TEXT NOT NULL,
    to_state     TEXT NOT NULL,
    value        REAL NOT NULL,
    timestamp_ms INTEGER NOT NULL,
    context_json TEXT,
    note         TEXT
);
CREATE INDEX IF NOT EXISTS idx_alarm_history_time ON alarm_history(timestamp_ms);
CREATE INDEX IF NOT EXISTS idx_alarm_history_point ON alarm_history(device_id, point_id, timestamp_ms);
",
    },
];

fn run_sqlite_thread(db_path: &Path, mut cmd_rx: mpsc::UnboundedReceiver<AlarmCmd>) {
    let conn = rusqlite::Connection::open(db_path).expect("failed to open alarms database");
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
        .expect("failed to set pragmas");
    run_migrations(&conn, "alarms", MIGRATIONS).expect("alarms: schema migration failed");

    while let Some(cmd) = cmd_rx.blocking_recv() {
        match cmd {
            AlarmCmd::CreateConfig {
                device_id,
                point_id,
                severity,
                params,
                reply,
            } => {
                let result = create_config_db(&conn, &device_id, &point_id, severity, &params);
                let _ = reply.send(result);
            }
            AlarmCmd::CreateConfigBatch {
                entries,
                severity,
                params,
                reply,
            } => {
                let result = create_configs_batch_db(&conn, &entries, severity, &params);
                let _ = reply.send(result);
            }
            AlarmCmd::UpdateConfig {
                id,
                severity,
                enabled,
                params,
                reply,
            } => {
                let result = update_config_db(&conn, id, severity, enabled, &params);
                let _ = reply.send(result);
            }
            AlarmCmd::DeleteConfig { id, reply } => {
                let result = delete_config_db(&conn, id);
                let _ = reply.send(result);
            }
            AlarmCmd::ListConfigs { reply } => {
                let _ = reply.send(list_configs_db(&conn));
            }
            AlarmCmd::GetConfigsForPoint {
                device_id,
                point_id,
                reply,
            } => {
                let _ = reply.send(get_configs_for_point_db(&conn, &device_id, &point_id));
            }
            AlarmCmd::GetActiveAlarms { reply } => {
                let _ = reply.send(get_active_alarms_db(&conn));
            }
            AlarmCmd::Acknowledge { config_id, reply } => {
                let result = acknowledge_db(&conn, config_id);
                let _ = reply.send(result);
            }
            AlarmCmd::AcknowledgeAll { reply } => {
                let result = acknowledge_all_db(&conn);
                let _ = reply.send(result);
            }
            AlarmCmd::QueryHistory { query, reply } => {
                let result = query_history_db(&conn, &query);
                let _ = reply.send(result);
            }
            AlarmCmd::CountHistory { query, reply } => {
                let result = count_history_db(&conn, &query);
                let _ = reply.send(result);
            }
            AlarmCmd::UpsertActive { alarm } => {
                upsert_active_db(&conn, &alarm);
            }
            AlarmCmd::RemoveActive { config_id } => {
                let _ = conn.execute(
                    "DELETE FROM alarm_active WHERE config_id = ?1",
                    rusqlite::params![config_id],
                );
            }
            AlarmCmd::InsertEvent {
                config_id,
                device_id,
                point_id,
                severity,
                from_state,
                to_state,
                value,
                timestamp_ms,
                context_json,
            } => {
                let _ = conn.execute(
                    "INSERT INTO alarm_history (config_id, device_id, point_id, severity, from_state, to_state, value, timestamp_ms, context_json)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    rusqlite::params![
                        config_id,
                        device_id,
                        point_id,
                        severity.as_str(),
                        from_state,
                        to_state,
                        value,
                        timestamp_ms,
                        context_json,
                    ],
                );
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

fn create_config_db(
    conn: &rusqlite::Connection,
    device_id: &str,
    point_id: &str,
    severity: AlarmSeverity,
    params: &AlarmParams,
) -> Result<AlarmConfigId, AlarmError> {
    let ts = now_ms();
    let alarm_type_str = params.alarm_type().as_str();
    let params_json = serde_json::to_string(params).map_err(|e| AlarmError::Db(e.to_string()))?;

    // Check for duplicate (same device + point + alarm type)
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM alarm_config WHERE device_id = ?1 AND point_id = ?2 AND alarm_type = ?3)",
            rusqlite::params![device_id, point_id, alarm_type_str],
            |row| row.get(0),
        )
        .map_err(|e| AlarmError::Db(e.to_string()))?;
    if exists {
        return Err(AlarmError::Db(format!(
            "Alarm already exists for {device_id}/{point_id} ({alarm_type_str})"
        )));
    }

    conn.execute(
        "INSERT INTO alarm_config (device_id, point_id, alarm_type, severity, enabled, params_json, created_ms, updated_ms)
         VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?7)",
        rusqlite::params![
            device_id,
            point_id,
            alarm_type_str,
            severity.as_str(),
            params_json,
            ts,
            ts,
        ],
    )
    .map_err(|e| AlarmError::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

fn create_configs_batch_db(
    conn: &rusqlite::Connection,
    entries: &[(String, String)],
    severity: AlarmSeverity,
    params: &AlarmParams,
) -> Result<Vec<AlarmConfigId>, AlarmError> {
    let ts = now_ms();
    let params_json = serde_json::to_string(params).map_err(|e| AlarmError::Db(e.to_string()))?;
    let alarm_type_str = params.alarm_type().as_str();
    let severity_str = severity.as_str();

    let tx = conn
        .unchecked_transaction()
        .map_err(|e| AlarmError::Db(e.to_string()))?;
    let mut ids = Vec::with_capacity(entries.len());
    {
        let mut insert_stmt = tx
            .prepare_cached(
                "INSERT INTO alarm_config (device_id, point_id, alarm_type, severity, enabled, params_json, created_ms, updated_ms)
                 VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?7)",
            )
            .map_err(|e| AlarmError::Db(e.to_string()))?;
        let mut check_stmt = tx
            .prepare_cached(
                "SELECT EXISTS(SELECT 1 FROM alarm_config WHERE device_id = ?1 AND point_id = ?2 AND alarm_type = ?3)",
            )
            .map_err(|e| AlarmError::Db(e.to_string()))?;

        for (device_id, point_id) in entries {
            let exists: bool = check_stmt
                .query_row(
                    rusqlite::params![device_id, point_id, alarm_type_str],
                    |row| row.get(0),
                )
                .map_err(|e| AlarmError::Db(e.to_string()))?;
            if exists {
                continue; // Skip duplicates silently in batch mode
            }
            insert_stmt
                .execute(rusqlite::params![
                    device_id,
                    point_id,
                    alarm_type_str,
                    severity_str,
                    params_json,
                    ts,
                    ts,
                ])
                .map_err(|e| AlarmError::Db(e.to_string()))?;
            ids.push(tx.last_insert_rowid());
        }
    }
    tx.commit().map_err(|e| AlarmError::Db(e.to_string()))?;
    Ok(ids)
}

fn update_config_db(
    conn: &rusqlite::Connection,
    id: AlarmConfigId,
    severity: AlarmSeverity,
    enabled: bool,
    params: &AlarmParams,
) -> Result<(), AlarmError> {
    let ts = now_ms();
    let params_json = serde_json::to_string(params).map_err(|e| AlarmError::Db(e.to_string()))?;
    let rows = conn
        .execute(
            "UPDATE alarm_config SET severity = ?1, enabled = ?2, alarm_type = ?3, params_json = ?4, updated_ms = ?5 WHERE id = ?6",
            rusqlite::params![
                severity.as_str(),
                enabled as i32,
                params.alarm_type().as_str(),
                params_json,
                ts,
                id,
            ],
        )
        .map_err(|e| AlarmError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(AlarmError::NotFound);
    }
    Ok(())
}

fn delete_config_db(conn: &rusqlite::Connection, id: AlarmConfigId) -> Result<(), AlarmError> {
    let _ = conn.execute(
        "DELETE FROM alarm_active WHERE config_id = ?1",
        rusqlite::params![id],
    );
    let rows = conn
        .execute(
            "DELETE FROM alarm_config WHERE id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| AlarmError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(AlarmError::NotFound);
    }
    Ok(())
}

fn parse_config_row(row: &rusqlite::Row) -> rusqlite::Result<AlarmConfig> {
    let id: i64 = row.get(0)?;
    let device_id: String = row.get(1)?;
    let point_id: String = row.get(2)?;
    let alarm_type_str: String = row.get(3)?;
    let severity_str: String = row.get(4)?;
    let enabled: bool = row.get::<_, i32>(5)? != 0;
    let params_json: String = row.get(6)?;

    let alarm_type = AlarmType::from_str(&alarm_type_str).unwrap_or(AlarmType::HighLimit);
    let severity = AlarmSeverity::from_str(&severity_str).unwrap_or(AlarmSeverity::Warning);
    let params: AlarmParams =
        serde_json::from_str(&params_json).unwrap_or(AlarmParams::HighLimit {
            limit: 0.0,
            deadband: 0.0,
            delay_secs: 0,
        });

    Ok(AlarmConfig::new(
        id, device_id, point_id, alarm_type, severity, enabled, params,
    ))
}

fn list_configs_db(conn: &rusqlite::Connection) -> Vec<AlarmConfig> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT id, device_id, point_id, alarm_type, severity, enabled, params_json FROM alarm_config ORDER BY id",
        )
        .unwrap();
    let rows = stmt.query_map([], parse_config_row).unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

fn get_configs_for_point_db(
    conn: &rusqlite::Connection,
    device_id: &str,
    point_id: &str,
) -> Vec<AlarmConfig> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT id, device_id, point_id, alarm_type, severity, enabled, params_json FROM alarm_config WHERE device_id = ?1 AND point_id = ?2 ORDER BY id",
        )
        .unwrap();
    let rows = stmt
        .query_map(rusqlite::params![device_id, point_id], parse_config_row)
        .unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

fn get_active_alarms_db(conn: &rusqlite::Connection) -> Vec<ActiveAlarm> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT config_id, device_id, point_id, alarm_type, severity, state, trigger_value, trigger_time_ms, ack_time_ms, context_json FROM alarm_active ORDER BY severity DESC, trigger_time_ms DESC",
        )
        .unwrap();
    let rows = stmt
        .query_map([], |row| {
            let alarm_type_str: String = row.get(3)?;
            let severity_str: String = row.get(4)?;
            let state_str: String = row.get(5)?;
            Ok(ActiveAlarm {
                config_id: row.get(0)?,
                device_id: row.get(1)?,
                point_id: row.get(2)?,
                alarm_type: AlarmType::from_str(&alarm_type_str).unwrap_or(AlarmType::HighLimit),
                severity: AlarmSeverity::from_str(&severity_str).unwrap_or(AlarmSeverity::Warning),
                state: AlarmState::from_str(&state_str).unwrap_or(AlarmState::Offnormal),
                trigger_value: row.get(6)?,
                trigger_time_ms: row.get(7)?,
                ack_time_ms: row.get(8)?,
                context_snapshot: row.get(9)?,
            })
        })
        .unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

fn acknowledge_db(conn: &rusqlite::Connection, config_id: AlarmConfigId) -> Result<(), AlarmError> {
    let ts = now_ms();
    let rows = conn
        .execute(
            "UPDATE alarm_active SET state = 'acknowledged', ack_time_ms = ?1 WHERE config_id = ?2 AND state = 'offnormal'",
            rusqlite::params![ts, config_id],
        )
        .map_err(|e| AlarmError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(AlarmError::NotFound);
    }
    // Log the transition
    if let Ok(alarm) = conn.query_row(
        "SELECT device_id, point_id, severity, trigger_value FROM alarm_active WHERE config_id = ?1",
        rusqlite::params![config_id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, f64>(3)?,
            ))
        },
    ) {
        let _ = conn.execute(
            "INSERT INTO alarm_history (config_id, device_id, point_id, severity, from_state, to_state, value, timestamp_ms)
             VALUES (?1, ?2, ?3, ?4, 'offnormal', 'acknowledged', ?5, ?6)",
            rusqlite::params![config_id, alarm.0, alarm.1, alarm.2, alarm.3, ts],
        );
    }
    Ok(())
}

fn acknowledge_all_db(conn: &rusqlite::Connection) -> Result<u32, AlarmError> {
    let ts = now_ms();
    // Get all offnormal alarms first for event logging
    let mut stmt = conn
        .prepare_cached(
            "SELECT config_id, device_id, point_id, severity, trigger_value FROM alarm_active WHERE state = 'offnormal'",
        )
        .unwrap();
    let alarms: Vec<(i64, String, String, String, f64)> = stmt
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    let count = alarms.len() as u32;

    if let Ok(tx) = conn.unchecked_transaction() {
        for (config_id, device_id, point_id, severity, value) in &alarms {
            let _ = tx.execute(
                "INSERT INTO alarm_history (config_id, device_id, point_id, severity, from_state, to_state, value, timestamp_ms)
                 VALUES (?1, ?2, ?3, ?4, 'offnormal', 'acknowledged', ?5, ?6)",
                rusqlite::params![config_id, device_id, point_id, severity, value, ts],
            );
        }
        let _ = tx.execute(
            "UPDATE alarm_active SET state = 'acknowledged', ack_time_ms = ?1 WHERE state = 'offnormal'",
            rusqlite::params![ts],
        );
        let _ = tx.commit();
    }

    Ok(count)
}

/// Build WHERE clauses for alarm history queries. Returns (where_sql, params).
fn build_history_where(
    query: &AlarmHistoryQuery,
) -> (String, Vec<Box<dyn rusqlite::types::ToSql>>) {
    let mut sql = String::from(" WHERE 1=1");
    let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1;

    if let Some(ref device_id) = query.device_id {
        sql.push_str(&format!(" AND device_id = ?{idx}"));
        params_vec.push(Box::new(device_id.clone()));
        idx += 1;
    }
    if let Some(ref point_id) = query.point_id {
        sql.push_str(&format!(" AND point_id = ?{idx}"));
        params_vec.push(Box::new(point_id.clone()));
        idx += 1;
    }
    if let Some(ref sev) = query.severity {
        sql.push_str(&format!(" AND severity = ?{idx}"));
        params_vec.push(Box::new(sev.as_str().to_string()));
        idx += 1;
    }
    if let Some(ref from) = query.from_state {
        sql.push_str(&format!(" AND from_state = ?{idx}"));
        params_vec.push(Box::new(from.clone()));
        idx += 1;
    }
    if let Some(ref to) = query.to_state {
        sql.push_str(&format!(" AND to_state = ?{idx}"));
        params_vec.push(Box::new(to.clone()));
        idx += 1;
    }
    if let Some(ref search) = query.search {
        let pattern = format!("%{search}%");
        sql.push_str(&format!(
            " AND (device_id LIKE ?{idx} OR point_id LIKE ?{idx})"
        ));
        params_vec.push(Box::new(pattern));
        idx += 1;
    }
    if let Some(start) = query.start_ms {
        sql.push_str(&format!(" AND timestamp_ms >= ?{idx}"));
        params_vec.push(Box::new(start));
        idx += 1;
    }
    if let Some(end) = query.end_ms {
        sql.push_str(&format!(" AND timestamp_ms <= ?{idx}"));
        params_vec.push(Box::new(end));
        idx += 1;
    }

    let _ = idx; // suppress unused warning
    (sql, params_vec)
}

fn query_history_db(
    conn: &rusqlite::Connection,
    query: &AlarmHistoryQuery,
) -> Result<Vec<AlarmEvent>, AlarmError> {
    let (where_sql, mut params_vec) = build_history_where(query);
    let mut sql = format!(
        "SELECT id, config_id, device_id, point_id, severity, from_state, to_state, value, timestamp_ms, context_json, note FROM alarm_history{where_sql} ORDER BY timestamp_ms DESC"
    );

    // None = uncapped (for exports); Some(n) = user-specified limit
    if let Some(limit) = query.limit {
        let idx = params_vec.len() + 1;
        sql.push_str(&format!(" LIMIT ?{idx}"));
        params_vec.push(Box::new(limit));
    }

    let params_refs: Vec<&dyn rusqlite::types::ToSql> =
        params_vec.iter().map(|p| p.as_ref()).collect();

    let mut stmt = conn
        .prepare_cached(&sql)
        .map_err(|e| AlarmError::Db(e.to_string()))?;
    let rows = stmt
        .query_map(params_refs.as_slice(), |row| {
            let severity_str: String = row.get(4)?;
            Ok(AlarmEvent {
                id: row.get(0)?,
                config_id: row.get(1)?,
                device_id: row.get(2)?,
                point_id: row.get(3)?,
                severity: AlarmSeverity::from_str(&severity_str).unwrap_or(AlarmSeverity::Warning),
                from_state: row.get(5)?,
                to_state: row.get(6)?,
                value: row.get(7)?,
                timestamp_ms: row.get(8)?,
                context_snapshot: row.get(9)?,
                note: row.get(10)?,
            })
        })
        .map_err(|e| AlarmError::Db(e.to_string()))?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

fn count_history_db(
    conn: &rusqlite::Connection,
    query: &AlarmHistoryQuery,
) -> Result<i64, AlarmError> {
    let (where_sql, params_vec) = build_history_where(query);
    let sql = format!("SELECT COUNT(*) FROM alarm_history{where_sql}");
    let params_refs: Vec<&dyn rusqlite::types::ToSql> =
        params_vec.iter().map(|p| p.as_ref()).collect();

    conn.query_row(&sql, params_refs.as_slice(), |row| row.get(0))
        .map_err(|e| AlarmError::Db(e.to_string()))
}

fn upsert_active_db(conn: &rusqlite::Connection, alarm: &ActiveAlarm) {
    let _ = conn.execute(
        "INSERT OR REPLACE INTO alarm_active (config_id, device_id, point_id, alarm_type, severity, state, trigger_value, trigger_time_ms, ack_time_ms, context_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![
            alarm.config_id,
            alarm.device_id,
            alarm.point_id,
            alarm.alarm_type.as_str(),
            alarm.severity.as_str(),
            alarm.state.as_str(),
            alarm.trigger_value,
            alarm.trigger_time_ms,
            alarm.ack_time_ms,
            alarm.context_snapshot,
        ],
    );
}

// ----------------------------------------------------------------
// Alarm engine
// ----------------------------------------------------------------

/// Runtime state per alarm config for the evaluation engine.
struct AlarmRuntime {
    state: AlarmState,
    /// When the condition first became true (for delay tracking).
    condition_since: Option<Instant>,
}

fn capture_context(store: &PointStore, device_id: &str) -> String {
    let points = store.get_all_for_device(device_id);
    let map: HashMap<String, f64> = points
        .into_iter()
        .map(|(k, v)| (k.point_id, v.value.as_f64()))
        .collect();
    serde_json::to_string(&map).unwrap_or_else(|_| "{}".to_string())
}

fn evaluate_condition(value: f64, params: &AlarmParams, store: &PointStore) -> bool {
    match params {
        AlarmParams::HighLimit { limit, .. } => value >= *limit,
        AlarmParams::LowLimit { limit, .. } => value <= *limit,
        AlarmParams::StateFault { fault_value, .. } => (value - fault_value).abs() < f64::EPSILON,
        AlarmParams::Stale { .. } => false, // Handled by stale checker
        AlarmParams::Deviation {
            ref_device_id,
            ref_point_id,
            threshold,
            ..
        } => {
            let ref_key = PointKey {
                device_instance_id: ref_device_id.clone(),
                point_id: ref_point_id.clone(),
            };
            if let Some(ref_val) = store.get(&ref_key) {
                (value - ref_val.value.as_f64()).abs() > *threshold
            } else {
                false
            }
        }
        AlarmParams::StateChange { alarm_value, .. } => {
            // Bool(true) → 1.0, Bool(false) → 0.0
            let target = if *alarm_value { 1.0 } else { 0.0 };
            (value - target).abs() < f64::EPSILON
        }
        AlarmParams::MultiStateAlarm { alarm_states, .. } => {
            let state_val = value.round() as i64;
            alarm_states.contains(&state_val)
        }
        AlarmParams::CommandMismatch {
            feedback_device_id,
            feedback_point_id,
            ..
        } => {
            let fb_key = PointKey {
                device_instance_id: feedback_device_id.clone(),
                point_id: feedback_point_id.clone(),
            };
            if let Some(fb_val) = store.get(&fb_key) {
                (value - fb_val.value.as_f64()).abs() > f64::EPSILON
            } else {
                false // No feedback value yet — don't alarm
            }
        }
    }
}

fn check_clear_condition(value: f64, params: &AlarmParams, store: &PointStore) -> bool {
    match params {
        AlarmParams::HighLimit {
            limit, deadband, ..
        } => value < (*limit - *deadband),
        AlarmParams::LowLimit {
            limit, deadband, ..
        } => value > (*limit + *deadband),
        AlarmParams::StateFault { fault_value, .. } => (value - fault_value).abs() >= f64::EPSILON,
        AlarmParams::Stale { .. } => true, // Cleared when we get a fresh value
        AlarmParams::Deviation {
            ref_device_id,
            ref_point_id,
            threshold,
            deadband,
            ..
        } => {
            let ref_key = PointKey {
                device_instance_id: ref_device_id.clone(),
                point_id: ref_point_id.clone(),
            };
            if let Some(ref_val) = store.get(&ref_key) {
                (value - ref_val.value.as_f64()).abs() < (*threshold - *deadband)
            } else {
                true
            }
        }
        AlarmParams::StateChange { alarm_value, .. } => {
            let target = if *alarm_value { 1.0 } else { 0.0 };
            (value - target).abs() >= f64::EPSILON
        }
        AlarmParams::MultiStateAlarm { alarm_states, .. } => {
            let state_val = value.round() as i64;
            !alarm_states.contains(&state_val)
        }
        AlarmParams::CommandMismatch {
            feedback_device_id,
            feedback_point_id,
            ..
        } => {
            let fb_key = PointKey {
                device_instance_id: feedback_device_id.clone(),
                point_id: feedback_point_id.clone(),
            };
            if let Some(fb_val) = store.get(&fb_key) {
                (value - fb_val.value.as_f64()).abs() < f64::EPSILON
            } else {
                true // Feedback gone — clear alarm
            }
        }
    }
}

/// Start the alarm system. Returns an `AlarmStore` handle.
pub fn start_alarm_engine(store: &PointStore, _loaded: &LoadedScenario) -> AlarmStore {
    let db_dir = Path::new("data");
    if !db_dir.exists() {
        std::fs::create_dir_all(db_dir).expect("failed to create data directory");
    }
    start_alarm_engine_with_path(store, &db_dir.join("alarms.db"), None)
}

pub fn start_alarm_engine_with_path(
    store: &PointStore,
    db_path: &Path,
    shutdown: Option<tokio_util::sync::CancellationToken>,
) -> AlarmStore {
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (config_version_tx, config_version_rx) = watch::channel(0u64);

    // Start SQLite thread
    let path_clone = db_path.to_path_buf();
    std::thread::Builder::new()
        .name("alarm-sqlite".into())
        .spawn(move || run_sqlite_thread(&path_clone, cmd_rx))
        .expect("failed to spawn alarm SQLite thread");

    let alarm_store = AlarmStore {
        cmd_tx,
        config_version_tx,
        config_version_rx,
        event_bus: None,
    };

    // Start evaluation loop
    {
        let eval_store = store.clone();
        let eval_alarm_store = alarm_store.clone();
        let shutdown_token = shutdown;

        tokio::spawn(async move {
            // Small delay to let SQLite thread initialize
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

            // Load initial configs and active alarms
            let mut configs = eval_alarm_store.list_configs().await;
            let active_alarms = eval_alarm_store.get_active_alarms().await;

            // Build runtime state from active alarms
            let mut runtime: HashMap<AlarmConfigId, AlarmRuntime> = HashMap::new();
            for alarm in &active_alarms {
                runtime.insert(
                    alarm.config_id,
                    AlarmRuntime {
                        state: alarm.state,
                        condition_since: None,
                    },
                );
            }

            // Build lookup: (device_id, point_id) → Vec<config index>
            let mut point_lookup: HashMap<(String, String), Vec<usize>> = HashMap::new();
            for (i, cfg) in configs.iter().enumerate() {
                if cfg.enabled {
                    point_lookup
                        .entry((cfg.device_id.clone(), cfg.point_id.clone()))
                        .or_default()
                        .push(i);
                }
            }

            let mut cov_rx = eval_store.subscribe_history();
            let mut config_watch = eval_alarm_store.subscribe_config_changes();
            let mut stale_ticker = tokio::time::interval(tokio::time::Duration::from_secs(10));
            stale_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    result = cov_rx.recv() => {
                        match result {
                            Ok((key, value)) => {
                                let lookup_key = (key.device_instance_id.clone(), key.point_id.clone());
                                if let Some(indices) = point_lookup.get(&lookup_key) {
                                    let f = value.as_f64();
                                    for &idx in indices {
                                        let cfg = &configs[idx];
                                        if !cfg.enabled {
                                            continue;
                                        }
                                        // Skip stale alarms here — handled by stale_ticker
                                        if matches!(cfg.params, AlarmParams::Stale { .. }) {
                                            continue;
                                        }
                                        evaluate_alarm(
                                            cfg,
                                            f,
                                            &mut runtime,
                                            &eval_alarm_store,
                                            &eval_store,
                                        );
                                    }
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                    _ = stale_ticker.tick() => {
                        check_stale_alarms(&configs, &mut runtime, &eval_alarm_store, &eval_store);
                    }
                    Ok(_) = config_watch.changed() => {
                        // Reload configs
                        configs = eval_alarm_store.list_configs().await;
                        point_lookup.clear();
                        for (i, cfg) in configs.iter().enumerate() {
                            if cfg.enabled {
                                point_lookup
                                    .entry((cfg.device_id.clone(), cfg.point_id.clone()))
                                    .or_default()
                                    .push(i);
                            }
                        }
                    }
                    _ = async {
                        match &shutdown_token {
                            Some(t) => t.cancelled().await,
                            None => std::future::pending().await,
                        }
                    } => {
                        tracing::info!("Alarm engine shutting down");
                        break;
                    }
                }
            }
        });
    }

    alarm_store
}

fn evaluate_alarm(
    cfg: &AlarmConfig,
    value: f64,
    runtime: &mut HashMap<AlarmConfigId, AlarmRuntime>,
    alarm_store: &AlarmStore,
    point_store: &PointStore,
) {
    let rt = runtime.entry(cfg.id).or_insert(AlarmRuntime {
        state: AlarmState::Normal,
        condition_since: None,
    });

    match rt.state {
        AlarmState::Normal => {
            // Check if condition triggers
            if evaluate_condition(value, &cfg.params, point_store) {
                let delay = cfg.params.delay_secs();
                if delay == 0 {
                    // Immediate transition to offnormal
                    transition_to_offnormal(cfg, value, rt, alarm_store, point_store);
                } else {
                    // Start delay tracking
                    match rt.condition_since {
                        Some(since) => {
                            if since.elapsed().as_secs() >= delay {
                                transition_to_offnormal(cfg, value, rt, alarm_store, point_store);
                            }
                        }
                        None => {
                            rt.condition_since = Some(Instant::now());
                        }
                    }
                }
            } else {
                // Condition cleared — reset delay tracking
                rt.condition_since = None;
            }
        }
        AlarmState::Offnormal | AlarmState::Acknowledged => {
            // Check if condition clears
            if check_clear_condition(value, &cfg.params, point_store) {
                let ts = now_ms();
                let from = rt.state.as_str();
                rt.state = AlarmState::Normal;
                rt.condition_since = None;

                alarm_store.remove_active(cfg.id, &cfg.device_id, &cfg.point_id);
                alarm_store.insert_event(
                    cfg.id,
                    &cfg.device_id,
                    &cfg.point_id,
                    cfg.severity,
                    from,
                    "normal",
                    value,
                    ts,
                    None,
                );
            }
        }
    }
}

fn transition_to_offnormal(
    cfg: &AlarmConfig,
    value: f64,
    rt: &mut AlarmRuntime,
    alarm_store: &AlarmStore,
    point_store: &PointStore,
) {
    let ts = now_ms();
    let context = capture_context(point_store, &cfg.device_id);
    let from = rt.state.as_str();

    rt.state = AlarmState::Offnormal;
    rt.condition_since = None;

    let active = ActiveAlarm {
        config_id: cfg.id,
        device_id: cfg.device_id.clone(),
        point_id: cfg.point_id.clone(),
        alarm_type: cfg.alarm_type.clone(),
        severity: cfg.severity,
        state: AlarmState::Offnormal,
        trigger_value: value,
        trigger_time_ms: ts,
        ack_time_ms: None,
        context_snapshot: context.clone(),
    };

    alarm_store.upsert_active(active);
    alarm_store.insert_event(
        cfg.id,
        &cfg.device_id,
        &cfg.point_id,
        cfg.severity,
        from,
        "offnormal",
        value,
        ts,
        Some(context),
    );
}

fn check_stale_alarms(
    configs: &[AlarmConfig],
    runtime: &mut HashMap<AlarmConfigId, AlarmRuntime>,
    alarm_store: &AlarmStore,
    point_store: &PointStore,
) {
    let now = Instant::now();
    for cfg in configs {
        if !cfg.enabled {
            continue;
        }
        if let AlarmParams::Stale { timeout_secs } = &cfg.params {
            let key = PointKey {
                device_instance_id: cfg.device_id.clone(),
                point_id: cfg.point_id.clone(),
            };
            let is_stale = match point_store.get(&key) {
                Some(tv) => now.duration_since(tv.timestamp).as_secs() >= *timeout_secs,
                None => true, // No value at all = stale
            };

            let rt = runtime.entry(cfg.id).or_insert(AlarmRuntime {
                state: AlarmState::Normal,
                condition_since: None,
            });

            if is_stale && rt.state == AlarmState::Normal {
                let value = point_store
                    .get(&key)
                    .map(|tv| tv.value.as_f64())
                    .unwrap_or(0.0);
                transition_to_offnormal(cfg, value, rt, alarm_store, point_store);
            } else if !is_stale
                && matches!(rt.state, AlarmState::Offnormal | AlarmState::Acknowledged)
            {
                let value = point_store
                    .get(&key)
                    .map(|tv| tv.value.as_f64())
                    .unwrap_or(0.0);
                let ts = now_ms();
                let from = rt.state.as_str();
                rt.state = AlarmState::Normal;
                alarm_store.remove_active(cfg.id, &cfg.device_id, &cfg.point_id);
                alarm_store.insert_event(
                    cfg.id,
                    &cfg.device_id,
                    &cfg.point_id,
                    cfg.severity,
                    from,
                    "normal",
                    value,
                    ts,
                    None,
                );
            }
        }
    }
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::profile::PointValue;
    use std::sync::atomic::{AtomicU32, Ordering};

    static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_db_path() -> std::path::PathBuf {
        let n = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join("opencrate-alarm-test");
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(format!("alarm-test-{n}.db"))
    }

    #[tokio::test]
    async fn config_crud() {
        let db_path = temp_db_path();
        let store = PointStore::new();
        let alarm_store = start_alarm_engine_with_path(&store, &db_path, None);

        // Create
        let id = alarm_store
            .create_config(
                "ahu-1",
                "dat",
                AlarmSeverity::Warning,
                AlarmParams::HighLimit {
                    limit: 80.0,
                    deadband: 2.0,
                    delay_secs: 0,
                },
            )
            .await
            .unwrap();
        assert!(id > 0);

        // List
        let configs = alarm_store.list_configs().await;
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].device_id, "ahu-1");
        assert_eq!(configs[0].point_id, "dat");

        // Get for point
        let pt_configs = alarm_store.get_configs_for_point("ahu-1", "dat").await;
        assert_eq!(pt_configs.len(), 1);

        // Update
        alarm_store
            .update_config(
                id,
                AlarmSeverity::Critical,
                true,
                AlarmParams::HighLimit {
                    limit: 85.0,
                    deadband: 3.0,
                    delay_secs: 5,
                },
            )
            .await
            .unwrap();
        let configs = alarm_store.list_configs().await;
        assert_eq!(configs[0].severity, AlarmSeverity::Critical);

        // Delete
        alarm_store.delete_config(id).await.unwrap();
        let configs = alarm_store.list_configs().await;
        assert!(configs.is_empty());

        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn config_batch_create() {
        let db_path = temp_db_path();
        let store = PointStore::new();
        let alarm_store = start_alarm_engine_with_path(&store, &db_path, None);

        let entries = vec![
            ("ahu-1".to_string(), "dat".to_string()),
            ("ahu-1".to_string(), "mat".to_string()),
            ("vav-1".to_string(), "zat".to_string()),
        ];

        let ids = alarm_store
            .create_configs_batch(
                &entries,
                AlarmSeverity::Warning,
                AlarmParams::HighLimit {
                    limit: 80.0,
                    deadband: 2.0,
                    delay_secs: 0,
                },
            )
            .await
            .unwrap();

        assert_eq!(ids.len(), 3);
        let configs = alarm_store.list_configs().await;
        assert_eq!(configs.len(), 3);

        // Verify each entry
        let pt_configs = alarm_store.get_configs_for_point("ahu-1", "dat").await;
        assert_eq!(pt_configs.len(), 1);
        let pt_configs = alarm_store.get_configs_for_point("vav-1", "zat").await;
        assert_eq!(pt_configs.len(), 1);

        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn high_limit_alarm_triggers() {
        let db_path = temp_db_path();
        let store = PointStore::new();
        let alarm_store = start_alarm_engine_with_path(&store, &db_path, None);

        // Set an initial value
        store.set(
            PointKey {
                device_instance_id: "ahu-1".into(),
                point_id: "dat".into(),
            },
            PointValue::Float(70.0),
        );

        // Create alarm config
        let _id = alarm_store
            .create_config(
                "ahu-1",
                "dat",
                AlarmSeverity::Warning,
                AlarmParams::HighLimit {
                    limit: 80.0,
                    deadband: 2.0,
                    delay_secs: 0,
                },
            )
            .await
            .unwrap();

        // Wait for config reload
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        // Trigger the alarm
        store.set(
            PointKey {
                device_instance_id: "ahu-1".into(),
                point_id: "dat".into(),
            },
            PointValue::Float(82.0),
        );

        // Wait for evaluation
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        let active = alarm_store.get_active_alarms().await;
        assert_eq!(active.len(), 1, "alarm should be active");
        assert_eq!(active[0].state, AlarmState::Offnormal);
        assert!((active[0].trigger_value - 82.0).abs() < f64::EPSILON);

        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn alarm_acknowledge_and_clear() {
        let db_path = temp_db_path();
        let store = PointStore::new();
        let alarm_store = start_alarm_engine_with_path(&store, &db_path, None);

        store.set(
            PointKey {
                device_instance_id: "ahu-1".into(),
                point_id: "dat".into(),
            },
            PointValue::Float(70.0),
        );

        let id = alarm_store
            .create_config(
                "ahu-1",
                "dat",
                AlarmSeverity::Critical,
                AlarmParams::HighLimit {
                    limit: 80.0,
                    deadband: 2.0,
                    delay_secs: 0,
                },
            )
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        // Trigger
        store.set(
            PointKey {
                device_instance_id: "ahu-1".into(),
                point_id: "dat".into(),
            },
            PointValue::Float(85.0),
        );
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        // Acknowledge
        alarm_store.acknowledge(id).await.unwrap();
        let active = alarm_store.get_active_alarms().await;
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].state, AlarmState::Acknowledged);

        // Clear — value goes below limit - deadband (80 - 2 = 78)
        store.set(
            PointKey {
                device_instance_id: "ahu-1".into(),
                point_id: "dat".into(),
            },
            PointValue::Float(75.0),
        );
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        let active = alarm_store.get_active_alarms().await;
        assert!(active.is_empty(), "alarm should be cleared");

        // Check history
        let history = alarm_store
            .query_history(AlarmHistoryQuery::default())
            .await
            .unwrap();
        assert!(
            history.len() >= 2,
            "should have offnormal + ack + normal transitions"
        );

        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn low_limit_alarm() {
        let db_path = temp_db_path();
        let store = PointStore::new();
        let alarm_store = start_alarm_engine_with_path(&store, &db_path, None);

        store.set(
            PointKey {
                device_instance_id: "ahu-1".into(),
                point_id: "dat".into(),
            },
            PointValue::Float(70.0),
        );

        let _id = alarm_store
            .create_config(
                "ahu-1",
                "dat",
                AlarmSeverity::Warning,
                AlarmParams::LowLimit {
                    limit: 50.0,
                    deadband: 2.0,
                    delay_secs: 0,
                },
            )
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        // Trigger
        store.set(
            PointKey {
                device_instance_id: "ahu-1".into(),
                point_id: "dat".into(),
            },
            PointValue::Float(48.0),
        );
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        let active = alarm_store.get_active_alarms().await;
        assert_eq!(active.len(), 1);

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn alarm_params_serde_roundtrip() {
        let params = AlarmParams::HighLimit {
            limit: 80.0,
            deadband: 2.0,
            delay_secs: 30,
        };
        let json = serde_json::to_string(&params).unwrap();
        let parsed: AlarmParams = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            parsed,
            AlarmParams::HighLimit {
                limit,
                deadband,
                delay_secs
            } if (limit - 80.0).abs() < f64::EPSILON
              && (deadband - 2.0).abs() < f64::EPSILON
              && delay_secs == 30
        ));
    }

    #[tokio::test]
    async fn state_change_alarm_triggers() {
        let db_path = temp_db_path();
        let store = PointStore::new();
        let alarm_store = start_alarm_engine_with_path(&store, &db_path, None);

        // Binary point starts ON (true → 1.0)
        store.set(
            PointKey {
                device_instance_id: "ahu-1".into(),
                point_id: "sf_status".into(),
            },
            PointValue::Bool(true),
        );

        // Alarm when OFF
        let _id = alarm_store
            .create_config(
                "ahu-1",
                "sf_status",
                AlarmSeverity::Critical,
                AlarmParams::StateChange {
                    alarm_value: false,
                    delay_secs: 0,
                },
            )
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        // Should be normal — fan is ON, alarm is for OFF
        let active = alarm_store.get_active_alarms().await;
        assert!(active.is_empty(), "Should have no alarms when fan is ON");

        // Fan goes OFF
        store.set(
            PointKey {
                device_instance_id: "ahu-1".into(),
                point_id: "sf_status".into(),
            },
            PointValue::Bool(false),
        );
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        let active = alarm_store.get_active_alarms().await;
        assert_eq!(active.len(), 1, "Should alarm when fan is OFF");

        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn multi_state_alarm_triggers() {
        let db_path = temp_db_path();
        let store = PointStore::new();
        let alarm_store = start_alarm_engine_with_path(&store, &db_path, None);

        // Multistate point starts at state 1 (Occupied)
        store.set(
            PointKey {
                device_instance_id: "ahu-1".into(),
                point_id: "occ_mode".into(),
            },
            PointValue::Integer(1),
        );

        // Alarm on states 3 (Standby) and 4 (Warmup)
        let _id = alarm_store
            .create_config(
                "ahu-1",
                "occ_mode",
                AlarmSeverity::Warning,
                AlarmParams::MultiStateAlarm {
                    alarm_states: vec![3, 4],
                    delay_secs: 0,
                },
            )
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        let active = alarm_store.get_active_alarms().await;
        assert!(active.is_empty(), "State 1 should not alarm");

        // Switch to Standby (3)
        store.set(
            PointKey {
                device_instance_id: "ahu-1".into(),
                point_id: "occ_mode".into(),
            },
            PointValue::Integer(3),
        );
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        let active = alarm_store.get_active_alarms().await;
        assert_eq!(active.len(), 1, "State 3 should trigger alarm");

        // Switch back to Occupied (1) — should clear
        store.set(
            PointKey {
                device_instance_id: "ahu-1".into(),
                point_id: "occ_mode".into(),
            },
            PointValue::Integer(1),
        );
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        let active = alarm_store.get_active_alarms().await;
        assert!(active.is_empty(), "State 1 should clear alarm");

        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn command_mismatch_alarm_triggers() {
        let db_path = temp_db_path();
        let store = PointStore::new();
        let alarm_store = start_alarm_engine_with_path(&store, &db_path, None);

        // Command ON, feedback ON — matched
        store.set(
            PointKey {
                device_instance_id: "ahu-1".into(),
                point_id: "sf_cmd".into(),
            },
            PointValue::Bool(true),
        );
        store.set(
            PointKey {
                device_instance_id: "ahu-1".into(),
                point_id: "sf_status".into(),
            },
            PointValue::Bool(true),
        );

        // Alarm when sf_cmd != sf_status
        let _id = alarm_store
            .create_config(
                "ahu-1",
                "sf_cmd",
                AlarmSeverity::Critical,
                AlarmParams::CommandMismatch {
                    feedback_device_id: "ahu-1".to_string(),
                    feedback_point_id: "sf_status".to_string(),
                    delay_secs: 0,
                },
            )
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        let active = alarm_store.get_active_alarms().await;
        assert!(
            active.is_empty(),
            "Matched command/feedback should not alarm"
        );

        // Feedback goes OFF while command stays ON — mismatch
        store.set(
            PointKey {
                device_instance_id: "ahu-1".into(),
                point_id: "sf_status".into(),
            },
            PointValue::Bool(false),
        );
        // Need to re-trigger evaluation by touching the command point
        store.set(
            PointKey {
                device_instance_id: "ahu-1".into(),
                point_id: "sf_cmd".into(),
            },
            PointValue::Bool(true),
        );
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        let active = alarm_store.get_active_alarms().await;
        assert_eq!(active.len(), 1, "Mismatched command/feedback should alarm");

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn severity_ordering() {
        assert!(AlarmSeverity::Info < AlarmSeverity::Warning);
        assert!(AlarmSeverity::Warning < AlarmSeverity::Critical);
        assert!(AlarmSeverity::Critical < AlarmSeverity::LifeSafety);
    }
}
