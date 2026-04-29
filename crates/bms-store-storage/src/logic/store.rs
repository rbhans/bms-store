use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::{mpsc, oneshot, watch};

use crate::event::bus::EventBus;

use super::model::{ExecutionResult, Program, ProgramId};

// ── Error ──

#[derive(Debug, thiserror::Error)]
pub enum ProgramError {
    #[error("program not found: {0}")]
    NotFound(String),
    #[error("already exists: {0}")]
    AlreadyExists(String),
    #[error("database error: {0}")]
    Db(String),
}

// ── Execution log entry ──

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExecutionLogEntry {
    pub program_id: ProgramId,
    pub executed_ms: i64,
    pub success: bool,
    pub error: Option<String>,
    pub duration_us: u64,
    pub outputs_written: usize,
}

// ── Commands ──

enum ProgramCmd {
    Create {
        program: Program,
        reply: oneshot::Sender<Result<(), ProgramError>>,
    },
    Update {
        program: Program,
        reply: oneshot::Sender<Result<(), ProgramError>>,
    },
    Delete {
        id: ProgramId,
        reply: oneshot::Sender<Result<(), ProgramError>>,
    },
    Get {
        id: ProgramId,
        reply: oneshot::Sender<Result<Program, ProgramError>>,
    },
    List {
        enabled_only: bool,
        reply: oneshot::Sender<Vec<Program>>,
    },
    SetEnabled {
        id: ProgramId,
        enabled: bool,
        reply: oneshot::Sender<Result<(), ProgramError>>,
    },
    LogExecution {
        result: ExecutionResult,
    },
    GetExecutionLog {
        program_id: ProgramId,
        limit: usize,
        reply: oneshot::Sender<Vec<ExecutionLogEntry>>,
    },
}

// ── Store handle ──

#[derive(Clone)]
pub struct ProgramStore {
    cmd_tx: mpsc::UnboundedSender<ProgramCmd>,
    version_tx: watch::Sender<u64>,
    version_rx: watch::Receiver<u64>,
    _event_bus: Option<EventBus>,
}

impl ProgramStore {
    pub fn with_event_bus(mut self, bus: EventBus) -> Self {
        self._event_bus = Some(bus);
        self
    }

    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.version_rx.clone()
    }

    fn bump_version(&self) {
        let v = *self.version_tx.borrow() + 1;
        let _ = self.version_tx.send(v);
    }

    pub async fn create(&self, program: Program) -> Result<(), ProgramError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(ProgramCmd::Create {
                program,
                reply: reply_tx,
            })
            .map_err(|_| ProgramError::Db("channel closed".into()))?;
        let result = reply_rx
            .await
            .map_err(|_| ProgramError::Db("recv".into()))?;
        if result.is_ok() {
            self.bump_version();
        }
        result
    }

    pub async fn update(&self, program: Program) -> Result<(), ProgramError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(ProgramCmd::Update {
                program,
                reply: reply_tx,
            })
            .map_err(|_| ProgramError::Db("channel closed".into()))?;
        let result = reply_rx
            .await
            .map_err(|_| ProgramError::Db("recv".into()))?;
        if result.is_ok() {
            self.bump_version();
        }
        result
    }

    pub async fn delete(&self, id: &str) -> Result<(), ProgramError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(ProgramCmd::Delete {
                id: id.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| ProgramError::Db("channel closed".into()))?;
        let result = reply_rx
            .await
            .map_err(|_| ProgramError::Db("recv".into()))?;
        if result.is_ok() {
            self.bump_version();
        }
        result
    }

    pub async fn get(&self, id: &str) -> Result<Program, ProgramError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(ProgramCmd::Get {
                id: id.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| ProgramError::Db("channel closed".into()))?;
        reply_rx
            .await
            .map_err(|_| ProgramError::Db("recv".into()))?
    }

    pub async fn list(&self, enabled_only: bool) -> Vec<Program> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(ProgramCmd::List {
            enabled_only,
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), ProgramError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(ProgramCmd::SetEnabled {
                id: id.to_string(),
                enabled,
                reply: reply_tx,
            })
            .map_err(|_| ProgramError::Db("channel closed".into()))?;
        let result = reply_rx
            .await
            .map_err(|_| ProgramError::Db("recv".into()))?;
        if result.is_ok() {
            self.bump_version();
        }
        result
    }

    pub fn log_execution(&self, result: ExecutionResult) {
        let _ = self.cmd_tx.send(ProgramCmd::LogExecution { result });
    }

    pub async fn get_execution_log(
        &self,
        program_id: &str,
        limit: usize,
    ) -> Vec<ExecutionLogEntry> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(ProgramCmd::GetExecutionLog {
            program_id: program_id.to_string(),
            limit,
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_default()
    }
}

