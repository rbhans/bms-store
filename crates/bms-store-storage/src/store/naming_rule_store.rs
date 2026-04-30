use std::path::Path;

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot, watch};

use super::migration::{run_migrations, Migration};

// ----------------------------------------------------------------
// Public types
// ----------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NamingRuleKind {
    Sequence,
    FindReplace,
    Template,
}

impl NamingRuleKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Sequence => "sequence",
            Self::FindReplace => "find_replace",
            Self::Template => "template",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "sequence" => Some(Self::Sequence),
            "find_replace" => Some(Self::FindReplace),
            "template" => Some(Self::Template),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamingRule {
    /// UUID-style identifier.
    pub id: String,
    /// User-given label, e.g. "VAV-### sequence".
    pub name: String,
    /// Rule kind discriminant.
    pub kind: NamingRuleKind,
    /// Rule-specific configuration; schema is kind-dependent.
    pub spec: serde_json::Value,
    /// Unix timestamp (ms) when the rule was created.
    pub created_ms: i64,
    /// Unix timestamp (ms) of the last update.
    pub updated_ms: i64,
}

// ----------------------------------------------------------------
// Error type
// ----------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum NamingRuleError {
    #[error("database error: {0}")]
    Db(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("channel closed")]
    ChannelClosed,
}

// ----------------------------------------------------------------
// Internal commands
// ----------------------------------------------------------------

enum NamingRuleCmd {
    Create {
        rule: NamingRule,
        reply: oneshot::Sender<Result<String, NamingRuleError>>,
    },
    Update {
        id: String,
        rule: NamingRule,
        reply: oneshot::Sender<Result<(), NamingRuleError>>,
    },
    Delete {
        id: String,
        reply: oneshot::Sender<Result<(), NamingRuleError>>,
    },
    List {
        reply: oneshot::Sender<Result<Vec<NamingRule>, NamingRuleError>>,
    },
    Get {
        id: String,
        reply: oneshot::Sender<Result<Option<NamingRule>, NamingRuleError>>,
    },
}

// ----------------------------------------------------------------
// Store handle
// ----------------------------------------------------------------

#[derive(Clone)]
pub struct NamingRuleStore {
    cmd_tx: mpsc::UnboundedSender<NamingRuleCmd>,
    #[allow(dead_code)]
    version_tx: watch::Sender<u64>,
    version_rx: watch::Receiver<u64>,
}

impl PartialEq for NamingRuleStore {
    fn eq(&self, _: &Self) -> bool {
        true // singleton
    }
}

impl NamingRuleStore {
    /// Persist a new rule; returns its id.
    pub async fn create_rule(&self, rule: NamingRule) -> Result<String, NamingRuleError> {
        let (reply, rx) = oneshot::channel();
        self.cmd_tx
            .send(NamingRuleCmd::Create { rule, reply })
            .map_err(|_| NamingRuleError::ChannelClosed)?;
        rx.await.map_err(|_| NamingRuleError::ChannelClosed)?
    }

    /// Overwrite an existing rule by id.
    pub async fn update_rule(&self, id: &str, rule: NamingRule) -> Result<(), NamingRuleError> {
        let (reply, rx) = oneshot::channel();
        self.cmd_tx
            .send(NamingRuleCmd::Update {
                id: id.to_string(),
                rule,
                reply,
            })
            .map_err(|_| NamingRuleError::ChannelClosed)?;
        rx.await.map_err(|_| NamingRuleError::ChannelClosed)?
    }

    /// Remove a rule by id.
    pub async fn delete_rule(&self, id: &str) -> Result<(), NamingRuleError> {
        let (reply, rx) = oneshot::channel();
        self.cmd_tx
            .send(NamingRuleCmd::Delete {
                id: id.to_string(),
                reply,
            })
            .map_err(|_| NamingRuleError::ChannelClosed)?;
        rx.await.map_err(|_| NamingRuleError::ChannelClosed)?
    }

