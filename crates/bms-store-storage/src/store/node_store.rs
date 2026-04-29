use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use tokio::sync::{mpsc, oneshot, watch};

use crate::config::profile::PointValue;
use crate::event::bus::{Event, EventBus};
use crate::node::{Node, NodeCapabilities, NodeId, NodeSnapshot, ProtocolBinding};
use crate::store::point_store::PointStatusFlags;

// ----------------------------------------------------------------
// Error type
// ----------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum NodeError {
    #[error("database error: {0}")]
    Db(String),
    #[error("channel closed")]
    ChannelClosed,
    #[error("not found: {0}")]
    NotFound(String),
    #[error("already exists: {0}")]
    AlreadyExists(String),
}

// ----------------------------------------------------------------
// Persistent representation (what goes to/from SQLite)
// ----------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct NodeRecord {
    pub id: NodeId,
    pub node_type: String,
    pub dis: String,
    pub parent_id: Option<NodeId>,
    pub tags: HashMap<String, Option<String>>,
    pub refs: HashMap<String, NodeId>,
    pub properties: HashMap<String, String>,
    pub capabilities: NodeCapabilities,
    pub binding: Option<ProtocolBinding>,
    pub created_ms: i64,
    pub updated_ms: i64,
}

// ----------------------------------------------------------------
// Commands sent to the SQLite thread
// ----------------------------------------------------------------

enum NodeCmd {
    CreateNode {
        record: Box<NodeRecord>,
        reply: oneshot::Sender<Result<(), NodeError>>,
    },
    GetNode {
        id: NodeId,
        reply: oneshot::Sender<Result<NodeRecord, NodeError>>,
    },
    ListNodes {
        node_type: Option<String>,
        parent_id: Option<String>,
        reply: oneshot::Sender<Vec<NodeRecord>>,
    },
    DeleteNode {
        id: NodeId,
        reply: oneshot::Sender<Result<(), NodeError>>,
    },
    UpdateDis {
        id: NodeId,
        dis: String,
        reply: oneshot::Sender<Result<(), NodeError>>,
    },
    SetTag {
        id: NodeId,
        tag_name: String,
        tag_value: Option<String>,
        reply: oneshot::Sender<Result<(), NodeError>>,
    },
    SetTags {
        id: NodeId,
        tags: Vec<(String, Option<String>)>,
        reply: oneshot::Sender<Result<(), NodeError>>,
    },
    RemoveTag {
        id: NodeId,
        tag_name: String,
        reply: oneshot::Sender<Result<(), NodeError>>,
    },
    SetRef {
        source_id: NodeId,
        ref_tag: String,
        target_id: NodeId,
        reply: oneshot::Sender<Result<(), NodeError>>,
    },
    RemoveRef {
        source_id: NodeId,
        ref_tag: String,
        reply: oneshot::Sender<Result<(), NodeError>>,
    },
    SetProperty {
        id: NodeId,
        key: String,
        value: String,
        reply: oneshot::Sender<Result<(), NodeError>>,
    },
    SetBinding {
        id: NodeId,
        binding: Option<ProtocolBinding>,
        reply: oneshot::Sender<Result<(), NodeError>>,
    },
    SetCapabilities {
        id: NodeId,
        capabilities: NodeCapabilities,
        reply: oneshot::Sender<Result<(), NodeError>>,
    },
    FindByTag {
        tag_name: String,
        tag_value: Option<String>,
        reply: oneshot::Sender<Vec<NodeRecord>>,
    },
    UpdateParent {
        id: NodeId,
        parent_id: Option<String>,
        reply: oneshot::Sender<Result<(), NodeError>>,
    },
    GetHierarchy {
        root_id: Option<NodeId>,
        reply: oneshot::Sender<Vec<NodeRecord>>,
    },
    GetAncestors {
        id: NodeId,
        reply: oneshot::Sender<Vec<NodeRecord>>,
    },
    /// Find all nodes whose ref_tag points to the given target.
    FindByRef {
        ref_tag: String,
        target_id: NodeId,
        reply: oneshot::Sender<Vec<NodeRecord>>,
    },
    /// Find ALL nodes that reference a given target_id (any ref_tag).
    FindAllReferencing {
        target_id: NodeId,
        reply: oneshot::Sender<Vec<(String, NodeRecord)>>,
    },
    /// Bulk set refs in a single transaction.
    SetRefs {
        updates: Vec<(NodeId, String, NodeId)>,
        reply: oneshot::Sender<Result<(), NodeError>>,
    },
}

// ----------------------------------------------------------------
// NodeStore — the unified store
// ----------------------------------------------------------------

/// Two-layer node store: hot in-memory cache for live values + SQLite for persistence.
#[derive(Clone)]
pub struct NodeStore {
    /// Hot cache: value + timestamp + status for point nodes. GUI reads here (O(1)).
    hot: Arc<RwLock<HashMap<NodeId, NodeSnapshot>>>,
    /// Command channel to the SQLite thread.
    cmd_tx: mpsc::UnboundedSender<NodeCmd>,
    /// Version counter for reactivity.
    version_tx: watch::Sender<u64>,
    version_rx: watch::Receiver<u64>,
    /// Optional event bus for cross-system events.
    event_bus: Option<EventBus>,
}

impl NodeStore {
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

    // ----------------------------------------------------------
    // Hot cache operations (live value updates — no SQLite)
    // ----------------------------------------------------------

    /// Update a point node's live value in the hot cache.
    pub fn update_value(&self, node_id: &str, value: PointValue) {
        let mut hot = self.hot.write().unwrap();
        let snap = hot.entry(node_id.to_string()).or_insert(NodeSnapshot::new(
            None,
            None,
            PointStatusFlags::default(),
        ));
        snap.value = Some(value.clone());
        snap.timestamp = Some(Instant::now());
        drop(hot);
        self.bump_version();

        if let Some(ref bus) = self.event_bus {
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;
            bus.publish(Event::ValueChanged {
                node_id: node_id.to_string(),
                value,
                timestamp_ms: now_ms,
            });
        }
    }

    /// Set a status flag on a node in the hot cache.
    pub fn set_status(&self, node_id: &str, flag: u8) {
        let mut hot = self.hot.write().unwrap();
        if let Some(snap) = hot.get_mut(node_id) {
            snap.status.set(flag);
        }
        drop(hot);
        self.bump_version();
    }

    /// Clear a status flag on a node in the hot cache.
    pub fn clear_status(&self, node_id: &str, flag: u8) {
        let mut hot = self.hot.write().unwrap();
        if let Some(snap) = hot.get_mut(node_id) {
            snap.status.clear(flag);
        }
        drop(hot);
        self.bump_version();
    }

    /// Get a snapshot of live state for a node.
    pub fn get_snapshot(&self, node_id: &str) -> Option<NodeSnapshot> {
        self.hot.read().unwrap().get(node_id).cloned()
    }

    /// Get all node IDs in the hot cache.
    pub fn hot_node_ids(&self) -> Vec<NodeId> {
        self.hot.read().unwrap().keys().cloned().collect()
    }

