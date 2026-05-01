use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot, watch};

use crate::event::bus::{Event, EventBus};

// ----------------------------------------------------------------
// Public types
// ----------------------------------------------------------------

pub type EntityId = String;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    pub id: EntityId,
    pub entity_type: String, // "site", "space", "equip", "point"
    pub dis: String,
    pub parent_id: Option<EntityId>,
    pub tags: HashMap<String, Option<String>>, // tag_name -> value (None = marker)
    pub refs: HashMap<String, EntityId>,       // ref_tag -> target entity
    pub created_ms: i64,
    pub updated_ms: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum EntityError {
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

enum EntityCmd {
    CreateEntity {
        id: EntityId,
        entity_type: String,
        dis: String,
        parent_id: Option<EntityId>,
        tags: Vec<(String, Option<String>)>,
        reply: oneshot::Sender<Result<Entity, EntityError>>,
    },
    UpdateEntity {
        id: EntityId,
        dis: String,
        reply: oneshot::Sender<Result<(), EntityError>>,
    },
    DeleteEntity {
        id: EntityId,
        reply: oneshot::Sender<Result<(), EntityError>>,
    },
    GetEntity {
        id: EntityId,
        reply: oneshot::Sender<Result<Entity, EntityError>>,
    },
    ListEntities {
        entity_type: Option<String>,
        parent_id: Option<String>, // use "__root__" for top-level (parent_id IS NULL)
        reply: oneshot::Sender<Vec<Entity>>,
    },

    // Tag operations
    SetTag {
        entity_id: EntityId,
        tag_name: String,
        tag_value: Option<String>,
        reply: oneshot::Sender<Result<(), EntityError>>,
    },
    SetTags {
        entity_id: EntityId,
        tags: Vec<(String, Option<String>)>,
        reply: oneshot::Sender<Result<(), EntityError>>,
    },
    RemoveTag {
        entity_id: EntityId,
        tag_name: String,
        reply: oneshot::Sender<Result<(), EntityError>>,
    },
    RemoveTags {
        entity_id: EntityId,
        tag_names: Vec<String>,
        reply: oneshot::Sender<Result<(), EntityError>>,
    },

    // Ref operations
    SetRef {
        source_id: EntityId,
        ref_tag: String,
        target_id: EntityId,
        reply: oneshot::Sender<Result<(), EntityError>>,
    },
    RemoveRef {
        source_id: EntityId,
        ref_tag: String,
        reply: oneshot::Sender<Result<(), EntityError>>,
    },
    GetEntitiesByRef {
        ref_tag: String,
        target_id: EntityId,
        reply: oneshot::Sender<Vec<Entity>>,
    },

    // Query
    FindByTag {
        tag_name: String,
        tag_value: Option<String>,
        reply: oneshot::Sender<Vec<Entity>>,
    },
    GetHierarchy {
        root_id: Option<EntityId>,
        reply: oneshot::Sender<Vec<Entity>>,
    },

    /// Run an externally-built SQL `WHERE` fragment over the `entity`
    /// table. The fragment must reference the outer alias `e` and the
    /// `entity_tag` / `entity_ref` join tables. Used by the Haystack
    /// HTTP facade for filter push-down. Params are JSON values that
    /// the SQLite layer maps to native types internally.
    FindBySqlFilter {
        sql: String,
        params: Vec<serde_json::Value>,
        reply: oneshot::Sender<Vec<Entity>>,
    },
}

// ----------------------------------------------------------------
// EntityStore — async handle to the SQLite thread
// ----------------------------------------------------------------

#[derive(Clone)]
pub struct EntityStore {
    cmd_tx: mpsc::UnboundedSender<EntityCmd>,
    version_tx: watch::Sender<u64>,
    version_rx: watch::Receiver<u64>,
    event_bus: Option<EventBus>,
}

impl EntityStore {
    pub fn with_event_bus(mut self, bus: EventBus) -> Self {
        self.event_bus = Some(bus);
        self
    }

    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.version_rx.clone()
    }

    fn bump_version(&self) {
        let v = *self.version_tx.borrow() + 1;
        let _ = self.version_tx.send(v);
    }

    pub async fn create_entity(
        &self,
        id: &str,
        entity_type: &str,
        dis: &str,
        parent_id: Option<&str>,
        tags: Vec<(String, Option<String>)>,
    ) -> Result<Entity, EntityError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(EntityCmd::CreateEntity {
                id: id.to_string(),
                entity_type: entity_type.to_string(),
                dis: dis.to_string(),
                parent_id: parent_id.map(|s| s.to_string()),
                tags,
                reply: reply_tx,
            })
            .map_err(|_| EntityError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| EntityError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_version();
            if let Some(ref bus) = self.event_bus {
                bus.publish(Event::EntityCreated {
                    entity_id: id.to_string(),
                });
            }
        }
        result
    }

    pub async fn update_entity(&self, id: &str, dis: &str) -> Result<(), EntityError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(EntityCmd::UpdateEntity {
                id: id.to_string(),
                dis: dis.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| EntityError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| EntityError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_version();
            if let Some(ref bus) = self.event_bus {
                bus.publish(Event::EntityUpdated {
                    entity_id: id.to_string(),
                });
            }
        }
        result
    }

    pub async fn delete_entity(&self, id: &str) -> Result<(), EntityError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(EntityCmd::DeleteEntity {
                id: id.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| EntityError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| EntityError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_version();
            if let Some(ref bus) = self.event_bus {
                bus.publish(Event::EntityDeleted {
                    entity_id: id.to_string(),
                });
            }
        }
        result
    }

    pub async fn get_entity(&self, id: &str) -> Result<Entity, EntityError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(EntityCmd::GetEntity {
                id: id.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| EntityError::ChannelClosed)?;
        reply_rx.await.map_err(|_| EntityError::ChannelClosed)?
    }

    /// Run a SQL `WHERE` fragment built by an external lowerer (e.g. the
    /// Haystack filter→SQL push-down) and return the matching entities.
    /// The fragment must reference outer alias `e` for the `entity` table.
    /// Params are JSON values mapped to SQLite types internally.
    pub async fn find_by_sql_filter(
        &self,
        sql: String,
        params: Vec<serde_json::Value>,
    ) -> Vec<Entity> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(EntityCmd::FindBySqlFilter {
            sql,
            params,
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn list_entities(
        &self,
        entity_type: Option<&str>,
        parent_id: Option<&str>,
    ) -> Vec<Entity> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(EntityCmd::ListEntities {
            entity_type: entity_type.map(|s| s.to_string()),
            parent_id: parent_id.map(|s| s.to_string()),
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_default()
    }

    // Tag operations

    pub async fn set_tag(
        &self,
        entity_id: &str,
        tag_name: &str,
        tag_value: Option<&str>,
    ) -> Result<(), EntityError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(EntityCmd::SetTag {
                entity_id: entity_id.to_string(),
                tag_name: tag_name.to_string(),
                tag_value: tag_value.map(|s| s.to_string()),
                reply: reply_tx,
            })
            .map_err(|_| EntityError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| EntityError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_version();
        }
        result
    }

    pub async fn set_tags(
        &self,
        entity_id: &str,
        tags: Vec<(String, Option<String>)>,
    ) -> Result<(), EntityError> {
        // Deliverable 5: lightweight tag validation warnings at the write path.
        // Runs without rejecting the write — only warns via tracing so automation
        // (prototype apply, API writes) gets the same signal as the GUI validator.
        emit_tag_warnings(entity_id, &tags);

        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(EntityCmd::SetTags {
                entity_id: entity_id.to_string(),
                tags,
                reply: reply_tx,
            })
            .map_err(|_| EntityError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| EntityError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_version();
        }
        result
    }

    pub async fn remove_tag(&self, entity_id: &str, tag_name: &str) -> Result<(), EntityError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(EntityCmd::RemoveTag {
                entity_id: entity_id.to_string(),
                tag_name: tag_name.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| EntityError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| EntityError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_version();
        }
        result
    }

    pub async fn remove_tags(
        &self,
        entity_id: &str,
        tag_names: Vec<String>,
    ) -> Result<(), EntityError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(EntityCmd::RemoveTags {
                entity_id: entity_id.to_string(),
                tag_names,
                reply: reply_tx,
            })
            .map_err(|_| EntityError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| EntityError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_version();
        }
        result
    }

    // Ref operations

    pub async fn set_ref(
        &self,
        source_id: &str,
        ref_tag: &str,
        target_id: &str,
    ) -> Result<(), EntityError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(EntityCmd::SetRef {
                source_id: source_id.to_string(),
                ref_tag: ref_tag.to_string(),
                target_id: target_id.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| EntityError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| EntityError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_version();
        }
        result
    }

    pub async fn remove_ref(&self, source_id: &str, ref_tag: &str) -> Result<(), EntityError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(EntityCmd::RemoveRef {
                source_id: source_id.to_string(),
                ref_tag: ref_tag.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| EntityError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| EntityError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_version();
        }
        result
    }

    pub async fn get_entities_by_ref(&self, ref_tag: &str, target_id: &str) -> Vec<Entity> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(EntityCmd::GetEntitiesByRef {
            ref_tag: ref_tag.to_string(),
            target_id: target_id.to_string(),
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_default()
    }

    // Query operations

    pub async fn find_by_tag(&self, tag_name: &str, tag_value: Option<&str>) -> Vec<Entity> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(EntityCmd::FindByTag {
            tag_name: tag_name.to_string(),
            tag_value: tag_value.map(|s| s.to_string()),
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn get_hierarchy(&self, root_id: Option<&str>) -> Vec<Entity> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(EntityCmd::GetHierarchy {
            root_id: root_id.map(|s| s.to_string()),
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_default()
    }
}

// ----------------------------------------------------------------
// Schema
// ----------------------------------------------------------------

use super::migration::{run_migrations, Migration};

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    label: "initial entity schema",
    sql: "
CREATE TABLE IF NOT EXISTS entity (
    id          TEXT PRIMARY KEY,
    entity_type TEXT NOT NULL,
    dis         TEXT NOT NULL DEFAULT '',
    parent_id   TEXT REFERENCES entity(id) ON DELETE SET NULL,
    created_ms  INTEGER NOT NULL,
    updated_ms  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS entity_tag (
    entity_id   TEXT NOT NULL REFERENCES entity(id) ON DELETE CASCADE,
    tag_name    TEXT NOT NULL,
    tag_value   TEXT,
    PRIMARY KEY (entity_id, tag_name)
);
CREATE INDEX IF NOT EXISTS idx_entity_tag_name ON entity_tag(tag_name);

CREATE TABLE IF NOT EXISTS entity_ref (
    source_id   TEXT NOT NULL REFERENCES entity(id) ON DELETE CASCADE,
    ref_tag     TEXT NOT NULL,
    target_id   TEXT NOT NULL REFERENCES entity(id) ON DELETE CASCADE,
    PRIMARY KEY (source_id, ref_tag)
);
CREATE INDEX IF NOT EXISTS idx_entity_ref_target ON entity_ref(target_id);
",
}];

// ----------------------------------------------------------------
// Start function
// ----------------------------------------------------------------

pub fn start_entity_store() -> EntityStore {
    start_entity_store_with_path(&PathBuf::from("data/entities.db"))
}

pub fn start_entity_store_with_path(db_path: &Path) -> EntityStore {
    let db_dir = db_path.parent().unwrap_or(Path::new("."));
    if !db_dir.exists() {
        std::fs::create_dir_all(db_dir).expect("failed to create data directory");
    }

    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (version_tx, version_rx) = watch::channel(0u64);

    let path_clone = db_path.to_path_buf();
    std::thread::Builder::new()
        .name("entity-sqlite".into())
        .spawn(move || run_sqlite_thread(&path_clone, cmd_rx))
        .expect("failed to spawn entity SQLite thread");

    EntityStore {
        cmd_tx,
        version_tx,
        version_rx,
        event_bus: None,
    }
}

// ----------------------------------------------------------------
// SQLite thread
// ----------------------------------------------------------------

fn run_sqlite_thread(db_path: &Path, mut cmd_rx: mpsc::UnboundedReceiver<EntityCmd>) {
    let conn = rusqlite::Connection::open(db_path).expect("failed to open entities database");
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;",
    )
    .expect("failed to set pragmas");
    run_migrations(&conn, "entities", MIGRATIONS).expect("entities: schema migration failed");

    while let Some(cmd) = cmd_rx.blocking_recv() {
        match cmd {
            EntityCmd::CreateEntity {
                id,
                entity_type,
                dis,
                parent_id,
                tags,
                reply,
            } => {
                let result =
                    create_entity_db(&conn, &id, &entity_type, &dis, parent_id.as_deref(), &tags);
                let _ = reply.send(result);
            }
            EntityCmd::UpdateEntity { id, dis, reply } => {
                let result = update_entity_db(&conn, &id, &dis);
                let _ = reply.send(result);
            }
            EntityCmd::DeleteEntity { id, reply } => {
                let result = delete_entity_db(&conn, &id);
                let _ = reply.send(result);
            }
            EntityCmd::GetEntity { id, reply } => {
                let result = get_entity_db(&conn, &id);
                let _ = reply.send(result);
            }
            EntityCmd::ListEntities {
                entity_type,
                parent_id,
                reply,
            } => {
                let result = list_entities_db(&conn, entity_type.as_deref(), parent_id.as_deref());
                let _ = reply.send(result);
            }
            EntityCmd::SetTag {
                entity_id,
                tag_name,
                tag_value,
                reply,
            } => {
                let result = set_tag_db(&conn, &entity_id, &tag_name, tag_value.as_deref());
                let _ = reply.send(result);
            }
            EntityCmd::SetTags {
                entity_id,
                tags,
                reply,
            } => {
                let result = set_tags_db(&conn, &entity_id, &tags);
                let _ = reply.send(result);
            }
            EntityCmd::RemoveTag {
                entity_id,
                tag_name,
                reply,
            } => {
                let result = remove_tag_db(&conn, &entity_id, &tag_name);
                let _ = reply.send(result);
            }
            EntityCmd::RemoveTags {
                entity_id,
                tag_names,
                reply,
            } => {
                let result = remove_tags_db(&conn, &entity_id, &tag_names);
                let _ = reply.send(result);
            }
            EntityCmd::SetRef {
                source_id,
                ref_tag,
                target_id,
                reply,
            } => {
                let result = set_ref_db(&conn, &source_id, &ref_tag, &target_id);
                let _ = reply.send(result);
            }
            EntityCmd::RemoveRef {
                source_id,
                ref_tag,
                reply,
            } => {
                let result = remove_ref_db(&conn, &source_id, &ref_tag);
                let _ = reply.send(result);
            }
            EntityCmd::GetEntitiesByRef {
                ref_tag,
                target_id,
                reply,
            } => {
                let result = get_entities_by_ref_db(&conn, &ref_tag, &target_id);
                let _ = reply.send(result);
            }
            EntityCmd::FindByTag {
                tag_name,
                tag_value,
                reply,
            } => {
                let result = find_by_tag_db(&conn, &tag_name, tag_value.as_deref());
                let _ = reply.send(result);
            }
            EntityCmd::GetHierarchy { root_id, reply } => {
                let result = get_hierarchy_db(&conn, root_id.as_deref());
                let _ = reply.send(result);
            }
            EntityCmd::FindBySqlFilter { sql, params, reply } => {
                let result = find_by_sql_filter_db(&conn, &sql, &params);
                let _ = reply.send(result);
            }
        }
    }
}

fn find_by_sql_filter_db(
    conn: &rusqlite::Connection,
    where_clause: &str,
    params: &[serde_json::Value],
) -> Vec<Entity> {
    let full_sql = format!("SELECT e.id FROM entity e WHERE {where_clause} ORDER BY e.dis");
    let mut stmt = match conn.prepare(&full_sql) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    // Map serde_json::Value → rusqlite::types::Value
    let typed: Vec<rusqlite::types::Value> = params
        .iter()
        .map(|v| match v {
            serde_json::Value::String(s) => rusqlite::types::Value::Text(s.clone()),
            serde_json::Value::Bool(b) => rusqlite::types::Value::Integer(if *b { 1 } else { 0 }),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    rusqlite::types::Value::Integer(i)
                } else if let Some(f) = n.as_f64() {
                    rusqlite::types::Value::Real(f)
                } else {
                    rusqlite::types::Value::Null
                }
            }
            serde_json::Value::Null => rusqlite::types::Value::Null,
            other => rusqlite::types::Value::Text(other.to_string()),
        })
        .collect();
    let param_refs: Vec<&dyn rusqlite::types::ToSql> =
        typed.iter().map(|v| v as &dyn rusqlite::types::ToSql).collect();
    let ids: Vec<String> = match stmt.query_map(param_refs.as_slice(), |row| row.get::<_, String>(0)) {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
        Err(_) => return Vec::new(),
    };
    ids.iter()
        .filter_map(|id| get_entity_db(conn, id).ok())
        .collect()
}