    /// Return all rules ordered by created_ms ascending.
    pub async fn list_rules(&self) -> Vec<NamingRule> {
        let (reply, rx) = oneshot::channel();
        let _ = self.cmd_tx.send(NamingRuleCmd::List { reply });
        match rx.await {
            Ok(Ok(rules)) => rules,
            _ => Vec::new(),
        }
    }

    /// Fetch a single rule by id, returning None if not found.
    pub async fn get_rule(&self, id: &str) -> Option<NamingRule> {
        let (reply, rx) = oneshot::channel();
        let _ = self.cmd_tx.send(NamingRuleCmd::Get {
            id: id.to_string(),
            reply,
        });
        match rx.await {
            Ok(Ok(maybe)) => maybe,
            _ => None,
        }
    }

    /// Subscribe to version bumps (incremented on every write).
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.version_rx.clone()
    }
}

// ----------------------------------------------------------------
// Schema migrations
// ----------------------------------------------------------------

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    label: "initial naming_rules schema",
    sql: "
CREATE TABLE IF NOT EXISTS naming_rules (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    kind        TEXT NOT NULL,
    spec        TEXT NOT NULL DEFAULT '{}',
    created_ms  INTEGER NOT NULL,
    updated_ms  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_naming_rules_created ON naming_rules(created_ms ASC);
",
}];

// ----------------------------------------------------------------
// SQLite worker thread
// ----------------------------------------------------------------

fn run_sqlite_thread(
    db_path: &Path,
    rx: mpsc::UnboundedReceiver<NamingRuleCmd>,
    version_tx: watch::Sender<u64>,
) {
    let conn = rusqlite::Connection::open(db_path).expect("failed to open naming_rules DB");
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
        .expect("failed to set pragmas on naming_rules DB");
    run_migrations(&conn, "naming_rules", MIGRATIONS)
        .expect("naming_rules: schema migration failed");

    let mut rx = rx;
    let mut version: u64 = 0;

    while let Some(cmd) = rx.blocking_recv() {
        match cmd {
            NamingRuleCmd::Create { rule, reply } => {
                let result = conn.execute(
                    "INSERT INTO naming_rules (id, name, kind, spec, created_ms, updated_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![
                        rule.id,
                        rule.name,
                        rule.kind.as_str(),
                        rule.spec.to_string(),
                        rule.created_ms,
                        rule.updated_ms,
                    ],
                );
                match result {
                    Ok(_) => {
                        let id = conn
                            .query_row(
                                "SELECT id FROM naming_rules ORDER BY rowid DESC LIMIT 1",
                                [],
                                |row| row.get::<_, String>(0),
                            )
                            .unwrap_or_default();
                        version += 1;
                        let _ = version_tx.send(version);
                        let _ = reply.send(Ok(id));
                    }
                    Err(e) => {
                        let _ = reply.send(Err(NamingRuleError::Db(e.to_string())));
                    }
                }
            }

            NamingRuleCmd::Update { id, rule, reply } => {
                let result = conn.execute(
                    "UPDATE naming_rules SET name=?1, kind=?2, spec=?3, updated_ms=?4 WHERE id=?5",
                    rusqlite::params![
                        rule.name,
                        rule.kind.as_str(),
                        rule.spec.to_string(),
                        rule.updated_ms,
                        id,
                    ],
                );
                match result {
                    Ok(changed) if changed > 0 => {
                        version += 1;
                        let _ = version_tx.send(version);
                        let _ = reply.send(Ok(()));
                    }
                    Ok(_) => {
                        let _ = reply.send(Err(NamingRuleError::NotFound(id)));
                    }
                    Err(e) => {
                        let _ = reply.send(Err(NamingRuleError::Db(e.to_string())));
                    }
                }
            }

            NamingRuleCmd::Delete { id, reply } => {
                let result = conn.execute(
                    "DELETE FROM naming_rules WHERE id=?1",
                    rusqlite::params![id],
                );
                match result {
                    Ok(changed) if changed > 0 => {
                        version += 1;
                        let _ = version_tx.send(version);
                        let _ = reply.send(Ok(()));
                    }
                    Ok(_) => {
                        let _ = reply.send(Err(NamingRuleError::NotFound(id)));
                    }
                    Err(e) => {
                        let _ = reply.send(Err(NamingRuleError::Db(e.to_string())));
                    }
                }
            }

            NamingRuleCmd::List { reply } => {
                let _ = reply.send(list_rules_inner(&conn));
            }

            NamingRuleCmd::Get { id, reply } => {
                let _ = reply.send(get_rule_inner(&conn, &id));
            }
        }
    }
}

