use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::{mpsc, oneshot};

use crate::config::profile::PointValue;
use crate::store::migration::{run_migrations, Migration};

use super::types::*;

// ----------------------------------------------------------------
// Commands sent to the SQLite thread
// ----------------------------------------------------------------

pub(super) enum ScheduleCmd {
    // Schedule CRUD
    CreateSchedule {
        name: String,
        description: String,
        value_type: ScheduleValueType,
        default_value: PointValue,
        weekly: WeeklySchedule,
        reply: oneshot::Sender<Result<ScheduleId, ScheduleError>>,
    },
    UpdateSchedule {
        id: ScheduleId,
        name: String,
        description: String,
        default_value: PointValue,
        enabled: bool,
        weekly: WeeklySchedule,
        reply: oneshot::Sender<Result<(), ScheduleError>>,
    },
    DeleteSchedule {
        id: ScheduleId,
        reply: oneshot::Sender<Result<(), ScheduleError>>,
    },
    ListSchedules {
        reply: oneshot::Sender<Vec<Schedule>>,
    },
    GetSchedule {
        id: ScheduleId,
        reply: oneshot::Sender<Option<Schedule>>,
    },

    // Exception groups
    CreateExceptionGroup {
        name: String,
        description: String,
        entries: Vec<DateSpec>,
        reply: oneshot::Sender<Result<ExceptionGroupId, ScheduleError>>,
    },
    UpdateExceptionGroup {
        id: ExceptionGroupId,
        name: String,
        description: String,
        entries: Vec<DateSpec>,
        reply: oneshot::Sender<Result<(), ScheduleError>>,
    },
    DeleteExceptionGroup {
        id: ExceptionGroupId,
        reply: oneshot::Sender<Result<(), ScheduleError>>,
    },
    ListExceptionGroups {
        reply: oneshot::Sender<Vec<ExceptionGroup>>,
    },

    // Schedule exceptions
    AddException {
        schedule_id: ScheduleId,
        group_id: Option<ExceptionGroupId>,
        name: String,
        date_spec: DateSpec,
        slots: DaySlots,
        use_default: bool,
        reply: oneshot::Sender<Result<i64, ScheduleError>>,
    },
    RemoveException {
        id: i64,
        reply: oneshot::Sender<Result<(), ScheduleError>>,
    },
    ListExceptions {
        schedule_id: ScheduleId,
        reply: oneshot::Sender<Vec<ScheduleException>>,
    },

    // Assignments
    CreateAssignment {
        schedule_id: ScheduleId,
        device_id: String,
        point_id: String,
        priority: i32,
        reply: oneshot::Sender<Result<AssignmentId, ScheduleError>>,
    },
    CreateAssignmentsBatch {
        schedule_id: ScheduleId,
        entries: Vec<(String, String)>,
        priority: i32,
        reply: oneshot::Sender<Result<Vec<AssignmentId>, ScheduleError>>,
    },
    DeleteAssignment {
        id: AssignmentId,
        reply: oneshot::Sender<Result<(), ScheduleError>>,
    },
    ListAssignmentsForSchedule {
        schedule_id: ScheduleId,
        reply: oneshot::Sender<Vec<ScheduleAssignment>>,
    },
    GetAssignmentsForPoint {
        device_id: String,
        point_id: String,
        reply: oneshot::Sender<Vec<ScheduleAssignment>>,
    },

    // Log
    InsertLog {
        assignment_id: AssignmentId,
        device_id: String,
        point_id: String,
        value_json: String,
        reason: String,
        timestamp_ms: i64,
    },
    QueryLog {
        device_id: String,
        point_id: String,
        limit: i64,
        reply: oneshot::Sender<Vec<ScheduleLogEntry>>,
    },

    // Engine queries (all schedules + assignments + exceptions in one shot)
    LoadEngineData {
        reply: oneshot::Sender<EngineData>,
    },
}

/// All data the engine needs, loaded in a single DB roundtrip.
#[derive(Debug, Clone)]
pub(super) struct EngineData {
    pub(super) schedules: Vec<Schedule>,
    pub(super) assignments: Vec<ScheduleAssignment>,
    pub(super) exceptions: Vec<ScheduleException>,
}