// ── Start function ──

pub fn start_program_store() -> ProgramStore {
    let path = PathBuf::from("data/programs.db");
    std::fs::create_dir_all("data").ok();
    start_program_store_with_path(&path)
}

pub fn start_program_store_with_path(path: &Path) -> ProgramStore {
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (version_tx, version_rx) = watch::channel(0u64);
    let db_path = path.to_path_buf();

    std::thread::spawn(move || {
        run_sqlite_thread(db_path, cmd_rx);
    });

    ProgramStore {
        cmd_tx,
        version_tx,
        version_rx,
        _event_bus: None,
    }
}

// ── SQLite thread ──

fn run_sqlite_thread(path: PathBuf, mut cmd_rx: mpsc::UnboundedReceiver<ProgramCmd>) {
    let conn = rusqlite::Connection::open(&path).expect("failed to open programs.db");
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
        .ok();

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS program (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            enabled     INTEGER NOT NULL DEFAULT 1,
            trigger_json TEXT NOT NULL,
            graph_json  TEXT NOT NULL,
            rhai_override TEXT,
            created_ms  INTEGER NOT NULL,
            updated_ms  INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS program_execution_log (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            program_id      TEXT NOT NULL REFERENCES program(id) ON DELETE CASCADE,
            executed_ms     INTEGER NOT NULL,
            success         INTEGER NOT NULL,
            error           TEXT,
            duration_us     INTEGER NOT NULL,
            outputs_written INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_exec_log_program
            ON program_execution_log(program_id, executed_ms DESC);",
    )
    .expect("failed to create program schema");

    while let Some(cmd) = cmd_rx.blocking_recv() {
        match cmd {
            ProgramCmd::Create { program, reply } => {
                let _ = reply.send(create_db(&conn, &program));
            }
            ProgramCmd::Update { program, reply } => {
                let _ = reply.send(update_db(&conn, &program));
            }
            ProgramCmd::Delete { id, reply } => {
                let _ = reply.send(delete_db(&conn, &id));
            }
            ProgramCmd::Get { id, reply } => {
                let _ = reply.send(get_db(&conn, &id));
            }
            ProgramCmd::List {
                enabled_only,
                reply,
            } => {
                let _ = reply.send(list_db(&conn, enabled_only));
            }
            ProgramCmd::SetEnabled { id, enabled, reply } => {
                let _ = reply.send(set_enabled_db(&conn, &id, enabled));
            }
            ProgramCmd::LogExecution { result } => {
                log_execution_db(&conn, &result);
            }
            ProgramCmd::GetExecutionLog {
                program_id,
                limit,
                reply,
            } => {
                let _ = reply.send(get_execution_log_db(&conn, &program_id, limit));
            }
        }
    }
}

// ── DB operations ──

fn create_db(conn: &rusqlite::Connection, prog: &Program) -> Result<(), ProgramError> {
    let trigger_json = serde_json::to_string(&prog.trigger).unwrap_or_default();
    let graph_json = serde_json::json!({
        "blocks": prog.blocks,
        "wires": prog.wires,
    })
    .to_string();

    conn.execute(
        "INSERT INTO program (id, name, description, enabled, trigger_json, graph_json, rhai_override, created_ms, updated_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            prog.id,
            prog.name,
            prog.description,
            prog.enabled as i32,
            trigger_json,
            graph_json,
            prog.rhai_override,
            prog.created_ms,
            prog.updated_ms,
        ],
    )
    .map_err(|e| {
        if matches!(
            e,
            rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error {
                    code: rusqlite::ffi::ErrorCode::ConstraintViolation,
                    ..
                },
                _
            )
        ) {
            ProgramError::AlreadyExists(prog.id.clone())
        } else {
            ProgramError::Db(e.to_string())
        }
    })?;

    Ok(())
}

fn update_db(conn: &rusqlite::Connection, prog: &Program) -> Result<(), ProgramError> {
    let trigger_json = serde_json::to_string(&prog.trigger).unwrap_or_default();
    let graph_json = serde_json::json!({
        "blocks": prog.blocks,
        "wires": prog.wires,
    })
    .to_string();

    let rows = conn
        .execute(
            "UPDATE program SET name=?2, description=?3, enabled=?4, trigger_json=?5, graph_json=?6, rhai_override=?7, updated_ms=?8
         WHERE id=?1",
            rusqlite::params![
                prog.id,
                prog.name,
                prog.description,
                prog.enabled as i32,
                trigger_json,
                graph_json,
                prog.rhai_override,
                now_ms(),
            ],
        )
        .map_err(|e| ProgramError::Db(e.to_string()))?;

    if rows == 0 {
        return Err(ProgramError::NotFound(prog.id.clone()));
    }
    Ok(())
}