fn list_rules_inner(
    conn: &rusqlite::Connection,
) -> Result<Vec<NamingRule>, NamingRuleError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, kind, spec, created_ms, updated_ms
             FROM naming_rules ORDER BY created_ms ASC",
        )
        .map_err(|e| NamingRuleError::Db(e.to_string()))?;

    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })
        .map_err(|e| NamingRuleError::Db(e.to_string()))?;

    let mut rules = Vec::new();
    for row in rows {
        let (id, name, kind_str, spec_str, created_ms, updated_ms) =
            row.map_err(|e| NamingRuleError::Db(e.to_string()))?;
        let kind = NamingRuleKind::from_str(&kind_str)
            .unwrap_or(NamingRuleKind::Template);
        let spec: serde_json::Value =
            serde_json::from_str(&spec_str).unwrap_or(serde_json::Value::Object(Default::default()));
        rules.push(NamingRule {
            id,
            name,
            kind,
            spec,
            created_ms,
            updated_ms,
        });
    }
    Ok(rules)
}

fn get_rule_inner(
    conn: &rusqlite::Connection,
    id: &str,
) -> Result<Option<NamingRule>, NamingRuleError> {
    let result = conn.query_row(
        "SELECT id, name, kind, spec, created_ms, updated_ms FROM naming_rules WHERE id=?1",
        rusqlite::params![id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
            ))
        },
    );

    match result {
        Ok((id, name, kind_str, spec_str, created_ms, updated_ms)) => {
            let kind = NamingRuleKind::from_str(&kind_str).unwrap_or(NamingRuleKind::Template);
            let spec: serde_json::Value =
                serde_json::from_str(&spec_str).unwrap_or(serde_json::Value::Object(Default::default()));
            Ok(Some(NamingRule {
                id,
                name,
                kind,
                spec,
                created_ms,
                updated_ms,
            }))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(NamingRuleError::Db(e.to_string())),
    }
}

// ----------------------------------------------------------------
// Constructor
// ----------------------------------------------------------------