    /// Initialize a node in the hot cache (used on startup).
    pub fn init_hot(&self, node_id: &str, value: Option<PointValue>) {
        let mut hot = self.hot.write().unwrap();
        hot.insert(
            node_id.to_string(),
            NodeSnapshot::new(value, Some(Instant::now()), PointStatusFlags::default()),
        );
    }

    // ----------------------------------------------------------
    // Persistent operations (via SQLite thread)
    // ----------------------------------------------------------

    pub async fn create_node(&self, node: Node) -> Result<(), NodeError> {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let record = NodeRecord {
            id: node.id.clone(),
            node_type: node.node_type.as_str().to_string(),
            dis: node.dis.clone(),
            parent_id: node.parent_id.clone(),
            tags: node.tags.clone(),
            refs: node.refs.clone(),
            properties: node.properties.clone(),
            capabilities: node.capabilities.clone(),
            binding: node.binding.clone(),
            created_ms: now_ms,
            updated_ms: now_ms,
        };

        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(NodeCmd::CreateNode {
                record: Box::new(record),
                reply: reply_tx,
            })
            .map_err(|_| NodeError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| NodeError::ChannelClosed)?;

        if result.is_ok() {
            // Initialize hot cache for point nodes
            if node.is_point() {
                self.init_hot(&node.id, node.value.clone());
            }
            self.bump_version();
            if let Some(ref bus) = self.event_bus {
                bus.publish(Event::EntityCreated {
                    entity_id: node.id.clone(),
                });
            }
        }
        result
    }

    pub async fn get_node(&self, id: &str) -> Result<NodeRecord, NodeError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(NodeCmd::GetNode {
                id: id.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| NodeError::ChannelClosed)?;
        reply_rx.await.map_err(|_| NodeError::ChannelClosed)?
    }

    pub async fn list_nodes(
        &self,
        node_type: Option<&str>,
        parent_id: Option<&str>,
    ) -> Vec<NodeRecord> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(NodeCmd::ListNodes {
            node_type: node_type.map(|s| s.to_string()),
            parent_id: parent_id.map(|s| s.to_string()),
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn delete_node(&self, id: &str) -> Result<(), NodeError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(NodeCmd::DeleteNode {
                id: id.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| NodeError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| NodeError::ChannelClosed)?;
        if result.is_ok() {
            // Remove from hot cache
            self.hot.write().unwrap().remove(id);
            self.bump_version();
            if let Some(ref bus) = self.event_bus {
                bus.publish(Event::EntityDeleted {
                    entity_id: id.to_string(),
                });
            }
        }
        result
    }

    pub async fn update_dis(&self, id: &str, dis: &str) -> Result<(), NodeError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(NodeCmd::UpdateDis {
                id: id.to_string(),
                dis: dis.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| NodeError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| NodeError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_version();
        }
        result
    }

    pub async fn update_parent(&self, id: &str, parent_id: Option<&str>) -> Result<(), NodeError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(NodeCmd::UpdateParent {
                id: id.to_string(),
                parent_id: parent_id.map(|s| s.to_string()),
                reply: reply_tx,
            })
            .map_err(|_| NodeError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| NodeError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_version();
        }
        result
    }

    pub async fn set_tag(
        &self,
        id: &str,
        tag_name: &str,
        tag_value: Option<&str>,
    ) -> Result<(), NodeError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(NodeCmd::SetTag {
                id: id.to_string(),
                tag_name: tag_name.to_string(),
                tag_value: tag_value.map(|s| s.to_string()),
                reply: reply_tx,
            })
            .map_err(|_| NodeError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| NodeError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_version();
        }
        result
    }