fn delete_db(conn: &rusqlite::Connection, id: &str) -> Result<(), ProgramError> {
    let rows = conn
        .execute("DELETE FROM program WHERE id=?1", rusqlite::params![id])
        .map_err(|e| ProgramError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(ProgramError::NotFound(id.to_string()));
    }
    Ok(())
}

fn get_db(conn: &rusqlite::Connection, id: &str) -> Result<Program, ProgramError> {
    conn.query_row(
        "SELECT id, name, description, enabled, trigger_json, graph_json, rhai_override, created_ms, updated_ms
         FROM program WHERE id=?1",
        rusqlite::params![id],
        row_to_program,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => ProgramError::NotFound(id.to_string()),
        other => ProgramError::Db(other.to_string()),
    })
}

fn list_db(conn: &rusqlite::Connection, enabled_only: bool) -> Vec<Program> {
    let sql = if enabled_only {
        "SELECT id, name, description, enabled, trigger_json, graph_json, rhai_override, created_ms, updated_ms
         FROM program WHERE enabled=1 ORDER BY name"
    } else {
        "SELECT id, name, description, enabled, trigger_json, graph_json, rhai_override, created_ms, updated_ms
         FROM program ORDER BY name"
    };

    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    stmt.query_map([], row_to_program)
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
}

fn set_enabled_db(
    conn: &rusqlite::Connection,
    id: &str,
    enabled: bool,
) -> Result<(), ProgramError> {
    let rows = conn
        .execute(
            "UPDATE program SET enabled=?2, updated_ms=?3 WHERE id=?1",
            rusqlite::params![id, enabled as i32, now_ms()],
        )
        .map_err(|e| ProgramError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(ProgramError::NotFound(id.to_string()));
    }
    Ok(())
}

fn log_execution_db(conn: &rusqlite::Connection, result: &ExecutionResult) {
    let _ = conn.execute(
        "INSERT INTO program_execution_log (program_id, executed_ms, success, error, duration_us, outputs_written)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            result.program_id,
            now_ms(),
            result.success as i32,
            result.error,
            result.duration_us as i64,
            result.outputs_written as i64,
        ],
    );

    // Prune old entries: keep last 100 per program
    let _ = conn.execute(
        "DELETE FROM program_execution_log
         WHERE program_id=?1 AND id NOT IN (
             SELECT id FROM program_execution_log
             WHERE program_id=?1
             ORDER BY executed_ms DESC LIMIT 100
         )",
        rusqlite::params![result.program_id],
    );
}

fn get_execution_log_db(
    conn: &rusqlite::Connection,
    program_id: &str,
    limit: usize,
) -> Vec<ExecutionLogEntry> {
    let mut stmt = match conn.prepare(
        "SELECT program_id, executed_ms, success, error, duration_us, outputs_written
         FROM program_execution_log
         WHERE program_id=?1
         ORDER BY executed_ms DESC
         LIMIT ?2",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    stmt.query_map(rusqlite::params![program_id, limit as i64], |row| {
        Ok(ExecutionLogEntry {
            program_id: row.get(0)?,
            executed_ms: row.get(1)?,
            success: row.get::<_, i32>(2)? != 0,
            error: row.get(3)?,
            duration_us: row.get::<_, i64>(4)? as u64,
            outputs_written: row.get::<_, i64>(5)? as usize,
        })
    })
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

