use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

use super::migration::{run_migrations, Migration};
use crate::energy::rollup::EnergyRollup;

// ----------------------------------------------------------------
// Public types
// ----------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UtilityRate {
    pub id: i64,
    pub name: String,
    pub rate_type: String,
    pub config: String, // JSON — parsed as cost::RateConfig
    pub currency: String,
    pub created_ms: i64,
    pub updated_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnergyMeter {
    pub id: i64,
    pub name: String,
    pub node_id: String,
    pub energy_node_id: Option<String>,
    pub utility_rate_id: Option<i64>,
    pub meter_type: String,
    pub unit: String,
    pub created_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnergyBaseline {
    pub id: i64,
    pub meter_id: i64,
    pub name: String,
    pub baseline_type: String,
    pub config: String, // JSON
    pub start_ms: i64,
    pub end_ms: i64,
    pub created_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredRollup {
    pub id: i64,
    pub meter_id: i64,
    pub period_type: String,
    pub period_start_ms: i64,
    pub consumption_kwh: f64,
    pub peak_demand_kw: f64,
    pub peak_demand_ms: i64,
    pub avg_kw: f64,
    pub cost: f64,
    pub hdd: f64,
    pub cdd: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum EnergyStoreError {
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

enum EnergyCmd {
    // Utility rate CRUD
    CreateRate {
        name: String,
        rate_type: String,
        config: String,
        currency: String,
        reply: oneshot::Sender<Result<i64, EnergyStoreError>>,
    },
    UpdateRate {
        id: i64,
        name: String,
        rate_type: String,
        config: String,
        currency: String,
        reply: oneshot::Sender<Result<(), EnergyStoreError>>,
    },
    DeleteRate {
        id: i64,
        reply: oneshot::Sender<Result<(), EnergyStoreError>>,
    },
    ListRates {
        reply: oneshot::Sender<Vec<UtilityRate>>,
    },
    GetRate {
        id: i64,
        reply: oneshot::Sender<Option<UtilityRate>>,
    },
    // Meter CRUD
    CreateMeter {
        name: String,
        node_id: String,
        energy_node_id: Option<String>,
        utility_rate_id: Option<i64>,
        meter_type: String,
        unit: String,
        reply: oneshot::Sender<Result<i64, EnergyStoreError>>,
    },
    UpdateMeter {
        id: i64,
        name: String,
        node_id: String,
        energy_node_id: Option<String>,
        utility_rate_id: Option<i64>,
        meter_type: String,
        unit: String,
        reply: oneshot::Sender<Result<(), EnergyStoreError>>,
    },
    DeleteMeter {
        id: i64,
        reply: oneshot::Sender<Result<(), EnergyStoreError>>,
    },
    ListMeters {
        reply: oneshot::Sender<Vec<EnergyMeter>>,
    },
    GetMeter {
        id: i64,
        reply: oneshot::Sender<Option<EnergyMeter>>,
    },
    // Baseline CRUD
    CreateBaseline {
        meter_id: i64,
        name: String,
        baseline_type: String,
        config: String,
        start_ms: i64,
        end_ms: i64,
        reply: oneshot::Sender<Result<i64, EnergyStoreError>>,
    },
    DeleteBaseline {
        id: i64,
        reply: oneshot::Sender<Result<(), EnergyStoreError>>,
    },
    ListBaselines {
        meter_id: i64,
        reply: oneshot::Sender<Vec<EnergyBaseline>>,
    },
    // Rollup queries
    UpsertRollup {
        rollup: EnergyRollup,
        reply: oneshot::Sender<()>,
    },
    GetRollup {
        meter_id: i64,
        period_type: String,
        period_start_ms: i64,
        reply: oneshot::Sender<Option<StoredRollup>>,
    },
    QueryRollups {
        meter_id: i64,
        period_type: String,
        start_ms: i64,
        end_ms: i64,
        reply: oneshot::Sender<Vec<StoredRollup>>,
    },
}

// ----------------------------------------------------------------
// EnergyStore — async handle to the SQLite thread
// ----------------------------------------------------------------

#[derive(Clone)]
pub struct EnergyStore {
    cmd_tx: mpsc::UnboundedSender<EnergyCmd>,
}

impl EnergyStore {
    // ---- Rate CRUD ----

    pub async fn create_rate(
        &self,
        name: &str,
        rate_type: &str,
        config: &str,
        currency: &str,
    ) -> Result<i64, EnergyStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(EnergyCmd::CreateRate {
                name: name.to_string(),
                rate_type: rate_type.to_string(),
                config: config.to_string(),
                currency: currency.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| EnergyStoreError::ChannelClosed)?;
        reply_rx
            .await
            .map_err(|_| EnergyStoreError::ChannelClosed)?
    }

    pub async fn update_rate(
        &self,
        id: i64,
        name: &str,
        rate_type: &str,
        config: &str,
        currency: &str,
    ) -> Result<(), EnergyStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(EnergyCmd::UpdateRate {
                id,
                name: name.to_string(),
                rate_type: rate_type.to_string(),
                config: config.to_string(),
                currency: currency.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| EnergyStoreError::ChannelClosed)?;
        reply_rx
            .await
            .map_err(|_| EnergyStoreError::ChannelClosed)?
    }

    pub async fn delete_rate(&self, id: i64) -> Result<(), EnergyStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(EnergyCmd::DeleteRate {
                id,
                reply: reply_tx,
            })
            .map_err(|_| EnergyStoreError::ChannelClosed)?;
        reply_rx
            .await
            .map_err(|_| EnergyStoreError::ChannelClosed)?
    }

    pub async fn list_rates(&self) -> Vec<UtilityRate> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(EnergyCmd::ListRates { reply: reply_tx });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn get_rate(&self, id: i64) -> Option<UtilityRate> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(EnergyCmd::GetRate {
            id,
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or(None)
    }

    // ---- Meter CRUD ----

    pub async fn create_meter(
        &self,
        name: &str,
        node_id: &str,
        energy_node_id: Option<&str>,
        utility_rate_id: Option<i64>,
        meter_type: &str,
        unit: &str,
    ) -> Result<i64, EnergyStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(EnergyCmd::CreateMeter {
                name: name.to_string(),
                node_id: node_id.to_string(),
                energy_node_id: energy_node_id.map(String::from),
                utility_rate_id,
                meter_type: meter_type.to_string(),
                unit: unit.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| EnergyStoreError::ChannelClosed)?;
        reply_rx
            .await
            .map_err(|_| EnergyStoreError::ChannelClosed)?
    }

    pub async fn update_meter(
        &self,
        id: i64,
        name: &str,
        node_id: &str,
        energy_node_id: Option<&str>,
        utility_rate_id: Option<i64>,
        meter_type: &str,
        unit: &str,
    ) -> Result<(), EnergyStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(EnergyCmd::UpdateMeter {
                id,
                name: name.to_string(),
                node_id: node_id.to_string(),
                energy_node_id: energy_node_id.map(String::from),
                utility_rate_id,
                meter_type: meter_type.to_string(),
                unit: unit.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| EnergyStoreError::ChannelClosed)?;
        reply_rx
            .await
            .map_err(|_| EnergyStoreError::ChannelClosed)?
    }

    pub async fn delete_meter(&self, id: i64) -> Result<(), EnergyStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(EnergyCmd::DeleteMeter {
                id,
                reply: reply_tx,
            })
            .map_err(|_| EnergyStoreError::ChannelClosed)?;
        reply_rx
            .await
            .map_err(|_| EnergyStoreError::ChannelClosed)?
    }

    pub async fn list_meters(&self) -> Vec<EnergyMeter> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(EnergyCmd::ListMeters { reply: reply_tx });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn get_meter(&self, id: i64) -> Option<EnergyMeter> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(EnergyCmd::GetMeter {
            id,
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or(None)
    }

    // ---- Baseline CRUD ----

    pub async fn create_baseline(
        &self,
        meter_id: i64,
        name: &str,
        baseline_type: &str,
        config: &str,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<i64, EnergyStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(EnergyCmd::CreateBaseline {
                meter_id,
                name: name.to_string(),
                baseline_type: baseline_type.to_string(),
                config: config.to_string(),
                start_ms,
                end_ms,
                reply: reply_tx,
            })
            .map_err(|_| EnergyStoreError::ChannelClosed)?;
        reply_rx
            .await
            .map_err(|_| EnergyStoreError::ChannelClosed)?
    }

    pub async fn delete_baseline(&self, id: i64) -> Result<(), EnergyStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(EnergyCmd::DeleteBaseline {
                id,
                reply: reply_tx,
            })
            .map_err(|_| EnergyStoreError::ChannelClosed)?;
        reply_rx
            .await
            .map_err(|_| EnergyStoreError::ChannelClosed)?
    }

    pub async fn list_baselines(&self, meter_id: i64) -> Vec<EnergyBaseline> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(EnergyCmd::ListBaselines {
            meter_id,
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_default()
    }

    // ---- Rollup operations ----

    pub async fn upsert_rollup(&self, rollup: &EnergyRollup) {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(EnergyCmd::UpsertRollup {
            rollup: rollup.clone(),
            reply: reply_tx,
        });
        let _ = reply_rx.await;
    }

    pub async fn get_rollup(
        &self,
        meter_id: i64,
        period_type: &str,
        period_start_ms: i64,
    ) -> Option<StoredRollup> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(EnergyCmd::GetRollup {
            meter_id,
            period_type: period_type.to_string(),
            period_start_ms,
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or(None)
    }

    pub async fn query_rollups(
        &self,
        meter_id: i64,
        period_type: &str,
        start_ms: i64,
        end_ms: i64,
    ) -> Vec<StoredRollup> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(EnergyCmd::QueryRollups {
            meter_id,
            period_type: period_type.to_string(),
            start_ms,
            end_ms,
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_default()
    }
}

// ----------------------------------------------------------------
// SQLite schema & thread
// ----------------------------------------------------------------

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    label: "initial energy schema",
    sql: "
CREATE TABLE IF NOT EXISTS utility_rate (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL,
    rate_type   TEXT NOT NULL,
    config      TEXT NOT NULL,
    currency    TEXT NOT NULL DEFAULT 'USD',
    created_ms  INTEGER NOT NULL,
    updated_ms  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS energy_meter (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    name            TEXT NOT NULL,
    node_id         TEXT NOT NULL,
    energy_node_id  TEXT,
    utility_rate_id INTEGER REFERENCES utility_rate(id) ON DELETE SET NULL,
    meter_type      TEXT NOT NULL DEFAULT 'electric',
    unit            TEXT NOT NULL DEFAULT 'kW',
    created_ms      INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS energy_baseline (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    meter_id        INTEGER NOT NULL REFERENCES energy_meter(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    baseline_type   TEXT NOT NULL,
    config          TEXT NOT NULL,
    start_ms        INTEGER NOT NULL,
    end_ms          INTEGER NOT NULL,
    created_ms      INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS energy_rollup (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    meter_id        INTEGER NOT NULL REFERENCES energy_meter(id) ON DELETE CASCADE,
    period_type     TEXT NOT NULL,
    period_start_ms INTEGER NOT NULL,
    consumption_kwh REAL,
    peak_demand_kw  REAL,
    peak_demand_ms  INTEGER,
    avg_kw          REAL,
    cost            REAL,
    hdd             REAL,
    cdd             REAL,
    UNIQUE(meter_id, period_type, period_start_ms)
);

CREATE INDEX IF NOT EXISTS idx_rollup_meter_period ON energy_rollup(meter_id, period_type, period_start_ms);
",
}];

fn run_sqlite_thread(db_path: &Path, mut cmd_rx: mpsc::UnboundedReceiver<EnergyCmd>) {
    let conn = rusqlite::Connection::open(db_path).expect("failed to open energy database");
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;",
    )
    .expect("failed to set pragmas");
    run_migrations(&conn, "energy", MIGRATIONS).expect("energy: schema migration failed");

    while let Some(cmd) = cmd_rx.blocking_recv() {
        match cmd {
            // ---- Rates ----
            EnergyCmd::CreateRate {
                name,
                rate_type,
                config,
                currency,
                reply,
            } => {
                let _ = reply.send(create_rate_db(&conn, &name, &rate_type, &config, &currency));
            }
            EnergyCmd::UpdateRate {
                id,
                name,
                rate_type,
                config,
                currency,
                reply,
            } => {
                let _ = reply.send(update_rate_db(
                    &conn, id, &name, &rate_type, &config, &currency,
                ));
            }
            EnergyCmd::DeleteRate { id, reply } => {
                let _ = reply.send(delete_rate_db(&conn, id));
            }
            EnergyCmd::ListRates { reply } => {
                let _ = reply.send(list_rates_db(&conn));
            }
            EnergyCmd::GetRate { id, reply } => {
                let _ = reply.send(get_rate_db(&conn, id));
            }
            // ---- Meters ----
            EnergyCmd::CreateMeter {
                name,
                node_id,
                energy_node_id,
                utility_rate_id,
                meter_type,
                unit,
                reply,
            } => {
                let _ = reply.send(create_meter_db(
                    &conn,
                    &name,
                    &node_id,
                    energy_node_id.as_deref(),
                    utility_rate_id,
                    &meter_type,
                    &unit,
                ));
            }
            EnergyCmd::UpdateMeter {
                id,
                name,
                node_id,
                energy_node_id,
                utility_rate_id,
                meter_type,
                unit,
                reply,
            } => {
                let _ = reply.send(update_meter_db(
                    &conn,
                    id,
                    &name,
                    &node_id,
                    energy_node_id.as_deref(),
                    utility_rate_id,
                    &meter_type,
                    &unit,
                ));
            }
            EnergyCmd::DeleteMeter { id, reply } => {
                let _ = reply.send(delete_meter_db(&conn, id));
            }
            EnergyCmd::ListMeters { reply } => {
                let _ = reply.send(list_meters_db(&conn));
            }
            EnergyCmd::GetMeter { id, reply } => {
                let _ = reply.send(get_meter_db(&conn, id));
            }
            // ---- Baselines ----
            EnergyCmd::CreateBaseline {
                meter_id,
                name,
                baseline_type,
                config,
                start_ms,
                end_ms,
                reply,
            } => {
                let _ = reply.send(create_baseline_db(
                    &conn,
                    meter_id,
                    &name,
                    &baseline_type,
                    &config,
                    start_ms,
                    end_ms,
                ));
            }
            EnergyCmd::DeleteBaseline { id, reply } => {
                let _ = reply.send(delete_baseline_db(&conn, id));
            }
            EnergyCmd::ListBaselines { meter_id, reply } => {
                let _ = reply.send(list_baselines_db(&conn, meter_id));
            }
            // ---- Rollups ----
            EnergyCmd::UpsertRollup { rollup, reply } => {
                upsert_rollup_db(&conn, &rollup);
                let _ = reply.send(());
            }
            EnergyCmd::GetRollup {
                meter_id,
                period_type,
                period_start_ms,
                reply,
            } => {
                let _ = reply.send(get_rollup_db(
                    &conn,
                    meter_id,
                    &period_type,
                    period_start_ms,
                ));
            }
            EnergyCmd::QueryRollups {
                meter_id,
                period_type,
                start_ms,
                end_ms,
                reply,
            } => {
                let _ = reply.send(query_rollups_db(
                    &conn,
                    meter_id,
                    &period_type,
                    start_ms,
                    end_ms,
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

// ---- Rates ----

fn create_rate_db(
    conn: &rusqlite::Connection,
    name: &str,
    rate_type: &str,
    config: &str,
    currency: &str,
) -> Result<i64, EnergyStoreError> {
    let ts = now_ms();
    conn.execute(
        "INSERT INTO utility_rate (name, rate_type, config, currency, created_ms, updated_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![name, rate_type, config, currency, ts, ts],
    )
    .map_err(|e| EnergyStoreError::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

fn update_rate_db(
    conn: &rusqlite::Connection,
    id: i64,
    name: &str,
    rate_type: &str,
    config: &str,
    currency: &str,
) -> Result<(), EnergyStoreError> {
    let ts = now_ms();
    let rows = conn
        .execute(
            "UPDATE utility_rate SET name = ?1, rate_type = ?2, config = ?3, currency = ?4, updated_ms = ?5 WHERE id = ?6",
            rusqlite::params![name, rate_type, config, currency, ts, id],
        )
        .map_err(|e| EnergyStoreError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(EnergyStoreError::NotFound);
    }
    Ok(())
}

fn delete_rate_db(conn: &rusqlite::Connection, id: i64) -> Result<(), EnergyStoreError> {
    let rows = conn
        .execute(
            "DELETE FROM utility_rate WHERE id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| EnergyStoreError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(EnergyStoreError::NotFound);
    }
    Ok(())
}

fn parse_rate_row(row: &rusqlite::Row) -> rusqlite::Result<UtilityRate> {
    Ok(UtilityRate {
        id: row.get(0)?,
        name: row.get(1)?,
        rate_type: row.get(2)?,
        config: row.get(3)?,
        currency: row.get(4)?,
        created_ms: row.get(5)?,
        updated_ms: row.get(6)?,
    })
}

fn list_rates_db(conn: &rusqlite::Connection) -> Vec<UtilityRate> {
    let mut stmt = conn
        .prepare_cached("SELECT id, name, rate_type, config, currency, created_ms, updated_ms FROM utility_rate ORDER BY id")
        .unwrap();
    let rows = stmt.query_map([], parse_rate_row).unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

fn get_rate_db(conn: &rusqlite::Connection, id: i64) -> Option<UtilityRate> {
    let mut stmt = conn
        .prepare_cached("SELECT id, name, rate_type, config, currency, created_ms, updated_ms FROM utility_rate WHERE id = ?1")
        .unwrap();
    stmt.query_row(rusqlite::params![id], parse_rate_row).ok()
}

// ---- Meters ----

fn create_meter_db(
    conn: &rusqlite::Connection,
    name: &str,
    node_id: &str,
    energy_node_id: Option<&str>,
    utility_rate_id: Option<i64>,
    meter_type: &str,
    unit: &str,
) -> Result<i64, EnergyStoreError> {
    let ts = now_ms();
    conn.execute(
        "INSERT INTO energy_meter (name, node_id, energy_node_id, utility_rate_id, meter_type, unit, created_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![name, node_id, energy_node_id, utility_rate_id, meter_type, unit, ts],
    )
    .map_err(|e| EnergyStoreError::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

fn update_meter_db(
    conn: &rusqlite::Connection,
    id: i64,
    name: &str,
    node_id: &str,
    energy_node_id: Option<&str>,
    utility_rate_id: Option<i64>,
    meter_type: &str,
    unit: &str,
) -> Result<(), EnergyStoreError> {
    let rows = conn
        .execute(
            "UPDATE energy_meter SET name = ?1, node_id = ?2, energy_node_id = ?3, utility_rate_id = ?4, meter_type = ?5, unit = ?6 WHERE id = ?7",
            rusqlite::params![name, node_id, energy_node_id, utility_rate_id, meter_type, unit, id],
        )
        .map_err(|e| EnergyStoreError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(EnergyStoreError::NotFound);
    }
    Ok(())
}

fn delete_meter_db(conn: &rusqlite::Connection, id: i64) -> Result<(), EnergyStoreError> {
    let rows = conn
        .execute(
            "DELETE FROM energy_meter WHERE id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| EnergyStoreError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(EnergyStoreError::NotFound);
    }
    Ok(())
}

fn parse_meter_row(row: &rusqlite::Row) -> rusqlite::Result<EnergyMeter> {
    Ok(EnergyMeter {
        id: row.get(0)?,
        name: row.get(1)?,
        node_id: row.get(2)?,
        energy_node_id: row.get(3)?,
        utility_rate_id: row.get(4)?,
        meter_type: row.get(5)?,
        unit: row.get(6)?,
        created_ms: row.get(7)?,
    })
}

fn list_meters_db(conn: &rusqlite::Connection) -> Vec<EnergyMeter> {
    let mut stmt = conn
        .prepare_cached("SELECT id, name, node_id, energy_node_id, utility_rate_id, meter_type, unit, created_ms FROM energy_meter ORDER BY id")
        .unwrap();
    let rows = stmt.query_map([], parse_meter_row).unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

fn get_meter_db(conn: &rusqlite::Connection, id: i64) -> Option<EnergyMeter> {
    let mut stmt = conn
        .prepare_cached("SELECT id, name, node_id, energy_node_id, utility_rate_id, meter_type, unit, created_ms FROM energy_meter WHERE id = ?1")
        .unwrap();
    stmt.query_row(rusqlite::params![id], parse_meter_row).ok()
}

// ---- Baselines ----

fn create_baseline_db(
    conn: &rusqlite::Connection,
    meter_id: i64,
    name: &str,
    baseline_type: &str,
    config: &str,
    start_ms: i64,
    end_ms: i64,
) -> Result<i64, EnergyStoreError> {
    let ts = now_ms();
    conn.execute(
        "INSERT INTO energy_baseline (meter_id, name, baseline_type, config, start_ms, end_ms, created_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![meter_id, name, baseline_type, config, start_ms, end_ms, ts],
    )
    .map_err(|e| EnergyStoreError::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

fn delete_baseline_db(conn: &rusqlite::Connection, id: i64) -> Result<(), EnergyStoreError> {
    let rows = conn
        .execute(
            "DELETE FROM energy_baseline WHERE id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| EnergyStoreError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(EnergyStoreError::NotFound);
    }
    Ok(())
}

fn list_baselines_db(conn: &rusqlite::Connection, meter_id: i64) -> Vec<EnergyBaseline> {
    let mut stmt = conn
        .prepare_cached("SELECT id, meter_id, name, baseline_type, config, start_ms, end_ms, created_ms FROM energy_baseline WHERE meter_id = ?1 ORDER BY id")
        .unwrap();
    let rows = stmt
        .query_map(rusqlite::params![meter_id], |row| {
            Ok(EnergyBaseline {
                id: row.get(0)?,
                meter_id: row.get(1)?,
                name: row.get(2)?,
                baseline_type: row.get(3)?,
                config: row.get(4)?,
                start_ms: row.get(5)?,
                end_ms: row.get(6)?,
                created_ms: row.get(7)?,
            })
        })
        .unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

// ---- Rollups ----

fn upsert_rollup_db(conn: &rusqlite::Connection, rollup: &EnergyRollup) {
    let _ = conn.execute(
        "INSERT INTO energy_rollup (meter_id, period_type, period_start_ms, consumption_kwh, peak_demand_kw, peak_demand_ms, avg_kw, cost, hdd, cdd)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(meter_id, period_type, period_start_ms)
         DO UPDATE SET consumption_kwh = ?4, peak_demand_kw = ?5, peak_demand_ms = ?6, avg_kw = ?7, cost = ?8, hdd = ?9, cdd = ?10",
        rusqlite::params![
            rollup.meter_id,
            rollup.period_type,
            rollup.period_start_ms,
            rollup.consumption_kwh,
            rollup.peak_demand_kw,
            rollup.peak_demand_ms,
            rollup.avg_kw,
            rollup.cost,
            rollup.hdd,
            rollup.cdd,
        ],
    );
}

fn parse_rollup_row(row: &rusqlite::Row) -> rusqlite::Result<StoredRollup> {
    Ok(StoredRollup {
        id: row.get(0)?,
        meter_id: row.get(1)?,
        period_type: row.get(2)?,
        period_start_ms: row.get(3)?,
        consumption_kwh: row.get(4)?,
        peak_demand_kw: row.get(5)?,
        peak_demand_ms: row.get(6)?,
        avg_kw: row.get(7)?,
        cost: row.get(8)?,
        hdd: row.get(9)?,
        cdd: row.get(10)?,
    })
}

fn get_rollup_db(
    conn: &rusqlite::Connection,
    meter_id: i64,
    period_type: &str,
    period_start_ms: i64,
) -> Option<StoredRollup> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT id, meter_id, period_type, period_start_ms, consumption_kwh, peak_demand_kw, peak_demand_ms, avg_kw, cost, hdd, cdd
             FROM energy_rollup WHERE meter_id = ?1 AND period_type = ?2 AND period_start_ms = ?3",
        )
        .unwrap();
    stmt.query_row(
        rusqlite::params![meter_id, period_type, period_start_ms],
        parse_rollup_row,
    )
    .ok()
}

fn query_rollups_db(
    conn: &rusqlite::Connection,
    meter_id: i64,
    period_type: &str,
    start_ms: i64,
    end_ms: i64,
) -> Vec<StoredRollup> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT id, meter_id, period_type, period_start_ms, consumption_kwh, peak_demand_kw, peak_demand_ms, avg_kw, cost, hdd, cdd
             FROM energy_rollup WHERE meter_id = ?1 AND period_type = ?2 AND period_start_ms >= ?3 AND period_start_ms < ?4
             ORDER BY period_start_ms",
        )
        .unwrap();
    let rows = stmt
        .query_map(
            rusqlite::params![meter_id, period_type, start_ms, end_ms],
            parse_rollup_row,
        )
        .unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

// ----------------------------------------------------------------
// Constructor
// ----------------------------------------------------------------

pub fn start_energy_store_with_path(db_path: &Path) -> EnergyStore {
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let path_clone = db_path.to_path_buf();
    std::thread::Builder::new()
        .name("energy-sqlite".into())
        .spawn(move || run_sqlite_thread(&path_clone, cmd_rx))
        .expect("failed to spawn energy SQLite thread");
    EnergyStore { cmd_tx }
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> EnergyStore {
        let tmp = std::env::temp_dir().join(format!(
            "energy_test_{}_{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        start_energy_store_with_path(&tmp)
    }

    #[tokio::test]
    async fn rate_crud() {
        let store = test_store();
        let config = r#"{"type":"flat","energy_rate":0.12,"demand_rate":10.0}"#;
        let id = store
            .create_rate("Test Rate", "flat", config, "USD")
            .await
            .unwrap();
        assert!(id > 0);

        let rates = store.list_rates().await;
        assert_eq!(rates.len(), 1);
        assert_eq!(rates[0].name, "Test Rate");

        store
            .update_rate(id, "Updated Rate", "flat", config, "EUR")
            .await
            .unwrap();
        let rate = store.get_rate(id).await.unwrap();
        assert_eq!(rate.name, "Updated Rate");
        assert_eq!(rate.currency, "EUR");

        store.delete_rate(id).await.unwrap();
        assert!(store.list_rates().await.is_empty());
    }

    #[tokio::test]
    async fn meter_crud() {
        let store = test_store();
        let id = store
            .create_meter("Main Meter", "1234/power", None, None, "electric", "kW")
            .await
            .unwrap();
        assert!(id > 0);

        let meters = store.list_meters().await;
        assert_eq!(meters.len(), 1);
        assert_eq!(meters[0].node_id, "1234/power");

        store.delete_meter(id).await.unwrap();
        assert!(store.list_meters().await.is_empty());
    }

    #[tokio::test]
    async fn rollup_upsert_and_query() {
        let store = test_store();
        let meter_id = store
            .create_meter("M1", "1/p", None, None, "electric", "kW")
            .await
            .unwrap();

        let rollup = crate::energy::rollup::EnergyRollup {
            meter_id,
            period_type: "daily".into(),
            period_start_ms: 1000000,
            consumption_kwh: 42.0,
            peak_demand_kw: 10.0,
            peak_demand_ms: 1500000,
            avg_kw: 1.75,
            cost: 5.04,
            hdd: 3.0,
            cdd: 0.0,
        };
        store.upsert_rollup(&rollup).await;

        let found = store.get_rollup(meter_id, "daily", 1000000).await;
        assert!(found.is_some());
        assert!((found.unwrap().consumption_kwh - 42.0).abs() < 0.01);

        // Upsert again with different value.
        let rollup2 = crate::energy::rollup::EnergyRollup {
            consumption_kwh: 55.0,
            ..rollup
        };
        store.upsert_rollup(&rollup2).await;
        let found2 = store.get_rollup(meter_id, "daily", 1000000).await;
        assert!((found2.unwrap().consumption_kwh - 55.0).abs() < 0.01);
    }
}