    pub async fn set_tags(
        &self,
        id: &str,
        tags: Vec<(String, Option<String>)>,
    ) -> Result<(), NodeError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(NodeCmd::SetTags {
                id: id.to_string(),
                tags,
                reply: reply_tx,
            })
            .map_err(|_| NodeError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| NodeError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_version();
        }
        result
    }

    pub async fn remove_tag(&self, id: &str, tag_name: &str) -> Result<(), NodeError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(NodeCmd::RemoveTag {
                id: id.to_string(),
                tag_name: tag_name.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| NodeError::ChannelClosed)?;
        reply_rx.await.map_err(|_| NodeError::ChannelClosed)?
    }

    pub async fn set_ref(
        &self,
        source_id: &str,
        ref_tag: &str,
        target_id: &str,
    ) -> Result<(), NodeError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(NodeCmd::SetRef {
                source_id: source_id.to_string(),
                ref_tag: ref_tag.to_string(),
                target_id: target_id.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| NodeError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| NodeError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_version();
        }
        result
    }

    pub async fn remove_ref(&self, source_id: &str, ref_tag: &str) -> Result<(), NodeError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(NodeCmd::RemoveRef {
                source_id: source_id.to_string(),
                ref_tag: ref_tag.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| NodeError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| NodeError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_version();
        }
        result
    }

    pub async fn set_property(&self, id: &str, key: &str, value: &str) -> Result<(), NodeError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(NodeCmd::SetProperty {
                id: id.to_string(),
                key: key.to_string(),
                value: value.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| NodeError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| NodeError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_version();
        }
        result
    }

    pub async fn set_binding(
        &self,
        id: &str,
        binding: Option<ProtocolBinding>,
    ) -> Result<(), NodeError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(NodeCmd::SetBinding {
                id: id.to_string(),
                binding,
                reply: reply_tx,
            })
            .map_err(|_| NodeError::ChannelClosed)?;
        reply_rx.await.map_err(|_| NodeError::ChannelClosed)?
    }

    pub async fn set_capabilities(
        &self,
        id: &str,
        capabilities: NodeCapabilities,
    ) -> Result<(), NodeError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(NodeCmd::SetCapabilities {
                id: id.to_string(),
                capabilities,
                reply: reply_tx,
            })
            .map_err(|_| NodeError::ChannelClosed)?;
        reply_rx.await.map_err(|_| NodeError::ChannelClosed)?
    }

    pub async fn find_by_tag(&self, tag_name: &str, tag_value: Option<&str>) -> Vec<NodeRecord> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(NodeCmd::FindByTag {
            tag_name: tag_name.to_string(),
            tag_value: tag_value.map(|s| s.to_string()),
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn get_hierarchy(&self, root_id: Option<&str>) -> Vec<NodeRecord> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(NodeCmd::GetHierarchy {
            root_id: root_id.map(|s| s.to_string()),
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_default()
    }

    /// Get ancestors of a node, ordered root-first (Site → Space → Equip).
    pub async fn get_ancestors(&self, id: &str) -> Result<Vec<NodeRecord>, NodeError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(NodeCmd::GetAncestors {
                id: id.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| NodeError::ChannelClosed)?;
        reply_rx.await.map_err(|_| NodeError::ChannelClosed)
    }

    /// Find all nodes whose `ref_tag` points to `target_id`.
    pub async fn find_by_ref(&self, ref_tag: &str, target_id: &str) -> Vec<NodeRecord> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(NodeCmd::FindByRef {
            ref_tag: ref_tag.to_string(),
            target_id: target_id.to_string(),
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_default()
    }

    /// Find ALL nodes that reference a given target, grouped by ref_tag.
    pub async fn find_all_referencing(&self, target_id: &str) -> Vec<(String, NodeRecord)> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(NodeCmd::FindAllReferencing {
            target_id: target_id.to_string(),
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_default()
    }

    /// Bulk set refs in a single transaction.
    pub async fn set_refs(&self, updates: Vec<(String, String, String)>) -> Result<(), NodeError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let updates = updates.into_iter().collect();
        let _ = self.cmd_tx.send(NodeCmd::SetRefs {
            updates,
            reply: reply_tx,
        });
        let result = reply_rx.await.map_err(|_| NodeError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_version();
        }
        result
    }
}

// ----------------------------------------------------------------
// SQLite schema & thread
// ----------------------------------------------------------------

use super::migration::{run_migrations, Migration};

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        label: "initial node schema",
        sql: "
        CREATE TABLE IF NOT EXISTS node (
            id TEXT PRIMARY KEY,
            node_type TEXT NOT NULL,
            dis TEXT NOT NULL DEFAULT '',
            parent_id TEXT REFERENCES node(id) ON DELETE SET NULL,
            created_ms INTEGER NOT NULL,
            updated_ms INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS node_tag (
            node_id TEXT NOT NULL REFERENCES node(id) ON DELETE CASCADE,
            tag_name TEXT NOT NULL,
            tag_value TEXT,
            PRIMARY KEY (node_id, tag_name)
        );

        CREATE TABLE IF NOT EXISTS node_ref (
            source_id TEXT NOT NULL REFERENCES node(id) ON DELETE CASCADE,
            ref_tag TEXT NOT NULL,
            target_id TEXT NOT NULL,
            PRIMARY KEY (source_id, ref_tag)
        );

        CREATE TABLE IF NOT EXISTS node_property (
            node_id TEXT NOT NULL REFERENCES node(id) ON DELETE CASCADE,
            key TEXT NOT NULL,
            value TEXT NOT NULL,
            PRIMARY KEY (node_id, key)
        );

        CREATE TABLE IF NOT EXISTS node_capability (
            node_id TEXT PRIMARY KEY REFERENCES node(id) ON DELETE CASCADE,
            readable INTEGER NOT NULL DEFAULT 0,
            writable INTEGER NOT NULL DEFAULT 0,
            historizable INTEGER NOT NULL DEFAULT 0,
            alarmable INTEGER NOT NULL DEFAULT 0,
            schedulable INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS protocol_binding (
            node_id TEXT PRIMARY KEY REFERENCES node(id) ON DELETE CASCADE,
            binding_json TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_node_parent ON node(parent_id);
        CREATE INDEX IF NOT EXISTS idx_node_type ON node(node_type);
        CREATE INDEX IF NOT EXISTS idx_node_tag_name ON node_tag(tag_name);
",
    },
    Migration {
        version: 2,
        label: "index node_ref target_id for reverse lookups",
        sql: "CREATE INDEX IF NOT EXISTS idx_node_ref_target ON node_ref(target_id);",
    },
];

fn init_schema(conn: &rusqlite::Connection) {
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA foreign_keys = ON;
        ",
    )
    .expect("failed to set node_store pragmas");
    run_migrations(conn, "nodes", MIGRATIONS).expect("nodes: schema migration failed");
}

fn run_sqlite_thread(db_path: &Path, mut cmd_rx: mpsc::UnboundedReceiver<NodeCmd>) {
    let conn = rusqlite::Connection::open(db_path).expect("failed to open nodes.db");
    init_schema(&conn);

    while let Some(cmd) = cmd_rx.blocking_recv() {
        match cmd {
            NodeCmd::CreateNode { record, reply } => {
                let result = create_node_db(&conn, &record);
                let _ = reply.send(result);
            }
            NodeCmd::GetNode { id, reply } => {
                let result = get_node_db(&conn, &id);
                let _ = reply.send(result);
            }
            NodeCmd::ListNodes {
                node_type,
                parent_id,
                reply,
            } => {
                let result = list_nodes_db(&conn, node_type.as_deref(), parent_id.as_deref());
                let _ = reply.send(result);
            }
            NodeCmd::DeleteNode { id, reply } => {
                let result = delete_node_db(&conn, &id);
                let _ = reply.send(result);
            }
            NodeCmd::UpdateDis { id, dis, reply } => {
                let result = update_dis_db(&conn, &id, &dis);
                let _ = reply.send(result);
            }
            NodeCmd::SetTag {
                id,
                tag_name,
                tag_value,
                reply,
            } => {
                let result = set_tag_db(&conn, &id, &tag_name, tag_value.as_deref());
                let _ = reply.send(result);
            }
            NodeCmd::SetTags { id, tags, reply } => {
                let result = set_tags_db(&conn, &id, &tags);
                let _ = reply.send(result);
            }
            NodeCmd::RemoveTag {
                id,
                tag_name,
                reply,
            } => {
                let result = remove_tag_db(&conn, &id, &tag_name);
                let _ = reply.send(result);
            }
            NodeCmd::SetRef {
                source_id,
                ref_tag,
                target_id,
                reply,
            } => {
                let result = set_ref_db(&conn, &source_id, &ref_tag, &target_id);
                let _ = reply.send(result);
            }
            NodeCmd::RemoveRef {
                source_id,
                ref_tag,
                reply,
            } => {
                let result = remove_ref_db(&conn, &source_id, &ref_tag);
                let _ = reply.send(result);
            }
            NodeCmd::SetProperty {
                id,
                key,
                value,
                reply,
            } => {
                let result = set_property_db(&conn, &id, &key, &value);
                let _ = reply.send(result);
            }
            NodeCmd::SetBinding { id, binding, reply } => {
                let result = set_binding_db(&conn, &id, &binding);
                let _ = reply.send(result);
            }
            NodeCmd::SetCapabilities {
                id,
                capabilities,
                reply,
            } => {
                let result = set_capabilities_db(&conn, &id, &capabilities);
                let _ = reply.send(result);
            }
            NodeCmd::FindByTag {
                tag_name,
                tag_value,
                reply,
            } => {
                let result = find_by_tag_db(&conn, &tag_name, tag_value.as_deref());
                let _ = reply.send(result);
            }
            NodeCmd::UpdateParent {
                id,
                parent_id,
                reply,
            } => {
                let result = update_parent_db(&conn, &id, parent_id.as_deref());
                let _ = reply.send(result);
            }
            NodeCmd::GetHierarchy { root_id, reply } => {
                let result = get_hierarchy_db(&conn, root_id.as_deref());
                let _ = reply.send(result);
            }
            NodeCmd::GetAncestors { id, reply } => {
                let result = get_ancestors_db(&conn, &id);
                let _ = reply.send(result);
            }
            NodeCmd::FindByRef {
                ref_tag,
                target_id,
                reply,
            } => {
                let result = find_by_ref_db(&conn, &ref_tag, &target_id);
                let _ = reply.send(result);
            }
            NodeCmd::FindAllReferencing { target_id, reply } => {
                let result = find_all_referencing_db(&conn, &target_id);
                let _ = reply.send(result);
            }
            NodeCmd::SetRefs { updates, reply } => {
                let result = set_refs_db(&conn, &updates);
                let _ = reply.send(result);
            }
        }
    }
}

// ----------------------------------------------------------------
// SQLite operations
// ----------------------------------------------------------------

fn create_node_db(conn: &rusqlite::Connection, rec: &NodeRecord) -> Result<(), NodeError> {
    conn.execute(
        "INSERT INTO node (id, node_type, dis, parent_id, created_ms, updated_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![rec.id, rec.node_type, rec.dis, rec.parent_id, rec.created_ms, rec.updated_ms],
    ).map_err(|e| {
        if matches!(e, rusqlite::Error::SqliteFailure(ref err, _) if err.extended_code == 1555) {
            NodeError::AlreadyExists(rec.id.clone())
        } else {
            NodeError::Db(e.to_string())
        }
    })?;

    // Insert tags
    for (name, value) in &rec.tags {
        conn.execute(
            "INSERT INTO node_tag (node_id, tag_name, tag_value) VALUES (?1, ?2, ?3)",
            rusqlite::params![rec.id, name, value],
        )
        .map_err(|e| NodeError::Db(e.to_string()))?;
    }

    // Insert refs
    for (ref_tag, target_id) in &rec.refs {
        conn.execute(
            "INSERT INTO node_ref (source_id, ref_tag, target_id) VALUES (?1, ?2, ?3)",
            rusqlite::params![rec.id, ref_tag, target_id],
        )
        .map_err(|e| NodeError::Db(e.to_string()))?;
    }

    // Insert properties
    for (key, value) in &rec.properties {
        conn.execute(
            "INSERT INTO node_property (node_id, key, value) VALUES (?1, ?2, ?3)",
            rusqlite::params![rec.id, key, value],
        )
        .map_err(|e| NodeError::Db(e.to_string()))?;
    }

    // Insert capabilities
    conn.execute(
        "INSERT INTO node_capability (node_id, readable, writable, historizable, alarmable, schedulable) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            rec.id,
            rec.capabilities.readable as i32,
            rec.capabilities.writable as i32,
            rec.capabilities.historizable as i32,
            rec.capabilities.alarmable as i32,
            rec.capabilities.schedulable as i32,
        ],
    ).map_err(|e| NodeError::Db(e.to_string()))?;

    // Insert binding
    if let Some(ref binding) = rec.binding {
        let json = serde_json::to_string(binding).unwrap_or_default();
        conn.execute(
            "INSERT INTO protocol_binding (node_id, binding_json) VALUES (?1, ?2)",
            rusqlite::params![rec.id, json],
        )
        .map_err(|e| NodeError::Db(e.to_string()))?;
    }

    Ok(())
}

fn get_node_db(conn: &rusqlite::Connection, id: &str) -> Result<NodeRecord, NodeError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, node_type, dis, parent_id, created_ms, updated_ms FROM node WHERE id = ?1",
        )
        .map_err(|e| NodeError::Db(e.to_string()))?;

    let rec = stmt
        .query_row(rusqlite::params![id], |row| {
            Ok(NodeRecord {
                id: row.get(0)?,
                node_type: row.get(1)?,
                dis: row.get(2)?,
                parent_id: row.get(3)?,
                tags: HashMap::new(),
                refs: HashMap::new(),
                properties: HashMap::new(),
                capabilities: NodeCapabilities::default(),
                binding: None,
                created_ms: row.get(4)?,
                updated_ms: row.get(5)?,
            })
        })
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => NodeError::NotFound(id.to_string()),
            _ => NodeError::Db(e.to_string()),
        })?;

    let mut rec = rec;
    load_node_relations(conn, &mut rec);
    Ok(rec)
}