// ----------------------------------------------------------------
// Database helper functions
// ----------------------------------------------------------------

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn create_entity_db(
    conn: &rusqlite::Connection,
    id: &str,
    entity_type: &str,
    dis: &str,
    parent_id: Option<&str>,
    tags: &[(String, Option<String>)],
) -> Result<Entity, EntityError> {
    let now = now_ms();
    conn.execute(
        "INSERT INTO entity (id, entity_type, dis, parent_id, created_ms, updated_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![id, entity_type, dis, parent_id, now, now],
    )
    .map_err(|e| EntityError::Db(e.to_string()))?;

    // Insert tags
    for (tag_name, tag_value) in tags {
        conn.execute(
            "INSERT OR REPLACE INTO entity_tag (entity_id, tag_name, tag_value) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, tag_name, tag_value.as_deref()],
        )
        .map_err(|e| EntityError::Db(e.to_string()))?;
    }

    get_entity_db(conn, id)
}

fn update_entity_db(conn: &rusqlite::Connection, id: &str, dis: &str) -> Result<(), EntityError> {
    let now = now_ms();
    let rows = conn
        .execute(
            "UPDATE entity SET dis = ?1, updated_ms = ?2 WHERE id = ?3",
            rusqlite::params![dis, now, id],
        )
        .map_err(|e| EntityError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(EntityError::NotFound);
    }
    Ok(())
}

