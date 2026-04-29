use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::{mpsc, oneshot, watch};

use super::migration::{run_migrations, Migration};
use crate::fdd::model::{
    FddBinding, FddCategory, FddCondition, FddFault, FddFaultEvent, FddFaultState, FddHistoryQuery,
    FddRule, FddSeverity,
};

// ----------------------------------------------------------------
// Public error type
// ----------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum FddStoreError {
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

enum FddCmd {
    // ---- Rule CRUD ----
    CreateRule {
        name: String,
        description: String,
        category: String,
        equip_tags: String,
        severity: String,
        condition: String,
        guidance: String,
        builtin: bool,
        builtin_id: Option<String>,
        enabled: bool,
        confirmation_count: u16,
        reply: oneshot::Sender<Result<i64, FddStoreError>>,
    },
    UpdateRule {
        id: i64,
        name: String,
        description: String,
        category: String,
        equip_tags: String,
        severity: String,
        condition: String,
        guidance: String,
        enabled: bool,
        confirmation_count: u16,
        reply: oneshot::Sender<Result<(), FddStoreError>>,
    },
    DeleteRule {
        id: i64,
        reply: oneshot::Sender<Result<(), FddStoreError>>,
    },
    ListRules {
        reply: oneshot::Sender<Vec<FddRule>>,
    },
    GetRule {
        id: i64,
        reply: oneshot::Sender<Option<FddRule>>,
    },

    // ---- Binding CRUD ----
    CreateBinding {
        rule_id: i64,
        equip_id: String,
        enabled: bool,
        config_overrides: Option<String>,
        reply: oneshot::Sender<Result<i64, FddStoreError>>,
    },
    UpdateBinding {
        id: i64,
        enabled: bool,
        config_overrides: Option<String>,
        reply: oneshot::Sender<Result<(), FddStoreError>>,
    },
    DeleteBinding {
        id: i64,
        reply: oneshot::Sender<Result<(), FddStoreError>>,
    },
    ListBindings {
        equip_id: Option<String>,
        rule_id: Option<i64>,
        reply: oneshot::Sender<Vec<FddBinding>>,
    },
    ListEnabledBindings {
        reply: oneshot::Sender<Vec<FddBinding>>,
    },

    // ---- Fault lifecycle ----
    CreateFault {
        binding_id: i64,
        rule_id: i64,
        equip_id: String,
        rule_name: String,
        severity: String,
        point_snapshot: String,
        guidance: String,
        reply: oneshot::Sender<Result<i64, FddStoreError>>,
    },
    AcknowledgeFault {
        id: i64,
        reply: oneshot::Sender<Result<(), FddStoreError>>,
    },
    AcknowledgeAll {
        reply: oneshot::Sender<u32>,
    },
    ClearFault {
        id: i64,
        reply: oneshot::Sender<Result<(), FddStoreError>>,
    },
    GetActiveFaults {
        reply: oneshot::Sender<Vec<FddFault>>,
    },
    GetFaultByBinding {
        binding_id: i64,
        reply: oneshot::Sender<Option<FddFault>>,
    },
    QueryHistory {
        query: FddHistoryQuery,
        reply: oneshot::Sender<Vec<FddFaultEvent>>,
    },
    CountActiveFaults {
        reply: oneshot::Sender<u32>,
    },

    // ---- Builtin seeding ----
    SeedBuiltinRules {
        rules: Vec<FddRule>,
        reply: oneshot::Sender<u32>,
    },
}

// ----------------------------------------------------------------
// FddStore — async handle to the SQLite thread
// ----------------------------------------------------------------

#[derive(Clone)]
pub struct FddStore {
    cmd_tx: mpsc::UnboundedSender<FddCmd>,
    version_tx: watch::Sender<u64>,
}