fn load_node_relations(conn: &rusqlite::Connection, rec: &mut NodeRecord) {
    // Tags
    if let Ok(mut stmt) =
        conn.prepare("SELECT tag_name, tag_value FROM node_tag WHERE node_id = ?1")
    {
        if let Ok(rows) = stmt.query_map(rusqlite::params![rec.id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        }) {
            for row in rows.flatten() {
                rec.tags.insert(row.0, row.1);
            }
        }
    }

    // Refs
    if let Ok(mut stmt) =
        conn.prepare("SELECT ref_tag, target_id FROM node_ref WHERE source_id = ?1")
    {
        if let Ok(rows) = stmt.query_map(rusqlite::params![rec.id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }) {
            for row in rows.flatten() {
                rec.refs.insert(row.0, row.1);
            }
        }
    }

    // Properties
    if let Ok(mut stmt) = conn.prepare("SELECT key, value FROM node_property WHERE node_id = ?1") {
        if let Ok(rows) = stmt.query_map(rusqlite::params![rec.id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }) {
            for row in rows.flatten() {
                rec.properties.insert(row.0, row.1);
            }
        }
    }

    // Capabilities
    if let Ok(mut stmt) = conn.prepare(
        "SELECT readable, writable, historizable, alarmable, schedulable FROM node_capability WHERE node_id = ?1",
    ) {
        if let Ok(caps) = stmt.query_row(rusqlite::params![rec.id], |row| {
            Ok(NodeCapabilities::new(
                row.get::<_, i32>(0)? != 0,
                row.get::<_, i32>(1)? != 0,
                row.get::<_, i32>(2)? != 0,
                row.get::<_, i32>(3)? != 0,
                row.get::<_, i32>(4)? != 0,
            ))
        }) {
            rec.capabilities = caps;
        }
    }

    // Binding
    if let Ok(mut stmt) =
        conn.prepare("SELECT binding_json FROM protocol_binding WHERE node_id = ?1")
    {
        if let Ok(json) = stmt.query_row(rusqlite::params![rec.id], |row| row.get::<_, String>(0)) {
            rec.binding = serde_json::from_str(&json).ok();
        }
    }
}