fn delete_entity_db(conn: &rusqlite::Connection, id: &str) -> Result<(), EntityError> {
    // CASCADE handles entity_tag and entity_ref cleanup
    let rows = conn
        .execute("DELETE FROM entity WHERE id = ?1", rusqlite::params![id])
        .map_err(|e| EntityError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(EntityError::NotFound);
    }
    Ok(())
}

fn get_entity_db(conn: &rusqlite::Connection, id: &str) -> Result<Entity, EntityError> {
    let mut stmt = conn
        .prepare("SELECT id, entity_type, dis, parent_id, created_ms, updated_ms FROM entity WHERE id = ?1")
        .map_err(|e| EntityError::Db(e.to_string()))?;

    let entity = stmt
        .query_row(rusqlite::params![id], |row| {
            Ok(Entity {
                id: row.get(0)?,
                entity_type: row.get(1)?,
                dis: row.get(2)?,
                parent_id: row.get(3)?,
                tags: HashMap::new(),
                refs: HashMap::new(),
                created_ms: row.get(4)?,
                updated_ms: row.get(5)?,
            })
        })
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => EntityError::NotFound,
            other => EntityError::Db(other.to_string()),
        })?;

    let mut entity = entity;
    entity.tags = load_tags(conn, id);
    entity.refs = load_refs(conn, id);
    Ok(entity)
}