pub fn start_naming_rule_store_with_path(db_path: &Path) -> NamingRuleStore {
    let path_clone = db_path.to_path_buf();
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (version_tx, version_rx) = watch::channel(0u64);
    let vtx = version_tx.clone();

    std::thread::Builder::new()
        .name("naming-rule-sqlite".into())
        .spawn(move || run_sqlite_thread(&path_clone, cmd_rx, vtx))
        .expect("failed to spawn naming_rule_store SQLite thread");

    tracing::info!("naming_rule_store started");

    NamingRuleStore {
        cmd_tx,
        version_tx,
        version_rx,
    }
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};
    use super::*;

    fn now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    }

    fn test_store(label: &str) -> NamingRuleStore {
        let path = std::path::PathBuf::from(format!("/tmp/test_naming_rules_{label}.db"));
        if path.exists() {
            std::fs::remove_file(&path).ok();
        }
        start_naming_rule_store_with_path(&path)
    }

    fn make_rule(name: &str, kind: NamingRuleKind) -> NamingRule {
        let now = now_ms();
        NamingRule {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            kind,
            spec: serde_json::json!({ "prefix": "VAV-", "start": 101 }),
            created_ms: now,
            updated_ms: now,
        }
    }

    #[tokio::test]
    async fn create_and_list() {
        let store = test_store("create_list");
        let rule = make_rule("VAV sequence", NamingRuleKind::Sequence);
        let id = store.create_rule(rule.clone()).await.unwrap();
        assert_eq!(id, rule.id);

        let rules = store.list_rules().await;
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].name, "VAV sequence");
        assert_eq!(rules[0].kind, NamingRuleKind::Sequence);
    }

    #[tokio::test]
    async fn get_by_id() {
        let store = test_store("get_by_id");
        let rule = make_rule("Find & Replace", NamingRuleKind::FindReplace);
        let id = store.create_rule(rule.clone()).await.unwrap();

        let fetched = store.get_rule(&id).await.unwrap();
        assert_eq!(fetched.name, "Find & Replace");
        assert_eq!(fetched.kind, NamingRuleKind::FindReplace);
    }

    #[tokio::test]
    async fn get_missing_returns_none() {
        let store = test_store("get_missing");
        assert!(store.get_rule("no-such-id").await.is_none());
    }

    #[tokio::test]
    async fn update_rule() {
        let store = test_store("update");
        let mut rule = make_rule("Template rule", NamingRuleKind::Template);
        let id = store.create_rule(rule.clone()).await.unwrap();

        rule.name = "Updated Template".to_string();
        rule.updated_ms = now_ms();
        store.update_rule(&id, rule).await.unwrap();

        let fetched = store.get_rule(&id).await.unwrap();
        assert_eq!(fetched.name, "Updated Template");
    }

    #[tokio::test]
    async fn update_missing_returns_not_found() {
        let store = test_store("update_missing");
        let rule = make_rule("x", NamingRuleKind::Sequence);
        let err = store.update_rule("no-such-id", rule).await.unwrap_err();
        assert!(matches!(err, NamingRuleError::NotFound(_)));
    }

    #[tokio::test]
    async fn delete_rule() {
        let store = test_store("delete");
        let rule = make_rule("To delete", NamingRuleKind::Sequence);
        let id = store.create_rule(rule).await.unwrap();

        store.delete_rule(&id).await.unwrap();
        assert!(store.get_rule(&id).await.is_none());

        let rules = store.list_rules().await;
        assert!(rules.is_empty());
    }

    #[tokio::test]
    async fn delete_missing_returns_not_found() {
        let store = test_store("delete_missing");
        let err = store.delete_rule("no-such-id").await.unwrap_err();
        assert!(matches!(err, NamingRuleError::NotFound(_)));
    }

    #[tokio::test]
    async fn round_trip_spec() {
        let store = test_store("spec_roundtrip");
        let spec = serde_json::json!({
            "find": "VAV",
            "replace": "FCU",
            "case_sensitive": true
        });
        let now = now_ms();
        let rule = NamingRule {
            id: uuid::Uuid::new_v4().to_string(),
            name: "FCU rename".to_string(),
            kind: NamingRuleKind::FindReplace,
            spec: spec.clone(),
            created_ms: now,
            updated_ms: now,
        };
        let id = store.create_rule(rule).await.unwrap();
        let fetched = store.get_rule(&id).await.unwrap();
        assert_eq!(fetched.spec["find"], "VAV");
        assert_eq!(fetched.spec["replace"], "FCU");
        assert_eq!(fetched.spec["case_sensitive"], true);
    }

    #[tokio::test]
    async fn version_increments_on_write() {
        let store = test_store("version_bump");
        let mut rx = store.subscribe();

        let rule = make_rule("test", NamingRuleKind::Sequence);
        store.create_rule(rule).await.unwrap();

        rx.changed().await.unwrap();
        assert_eq!(*rx.borrow(), 1);
    }

    #[tokio::test]
    async fn list_ordered_by_created() {
        let store = test_store("list_order");
        let now = now_ms();
        for i in 0..5u64 {
            let rule = NamingRule {
                id: uuid::Uuid::new_v4().to_string(),
                name: format!("rule-{i}"),
                kind: NamingRuleKind::Sequence,
                spec: serde_json::Value::Null,
                created_ms: now + i as i64,
                updated_ms: now + i as i64,
            };
            store.create_rule(rule).await.unwrap();
        }
        let rules = store.list_rules().await;
        assert_eq!(rules.len(), 5);
        for i in 0..4 {
            assert!(rules[i].created_ms <= rules[i + 1].created_ms);
        }
    }
}