/// Load tags, refs, properties, capabilities, and bindings for multiple nodes in bulk.
/// Uses 5 queries total (per chunk) instead of 5 per node (eliminates N+1).
/// Chunks into batches of 500 to stay within SQLite parameter limits.
fn load_node_relations_bulk(conn: &rusqlite::Connection, records: &mut [NodeRecord]) {
    if records.is_empty() {
        return;
    }

    // For large sets, chunk to stay within SQLite's SQLITE_MAX_VARIABLE_NUMBER limit
    const CHUNK_SIZE: usize = 500;
    if records.len() > CHUNK_SIZE {
        for chunk_start in (0..records.len()).step_by(CHUNK_SIZE) {
            let chunk_end = (chunk_start + CHUNK_SIZE).min(records.len());
            load_node_relations_bulk(conn, &mut records[chunk_start..chunk_end]);
        }
        return;
    }

    // Collect owned IDs to avoid borrowing records
    let ids: Vec<String> = records.iter().map(|r| r.id.clone()).collect();
    let id_to_idx: HashMap<String, Vec<usize>> = {
        let mut map: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, id) in ids.iter().enumerate() {
            map.entry(id.clone()).or_default().push(i);
        }
        map
    };
    let placeholders: String = ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("?{}", i + 1))
        .collect::<Vec<_>>()
        .join(",");

    // Tags
    {
        let sql = format!(
            "SELECT node_id, tag_name, tag_value FROM node_tag WHERE node_id IN ({placeholders})"
        );
        if let Ok(mut stmt) = conn.prepare(&sql) {
            let params: Vec<&dyn rusqlite::types::ToSql> = ids
                .iter()
                .map(|s| s as &dyn rusqlite::types::ToSql)
                .collect();
            if let Ok(rows) = stmt.query_map(params.as_slice(), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            }) {
                for row in rows.flatten() {
                    if let Some(indices) = id_to_idx.get(row.0.as_str()) {
                        for &idx in indices {
                            records[idx].tags.insert(row.1.clone(), row.2.clone());
                        }
                    }
                }
            }
        }
    }

    // Refs
    {
        let sql = format!("SELECT source_id, ref_tag, target_id FROM node_ref WHERE source_id IN ({placeholders})");
        if let Ok(mut stmt) = conn.prepare(&sql) {
            let params: Vec<&dyn rusqlite::types::ToSql> = ids
                .iter()
                .map(|s| s as &dyn rusqlite::types::ToSql)
                .collect();
            if let Ok(rows) = stmt.query_map(params.as_slice(), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            }) {
                for row in rows.flatten() {
                    if let Some(indices) = id_to_idx.get(row.0.as_str()) {
                        for &idx in indices {
                            records[idx].refs.insert(row.1.clone(), row.2.clone());
                        }
                    }
                }
            }
        }
    }

    // Properties
    {
        let sql = format!(
            "SELECT node_id, key, value FROM node_property WHERE node_id IN ({placeholders})"
        );
        if let Ok(mut stmt) = conn.prepare(&sql) {
            let params: Vec<&dyn rusqlite::types::ToSql> = ids
                .iter()
                .map(|s| s as &dyn rusqlite::types::ToSql)
                .collect();
            if let Ok(rows) = stmt.query_map(params.as_slice(), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            }) {
                for row in rows.flatten() {
                    if let Some(indices) = id_to_idx.get(row.0.as_str()) {
                        for &idx in indices {
                            records[idx].properties.insert(row.1.clone(), row.2.clone());
                        }
                    }
                }
            }
        }
    }

    // Capabilities
    {
        let sql = format!("SELECT node_id, readable, writable, historizable, alarmable, schedulable FROM node_capability WHERE node_id IN ({placeholders})");
        if let Ok(mut stmt) = conn.prepare(&sql) {
            let params: Vec<&dyn rusqlite::types::ToSql> = ids
                .iter()
                .map(|s| s as &dyn rusqlite::types::ToSql)
                .collect();
            if let Ok(rows) = stmt.query_map(params.as_slice(), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    NodeCapabilities::new(
                        row.get::<_, i32>(1)? != 0,
                        row.get::<_, i32>(2)? != 0,
                        row.get::<_, i32>(3)? != 0,
                        row.get::<_, i32>(4)? != 0,
                        row.get::<_, i32>(5)? != 0,
                    ),
                ))
            }) {
                for row in rows.flatten() {
                    if let Some(indices) = id_to_idx.get(row.0.as_str()) {
                        for &idx in indices {
                            records[idx].capabilities = row.1.clone();
                        }
                    }
                }
            }
        }
    }

    // Bindings
    {
        let sql = format!(
            "SELECT node_id, binding_json FROM protocol_binding WHERE node_id IN ({placeholders})"
        );
        if let Ok(mut stmt) = conn.prepare(&sql) {
            let params: Vec<&dyn rusqlite::types::ToSql> = ids
                .iter()
                .map(|s| s as &dyn rusqlite::types::ToSql)
                .collect();
            if let Ok(rows) = stmt.query_map(params.as_slice(), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            }) {
                for row in rows.flatten() {
                    if let Some(indices) = id_to_idx.get(row.0.as_str()) {
                        let binding: Option<ProtocolBinding> = serde_json::from_str(&row.1).ok();
                        for &idx in indices {
                            records[idx].binding = binding.clone();
                        }
                    }
                }
            }
        }
    }
}