fn load_tags(conn: &rusqlite::Connection, entity_id: &str) -> HashMap<String, Option<String>> {
    let mut stmt = conn
        .prepare("SELECT tag_name, tag_value FROM entity_tag WHERE entity_id = ?1")
        .unwrap();
    let mut tags = HashMap::new();
    let rows = stmt
        .query_map(rusqlite::params![entity_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })
        .unwrap();
    for (name, value) in rows.flatten() {
        tags.insert(name, value);
    }
    tags
}

fn load_refs(conn: &rusqlite::Connection, source_id: &str) -> HashMap<String, EntityId> {
    let mut stmt = conn
        .prepare("SELECT ref_tag, target_id FROM entity_ref WHERE source_id = ?1")
        .unwrap();
    let mut refs = HashMap::new();
    let rows = stmt
        .query_map(rusqlite::params![source_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .unwrap();
    for (tag, target) in rows.flatten() {
        refs.insert(tag, target);
    }
    refs
}

fn list_entities_db(
    conn: &rusqlite::Connection,
    entity_type: Option<&str>,
    parent_id: Option<&str>,
) -> Vec<Entity> {
    let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) =
        match (entity_type, parent_id) {
            (Some(et), Some("__root__")) => (
                "SELECT id FROM entity WHERE entity_type = ?1 AND parent_id IS NULL ORDER BY dis"
                    .into(),
                vec![Box::new(et.to_string())],
            ),
            (Some(et), Some(pid)) => (
                "SELECT id FROM entity WHERE entity_type = ?1 AND parent_id = ?2 ORDER BY dis"
                    .into(),
                vec![Box::new(et.to_string()), Box::new(pid.to_string())],
            ),
            (None, Some("__root__")) => (
                "SELECT id FROM entity WHERE parent_id IS NULL ORDER BY dis".into(),
                vec![],
            ),
            (None, Some(pid)) => (
                "SELECT id FROM entity WHERE parent_id = ?1 ORDER BY dis".into(),
                vec![Box::new(pid.to_string())],
            ),
            (Some(et), None) => (
                "SELECT id FROM entity WHERE entity_type = ?1 ORDER BY dis".into(),
                vec![Box::new(et.to_string())],
            ),
            (None, None) => ("SELECT id FROM entity ORDER BY dis".into(), vec![]),
        };

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare(&sql).unwrap();
    let ids: Vec<String> = stmt
        .query_map(param_refs.as_slice(), |row| row.get(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    ids.iter()
        .filter_map(|id| get_entity_db(conn, id).ok())
        .collect()
}

fn set_tag_db(
    conn: &rusqlite::Connection,
    entity_id: &str,
    tag_name: &str,
    tag_value: Option<&str>,
) -> Result<(), EntityError> {
    // Verify entity exists
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM entity WHERE id = ?1",
            rusqlite::params![entity_id],
            |row| row.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .map_err(|e| EntityError::Db(e.to_string()))?;

    if !exists {
        return Err(EntityError::NotFound);
    }

    conn.execute(
        "INSERT OR REPLACE INTO entity_tag (entity_id, tag_name, tag_value) VALUES (?1, ?2, ?3)",
        rusqlite::params![entity_id, tag_name, tag_value],
    )
    .map_err(|e| EntityError::Db(e.to_string()))?;

    touch_entity(conn, entity_id);
    Ok(())
}

fn set_tags_db(
    conn: &rusqlite::Connection,
    entity_id: &str,
    tags: &[(String, Option<String>)],
) -> Result<(), EntityError> {
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM entity WHERE id = ?1",
            rusqlite::params![entity_id],
            |row| row.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .map_err(|e| EntityError::Db(e.to_string()))?;

    if !exists {
        return Err(EntityError::NotFound);
    }

    for (tag_name, tag_value) in tags {
        conn.execute(
            "INSERT OR REPLACE INTO entity_tag (entity_id, tag_name, tag_value) VALUES (?1, ?2, ?3)",
            rusqlite::params![entity_id, tag_name, tag_value.as_deref()],
        )
        .map_err(|e| EntityError::Db(e.to_string()))?;
    }

    touch_entity(conn, entity_id);
    Ok(())
}

fn remove_tag_db(
    conn: &rusqlite::Connection,
    entity_id: &str,
    tag_name: &str,
) -> Result<(), EntityError> {
    conn.execute(
        "DELETE FROM entity_tag WHERE entity_id = ?1 AND tag_name = ?2",
        rusqlite::params![entity_id, tag_name],
    )
    .map_err(|e| EntityError::Db(e.to_string()))?;
    touch_entity(conn, entity_id);
    Ok(())
}

fn remove_tags_db(
    conn: &rusqlite::Connection,
    entity_id: &str,
    tag_names: &[String],
) -> Result<(), EntityError> {
    for tag_name in tag_names {
        conn.execute(
            "DELETE FROM entity_tag WHERE entity_id = ?1 AND tag_name = ?2",
            rusqlite::params![entity_id, tag_name],
        )
        .map_err(|e| EntityError::Db(e.to_string()))?;
    }
    touch_entity(conn, entity_id);
    Ok(())
}

fn set_ref_db(
    conn: &rusqlite::Connection,
    source_id: &str,
    ref_tag: &str,
    target_id: &str,
) -> Result<(), EntityError> {
    conn.execute(
        "INSERT OR REPLACE INTO entity_ref (source_id, ref_tag, target_id) VALUES (?1, ?2, ?3)",
        rusqlite::params![source_id, ref_tag, target_id],
    )
    .map_err(|e| EntityError::Db(e.to_string()))?;
    touch_entity(conn, source_id);
    Ok(())
}

fn remove_ref_db(
    conn: &rusqlite::Connection,
    source_id: &str,
    ref_tag: &str,
) -> Result<(), EntityError> {
    conn.execute(
        "DELETE FROM entity_ref WHERE source_id = ?1 AND ref_tag = ?2",
        rusqlite::params![source_id, ref_tag],
    )
    .map_err(|e| EntityError::Db(e.to_string()))?;
    touch_entity(conn, source_id);
    Ok(())
}

fn get_entities_by_ref_db(
    conn: &rusqlite::Connection,
    ref_tag: &str,
    target_id: &str,
) -> Vec<Entity> {
    let mut stmt = conn
        .prepare("SELECT source_id FROM entity_ref WHERE ref_tag = ?1 AND target_id = ?2")
        .unwrap();
    let ids: Vec<String> = stmt
        .query_map(rusqlite::params![ref_tag, target_id], |row| row.get(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    ids.iter()
        .filter_map(|id| get_entity_db(conn, id).ok())
        .collect()
}

fn find_by_tag_db(
    conn: &rusqlite::Connection,
    tag_name: &str,
    tag_value: Option<&str>,
) -> Vec<Entity> {
    let ids: Vec<String> = if let Some(val) = tag_value {
        let mut stmt = conn
            .prepare("SELECT entity_id FROM entity_tag WHERE tag_name = ?1 AND tag_value = ?2")
            .unwrap();
        stmt.query_map(rusqlite::params![tag_name, val], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect()
    } else {
        let mut stmt = conn
            .prepare("SELECT entity_id FROM entity_tag WHERE tag_name = ?1")
            .unwrap();
        stmt.query_map(rusqlite::params![tag_name], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect()
    };

    ids.iter()
        .filter_map(|id| get_entity_db(conn, id).ok())
        .collect()
}

fn get_hierarchy_db(conn: &rusqlite::Connection, root_id: Option<&str>) -> Vec<Entity> {
    // Recursive CTE to get all descendants
    let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = if let Some(rid) = root_id {
        (
            "WITH RECURSIVE descendants AS (
                SELECT id FROM entity WHERE id = ?1
                UNION ALL
                SELECT e.id FROM entity e JOIN descendants d ON e.parent_id = d.id
            )
            SELECT id FROM descendants ORDER BY id"
                .into(),
            vec![Box::new(rid.to_string())],
        )
    } else {
        // All top-level entities and their descendants
        (
            "WITH RECURSIVE descendants AS (
                SELECT id FROM entity WHERE parent_id IS NULL
                UNION ALL
                SELECT e.id FROM entity e JOIN descendants d ON e.parent_id = d.id
            )
            SELECT id FROM descendants ORDER BY id"
                .into(),
            vec![],
        )
    };

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare(&sql).unwrap();
    let ids: Vec<String> = stmt
        .query_map(param_refs.as_slice(), |row| row.get(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    ids.iter()
        .filter_map(|id| get_entity_db(conn, id).ok())
        .collect()
}

fn touch_entity(conn: &rusqlite::Connection, id: &str) {
    let now = now_ms();
    let _ = conn.execute(
        "UPDATE entity SET updated_ms = ?1 WHERE id = ?2",
        rusqlite::params![now, id],
    );
}

// ----------------------------------------------------------------
// Tag validation warnings (deliverable 5)
// ----------------------------------------------------------------

/// Emit `tracing::warn!` for any structural tag issues on the about-to-be-written
/// tag set.  Does NOT reject the write — purely advisory.  Called from `set_tags`
/// so that automation (prototype apply, API writes) gets the same warnings as the
/// GUI validator without requiring a dependency on `bms-store-bridges`.
fn emit_tag_warnings(entity_id: &str, tags: &[(String, Option<String>)]) {
    let tag_names: std::collections::HashSet<&str> = tags.iter().map(|(k, _)| k.as_str()).collect();

    // point + equip marker without equipRef
    if tag_names.contains("equip") && tag_names.contains("point") && !tag_names.contains("equipRef") {
        tracing::warn!(
            entity_id,
            "tag-validation: point carries 'equip' marker but is missing 'equipRef'"
        );
    }

    // equip without siteRef or floorRef (advisory — common in staging data)
    if tag_names.contains("equip") && !tag_names.contains("siteRef") {
        tracing::warn!(
            entity_id,
            "tag-validation: equip entity is missing 'siteRef'"
        );
    }

    // sensor without point
    if tag_names.contains("sensor") && !tag_names.contains("point") {
        tracing::warn!(
            entity_id,
            "tag-validation: 'sensor' tag present without 'point' marker"
        );
    }

    // cmd without point
    if tag_names.contains("cmd") && !tag_names.contains("point") {
        tracing::warn!(
            entity_id,
            "tag-validation: 'cmd' tag present without 'point' marker"
        );
    }

    // sp without point
    if tag_names.contains("sp") && !tag_names.contains("point") {
        tracing::warn!(
            entity_id,
            "tag-validation: 'sp' (setpoint) tag present without 'point' marker"
        );
    }
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store(path: &str) -> EntityStore {
        let db_path = PathBuf::from(path);
        if db_path.exists() {
            std::fs::remove_file(&db_path).ok();
        }
        start_entity_store_with_path(&db_path)
    }

    #[tokio::test]
    async fn entity_crud() {
        let store = test_store("/tmp/test_entity_crud.db");

        // Create
        let entity = store
            .create_entity(
                "site-1",
                "site",
                "Main Campus",
                None,
                vec![
                    ("site".into(), None),
                    ("dis".into(), Some("Main Campus".into())),
                ],
            )
            .await
            .unwrap();
        assert_eq!(entity.id, "site-1");
        assert_eq!(entity.entity_type, "site");
        assert_eq!(entity.dis, "Main Campus");
        assert!(entity.tags.contains_key("site"));

        // Read
        let fetched = store.get_entity("site-1").await.unwrap();
        assert_eq!(fetched.dis, "Main Campus");

        // Update
        store
            .update_entity("site-1", "Updated Campus")
            .await
            .unwrap();
        let updated = store.get_entity("site-1").await.unwrap();
        assert_eq!(updated.dis, "Updated Campus");

        // Delete
        store.delete_entity("site-1").await.unwrap();
        assert!(store.get_entity("site-1").await.is_err());

        // Cleanup
        std::fs::remove_file("/tmp/test_entity_crud.db").ok();
    }

    #[tokio::test]
    async fn tag_operations() {
        let store = test_store("/tmp/test_entity_tags.db");

        store
            .create_entity("equip-1", "equip", "AHU-1", None, vec![])
            .await
            .unwrap();

        // Set single tag
        store.set_tag("equip-1", "ahu", None).await.unwrap();
        let e = store.get_entity("equip-1").await.unwrap();
        assert!(e.tags.contains_key("ahu"));
        assert_eq!(e.tags["ahu"], None);

        // Set value tag
        store
            .set_tag("equip-1", "dis", Some("Air Handler 1"))
            .await
            .unwrap();
        let e = store.get_entity("equip-1").await.unwrap();
        assert_eq!(e.tags["dis"], Some("Air Handler 1".into()));

        // Batch set
        store
            .set_tags(
                "equip-1",
                vec![
                    ("equip".into(), None),
                    ("air".into(), None),
                    ("singleDuct".into(), None),
                ],
            )
            .await
            .unwrap();
        let e = store.get_entity("equip-1").await.unwrap();
        assert!(e.tags.contains_key("equip"));
        assert!(e.tags.contains_key("air"));
        assert!(e.tags.contains_key("singleDuct"));

        // Remove tag
        store.remove_tag("equip-1", "singleDuct").await.unwrap();
        let e = store.get_entity("equip-1").await.unwrap();
        assert!(!e.tags.contains_key("singleDuct"));

        // Remove multiple tags
        store
            .remove_tags("equip-1", vec!["air".into(), "equip".into()])
            .await
            .unwrap();
        let e = store.get_entity("equip-1").await.unwrap();
        assert!(!e.tags.contains_key("air"));
        assert!(!e.tags.contains_key("equip"));

        std::fs::remove_file("/tmp/test_entity_tags.db").ok();
    }

    #[tokio::test]
    async fn ref_operations() {
        let store = test_store("/tmp/test_entity_refs.db");

        store
            .create_entity(
                "site-1",
                "site",
                "Campus",
                None,
                vec![("site".into(), None)],
            )
            .await
            .unwrap();
        store
            .create_entity(
                "equip-1",
                "equip",
                "AHU-1",
                None,
                vec![("equip".into(), None)],
            )
            .await
            .unwrap();

        // Set ref
        store.set_ref("equip-1", "siteRef", "site-1").await.unwrap();
        let e = store.get_entity("equip-1").await.unwrap();
        assert_eq!(e.refs["siteRef"], "site-1");

        // Query by ref
        let equips = store.get_entities_by_ref("siteRef", "site-1").await;
        assert_eq!(equips.len(), 1);
        assert_eq!(equips[0].id, "equip-1");

        // Remove ref
        store.remove_ref("equip-1", "siteRef").await.unwrap();
        let e = store.get_entity("equip-1").await.unwrap();
        assert!(!e.refs.contains_key("siteRef"));

        std::fs::remove_file("/tmp/test_entity_refs.db").ok();
    }

    #[tokio::test]
    async fn hierarchy_query() {
        let store = test_store("/tmp/test_entity_hierarchy.db");

        store
            .create_entity(
                "site-1",
                "site",
                "Campus",
                None,
                vec![("site".into(), None)],
            )
            .await
            .unwrap();
        store
            .create_entity(
                "bldg-1",
                "space",
                "Building A",
                Some("site-1"),
                vec![("space".into(), None), ("building".into(), None)],
            )
            .await
            .unwrap();
        store
            .create_entity(
                "floor-1",
                "space",
                "Floor 1",
                Some("bldg-1"),
                vec![("space".into(), None), ("floor".into(), None)],
            )
            .await
            .unwrap();
        store
            .create_entity(
                "room-101",
                "space",
                "Room 101",
                Some("floor-1"),
                vec![("space".into(), None), ("room".into(), None)],
            )
            .await
            .unwrap();

        // Get full hierarchy from site
        let all = store.get_hierarchy(Some("site-1")).await;
        assert_eq!(all.len(), 4); // site + building + floor + room

        // Get from building
        let bldg = store.get_hierarchy(Some("bldg-1")).await;
        assert_eq!(bldg.len(), 3); // building + floor + room

        // List children of a parent
        let floors = store.list_entities(None, Some("bldg-1")).await;
        assert_eq!(floors.len(), 1);
        assert_eq!(floors[0].id, "floor-1");

        std::fs::remove_file("/tmp/test_entity_hierarchy.db").ok();
    }

    #[tokio::test]
    async fn find_by_tag() {
        let store = test_store("/tmp/test_entity_find_tag.db");

        store
            .create_entity(
                "e1",
                "equip",
                "AHU-1",
                None,
                vec![("equip".into(), None), ("ahu".into(), None)],
            )
            .await
            .unwrap();
        store
            .create_entity(
                "e2",
                "equip",
                "VAV-1",
                None,
                vec![("equip".into(), None), ("vav".into(), None)],
            )
            .await
            .unwrap();
        store
            .create_entity(
                "e3",
                "equip",
                "AHU-2",
                None,
                vec![("equip".into(), None), ("ahu".into(), None)],
            )
            .await
            .unwrap();

        let ahus = store.find_by_tag("ahu", None).await;
        assert_eq!(ahus.len(), 2);

        let equips = store.find_by_tag("equip", None).await;
        assert_eq!(equips.len(), 3);

        let vavs = store.find_by_tag("vav", None).await;
        assert_eq!(vavs.len(), 1);

        std::fs::remove_file("/tmp/test_entity_find_tag.db").ok();
    }

    #[tokio::test]
    async fn prototype_application() {
        use crate::haystack::prototypes::find_equip_prototype;

        let store = test_store("/tmp/test_entity_prototype.db");

        // Apply AHU prototype
        let proto = find_equip_prototype("ahu").unwrap();
        let tags: Vec<(String, Option<String>)> = proto
            .tags
            .iter()
            .map(|&(name, val)| (name.to_string(), val.map(|v| v.to_string())))
            .collect();

        store
            .create_entity("ahu-1", "equip", "AHU-1", None, tags)
            .await
            .unwrap();

        let e = store.get_entity("ahu-1").await.unwrap();
        assert!(e.tags.contains_key("equip"));
        assert!(e.tags.contains_key("ahu"));
        assert!(e.tags.contains_key("air"));

        std::fs::remove_file("/tmp/test_entity_prototype.db").ok();
    }
}