fn row_to_program(row: &rusqlite::Row) -> rusqlite::Result<Program> {
    let trigger_json: String = row.get(4)?;
    let graph_json: String = row.get(5)?;

    let trigger = serde_json::from_str(&trigger_json)
        .unwrap_or(super::model::Trigger::Periodic { interval_ms: 5000 });

    let graph: serde_json::Value =
        serde_json::from_str(&graph_json).unwrap_or(serde_json::json!({"blocks":[],"wires":[]}));
    let blocks = graph
        .get("blocks")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let wires = graph
        .get("wires")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    Ok(Program {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        enabled: row.get::<_, i32>(3)? != 0,
        trigger,
        blocks,
        wires,
        rhai_override: row.get(6)?,
        created_ms: row.get(7)?,
        updated_ms: row.get(8)?,
    })
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::logic::model::*;

    fn sample_program() -> Program {
        Program {
            id: "test-prog".into(),
            name: "Test Program".into(),
            description: "A test".into(),
            enabled: true,
            trigger: Trigger::Periodic { interval_ms: 5000 },
            blocks: vec![Block {
                id: "r1".into(),
                block_type: BlockType::PointRead {
                    node_id: "ahu-1/oat".into(),
                },
                x: 0.0,
                y: 0.0,
                enabled: true,
            }],
            wires: vec![],
            rhai_override: None,
            created_ms: 1000,
            updated_ms: 1000,
        }
    }

    #[tokio::test]
    async fn create_and_get() {
        let path = std::env::temp_dir()
            .join("opencrate_prog_tests")
            .join(format!("test_create_{}.db", std::process::id()));
        std::fs::create_dir_all(path.parent().unwrap()).ok();

        let store = start_program_store_with_path(&path);
        let prog = sample_program();

        store.create(prog.clone()).await.unwrap();

        let fetched = store.get("test-prog").await.unwrap();
        assert_eq!(fetched.id, "test-prog");
        assert_eq!(fetched.name, "Test Program");
        assert!(fetched.enabled);
        assert_eq!(fetched.blocks.len(), 1);

        std::fs::remove_file(&path).ok();
    }

    #[tokio::test]
    async fn update_program() {
        let path = std::env::temp_dir()
            .join("opencrate_prog_tests")
            .join(format!("test_update_{}.db", std::process::id()));
        std::fs::create_dir_all(path.parent().unwrap()).ok();

        let store = start_program_store_with_path(&path);
        let mut prog = sample_program();
        store.create(prog.clone()).await.unwrap();

        prog.name = "Updated Name".into();
        prog.blocks.push(Block {
            id: "w1".into(),
            block_type: BlockType::PointWrite {
                node_id: "ahu-1/out".into(),
                priority: None,
            },
            x: 0.0,
            y: 0.0,
            enabled: true,
        });
        store.update(prog).await.unwrap();

        let fetched = store.get("test-prog").await.unwrap();
        assert_eq!(fetched.name, "Updated Name");
        assert_eq!(fetched.blocks.len(), 2);

        std::fs::remove_file(&path).ok();
    }

    #[tokio::test]
    async fn list_and_enable_disable() {
        let path = std::env::temp_dir()
            .join("opencrate_prog_tests")
            .join(format!("test_list_{}.db", std::process::id()));
        std::fs::create_dir_all(path.parent().unwrap()).ok();

        let store = start_program_store_with_path(&path);

        let mut prog1 = sample_program();
        prog1.id = "prog-1".into();
        store.create(prog1).await.unwrap();

        let mut prog2 = sample_program();
        prog2.id = "prog-2".into();
        prog2.enabled = false;
        store.create(prog2).await.unwrap();

        let all = store.list(false).await;
        assert_eq!(all.len(), 2);

        let enabled = store.list(true).await;
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].id, "prog-1");

        store.set_enabled("prog-2", true).await.unwrap();
        let enabled = store.list(true).await;
        assert_eq!(enabled.len(), 2);

        std::fs::remove_file(&path).ok();
    }

    #[tokio::test]
    async fn delete_program() {
        let path = std::env::temp_dir()
            .join("opencrate_prog_tests")
            .join(format!("test_delete_{}.db", std::process::id()));
        std::fs::create_dir_all(path.parent().unwrap()).ok();

        let store = start_program_store_with_path(&path);
        store.create(sample_program()).await.unwrap();
        store.delete("test-prog").await.unwrap();

        let result = store.get("test-prog").await;
        assert!(result.is_err());

        std::fs::remove_file(&path).ok();
    }

    #[tokio::test]
    async fn execution_log() {
        let path = std::env::temp_dir()
            .join("opencrate_prog_tests")
            .join(format!("test_execlog_{}.db", std::process::id()));
        std::fs::create_dir_all(path.parent().unwrap()).ok();

        let store = start_program_store_with_path(&path);
        store.create(sample_program()).await.unwrap();

        store.log_execution(ExecutionResult {
            program_id: "test-prog".into(),
            success: true,
            error: None,
            duration_us: 150,
            outputs_written: 2,
        });

        // Small delay for the async command to process
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let log = store.get_execution_log("test-prog", 10).await;
        assert_eq!(log.len(), 1);
        assert!(log[0].success);
        assert_eq!(log[0].duration_us, 150);
        assert_eq!(log[0].outputs_written, 2);

        std::fs::remove_file(&path).ok();
    }
}