// ----------------------------------------------------------------
// SQLite schema & thread
// ----------------------------------------------------------------

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        label: "initial schedule schema",
        sql: "
CREATE TABLE IF NOT EXISTS schedule (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    name          TEXT NOT NULL UNIQUE,
    description   TEXT NOT NULL DEFAULT '',
    value_type    TEXT NOT NULL,
    default_value TEXT NOT NULL,
    enabled       INTEGER NOT NULL DEFAULT 1,
    weekly_json   TEXT NOT NULL,
    created_ms    INTEGER NOT NULL,
    updated_ms    INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS exception_group (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    name          TEXT NOT NULL UNIQUE,
    description   TEXT NOT NULL DEFAULT '',
    entries_json  TEXT NOT NULL,
    created_ms    INTEGER NOT NULL,
    updated_ms    INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS schedule_exception (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    schedule_id     INTEGER NOT NULL REFERENCES schedule(id) ON DELETE CASCADE,
    group_id        INTEGER REFERENCES exception_group(id) ON DELETE SET NULL,
    name            TEXT NOT NULL DEFAULT '',
    date_spec_json  TEXT NOT NULL,
    slots_json      TEXT NOT NULL,
    use_default     INTEGER NOT NULL DEFAULT 0,
    created_ms      INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_sched_exc ON schedule_exception(schedule_id);

CREATE TABLE IF NOT EXISTS schedule_assignment (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    schedule_id   INTEGER NOT NULL REFERENCES schedule(id) ON DELETE CASCADE,
    device_id     TEXT NOT NULL,
    point_id      TEXT NOT NULL,
    priority      INTEGER NOT NULL DEFAULT 12,
    enabled       INTEGER NOT NULL DEFAULT 1,
    created_ms    INTEGER NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_assign_unique ON schedule_assignment(schedule_id, device_id, point_id);
CREATE INDEX IF NOT EXISTS idx_assign_point ON schedule_assignment(device_id, point_id);

CREATE TABLE IF NOT EXISTS schedule_log (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    assignment_id   INTEGER NOT NULL,
    device_id       TEXT NOT NULL,
    point_id        TEXT NOT NULL,
    value_json      TEXT NOT NULL,
    reason          TEXT NOT NULL,
    timestamp_ms    INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_sched_log_time ON schedule_log(timestamp_ms);
",
    },
];

pub(super) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

pub(super) fn run_sqlite_thread(
    db_path: &std::path::Path,
    mut cmd_rx: mpsc::UnboundedReceiver<ScheduleCmd>,
) {
    let conn = rusqlite::Connection::open(db_path).expect("failed to open schedules database");
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;",
    )
    .expect("failed to set pragmas");
    run_migrations(&conn, "schedules", MIGRATIONS).expect("schedules: schema migration failed");

    while let Some(cmd) = cmd_rx.blocking_recv() {
        match cmd {
            ScheduleCmd::CreateSchedule {
                name,
                description,
                value_type,
                default_value,
                weekly,
                reply,
            } => {
                let _ = reply.send(create_schedule_db(
                    &conn,
                    &name,
                    &description,
                    &value_type,
                    &default_value,
                    &weekly,
                ));
            }
            ScheduleCmd::UpdateSchedule {
                id,
                name,
                description,
                default_value,
                enabled,
                weekly,
                reply,
            } => {
                let _ = reply.send(update_schedule_db(
                    &conn,
                    id,
                    &name,
                    &description,
                    &default_value,
                    enabled,
                    &weekly,
                ));
            }
            ScheduleCmd::DeleteSchedule { id, reply } => {
                let _ = reply.send(delete_schedule_db(&conn, id));
            }
            ScheduleCmd::ListSchedules { reply } => {
                let _ = reply.send(list_schedules_db(&conn));
            }
            ScheduleCmd::GetSchedule { id, reply } => {
                let _ = reply.send(get_schedule_db(&conn, id));
            }
            ScheduleCmd::CreateExceptionGroup {
                name,
                description,
                entries,
                reply,
            } => {
                let _ = reply.send(create_exception_group_db(
                    &conn,
                    &name,
                    &description,
                    &entries,
                ));
            }
            ScheduleCmd::UpdateExceptionGroup {
                id,
                name,
                description,
                entries,
                reply,
            } => {
                let _ = reply.send(update_exception_group_db(
                    &conn,
                    id,
                    &name,
                    &description,
                    &entries,
                ));
            }
            ScheduleCmd::DeleteExceptionGroup { id, reply } => {
                let _ = reply.send(delete_exception_group_db(&conn, id));
            }
            ScheduleCmd::ListExceptionGroups { reply } => {
                let _ = reply.send(list_exception_groups_db(&conn));
            }
            ScheduleCmd::AddException {
                schedule_id,
                group_id,
                name,
                date_spec,
                slots,
                use_default,
                reply,
            } => {
                let _ = reply.send(add_exception_db(
                    &conn,
                    schedule_id,
                    group_id,
                    &name,
                    &date_spec,
                    &slots,
                    use_default,
                ));
            }
            ScheduleCmd::RemoveException { id, reply } => {
                let _ = reply.send(remove_exception_db(&conn, id));
            }
            ScheduleCmd::ListExceptions { schedule_id, reply } => {
                let _ = reply.send(list_exceptions_db(&conn, schedule_id));
            }
            ScheduleCmd::CreateAssignment {
                schedule_id,
                device_id,
                point_id,
                priority,
                reply,
            } => {
                let _ = reply.send(create_assignment_db(
                    &conn,
                    schedule_id,
                    &device_id,
                    &point_id,
                    priority,
                ));
            }
            ScheduleCmd::CreateAssignmentsBatch {
                schedule_id,
                entries,
                priority,
                reply,
            } => {
                let _ = reply.send(create_assignments_batch_db(
                    &conn,
                    schedule_id,
                    &entries,
                    priority,
                ));
            }
            ScheduleCmd::DeleteAssignment { id, reply } => {
                let _ = reply.send(delete_assignment_db(&conn, id));
            }
            ScheduleCmd::ListAssignmentsForSchedule { schedule_id, reply } => {
                let _ = reply.send(list_assignments_for_schedule_db(&conn, schedule_id));
            }
            ScheduleCmd::GetAssignmentsForPoint {
                device_id,
                point_id,
                reply,
            } => {
                let _ = reply.send(get_assignments_for_point_db(&conn, &device_id, &point_id));
            }
            ScheduleCmd::InsertLog {
                assignment_id,
                device_id,
                point_id,
                value_json,
                reason,
                timestamp_ms,
            } => {
                let _ = conn.execute(
                    "INSERT INTO schedule_log (assignment_id, device_id, point_id, value_json, reason, timestamp_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![assignment_id, device_id, point_id, value_json, reason, timestamp_ms],
                );
            }
            ScheduleCmd::QueryLog {
                device_id,
                point_id,
                limit,
                reply,
            } => {
                let _ = reply.send(query_log_db(&conn, &device_id, &point_id, limit));
            }
            ScheduleCmd::LoadEngineData { reply } => {
                let data = EngineData {
                    schedules: list_schedules_db(&conn),
                    assignments: list_all_assignments_db(&conn),
                    exceptions: list_all_exceptions_db(&conn),
                };
                let _ = reply.send(data);
            }
        }
    }
}

// ----------------------------------------------------------------
// DB helpers
// ----------------------------------------------------------------

fn create_schedule_db(
    conn: &rusqlite::Connection,
    name: &str,
    description: &str,
    value_type: &ScheduleValueType,
    default_value: &PointValue,
    weekly: &WeeklySchedule,
) -> Result<ScheduleId, ScheduleError> {
    let ts = now_ms();
    let default_json =
        serde_json::to_string(default_value).map_err(|e| ScheduleError::Db(e.to_string()))?;
    let weekly_json =
        serde_json::to_string(weekly).map_err(|e| ScheduleError::Db(e.to_string()))?;

    conn.execute(
        "INSERT INTO schedule (name, description, value_type, default_value, enabled, weekly_json, created_ms, updated_ms)
         VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?7)",
        rusqlite::params![
            name,
            description,
            value_type.as_str(),
            default_json,
            weekly_json,
            ts,
            ts,
        ],
    )
    .map_err(|e| ScheduleError::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

fn update_schedule_db(
    conn: &rusqlite::Connection,
    id: ScheduleId,
    name: &str,
    description: &str,
    default_value: &PointValue,
    enabled: bool,
    weekly: &WeeklySchedule,
) -> Result<(), ScheduleError> {
    let ts = now_ms();
    let default_json =
        serde_json::to_string(default_value).map_err(|e| ScheduleError::Db(e.to_string()))?;
    let weekly_json =
        serde_json::to_string(weekly).map_err(|e| ScheduleError::Db(e.to_string()))?;

    let rows = conn
        .execute(
            "UPDATE schedule SET name = ?1, description = ?2, default_value = ?3, enabled = ?4, weekly_json = ?5, updated_ms = ?6 WHERE id = ?7",
            rusqlite::params![name, description, default_json, enabled as i32, weekly_json, ts, id],
        )
        .map_err(|e| ScheduleError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(ScheduleError::NotFound);
    }
    Ok(())
}

fn delete_schedule_db(conn: &rusqlite::Connection, id: ScheduleId) -> Result<(), ScheduleError> {
    // CASCADE handles assignments and exceptions
    let rows = conn
        .execute("DELETE FROM schedule WHERE id = ?1", rusqlite::params![id])
        .map_err(|e| ScheduleError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(ScheduleError::NotFound);
    }
    Ok(())
}

fn parse_schedule_row(row: &rusqlite::Row) -> rusqlite::Result<Schedule> {
    let id: i64 = row.get(0)?;
    let name: String = row.get(1)?;
    let description: String = row.get(2)?;
    let value_type_str: String = row.get(3)?;
    let default_json: String = row.get(4)?;
    let enabled: bool = row.get::<_, i32>(5)? != 0;
    let weekly_json: String = row.get(6)?;
    let created_ms: i64 = row.get(7)?;
    let updated_ms: i64 = row.get(8)?;

    let value_type =
        ScheduleValueType::from_str(&value_type_str).unwrap_or(ScheduleValueType::Analog);
    let default_value: PointValue =
        serde_json::from_str(&default_json).unwrap_or(PointValue::Float(0.0));
    let weekly: WeeklySchedule =
        serde_json::from_str(&weekly_json).unwrap_or_else(|_| empty_weekly());

    Ok(Schedule {
        id,
        name,
        description,
        value_type,
        default_value,
        enabled,
        weekly,
        created_ms,
        updated_ms,
    })
}

fn list_schedules_db(conn: &rusqlite::Connection) -> Vec<Schedule> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT id, name, description, value_type, default_value, enabled, weekly_json, created_ms, updated_ms FROM schedule ORDER BY id",
        )
        .unwrap();
    let rows = stmt.query_map([], parse_schedule_row).unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

fn get_schedule_db(conn: &rusqlite::Connection, id: ScheduleId) -> Option<Schedule> {
    conn.query_row(
        "SELECT id, name, description, value_type, default_value, enabled, weekly_json, created_ms, updated_ms FROM schedule WHERE id = ?1",
        rusqlite::params![id],
        parse_schedule_row,
    )
    .ok()
}

fn create_exception_group_db(
    conn: &rusqlite::Connection,
    name: &str,
    description: &str,
    entries: &[DateSpec],
) -> Result<ExceptionGroupId, ScheduleError> {
    let ts = now_ms();
    let entries_json =
        serde_json::to_string(entries).map_err(|e| ScheduleError::Db(e.to_string()))?;
    conn.execute(
        "INSERT INTO exception_group (name, description, entries_json, created_ms, updated_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![name, description, entries_json, ts, ts],
    )
    .map_err(|e| ScheduleError::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

fn update_exception_group_db(
    conn: &rusqlite::Connection,
    id: ExceptionGroupId,
    name: &str,
    description: &str,
    entries: &[DateSpec],
) -> Result<(), ScheduleError> {
    let ts = now_ms();
    let entries_json =
        serde_json::to_string(entries).map_err(|e| ScheduleError::Db(e.to_string()))?;
    let rows = conn
        .execute(
            "UPDATE exception_group SET name = ?1, description = ?2, entries_json = ?3, updated_ms = ?4 WHERE id = ?5",
            rusqlite::params![name, description, entries_json, ts, id],
        )
        .map_err(|e| ScheduleError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(ScheduleError::NotFound);
    }
    Ok(())
}

fn delete_exception_group_db(
    conn: &rusqlite::Connection,
    id: ExceptionGroupId,
) -> Result<(), ScheduleError> {
    let rows = conn
        .execute(
            "DELETE FROM exception_group WHERE id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| ScheduleError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(ScheduleError::NotFound);
    }
    Ok(())
}

fn list_exception_groups_db(conn: &rusqlite::Connection) -> Vec<ExceptionGroup> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT id, name, description, entries_json, created_ms, updated_ms FROM exception_group ORDER BY id",
        )
        .unwrap();
    let rows = stmt
        .query_map([], |row| {
            let entries_json: String = row.get(3)?;
            let entries: Vec<DateSpec> = serde_json::from_str(&entries_json).unwrap_or_default();
            Ok(ExceptionGroup {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                entries,
                created_ms: row.get(4)?,
                updated_ms: row.get(5)?,
            })
        })
        .unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

fn add_exception_db(
    conn: &rusqlite::Connection,
    schedule_id: ScheduleId,
    group_id: Option<ExceptionGroupId>,
    name: &str,
    date_spec: &DateSpec,
    slots: &DaySlots,
    use_default: bool,
) -> Result<i64, ScheduleError> {
    let ts = now_ms();
    let date_spec_json =
        serde_json::to_string(date_spec).map_err(|e| ScheduleError::Db(e.to_string()))?;
    let slots_json = serde_json::to_string(slots).map_err(|e| ScheduleError::Db(e.to_string()))?;
    conn.execute(
        "INSERT INTO schedule_exception (schedule_id, group_id, name, date_spec_json, slots_json, use_default, created_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            schedule_id,
            group_id,
            name,
            date_spec_json,
            slots_json,
            use_default as i32,
            ts,
        ],
    )
    .map_err(|e| ScheduleError::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

fn remove_exception_db(conn: &rusqlite::Connection, id: i64) -> Result<(), ScheduleError> {
    let rows = conn
        .execute(
            "DELETE FROM schedule_exception WHERE id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| ScheduleError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(ScheduleError::NotFound);
    }
    Ok(())
}

fn parse_exception_row(row: &rusqlite::Row) -> rusqlite::Result<ScheduleException> {
    let date_spec_json: String = row.get(4)?;
    let slots_json: String = row.get(5)?;
    Ok(ScheduleException {
        id: row.get(0)?,
        schedule_id: row.get(1)?,
        group_id: row.get(2)?,
        name: row.get(3)?,
        date_spec: serde_json::from_str(&date_spec_json)
            .unwrap_or(DateSpec::Fixed { month: 1, day: 1 }),
        slots: serde_json::from_str(&slots_json).unwrap_or_default(),
        use_default: row.get::<_, i32>(6)? != 0,
        created_ms: row.get(7)?,
    })
}

fn list_exceptions_db(
    conn: &rusqlite::Connection,
    schedule_id: ScheduleId,
) -> Vec<ScheduleException> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT id, schedule_id, group_id, name, date_spec_json, slots_json, use_default, created_ms
             FROM schedule_exception WHERE schedule_id = ?1 ORDER BY id",
        )
        .unwrap();
    let rows = stmt
        .query_map(rusqlite::params![schedule_id], parse_exception_row)
        .unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

fn list_all_exceptions_db(conn: &rusqlite::Connection) -> Vec<ScheduleException> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT id, schedule_id, group_id, name, date_spec_json, slots_json, use_default, created_ms
             FROM schedule_exception ORDER BY id",
        )
        .unwrap();
    let rows = stmt.query_map([], parse_exception_row).unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

fn create_assignment_db(
    conn: &rusqlite::Connection,
    schedule_id: ScheduleId,
    device_id: &str,
    point_id: &str,
    priority: i32,
) -> Result<AssignmentId, ScheduleError> {
    let ts = now_ms();
    conn.execute(
        "INSERT INTO schedule_assignment (schedule_id, device_id, point_id, priority, enabled, created_ms)
         VALUES (?1, ?2, ?3, ?4, 1, ?5)",
        rusqlite::params![schedule_id, device_id, point_id, priority, ts],
    )
    .map_err(|e| ScheduleError::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

fn create_assignments_batch_db(
    conn: &rusqlite::Connection,
    schedule_id: ScheduleId,
    entries: &[(String, String)],
    priority: i32,
) -> Result<Vec<AssignmentId>, ScheduleError> {
    let ts = now_ms();
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| ScheduleError::Db(e.to_string()))?;
    let mut ids = Vec::with_capacity(entries.len());
    {
        let mut stmt = tx
            .prepare_cached(
                "INSERT OR IGNORE INTO schedule_assignment (schedule_id, device_id, point_id, priority, enabled, created_ms)
                 VALUES (?1, ?2, ?3, ?4, 1, ?5)",
            )
            .map_err(|e| ScheduleError::Db(e.to_string()))?;
        for (device_id, point_id) in entries {
            let rows = stmt
                .execute(rusqlite::params![
                    schedule_id,
                    device_id,
                    point_id,
                    priority,
                    ts
                ])
                .map_err(|e| ScheduleError::Db(e.to_string()))?;
            if rows > 0 {
                ids.push(tx.last_insert_rowid());
            }
        }
    }
    tx.commit().map_err(|e| ScheduleError::Db(e.to_string()))?;
    Ok(ids)
}

fn delete_assignment_db(
    conn: &rusqlite::Connection,
    id: AssignmentId,
) -> Result<(), ScheduleError> {
    let rows = conn
        .execute(
            "DELETE FROM schedule_assignment WHERE id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| ScheduleError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(ScheduleError::NotFound);
    }
    Ok(())
}

fn parse_assignment_row(row: &rusqlite::Row) -> rusqlite::Result<ScheduleAssignment> {
    Ok(ScheduleAssignment {
        id: row.get(0)?,
        schedule_id: row.get(1)?,
        device_id: row.get(2)?,
        point_id: row.get(3)?,
        priority: row.get(4)?,
        enabled: row.get::<_, i32>(5)? != 0,
        created_ms: row.get(6)?,
    })
}

fn list_assignments_for_schedule_db(
    conn: &rusqlite::Connection,
    schedule_id: ScheduleId,
) -> Vec<ScheduleAssignment> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT id, schedule_id, device_id, point_id, priority, enabled, created_ms
             FROM schedule_assignment WHERE schedule_id = ?1 ORDER BY id",
        )
        .unwrap();
    let rows = stmt
        .query_map(rusqlite::params![schedule_id], parse_assignment_row)
        .unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

fn get_assignments_for_point_db(
    conn: &rusqlite::Connection,
    device_id: &str,
    point_id: &str,
) -> Vec<ScheduleAssignment> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT id, schedule_id, device_id, point_id, priority, enabled, created_ms
             FROM schedule_assignment WHERE device_id = ?1 AND point_id = ?2 ORDER BY priority",
        )
        .unwrap();
    let rows = stmt
        .query_map(rusqlite::params![device_id, point_id], parse_assignment_row)
        .unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

fn list_all_assignments_db(conn: &rusqlite::Connection) -> Vec<ScheduleAssignment> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT id, schedule_id, device_id, point_id, priority, enabled, created_ms
             FROM schedule_assignment ORDER BY id",
        )
        .unwrap();
    let rows = stmt.query_map([], parse_assignment_row).unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

fn query_log_db(
    conn: &rusqlite::Connection,
    device_id: &str,
    point_id: &str,
    limit: i64,
) -> Vec<ScheduleLogEntry> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT id, assignment_id, device_id, point_id, value_json, reason, timestamp_ms
             FROM schedule_log WHERE device_id = ?1 AND point_id = ?2
             ORDER BY timestamp_ms DESC LIMIT ?3",
        )
        .unwrap();
    let rows = stmt
        .query_map(rusqlite::params![device_id, point_id, limit], |row| {
            Ok(ScheduleLogEntry {
                id: row.get(0)?,
                assignment_id: row.get(1)?,
                device_id: row.get(2)?,
                point_id: row.get(3)?,
                value_json: row.get(4)?,
                reason: row.get(5)?,
                timestamp_ms: row.get(6)?,
            })
        })
        .unwrap();
    rows.filter_map(|r| r.ok()).collect()
}