impl FddStore {
    /// Subscribe to version changes. The version is bumped on any mutation.
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.version_tx.subscribe()
    }

    // ---- Rule CRUD ----

    pub async fn create_rule(
        &self,
        name: &str,
        description: &str,
        category: &FddCategory,
        equip_tags: &[String],
        severity: &FddSeverity,
        condition: &FddCondition,
        guidance: &str,
        builtin: bool,
        builtin_id: Option<&str>,
        enabled: bool,
        confirmation_count: u16,
    ) -> Result<i64, FddStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(FddCmd::CreateRule {
                name: name.to_string(),
                description: description.to_string(),
                category: category.key().to_string(),
                equip_tags: serde_json::to_string(equip_tags).unwrap_or_else(|_| "[]".into()),
                severity: severity.key().to_string(),
                condition: serde_json::to_string(condition).unwrap_or_else(|_| "{}".into()),
                guidance: guidance.to_string(),
                builtin,
                builtin_id: builtin_id.map(String::from),
                enabled,
                confirmation_count,
                reply: reply_tx,
            })
            .map_err(|_| FddStoreError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| FddStoreError::ChannelClosed)?;
        if result.is_ok() {
            self.version_tx.send_modify(|v| *v += 1);
        }
        result
    }

    pub async fn update_rule(
        &self,
        id: i64,
        name: &str,
        description: &str,
        category: &FddCategory,
        equip_tags: &[String],
        severity: &FddSeverity,
        condition: &FddCondition,
        guidance: &str,
        enabled: bool,
        confirmation_count: u16,
    ) -> Result<(), FddStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(FddCmd::UpdateRule {
                id,
                name: name.to_string(),
                description: description.to_string(),
                category: category.key().to_string(),
                equip_tags: serde_json::to_string(equip_tags).unwrap_or_else(|_| "[]".into()),
                severity: severity.key().to_string(),
                condition: serde_json::to_string(condition).unwrap_or_else(|_| "{}".into()),
                guidance: guidance.to_string(),
                enabled,
                confirmation_count,
                reply: reply_tx,
            })
            .map_err(|_| FddStoreError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| FddStoreError::ChannelClosed)?;
        if result.is_ok() {
            self.version_tx.send_modify(|v| *v += 1);
        }
        result
    }

    pub async fn delete_rule(&self, id: i64) -> Result<(), FddStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(FddCmd::DeleteRule {
                id,
                reply: reply_tx,
            })
            .map_err(|_| FddStoreError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| FddStoreError::ChannelClosed)?;
        if result.is_ok() {
            self.version_tx.send_modify(|v| *v += 1);
        }
        result
    }

    pub async fn list_rules(&self) -> Vec<FddRule> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(FddCmd::ListRules { reply: reply_tx });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn get_rule(&self, id: i64) -> Option<FddRule> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(FddCmd::GetRule {
            id,
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or(None)
    }

    // ---- Binding CRUD ----

    pub async fn create_binding(
        &self,
        rule_id: i64,
        equip_id: &str,
        enabled: bool,
        config_overrides: Option<&str>,
    ) -> Result<i64, FddStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(FddCmd::CreateBinding {
                rule_id,
                equip_id: equip_id.to_string(),
                enabled,
                config_overrides: config_overrides.map(String::from),
                reply: reply_tx,
            })
            .map_err(|_| FddStoreError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| FddStoreError::ChannelClosed)?;
        if result.is_ok() {
            self.version_tx.send_modify(|v| *v += 1);
        }
        result
    }

    pub async fn update_binding(
        &self,
        id: i64,
        enabled: bool,
        config_overrides: Option<&str>,
    ) -> Result<(), FddStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(FddCmd::UpdateBinding {
                id,
                enabled,
                config_overrides: config_overrides.map(String::from),
                reply: reply_tx,
            })
            .map_err(|_| FddStoreError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| FddStoreError::ChannelClosed)?;
        if result.is_ok() {
            self.version_tx.send_modify(|v| *v += 1);
        }
        result
    }

    pub async fn delete_binding(&self, id: i64) -> Result<(), FddStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(FddCmd::DeleteBinding {
                id,
                reply: reply_tx,
            })
            .map_err(|_| FddStoreError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| FddStoreError::ChannelClosed)?;
        if result.is_ok() {
            self.version_tx.send_modify(|v| *v += 1);
        }
        result
    }

    pub async fn list_bindings(
        &self,
        equip_id: Option<&str>,
        rule_id: Option<i64>,
    ) -> Vec<FddBinding> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(FddCmd::ListBindings {
            equip_id: equip_id.map(String::from),
            rule_id,
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn list_enabled_bindings(&self) -> Vec<FddBinding> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .cmd_tx
            .send(FddCmd::ListEnabledBindings { reply: reply_tx });
        reply_rx.await.unwrap_or_default()
    }

    // ---- Fault lifecycle ----

    pub async fn create_fault(
        &self,
        binding_id: i64,
        rule_id: i64,
        equip_id: &str,
        rule_name: &str,
        severity: &FddSeverity,
        point_snapshot: &str,
        guidance: &str,
    ) -> Result<i64, FddStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(FddCmd::CreateFault {
                binding_id,
                rule_id,
                equip_id: equip_id.to_string(),
                rule_name: rule_name.to_string(),
                severity: severity.key().to_string(),
                point_snapshot: point_snapshot.to_string(),
                guidance: guidance.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| FddStoreError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| FddStoreError::ChannelClosed)?;
        if result.is_ok() {
            self.version_tx.send_modify(|v| *v += 1);
        }
        result
    }

    pub async fn acknowledge_fault(&self, id: i64) -> Result<(), FddStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(FddCmd::AcknowledgeFault {
                id,
                reply: reply_tx,
            })
            .map_err(|_| FddStoreError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| FddStoreError::ChannelClosed)?;
        if result.is_ok() {
            self.version_tx.send_modify(|v| *v += 1);
        }
        result
    }

    pub async fn acknowledge_all(&self) -> u32 {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(FddCmd::AcknowledgeAll { reply: reply_tx });
        let count = reply_rx.await.unwrap_or(0);
        if count > 0 {
            self.version_tx.send_modify(|v| *v += 1);
        }
        count
    }

    pub async fn clear_fault(&self, id: i64) -> Result<(), FddStoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(FddCmd::ClearFault {
                id,
                reply: reply_tx,
            })
            .map_err(|_| FddStoreError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| FddStoreError::ChannelClosed)?;
        if result.is_ok() {
            self.version_tx.send_modify(|v| *v += 1);
        }
        result
    }

    pub async fn get_active_faults(&self) -> Vec<FddFault> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .cmd_tx
            .send(FddCmd::GetActiveFaults { reply: reply_tx });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn get_fault_by_binding(&self, binding_id: i64) -> Option<FddFault> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(FddCmd::GetFaultByBinding {
            binding_id,
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or(None)
    }

    pub async fn query_history(&self, query: FddHistoryQuery) -> Vec<FddFaultEvent> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(FddCmd::QueryHistory {
            query,
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn count_active_faults(&self) -> u32 {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .cmd_tx
            .send(FddCmd::CountActiveFaults { reply: reply_tx });
        reply_rx.await.unwrap_or(0)
    }

    // ---- Builtin seeding ----

    pub async fn seed_builtin_rules(&self, rules: Vec<FddRule>) -> u32 {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(FddCmd::SeedBuiltinRules {
            rules,
            reply: reply_tx,
        });
        let count = reply_rx.await.unwrap_or(0);
        if count > 0 {
            self.version_tx.send_modify(|v| *v += 1);
        }
        count
    }
}

// ----------------------------------------------------------------
// SQLite schema & thread
// ----------------------------------------------------------------

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    label: "initial FDD schema",
    sql: "
CREATE TABLE IF NOT EXISTS fdd_rule (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    name                TEXT NOT NULL,
    description         TEXT NOT NULL DEFAULT '',
    category            TEXT NOT NULL DEFAULT 'general',
    equip_tags          TEXT NOT NULL DEFAULT '[]',
    severity            TEXT NOT NULL DEFAULT 'warning',
    condition           TEXT NOT NULL,
    guidance            TEXT NOT NULL DEFAULT '',
    builtin             INTEGER NOT NULL DEFAULT 0,
    builtin_id          TEXT,
    enabled             INTEGER NOT NULL DEFAULT 1,
    confirmation_count  INTEGER NOT NULL DEFAULT 3,
    created_ms          INTEGER NOT NULL,
    updated_ms          INTEGER NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_fdd_rule_builtin ON fdd_rule(builtin_id) WHERE builtin_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS fdd_binding (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    rule_id          INTEGER NOT NULL REFERENCES fdd_rule(id) ON DELETE CASCADE,
    equip_id         TEXT NOT NULL,
    enabled          INTEGER NOT NULL DEFAULT 1,
    config_overrides TEXT,
    created_ms       INTEGER NOT NULL,
    UNIQUE(rule_id, equip_id)
);
CREATE INDEX IF NOT EXISTS idx_fdd_binding_equip ON fdd_binding(equip_id);

CREATE TABLE IF NOT EXISTS fdd_fault (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    binding_id      INTEGER NOT NULL REFERENCES fdd_binding(id) ON DELETE CASCADE,
    rule_id         INTEGER NOT NULL,
    equip_id        TEXT NOT NULL,
    rule_name       TEXT NOT NULL DEFAULT '',
    severity        TEXT NOT NULL,
    state           TEXT NOT NULL DEFAULT 'active',
    detected_ms     INTEGER NOT NULL,
    ack_ms          INTEGER,
    point_snapshot  TEXT NOT NULL DEFAULT '{}',
    guidance        TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_fdd_fault_state ON fdd_fault(state);
CREATE INDEX IF NOT EXISTS idx_fdd_fault_equip ON fdd_fault(equip_id);

CREATE TABLE IF NOT EXISTS fdd_fault_history (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    fault_id     INTEGER NOT NULL,
    binding_id   INTEGER NOT NULL,
    rule_id      INTEGER NOT NULL,
    equip_id     TEXT NOT NULL,
    severity     TEXT NOT NULL,
    from_state   TEXT NOT NULL,
    to_state     TEXT NOT NULL,
    timestamp_ms INTEGER NOT NULL,
    note         TEXT
);
CREATE INDEX IF NOT EXISTS idx_fdd_history_time ON fdd_fault_history(timestamp_ms);
",
}];

fn run_sqlite_thread(db_path: &Path, mut cmd_rx: mpsc::UnboundedReceiver<FddCmd>) {
    let conn = rusqlite::Connection::open(db_path).expect("failed to open FDD database");
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;",
    )
    .expect("failed to set pragmas");
    run_migrations(&conn, "fdd", MIGRATIONS).expect("fdd: schema migration failed");

    while let Some(cmd) = cmd_rx.blocking_recv() {
        match cmd {
            // ---- Rules ----
            FddCmd::CreateRule {
                name,
                description,
                category,
                equip_tags,
                severity,
                condition,
                guidance,
                builtin,
                builtin_id,
                enabled,
                confirmation_count,
                reply,
            } => {
                let _ = reply.send(create_rule_db(
                    &conn,
                    &name,
                    &description,
                    &category,
                    &equip_tags,
                    &severity,
                    &condition,
                    &guidance,
                    builtin,
                    builtin_id.as_deref(),
                    enabled,
                    confirmation_count,
                ));
            }
            FddCmd::UpdateRule {
                id,
                name,
                description,
                category,
                equip_tags,
                severity,
                condition,
                guidance,
                enabled,
                confirmation_count,
                reply,
            } => {
                let _ = reply.send(update_rule_db(
                    &conn,
                    id,
                    &name,
                    &description,
                    &category,
                    &equip_tags,
                    &severity,
                    &condition,
                    &guidance,
                    enabled,
                    confirmation_count,
                ));
            }
            FddCmd::DeleteRule { id, reply } => {
                let _ = reply.send(delete_rule_db(&conn, id));
            }
            FddCmd::ListRules { reply } => {
                let _ = reply.send(list_rules_db(&conn));
            }
            FddCmd::GetRule { id, reply } => {
                let _ = reply.send(get_rule_db(&conn, id));
            }

            // ---- Bindings ----
            FddCmd::CreateBinding {
                rule_id,
                equip_id,
                enabled,
                config_overrides,
                reply,
            } => {
                let _ = reply.send(create_binding_db(
                    &conn,
                    rule_id,
                    &equip_id,
                    enabled,
                    config_overrides.as_deref(),
                ));
            }
            FddCmd::UpdateBinding {
                id,
                enabled,
                config_overrides,
                reply,
            } => {
                let _ = reply.send(update_binding_db(
                    &conn,
                    id,
                    enabled,
                    config_overrides.as_deref(),
                ));
            }
            FddCmd::DeleteBinding { id, reply } => {
                let _ = reply.send(delete_binding_db(&conn, id));
            }
            FddCmd::ListBindings {
                equip_id,
                rule_id,
                reply,
            } => {
                let _ = reply.send(list_bindings_db(&conn, equip_id.as_deref(), rule_id));
            }
            FddCmd::ListEnabledBindings { reply } => {
                let _ = reply.send(list_enabled_bindings_db(&conn));
            }

            // ---- Faults ----
            FddCmd::CreateFault {
                binding_id,
                rule_id,
                equip_id,
                rule_name,
                severity,
                point_snapshot,
                guidance,
                reply,
            } => {
                let _ = reply.send(create_fault_db(
                    &conn,
                    binding_id,
                    rule_id,
                    &equip_id,
                    &rule_name,
                    &severity,
                    &point_snapshot,
                    &guidance,
                ));
            }
            FddCmd::AcknowledgeFault { id, reply } => {
                let _ = reply.send(acknowledge_fault_db(&conn, id));
            }
            FddCmd::AcknowledgeAll { reply } => {
                let _ = reply.send(acknowledge_all_db(&conn));
            }
            FddCmd::ClearFault { id, reply } => {
                let _ = reply.send(clear_fault_db(&conn, id));
            }
            FddCmd::GetActiveFaults { reply } => {
                let _ = reply.send(get_active_faults_db(&conn));
            }
            FddCmd::GetFaultByBinding { binding_id, reply } => {
                let _ = reply.send(get_fault_by_binding_db(&conn, binding_id));
            }
            FddCmd::QueryHistory { query, reply } => {
                let _ = reply.send(query_history_db(&conn, &query));
            }
            FddCmd::CountActiveFaults { reply } => {
                let _ = reply.send(count_active_faults_db(&conn));
            }

            // ---- Seeding ----
            FddCmd::SeedBuiltinRules { rules, reply } => {
                let _ = reply.send(seed_builtin_rules_db(&conn, &rules));
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

// ---- Rules ----

fn create_rule_db(
    conn: &rusqlite::Connection,
    name: &str,
    description: &str,
    category: &str,
    equip_tags: &str,
    severity: &str,
    condition: &str,
    guidance: &str,
    builtin: bool,
    builtin_id: Option<&str>,
    enabled: bool,
    confirmation_count: u16,
) -> Result<i64, FddStoreError> {
    let ts = now_ms();
    conn.execute(
        "INSERT INTO fdd_rule (name, description, category, equip_tags, severity, condition, guidance, builtin, builtin_id, enabled, confirmation_count, created_ms, updated_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        rusqlite::params![
            name,
            description,
            category,
            equip_tags,
            severity,
            condition,
            guidance,
            builtin as i32,
            builtin_id,
            enabled as i32,
            confirmation_count as i32,
            ts,
            ts,
        ],
    )
    .map_err(|e| FddStoreError::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

fn update_rule_db(
    conn: &rusqlite::Connection,
    id: i64,
    name: &str,
    description: &str,
    category: &str,
    equip_tags: &str,
    severity: &str,
    condition: &str,
    guidance: &str,
    enabled: bool,
    confirmation_count: u16,
) -> Result<(), FddStoreError> {
    let ts = now_ms();
    let rows = conn
        .execute(
            "UPDATE fdd_rule SET name = ?1, description = ?2, category = ?3, equip_tags = ?4, severity = ?5, condition = ?6, guidance = ?7, enabled = ?8, confirmation_count = ?9, updated_ms = ?10 WHERE id = ?11",
            rusqlite::params![
                name,
                description,
                category,
                equip_tags,
                severity,
                condition,
                guidance,
                enabled as i32,
                confirmation_count as i32,
                ts,
                id,
            ],
        )
        .map_err(|e| FddStoreError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(FddStoreError::NotFound);
    }
    Ok(())
}

fn delete_rule_db(conn: &rusqlite::Connection, id: i64) -> Result<(), FddStoreError> {
    let rows = conn
        .execute("DELETE FROM fdd_rule WHERE id = ?1", rusqlite::params![id])
        .map_err(|e| FddStoreError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(FddStoreError::NotFound);
    }
    Ok(())
}

fn parse_rule_row(row: &rusqlite::Row) -> rusqlite::Result<FddRule> {
    let category_str: String = row.get(3)?;
    let severity_str: String = row.get(5)?;
    let condition_str: String = row.get(6)?;
    let equip_tags_str: String = row.get(4)?;
    let builtin_int: i32 = row.get(8)?;
    let enabled_int: i32 = row.get(10)?;
    let confirmation_count_int: i32 = row.get(11)?;

    Ok(FddRule {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        category: FddCategory::from_key(&category_str).unwrap_or(FddCategory::General),
        equip_tags: serde_json::from_str(&equip_tags_str).unwrap_or_default(),
        severity: FddSeverity::from_key(&severity_str).unwrap_or(FddSeverity::Warning),
        condition: serde_json::from_str(&condition_str).unwrap_or(FddCondition::Custom {
            script: String::new(),
        }),
        guidance: row.get(7)?,
        builtin: builtin_int != 0,
        builtin_id: row.get(9)?,
        enabled: enabled_int != 0,
        confirmation_count: confirmation_count_int as u16,
        created_ms: row.get(12)?,
        updated_ms: row.get(13)?,
    })
}

fn list_rules_db(conn: &rusqlite::Connection) -> Vec<FddRule> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT id, name, description, category, equip_tags, severity, condition, guidance, builtin, builtin_id, enabled, confirmation_count, created_ms, updated_ms FROM fdd_rule ORDER BY id",
        )
        .unwrap();
    let rows = stmt.query_map([], parse_rule_row).unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

fn get_rule_db(conn: &rusqlite::Connection, id: i64) -> Option<FddRule> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT id, name, description, category, equip_tags, severity, condition, guidance, builtin, builtin_id, enabled, confirmation_count, created_ms, updated_ms FROM fdd_rule WHERE id = ?1",
        )
        .unwrap();
    stmt.query_row(rusqlite::params![id], parse_rule_row).ok()
}

// ---- Bindings ----

fn create_binding_db(
    conn: &rusqlite::Connection,
    rule_id: i64,
    equip_id: &str,
    enabled: bool,
    config_overrides: Option<&str>,
) -> Result<i64, FddStoreError> {
    let ts = now_ms();
    conn.execute(
        "INSERT INTO fdd_binding (rule_id, equip_id, enabled, config_overrides, created_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![rule_id, equip_id, enabled as i32, config_overrides, ts],
    )
    .map_err(|e| FddStoreError::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

fn update_binding_db(
    conn: &rusqlite::Connection,
    id: i64,
    enabled: bool,
    config_overrides: Option<&str>,
) -> Result<(), FddStoreError> {
    let rows = conn
        .execute(
            "UPDATE fdd_binding SET enabled = ?1, config_overrides = ?2 WHERE id = ?3",
            rusqlite::params![enabled as i32, config_overrides, id],
        )
        .map_err(|e| FddStoreError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(FddStoreError::NotFound);
    }
    Ok(())
}

fn delete_binding_db(conn: &rusqlite::Connection, id: i64) -> Result<(), FddStoreError> {
    let rows = conn
        .execute(
            "DELETE FROM fdd_binding WHERE id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| FddStoreError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(FddStoreError::NotFound);
    }
    Ok(())
}

fn parse_binding_row(row: &rusqlite::Row) -> rusqlite::Result<FddBinding> {
    let enabled_int: i32 = row.get(3)?;
    Ok(FddBinding {
        id: row.get(0)?,
        rule_id: row.get(1)?,
        equip_id: row.get(2)?,
        enabled: enabled_int != 0,
        config_overrides: row.get(4)?,
        created_ms: row.get(5)?,
    })
}

fn list_bindings_db(
    conn: &rusqlite::Connection,
    equip_id: Option<&str>,
    rule_id: Option<i64>,
) -> Vec<FddBinding> {
    match (equip_id, rule_id) {
        (Some(eid), Some(rid)) => {
            let mut stmt = conn
                .prepare_cached(
                    "SELECT id, rule_id, equip_id, enabled, config_overrides, created_ms FROM fdd_binding WHERE equip_id = ?1 AND rule_id = ?2 ORDER BY id",
                )
                .unwrap();
            let rows = stmt
                .query_map(rusqlite::params![eid, rid], parse_binding_row)
                .unwrap();
            rows.filter_map(|r| r.ok()).collect()
        }
        (Some(eid), None) => {
            let mut stmt = conn
                .prepare_cached(
                    "SELECT id, rule_id, equip_id, enabled, config_overrides, created_ms FROM fdd_binding WHERE equip_id = ?1 ORDER BY id",
                )
                .unwrap();
            let rows = stmt
                .query_map(rusqlite::params![eid], parse_binding_row)
                .unwrap();
            rows.filter_map(|r| r.ok()).collect()
        }
        (None, Some(rid)) => {
            let mut stmt = conn
                .prepare_cached(
                    "SELECT id, rule_id, equip_id, enabled, config_overrides, created_ms FROM fdd_binding WHERE rule_id = ?1 ORDER BY id",
                )
                .unwrap();
            let rows = stmt
                .query_map(rusqlite::params![rid], parse_binding_row)
                .unwrap();
            rows.filter_map(|r| r.ok()).collect()
        }
        (None, None) => {
            let mut stmt = conn
                .prepare_cached(
                    "SELECT id, rule_id, equip_id, enabled, config_overrides, created_ms FROM fdd_binding ORDER BY id",
                )
                .unwrap();
            let rows = stmt.query_map([], parse_binding_row).unwrap();
            rows.filter_map(|r| r.ok()).collect()
        }
    }
}

fn list_enabled_bindings_db(conn: &rusqlite::Connection) -> Vec<FddBinding> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT b.id, b.rule_id, b.equip_id, b.enabled, b.config_overrides, b.created_ms
             FROM fdd_binding b
             INNER JOIN fdd_rule r ON r.id = b.rule_id
             WHERE b.enabled = 1 AND r.enabled = 1
             ORDER BY b.id",
        )
        .unwrap();
    let rows = stmt.query_map([], parse_binding_row).unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

// ---- Faults ----

fn create_fault_db(
    conn: &rusqlite::Connection,
    binding_id: i64,
    rule_id: i64,
    equip_id: &str,
    rule_name: &str,
    severity: &str,
    point_snapshot: &str,
    guidance: &str,
) -> Result<i64, FddStoreError> {
    let ts = now_ms();
    conn.execute(
        "INSERT INTO fdd_fault (binding_id, rule_id, equip_id, rule_name, severity, state, detected_ms, point_snapshot, guidance)
         VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6, ?7, ?8)",
        rusqlite::params![binding_id, rule_id, equip_id, rule_name, severity, ts, point_snapshot, guidance],
    )
    .map_err(|e| FddStoreError::Db(e.to_string()))?;
    let fault_id = conn.last_insert_rowid();

    // History entry: normal → active
    conn.execute(
        "INSERT INTO fdd_fault_history (fault_id, binding_id, rule_id, equip_id, severity, from_state, to_state, timestamp_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, 'normal', 'active', ?6)",
        rusqlite::params![fault_id, binding_id, rule_id, equip_id, severity, ts],
    )
    .map_err(|e| FddStoreError::Db(e.to_string()))?;

    Ok(fault_id)
}

fn acknowledge_fault_db(conn: &rusqlite::Connection, id: i64) -> Result<(), FddStoreError> {
    let ts = now_ms();

    // Read current state
    let current_state: String = conn
        .query_row(
            "SELECT state FROM fdd_fault WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get(0),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => FddStoreError::NotFound,
            other => FddStoreError::Db(other.to_string()),
        })?;

    let rows = conn
        .execute(
            "UPDATE fdd_fault SET state = 'acknowledged', ack_ms = ?1 WHERE id = ?2 AND state = 'active'",
            rusqlite::params![ts, id],
        )
        .map_err(|e| FddStoreError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(FddStoreError::NotFound);
    }

    // Read fault details for history
    let (binding_id, rule_id, equip_id, severity): (i64, i64, String, String) = conn
        .query_row(
            "SELECT binding_id, rule_id, equip_id, severity FROM fdd_fault WHERE id = ?1",
            rusqlite::params![id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(|e| FddStoreError::Db(e.to_string()))?;

    conn.execute(
        "INSERT INTO fdd_fault_history (fault_id, binding_id, rule_id, equip_id, severity, from_state, to_state, timestamp_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'acknowledged', ?7)",
        rusqlite::params![id, binding_id, rule_id, equip_id, severity, current_state, ts],
    )
    .map_err(|e| FddStoreError::Db(e.to_string()))?;

    Ok(())
}

fn acknowledge_all_db(conn: &rusqlite::Connection) -> u32 {
    let ts = now_ms();

    // Collect all active faults first for history entries
    let mut stmt = conn
        .prepare_cached(
            "SELECT id, binding_id, rule_id, equip_id, severity FROM fdd_fault WHERE state = 'active'",
        )
        .unwrap();
    let faults: Vec<(i64, i64, i64, String, String)> = stmt
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

    if faults.is_empty() {
        return 0;
    }

    let count = faults.len() as u32;

    // Update all active to acknowledged
    let _ = conn.execute(
        "UPDATE fdd_fault SET state = 'acknowledged', ack_ms = ?1 WHERE state = 'active'",
        rusqlite::params![ts],
    );

    // Insert history entries
    for (fault_id, binding_id, rule_id, equip_id, severity) in &faults {
        let _ = conn.execute(
            "INSERT INTO fdd_fault_history (fault_id, binding_id, rule_id, equip_id, severity, from_state, to_state, timestamp_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, 'active', 'acknowledged', ?6)",
            rusqlite::params![fault_id, binding_id, rule_id, equip_id, severity, ts],
        );
    }

    count
}

fn clear_fault_db(conn: &rusqlite::Connection, id: i64) -> Result<(), FddStoreError> {
    let ts = now_ms();

    // Read fault details for history before deleting
    let (binding_id, rule_id, equip_id, severity, state): (i64, i64, String, String, String) = conn
        .query_row(
            "SELECT binding_id, rule_id, equip_id, severity, state FROM fdd_fault WHERE id = ?1",
            rusqlite::params![id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => FddStoreError::NotFound,
            other => FddStoreError::Db(other.to_string()),
        })?;

    // History entry: current_state → cleared
    conn.execute(
        "INSERT INTO fdd_fault_history (fault_id, binding_id, rule_id, equip_id, severity, from_state, to_state, timestamp_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'cleared', ?7)",
        rusqlite::params![id, binding_id, rule_id, equip_id, severity, state, ts],
    )
    .map_err(|e| FddStoreError::Db(e.to_string()))?;

    // Remove from active faults table
    let rows = conn
        .execute("DELETE FROM fdd_fault WHERE id = ?1", rusqlite::params![id])
        .map_err(|e| FddStoreError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(FddStoreError::NotFound);
    }
    Ok(())
}

fn parse_fault_row(row: &rusqlite::Row) -> rusqlite::Result<FddFault> {
    let severity_str: String = row.get(5)?;
    let state_str: String = row.get(6)?;
    Ok(FddFault {
        id: row.get(0)?,
        binding_id: row.get(1)?,
        rule_id: row.get(2)?,
        equip_id: row.get(3)?,
        rule_name: row.get(4)?,
        severity: FddSeverity::from_key(&severity_str).unwrap_or(FddSeverity::Warning),
        state: FddFaultState::from_key(&state_str).unwrap_or(FddFaultState::Active),
        detected_ms: row.get(7)?,
        ack_ms: row.get(8)?,
        point_snapshot: row.get(9)?,
        guidance: row.get(10)?,
    })
}

fn get_active_faults_db(conn: &rusqlite::Connection) -> Vec<FddFault> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT id, binding_id, rule_id, equip_id, rule_name, severity, state, detected_ms, ack_ms, point_snapshot, guidance
             FROM fdd_fault WHERE state IN ('active', 'acknowledged') ORDER BY detected_ms DESC",
        )
        .unwrap();
    let rows = stmt.query_map([], parse_fault_row).unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

fn get_fault_by_binding_db(conn: &rusqlite::Connection, binding_id: i64) -> Option<FddFault> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT id, binding_id, rule_id, equip_id, rule_name, severity, state, detected_ms, ack_ms, point_snapshot, guidance
             FROM fdd_fault WHERE binding_id = ?1 AND state IN ('active', 'acknowledged') LIMIT 1",
        )
        .unwrap();
    stmt.query_row(rusqlite::params![binding_id], parse_fault_row)
        .ok()
}

fn count_active_faults_db(conn: &rusqlite::Connection) -> u32 {
    conn.query_row(
        "SELECT COUNT(*) FROM fdd_fault WHERE state IN ('active', 'acknowledged')",
        [],
        |row| row.get::<_, u32>(0),
    )
    .unwrap_or(0)
}

fn query_history_db(conn: &rusqlite::Connection, query: &FddHistoryQuery) -> Vec<FddFaultEvent> {
    let mut sql = String::from(
        "SELECT id, fault_id, binding_id, rule_id, equip_id, severity, from_state, to_state, timestamp_ms, note
         FROM fdd_fault_history WHERE 1=1",
    );
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1;

    if let Some(ref eid) = query.equip_id {
        sql.push_str(&format!(" AND equip_id = ?{idx}"));
        params.push(Box::new(eid.clone()));
        idx += 1;
    }
    if let Some(rid) = query.rule_id {
        sql.push_str(&format!(" AND rule_id = ?{idx}"));
        params.push(Box::new(rid));
        idx += 1;
    }
    if let Some(ref sev) = query.severity {
        sql.push_str(&format!(" AND severity = ?{idx}"));
        params.push(Box::new(sev.clone()));
        idx += 1;
    }
    if let Some(start) = query.start_ms {
        sql.push_str(&format!(" AND timestamp_ms >= ?{idx}"));
        params.push(Box::new(start));
        idx += 1;
    }
    if let Some(end) = query.end_ms {
        sql.push_str(&format!(" AND timestamp_ms < ?{idx}"));
        params.push(Box::new(end));
        idx += 1;
    }

    sql.push_str(" ORDER BY timestamp_ms DESC");

    if let Some(limit) = query.limit {
        sql.push_str(&format!(" LIMIT ?{idx}"));
        params.push(Box::new(limit as i64));
    }

    let mut stmt = conn.prepare_cached(&sql).unwrap();
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows = stmt
        .query_map(param_refs.as_slice(), |row| {
            Ok(FddFaultEvent {
                id: row.get(0)?,
                fault_id: row.get(1)?,
                binding_id: row.get(2)?,
                rule_id: row.get(3)?,
                equip_id: row.get(4)?,
                severity: row.get(5)?,
                from_state: row.get(6)?,
                to_state: row.get(7)?,
                timestamp_ms: row.get(8)?,
                note: row.get(9)?,
            })
        })
        .unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

// ---- Builtin seeding ----

fn seed_builtin_rules_db(conn: &rusqlite::Connection, rules: &[FddRule]) -> u32 {
    let mut count = 0u32;
    let ts = now_ms();

    for rule in rules {
        let builtin_id = match &rule.builtin_id {
            Some(id) => id.as_str(),
            None => continue, // skip rules without a builtin_id
        };

        let condition_json = serde_json::to_string(&rule.condition).unwrap_or_else(|_| "{}".into());
        let equip_tags_json =
            serde_json::to_string(&rule.equip_tags).unwrap_or_else(|_| "[]".into());

        // Check if rule exists and if it was user-modified (updated_ms != created_ms)
        let existing: Option<(i64, i64, i64)> = conn
            .query_row(
                "SELECT id, created_ms, updated_ms FROM fdd_rule WHERE builtin_id = ?1",
                rusqlite::params![builtin_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .ok();

        match existing {
            None => {
                // Insert new
                let _ = conn.execute(
                    "INSERT INTO fdd_rule (name, description, category, equip_tags, severity, condition, guidance, builtin, builtin_id, enabled, confirmation_count, created_ms, updated_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8, ?9, ?10, ?11, ?12)",
                    rusqlite::params![
                        rule.name,
                        rule.description,
                        rule.category.key(),
                        equip_tags_json,
                        rule.severity.key(),
                        condition_json,
                        rule.guidance,
                        builtin_id,
                        rule.enabled as i32,
                        rule.confirmation_count as i32,
                        ts,
                        ts,
                    ],
                );
                count += 1;
            }
            Some((_id, created_ms, updated_ms)) => {
                // Only update if user hasn't modified the rule (timestamps match)
                if created_ms == updated_ms {
                    let _ = conn.execute(
                        "UPDATE fdd_rule SET name = ?1, description = ?2, category = ?3, equip_tags = ?4, severity = ?5, condition = ?6, guidance = ?7, confirmation_count = ?8, updated_ms = ?9 WHERE builtin_id = ?10",
                        rusqlite::params![
                            rule.name,
                            rule.description,
                            rule.category.key(),
                            equip_tags_json,
                            rule.severity.key(),
                            condition_json,
                            rule.guidance,
                            rule.confirmation_count as i32,
                            ts,
                            builtin_id,
                        ],
                    );
                    count += 1;
                }
                // If user modified, skip to preserve their changes.
            }
        }
    }
    count
}

// ----------------------------------------------------------------
// Constructor
// ----------------------------------------------------------------

pub fn start_fdd_store_with_path(db_path: &Path) -> FddStore {
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (version_tx, _version_rx) = watch::channel(0u64);
    let path_clone = db_path.to_path_buf();
    std::thread::Builder::new()
        .name("fdd-sqlite".into())
        .spawn(move || run_sqlite_thread(&path_clone, cmd_rx))
        .expect("failed to spawn FDD SQLite thread");
    FddStore { cmd_tx, version_tx }
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fdd::model::{CompareOp, FddCondition, PointPredicate, PointRef, PredicateValue};

    fn test_store() -> FddStore {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let tmp = std::env::temp_dir().join(format!(
            "fdd_test_{}_{}_{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            n,
        ));
        start_fdd_store_with_path(&tmp)
    }

    fn test_condition() -> FddCondition {
        FddCondition::AllTrue {
            predicates: vec![PointPredicate {
                point_ref: PointRef {
                    tags: vec!["supply".into(), "air".into(), "temp".into()],
                    role: "SAT".into(),
                },
                op: CompareOp::Gt,
                value: PredicateValue::Literal(90.0),
                tolerance: 1.0,
            }],
            delay_secs: 60,
            applicable_states: None,
        }
    }

    #[tokio::test]
    async fn test_rule_crud() {
        let store = test_store();
        let cond = test_condition();

        // Create
        let id = store
            .create_rule(
                "Test Rule",
                "A test rule",
                &FddCategory::Ahu,
                &["ahu".into(), "equip".into()],
                &FddSeverity::Warning,
                &cond,
                "Check the supply air temperature",
                false,
                None,
                true,
                3,
            )
            .await
            .unwrap();
        assert!(id > 0);

        // List
        let rules = store.list_rules().await;
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].name, "Test Rule");
        assert_eq!(rules[0].category, FddCategory::Ahu);
        assert_eq!(rules[0].severity, FddSeverity::Warning);
        assert!(rules[0].enabled);
        assert_eq!(rules[0].confirmation_count, 3);
        assert_eq!(rules[0].equip_tags, vec!["ahu", "equip"]);

        // Get
        let rule = store.get_rule(id).await.unwrap();
        assert_eq!(rule.name, "Test Rule");

        // Update
        store
            .update_rule(
                id,
                "Updated Rule",
                "Updated description",
                &FddCategory::SensorValidation,
                &["sensor".into()],
                &FddSeverity::Critical,
                &cond,
                "Updated guidance",
                false,
                5,
            )
            .await
            .unwrap();
        let rule = store.get_rule(id).await.unwrap();
        assert_eq!(rule.name, "Updated Rule");
        assert_eq!(rule.category, FddCategory::SensorValidation);
        assert_eq!(rule.severity, FddSeverity::Critical);
        assert!(!rule.enabled);
        assert_eq!(rule.confirmation_count, 5);

        // Delete
        store.delete_rule(id).await.unwrap();
        assert!(store.list_rules().await.is_empty());
    }

    #[tokio::test]
    async fn test_binding_crud() {
        let store = test_store();
        let cond = test_condition();

        let rule_id = store
            .create_rule(
                "R1",
                "",
                &FddCategory::Ahu,
                &[],
                &FddSeverity::Warning,
                &cond,
                "",
                false,
                None,
                true,
                3,
            )
            .await
            .unwrap();

        // Create
        let bid = store
            .create_binding(rule_id, "equip-001", true, None)
            .await
            .unwrap();
        assert!(bid > 0);

        let bid2 = store
            .create_binding(
                rule_id,
                "equip-002",
                false,
                Some(r#"{"temp_tolerance":2.0}"#),
            )
            .await
            .unwrap();
        assert!(bid2 > 0);

        // List all
        let all = store.list_bindings(None, None).await;
        assert_eq!(all.len(), 2);

        // List by equip_id
        let by_equip = store.list_bindings(Some("equip-001"), None).await;
        assert_eq!(by_equip.len(), 1);
        assert_eq!(by_equip[0].equip_id, "equip-001");

        // List by rule_id
        let by_rule = store.list_bindings(None, Some(rule_id)).await;
        assert_eq!(by_rule.len(), 2);

        // Update
        store
            .update_binding(bid, false, Some(r#"{"temp_tolerance":3.0}"#))
            .await
            .unwrap();
        let updated = store.list_bindings(Some("equip-001"), None).await;
        assert!(!updated[0].enabled);
        assert_eq!(
            updated[0].config_overrides.as_deref(),
            Some(r#"{"temp_tolerance":3.0}"#)
        );

        // Delete
        store.delete_binding(bid).await.unwrap();
        assert_eq!(store.list_bindings(None, None).await.len(), 1);
    }

    #[tokio::test]
    async fn test_fault_lifecycle() {
        let store = test_store();
        let cond = test_condition();

        let rule_id = store
            .create_rule(
                "Fault Rule",
                "",
                &FddCategory::Ahu,
                &[],
                &FddSeverity::Critical,
                &cond,
                "Check equipment",
                false,
                None,
                true,
                3,
            )
            .await
            .unwrap();

        let binding_id = store
            .create_binding(rule_id, "ahu-01", true, None)
            .await
            .unwrap();

        // Create fault
        let fault_id = store
            .create_fault(
                binding_id,
                rule_id,
                "ahu-01",
                "Fault Rule",
                &FddSeverity::Critical,
                r#"{"sat":95.0}"#,
                "Check equipment",
            )
            .await
            .unwrap();
        assert!(fault_id > 0);

        // Get active faults
        let active = store.get_active_faults().await;
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].state, FddFaultState::Active);

        // Count
        assert_eq!(store.count_active_faults().await, 1);

        // Get by binding
        let by_binding = store.get_fault_by_binding(binding_id).await;
        assert!(by_binding.is_some());
        assert_eq!(by_binding.unwrap().id, fault_id);

        // Acknowledge
        store.acknowledge_fault(fault_id).await.unwrap();
        let active = store.get_active_faults().await;
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].state, FddFaultState::Acknowledged);
        assert!(active[0].ack_ms.is_some());

        // Clear
        store.clear_fault(fault_id).await.unwrap();
        assert!(store.get_active_faults().await.is_empty());
        assert_eq!(store.count_active_faults().await, 0);

        // Check history
        let history = store.query_history(FddHistoryQuery::default()).await;
        assert_eq!(history.len(), 3); // normal→active, active→acknowledged, acknowledged→cleared
        assert_eq!(history[0].to_state, "cleared");
        assert_eq!(history[1].to_state, "acknowledged");
        assert_eq!(history[2].to_state, "active");
    }

    #[tokio::test]
    async fn test_seed_builtin_rules() {
        let store = test_store();
        let cond = test_condition();

        let builtin_rules = vec![
            FddRule {
                id: 0,
                name: "Builtin Rule 1".into(),
                description: "First builtin".into(),
                category: FddCategory::Ahu,
                equip_tags: vec!["ahu".into()],
                severity: FddSeverity::Warning,
                condition: cond.clone(),
                guidance: "Check AHU".into(),
                builtin: true,
                builtin_id: Some("ahu-01".into()),
                enabled: true,
                confirmation_count: 3,
                created_ms: 0,
                updated_ms: 0,
            },
            FddRule {
                id: 0,
                name: "Builtin Rule 2".into(),
                description: "Second builtin".into(),
                category: FddCategory::Vav,
                equip_tags: vec!["vav".into()],
                severity: FddSeverity::Critical,
                condition: cond.clone(),
                guidance: "Check VAV".into(),
                builtin: true,
                builtin_id: Some("vav-01".into()),
                enabled: true,
                confirmation_count: 5,
                created_ms: 0,
                updated_ms: 0,
            },
        ];

        // First seed
        let inserted = store.seed_builtin_rules(builtin_rules.clone()).await;
        assert_eq!(inserted, 2);

        let rules = store.list_rules().await;
        assert_eq!(rules.len(), 2);

        // Seed again — same rules, not user-modified, so they get updated (count = 2)
        let re_seeded = store.seed_builtin_rules(builtin_rules).await;
        assert_eq!(re_seeded, 2);

        // Still only 2 rules (no duplicates)
        let rules = store.list_rules().await;
        assert_eq!(rules.len(), 2);
    }

    #[tokio::test]
    async fn test_enabled_bindings() {
        let store = test_store();
        let cond = test_condition();

        // Create enabled rule + enabled binding
        let r1 = store
            .create_rule(
                "Enabled Rule",
                "",
                &FddCategory::General,
                &[],
                &FddSeverity::Warning,
                &cond,
                "",
                false,
                None,
                true,
                3,
            )
            .await
            .unwrap();

        // Create disabled rule
        let r2 = store
            .create_rule(
                "Disabled Rule",
                "",
                &FddCategory::General,
                &[],
                &FddSeverity::Warning,
                &cond,
                "",
                false,
                None,
                false,
                3,
            )
            .await
            .unwrap();

        // Enabled binding on enabled rule
        store
            .create_binding(r1, "equip-a", true, None)
            .await
            .unwrap();

        // Disabled binding on enabled rule
        store
            .create_binding(r1, "equip-b", false, None)
            .await
            .unwrap();

        // Enabled binding on disabled rule
        store
            .create_binding(r2, "equip-c", true, None)
            .await
            .unwrap();

        let enabled = store.list_enabled_bindings().await;
        // Only equip-a: both binding and rule are enabled
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].equip_id, "equip-a");
    }
}