fn list_nodes_db(
    conn: &rusqlite::Connection,
    node_type: Option<&str>,
    parent_id: Option<&str>,
) -> Vec<NodeRecord> {
    let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match (node_type, parent_id) {
        (Some(nt), Some("__root__")) => (
            "SELECT id, node_type, dis, parent_id, created_ms, updated_ms FROM node WHERE node_type = ?1 AND parent_id IS NULL ORDER BY dis".into(),
            vec![Box::new(nt.to_string())],
        ),
        (Some(nt), Some(pid)) => (
            "SELECT id, node_type, dis, parent_id, created_ms, updated_ms FROM node WHERE node_type = ?1 AND parent_id = ?2 ORDER BY dis".into(),
            vec![Box::new(nt.to_string()), Box::new(pid.to_string())],
        ),
        (Some(nt), None) => (
            "SELECT id, node_type, dis, parent_id, created_ms, updated_ms FROM node WHERE node_type = ?1 ORDER BY dis".into(),
            vec![Box::new(nt.to_string())],
        ),
        (None, Some("__root__")) => (
            "SELECT id, node_type, dis, parent_id, created_ms, updated_ms FROM node WHERE parent_id IS NULL ORDER BY dis".into(),
            vec![],
        ),
        (None, Some(pid)) => (
            "SELECT id, node_type, dis, parent_id, created_ms, updated_ms FROM node WHERE parent_id = ?1 ORDER BY dis".into(),
            vec![Box::new(pid.to_string())],
        ),
        (None, None) => (
            "SELECT id, node_type, dis, parent_id, created_ms, updated_ms FROM node ORDER BY dis".into(),
            vec![],
        ),
    };

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows = match stmt.query_map(param_refs.as_slice(), |row| {
        Ok(NodeRecord {
            id: row.get(0)?,
            node_type: row.get(1)?,
            dis: row.get(2)?,
            parent_id: row.get(3)?,
            tags: HashMap::new(),
            refs: HashMap::new(),
            properties: HashMap::new(),
            capabilities: NodeCapabilities::default(),
            binding: None,
            created_ms: row.get(4)?,
            updated_ms: row.get(5)?,
        })
    }) {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    let mut result: Vec<NodeRecord> = rows.flatten().collect();
    load_node_relations_bulk(conn, &mut result);
    result
}

fn delete_node_db(conn: &rusqlite::Connection, id: &str) -> Result<(), NodeError> {
    let changes = conn
        .execute("DELETE FROM node WHERE id = ?1", rusqlite::params![id])
        .map_err(|e| NodeError::Db(e.to_string()))?;
    if changes == 0 {
        return Err(NodeError::NotFound(id.to_string()));
    }
    Ok(())
}

fn update_dis_db(conn: &rusqlite::Connection, id: &str, dis: &str) -> Result<(), NodeError> {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let changes = conn
        .execute(
            "UPDATE node SET dis = ?1, updated_ms = ?2 WHERE id = ?3",
            rusqlite::params![dis, now_ms, id],
        )
        .map_err(|e| NodeError::Db(e.to_string()))?;
    if changes == 0 {
        return Err(NodeError::NotFound(id.to_string()));
    }
    Ok(())
}

fn update_parent_db(
    conn: &rusqlite::Connection,
    id: &str,
    parent_id: Option<&str>,
) -> Result<(), NodeError> {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let changes = conn
        .execute(
            "UPDATE node SET parent_id = ?1, updated_ms = ?2 WHERE id = ?3",
            rusqlite::params![parent_id, now_ms, id],
        )
        .map_err(|e| NodeError::Db(e.to_string()))?;
    if changes == 0 {
        return Err(NodeError::NotFound(id.to_string()));
    }
    Ok(())
}

fn set_tag_db(
    conn: &rusqlite::Connection,
    id: &str,
    tag_name: &str,
    tag_value: Option<&str>,
) -> Result<(), NodeError> {
    conn.execute(
        "INSERT OR REPLACE INTO node_tag (node_id, tag_name, tag_value) VALUES (?1, ?2, ?3)",
        rusqlite::params![id, tag_name, tag_value],
    )
    .map_err(|e| NodeError::Db(e.to_string()))?;
    Ok(())
}

fn set_tags_db(
    conn: &rusqlite::Connection,
    id: &str,
    tags: &[(String, Option<String>)],
) -> Result<(), NodeError> {
    for (name, value) in tags {
        conn.execute(
            "INSERT OR REPLACE INTO node_tag (node_id, tag_name, tag_value) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, name, value],
        )
        .map_err(|e| NodeError::Db(e.to_string()))?;
    }
    Ok(())
}

fn remove_tag_db(conn: &rusqlite::Connection, id: &str, tag_name: &str) -> Result<(), NodeError> {
    conn.execute(
        "DELETE FROM node_tag WHERE node_id = ?1 AND tag_name = ?2",
        rusqlite::params![id, tag_name],
    )
    .map_err(|e| NodeError::Db(e.to_string()))?;
    Ok(())
}

fn set_ref_db(
    conn: &rusqlite::Connection,
    source_id: &str,
    ref_tag: &str,
    target_id: &str,
) -> Result<(), NodeError> {
    conn.execute(
        "INSERT OR REPLACE INTO node_ref (source_id, ref_tag, target_id) VALUES (?1, ?2, ?3)",
        rusqlite::params![source_id, ref_tag, target_id],
    )
    .map_err(|e| NodeError::Db(e.to_string()))?;
    Ok(())
}

fn remove_ref_db(
    conn: &rusqlite::Connection,
    source_id: &str,
    ref_tag: &str,
) -> Result<(), NodeError> {
    conn.execute(
        "DELETE FROM node_ref WHERE source_id = ?1 AND ref_tag = ?2",
        rusqlite::params![source_id, ref_tag],
    )
    .map_err(|e| NodeError::Db(e.to_string()))?;
    Ok(())
}

fn set_property_db(
    conn: &rusqlite::Connection,
    id: &str,
    key: &str,
    value: &str,
) -> Result<(), NodeError> {
    conn.execute(
        "INSERT OR REPLACE INTO node_property (node_id, key, value) VALUES (?1, ?2, ?3)",
        rusqlite::params![id, key, value],
    )
    .map_err(|e| NodeError::Db(e.to_string()))?;
    Ok(())
}

fn set_binding_db(
    conn: &rusqlite::Connection,
    id: &str,
    binding: &Option<ProtocolBinding>,
) -> Result<(), NodeError> {
    match binding {
        Some(b) => {
            let json = serde_json::to_string(b).unwrap_or_default();
            conn.execute(
                "INSERT OR REPLACE INTO protocol_binding (node_id, binding_json) VALUES (?1, ?2)",
                rusqlite::params![id, json],
            )
            .map_err(|e| NodeError::Db(e.to_string()))?;
        }
        None => {
            conn.execute(
                "DELETE FROM protocol_binding WHERE node_id = ?1",
                rusqlite::params![id],
            )
            .map_err(|e| NodeError::Db(e.to_string()))?;
        }
    }
    Ok(())
}

fn set_capabilities_db(
    conn: &rusqlite::Connection,
    id: &str,
    caps: &NodeCapabilities,
) -> Result<(), NodeError> {
    conn.execute(
        "INSERT OR REPLACE INTO node_capability (node_id, readable, writable, historizable, alarmable, schedulable) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            id,
            caps.readable as i32,
            caps.writable as i32,
            caps.historizable as i32,
            caps.alarmable as i32,
            caps.schedulable as i32,
        ],
    ).map_err(|e| NodeError::Db(e.to_string()))?;
    Ok(())
}

fn find_by_tag_db(
    conn: &rusqlite::Connection,
    tag_name: &str,
    tag_value: Option<&str>,
) -> Vec<NodeRecord> {
    let (sql, params): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = match tag_value {
        Some(v) => (
            "SELECT n.id, n.node_type, n.dis, n.parent_id, n.created_ms, n.updated_ms FROM node n JOIN node_tag t ON n.id = t.node_id WHERE t.tag_name = ?1 AND t.tag_value = ?2 ORDER BY n.dis",
            vec![Box::new(tag_name.to_string()), Box::new(v.to_string())],
        ),
        None => (
            "SELECT n.id, n.node_type, n.dis, n.parent_id, n.created_ms, n.updated_ms FROM node n JOIN node_tag t ON n.id = t.node_id WHERE t.tag_name = ?1 ORDER BY n.dis",
            vec![Box::new(tag_name.to_string())],
        ),
    };

    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows = match stmt.query_map(param_refs.as_slice(), |row| {
        Ok(NodeRecord {
            id: row.get(0)?,
            node_type: row.get(1)?,
            dis: row.get(2)?,
            parent_id: row.get(3)?,
            tags: HashMap::new(),
            refs: HashMap::new(),
            properties: HashMap::new(),
            capabilities: NodeCapabilities::default(),
            binding: None,
            created_ms: row.get(4)?,
            updated_ms: row.get(5)?,
        })
    }) {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    let mut result: Vec<NodeRecord> = rows.flatten().collect();
    load_node_relations_bulk(conn, &mut result);
    result
}

fn get_hierarchy_db(conn: &rusqlite::Connection, root_id: Option<&str>) -> Vec<NodeRecord> {
    let sql = match root_id {
        Some(_) => {
            "WITH RECURSIVE descendants(id) AS (
                SELECT id FROM node WHERE id = ?1
                UNION ALL
                SELECT n.id FROM node n JOIN descendants d ON n.parent_id = d.id
            )
            SELECT n.id, n.node_type, n.dis, n.parent_id, n.created_ms, n.updated_ms
            FROM node n JOIN descendants d ON n.id = d.id
            ORDER BY n.dis"
        }
        None => {
            "WITH RECURSIVE descendants(id) AS (
                SELECT id FROM node WHERE parent_id IS NULL
                UNION ALL
                SELECT n.id FROM node n JOIN descendants d ON n.parent_id = d.id
            )
            SELECT n.id, n.node_type, n.dis, n.parent_id, n.created_ms, n.updated_ms
            FROM node n JOIN descendants d ON n.id = d.id
            ORDER BY n.dis"
        }
    };

    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let params: Vec<Box<dyn rusqlite::types::ToSql>> = match root_id {
        Some(id) => vec![Box::new(id.to_string())],
        None => vec![],
    };
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let rows = match stmt.query_map(param_refs.as_slice(), |row| {
        Ok(NodeRecord {
            id: row.get(0)?,
            node_type: row.get(1)?,
            dis: row.get(2)?,
            parent_id: row.get(3)?,
            tags: HashMap::new(),
            refs: HashMap::new(),
            properties: HashMap::new(),
            capabilities: NodeCapabilities::default(),
            binding: None,
            created_ms: row.get(4)?,
            updated_ms: row.get(5)?,
        })
    }) {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    let mut result: Vec<NodeRecord> = rows.flatten().collect();
    load_node_relations_bulk(conn, &mut result);
    result
}

fn get_ancestors_db(conn: &rusqlite::Connection, id: &str) -> Vec<NodeRecord> {
    let sql = "
        WITH RECURSIVE ancestors(id, depth) AS (
            SELECT id, 0 FROM node WHERE id = ?1
            UNION ALL
            SELECT n.parent_id, a.depth + 1
            FROM node n
            JOIN ancestors a ON n.id = a.id
            WHERE n.parent_id IS NOT NULL
        )
        SELECT n.id, n.node_type, n.dis, n.parent_id, n.created_ms, n.updated_ms
        FROM node n
        WHERE n.id IN (SELECT id FROM ancestors)
        ORDER BY (SELECT depth FROM ancestors WHERE ancestors.id = n.id) DESC
    ";

    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let rows = match stmt.query_map(rusqlite::params![id], |row| {
        Ok(NodeRecord {
            id: row.get(0)?,
            node_type: row.get(1)?,
            dis: row.get(2)?,
            parent_id: row.get(3)?,
            tags: HashMap::new(),
            refs: HashMap::new(),
            properties: HashMap::new(),
            capabilities: NodeCapabilities::default(),
            binding: None,
            created_ms: row.get(4)?,
            updated_ms: row.get(5)?,
        })
    }) {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    let mut result: Vec<NodeRecord> = rows.flatten().collect();
    load_node_relations_bulk(conn, &mut result);
    result
}

fn find_by_ref_db(conn: &rusqlite::Connection, ref_tag: &str, target_id: &str) -> Vec<NodeRecord> {
    let sql = "SELECT n.id, n.node_type, n.dis, n.parent_id, n.created_ms, n.updated_ms
               FROM node n JOIN node_ref r ON n.id = r.source_id
               WHERE r.ref_tag = ?1 AND r.target_id = ?2 ORDER BY n.dis";
    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let rows = match stmt.query_map(rusqlite::params![ref_tag, target_id], |row| {
        Ok(NodeRecord {
            id: row.get(0)?,
            node_type: row.get(1)?,
            dis: row.get(2)?,
            parent_id: row.get(3)?,
            tags: HashMap::new(),
            refs: HashMap::new(),
            properties: HashMap::new(),
            capabilities: NodeCapabilities::default(),
            binding: None,
            created_ms: row.get(4)?,
            updated_ms: row.get(5)?,
        })
    }) {
        Ok(r) => r,
        Err(_) => return vec![],
    };
    let mut result: Vec<NodeRecord> = rows.flatten().collect();
    load_node_relations_bulk(conn, &mut result);
    result
}

fn find_all_referencing_db(
    conn: &rusqlite::Connection,
    target_id: &str,
) -> Vec<(String, NodeRecord)> {
    let sql = "SELECT r.ref_tag, n.id, n.node_type, n.dis, n.parent_id, n.created_ms, n.updated_ms
               FROM node n JOIN node_ref r ON n.id = r.source_id
               WHERE r.target_id = ?1 ORDER BY r.ref_tag, n.dis";
    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let rows = match stmt.query_map(rusqlite::params![target_id], |row| {
        let ref_tag: String = row.get(0)?;
        let rec = NodeRecord {
            id: row.get(1)?,
            node_type: row.get(2)?,
            dis: row.get(3)?,
            parent_id: row.get(4)?,
            tags: HashMap::new(),
            refs: HashMap::new(),
            properties: HashMap::new(),
            capabilities: NodeCapabilities::default(),
            binding: None,
            created_ms: row.get(5)?,
            updated_ms: row.get(6)?,
        };
        Ok((ref_tag, rec))
    }) {
        Ok(r) => r,
        Err(_) => return vec![],
    };
    let mut result: Vec<(String, NodeRecord)> = rows.flatten().collect();
    // Unzip, bulk load, re-zip
    let mut recs: Vec<NodeRecord> = result.iter().map(|(_, r)| r.clone()).collect();
    load_node_relations_bulk(conn, &mut recs);
    for ((_tag, dest), src) in result.iter_mut().zip(recs.into_iter()) {
        dest.tags = src.tags;
        dest.refs = src.refs;
        dest.properties = src.properties;
        dest.capabilities = src.capabilities;
        dest.binding = src.binding;
    }
    result
}

fn set_refs_db(
    conn: &rusqlite::Connection,
    updates: &[(NodeId, String, NodeId)],
) -> Result<(), NodeError> {
    let tx: rusqlite::Transaction<'_> = conn
        .unchecked_transaction()
        .map_err(|e| NodeError::Db(e.to_string()))?;
    for (source_id, ref_tag, target_id) in updates {
        tx.execute(
            "INSERT OR REPLACE INTO node_ref (source_id, ref_tag, target_id) VALUES (?1, ?2, ?3)",
            rusqlite::params![source_id, ref_tag, target_id],
        )
        .map_err(|e| NodeError::Db(e.to_string()))?;
    }
    tx.commit().map_err(|e| NodeError::Db(e.to_string()))?;
    Ok(())
}

// ----------------------------------------------------------------
// Public startup function
// ----------------------------------------------------------------

pub fn start_node_store() -> NodeStore {
    start_node_store_with_path(&PathBuf::from("data/nodes.db"))
}

pub fn start_node_store_with_path(db_path: &Path) -> NodeStore {
    let db_dir = db_path.parent().unwrap_or(Path::new("."));
    if !db_dir.exists() {
        std::fs::create_dir_all(db_dir).expect("failed to create data directory");
    }

    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (version_tx, version_rx) = watch::channel(0u64);

    let path_clone = db_path.to_path_buf();
    std::thread::Builder::new()
        .name("node-sqlite".into())
        .spawn(move || run_sqlite_thread(&path_clone, cmd_rx))
        .expect("failed to spawn node SQLite thread");

    NodeStore {
        hot: Arc::new(RwLock::new(HashMap::new())),
        cmd_tx,
        version_tx,
        version_rx,
        event_bus: None,
    }
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::profile::PointValue;
    use crate::node::{Node, NodeCapabilities, NodeType};
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_db_path() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join("opencrate_node_tests");
        std::fs::create_dir_all(&dir).ok();
        dir.join(format!("test_nodes_{}_{}.db", std::process::id(), n))
    }

    #[tokio::test]
    async fn create_and_get_node() {
        let path = temp_db_path();
        let store = start_node_store_with_path(&path);

        let node = Node::new("ahu-1", NodeType::Equip, "AHU-1");
        store.create_node(node).await.unwrap();

        let rec = store.get_node("ahu-1").await.unwrap();
        assert_eq!(rec.id, "ahu-1");
        assert_eq!(rec.node_type, "equip");
        assert_eq!(rec.dis, "AHU-1");
    }

    #[tokio::test]
    async fn create_point_with_capabilities() {
        let path = temp_db_path();
        let store = start_node_store_with_path(&path);

        // Parent must exist first (FK constraint)
        store
            .create_node(Node::new("ahu-1", NodeType::Equip, "AHU-1"))
            .await
            .unwrap();

        let node = Node::new("ahu-1/dat", NodeType::Point, "Discharge Air Temp")
            .with_parent("ahu-1")
            .with_capabilities(NodeCapabilities::new(true, false, true, true, false));

        store.create_node(node).await.unwrap();

        let rec = store.get_node("ahu-1/dat").await.unwrap();
        assert!(rec.capabilities.readable);
        assert!(rec.capabilities.historizable);
        assert!(!rec.capabilities.writable);
        assert_eq!(rec.parent_id.as_deref(), Some("ahu-1"));
    }

    #[tokio::test]
    async fn tags_and_refs() {
        let path = temp_db_path();
        let store = start_node_store_with_path(&path);

        let node = Node::new("ahu-1", NodeType::Equip, "AHU-1");
        store.create_node(node).await.unwrap();

        store.set_tag("ahu-1", "ahu", None).await.unwrap();
        store.set_tag("ahu-1", "dis", Some("AHU-1")).await.unwrap();

        let rec = store.get_node("ahu-1").await.unwrap();
        assert!(rec.tags.contains_key("ahu"));
        assert_eq!(rec.tags.get("dis"), Some(&Some("AHU-1".to_string())));

        // Remove tag
        store.remove_tag("ahu-1", "dis").await.unwrap();
        let rec = store.get_node("ahu-1").await.unwrap();
        assert!(!rec.tags.contains_key("dis"));
    }

    #[tokio::test]
    async fn hot_cache_values() {
        let path = temp_db_path();
        let store = start_node_store_with_path(&path);

        store.init_hot("ahu-1/dat", Some(PointValue::Float(72.0)));
        let snap = store.get_snapshot("ahu-1/dat").unwrap();
        assert!(
            matches!(snap.value, Some(PointValue::Float(f)) if (f - 72.0).abs() < f64::EPSILON)
        );

        store.update_value("ahu-1/dat", PointValue::Float(73.5));
        let snap = store.get_snapshot("ahu-1/dat").unwrap();
        assert!(
            matches!(snap.value, Some(PointValue::Float(f)) if (f - 73.5).abs() < f64::EPSILON)
        );

        store.set_status("ahu-1/dat", PointStatusFlags::ALARM);
        let snap = store.get_snapshot("ahu-1/dat").unwrap();
        assert!(snap.status.has(PointStatusFlags::ALARM));

        store.clear_status("ahu-1/dat", PointStatusFlags::ALARM);
        let snap = store.get_snapshot("ahu-1/dat").unwrap();
        assert!(!snap.status.has(PointStatusFlags::ALARM));
    }

    #[tokio::test]
    async fn list_and_delete() {
        let path = temp_db_path();
        let store = start_node_store_with_path(&path);

        store
            .create_node(Node::new("a", NodeType::Equip, "A"))
            .await
            .unwrap();
        store
            .create_node(Node::new("b", NodeType::Point, "B"))
            .await
            .unwrap();
        store
            .create_node(Node::new("c", NodeType::Equip, "C"))
            .await
            .unwrap();

        let all = store.list_nodes(None, None).await;
        assert_eq!(all.len(), 3);

        let equips = store.list_nodes(Some("equip"), None).await;
        assert_eq!(equips.len(), 2);

        store.delete_node("b").await.unwrap();
        let all = store.list_nodes(None, None).await;
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn find_by_tag() {
        let path = temp_db_path();
        let store = start_node_store_with_path(&path);

        store
            .create_node(Node::new("ahu-1", NodeType::Equip, "AHU"))
            .await
            .unwrap();
        store
            .create_node(Node::new("vav-1", NodeType::Equip, "VAV"))
            .await
            .unwrap();

        store.set_tag("ahu-1", "ahu", None).await.unwrap();
        store.set_tag("vav-1", "vav", None).await.unwrap();

        let ahus = store.find_by_tag("ahu", None).await;
        assert_eq!(ahus.len(), 1);
        assert_eq!(ahus[0].id, "ahu-1");
    }

    #[tokio::test]
    async fn hierarchy_query() {
        let path = temp_db_path();
        let store = start_node_store_with_path(&path);

        store
            .create_node(Node::new("site-1", NodeType::Site, "Building"))
            .await
            .unwrap();
        store
            .create_node(Node::new("floor-1", NodeType::Space, "Floor 1").with_parent("site-1"))
            .await
            .unwrap();
        store
            .create_node(Node::new("ahu-1", NodeType::Equip, "AHU-1").with_parent("floor-1"))
            .await
            .unwrap();

        let tree = store.get_hierarchy(Some("site-1")).await;
        assert_eq!(tree.len(), 3);
    }

    #[tokio::test]
    async fn binding_persistence() {
        let path = temp_db_path();
        let store = start_node_store_with_path(&path);

        let node = Node::new("ahu-1/dat", NodeType::Point, "DAT")
            .with_binding(ProtocolBinding::bacnet(1000, "analog-input", 1));

        store.create_node(node).await.unwrap();

        let rec = store.get_node("ahu-1/dat").await.unwrap();
        let binding = rec.binding.expect("expected binding");
        assert!(binding.is_bacnet());
        assert_eq!(binding.config["device_instance"], 1000);
    }
}
