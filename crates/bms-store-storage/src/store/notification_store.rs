use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

use super::migration::{run_migrations, Migration};

// ----------------------------------------------------------------
// Public types
// ----------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelType {
    Webhook,
    Email,
    Sms,
}

impl ChannelType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Webhook => "webhook",
            Self::Email => "email",
            Self::Sms => "sms",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "webhook" => Some(Self::Webhook),
            "email" => Some(Self::Email),
            "sms" => Some(Self::Sms),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Webhook => "Webhook",
            Self::Email => "Email",
            Self::Sms => "SMS",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryStatus {
    Pending,
    Delivered,
    Failed,
    Retrying,
}

impl DeliveryStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Delivered => "delivered",
            Self::Failed => "failed",
            Self::Retrying => "retrying",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "delivered" => Some(Self::Delivered),
            "failed" => Some(Self::Failed),
            "retrying" => Some(Self::Retrying),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlarmRecipient {
    pub id: i64,
    pub name: String,
    pub channel_type: ChannelType,
    pub address: String,
    pub channel_config: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutingRule {
    pub id: i64,
    pub recipient_id: i64,
    pub min_severity: String,
    pub device_filter: String,
    pub alarm_type_filter: String,
    pub schedule_id: Option<i64>,
    pub escalation_tier: u8,
    pub escalation_delay_mins: u32,
    pub notify_on_clear: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlarmShelving {
    pub id: i64,
    pub alarm_config_id: Option<i64>,
    pub device_id: Option<String>,
    pub shelved_by: String,
    pub reason: String,
    pub expires_ms: Option<i64>,
    pub created_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotificationRecord {
    pub id: i64,
    pub alarm_id: i64,
    pub recipient_id: i64,
    pub rule_id: i64,
    pub channel_type: ChannelType,
    pub address: String,
    pub status: DeliveryStatus,
    pub attempt_count: u32,
    pub last_error: Option<String>,
    pub created_ms: i64,
    pub updated_ms: i64,
    pub next_retry_ms: Option<i64>,
}

#[derive(Debug, thiserror::Error)]
pub enum NotificationError {
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

enum NotifCmd {
    // Recipient CRUD
    CreateRecipient {
        name: String,
        channel_type: ChannelType,
        address: String,
        channel_config: String,
        reply: oneshot::Sender<Result<i64, NotificationError>>,
    },
    UpdateRecipient {
        id: i64,
        name: String,
        channel_type: ChannelType,
        address: String,
        channel_config: String,
        enabled: bool,
        reply: oneshot::Sender<Result<(), NotificationError>>,
    },
    DeleteRecipient {
        id: i64,
        reply: oneshot::Sender<Result<(), NotificationError>>,
    },
    ListRecipients {
        reply: oneshot::Sender<Vec<AlarmRecipient>>,
    },
    // Routing rule CRUD
    CreateRule {
        recipient_id: i64,
        min_severity: String,
        device_filter: String,
        alarm_type_filter: String,
        schedule_id: Option<i64>,
        escalation_tier: u8,
        escalation_delay_mins: u32,
        notify_on_clear: bool,
        reply: oneshot::Sender<Result<i64, NotificationError>>,
    },
    UpdateRule {
        id: i64,
        min_severity: String,
        device_filter: String,
        alarm_type_filter: String,
        schedule_id: Option<i64>,
        escalation_tier: u8,
        escalation_delay_mins: u32,
        notify_on_clear: bool,
        enabled: bool,
        reply: oneshot::Sender<Result<(), NotificationError>>,
    },
    DeleteRule {
        id: i64,
        reply: oneshot::Sender<Result<(), NotificationError>>,
    },
    ListRules {
        reply: oneshot::Sender<Vec<RoutingRule>>,
    },
    ListRulesForRecipient {
        recipient_id: i64,
        reply: oneshot::Sender<Vec<RoutingRule>>,
    },
    // Shelving
    CreateShelving {
        alarm_config_id: Option<i64>,
        device_id: Option<String>,
        shelved_by: String,
        reason: String,
        expires_ms: Option<i64>,
        reply: oneshot::Sender<Result<i64, NotificationError>>,
    },
    DeleteShelving {
        id: i64,
        reply: oneshot::Sender<Result<(), NotificationError>>,
    },
    ListActiveShelving {
        reply: oneshot::Sender<Vec<AlarmShelving>>,
    },
    IsShelved {
        alarm_config_id: Option<i64>,
        device_id: Option<String>,
        reply: oneshot::Sender<bool>,
    },
    CleanExpiredShelving {
        reply: oneshot::Sender<u32>,
    },
    // Notification log
    InsertNotification {
        alarm_id: i64,
        recipient_id: i64,
        rule_id: i64,
        channel_type: ChannelType,
        address: String,
        reply: oneshot::Sender<Result<i64, NotificationError>>,
    },
    UpdateNotificationStatus {
        id: i64,
        status: DeliveryStatus,
        error: Option<String>,
        next_retry_ms: Option<i64>,
        reply: oneshot::Sender<Result<(), NotificationError>>,
    },
    IncrementAttemptCount {
        id: i64,
    },
    GetPendingRetries {
        reply: oneshot::Sender<Vec<NotificationRecord>>,
    },
    QueryNotificationLog {
        limit: i64,
        reply: oneshot::Sender<Vec<NotificationRecord>>,
    },
    CountFailedRecent {
        since_ms: i64,
        reply: oneshot::Sender<i64>,
    },
}

// ----------------------------------------------------------------
// NotificationStore — async handle to the SQLite thread
// ----------------------------------------------------------------

#[derive(Clone)]
pub struct NotificationStore {
    cmd_tx: mpsc::UnboundedSender<NotifCmd>,
}

impl NotificationStore {
    pub async fn create_recipient(
        &self,
        name: &str,
        channel_type: ChannelType,
        address: &str,
        channel_config: &str,
    ) -> Result<i64, NotificationError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(NotifCmd::CreateRecipient {
                name: name.to_string(),
                channel_type,
                address: address.to_string(),
                channel_config: channel_config.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| NotificationError::ChannelClosed)?;
        reply_rx
            .await
            .map_err(|_| NotificationError::ChannelClosed)?
    }

    pub async fn update_recipient(
        &self,
        id: i64,
        name: &str,
        channel_type: ChannelType,
        address: &str,
        channel_config: &str,
        enabled: bool,
    ) -> Result<(), NotificationError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(NotifCmd::UpdateRecipient {
                id,
                name: name.to_string(),
                channel_type,
                address: address.to_string(),
                channel_config: channel_config.to_string(),
                enabled,
                reply: reply_tx,
            })
            .map_err(|_| NotificationError::ChannelClosed)?;
        reply_rx
            .await
            .map_err(|_| NotificationError::ChannelClosed)?
    }

    pub async fn delete_recipient(&self, id: i64) -> Result<(), NotificationError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(NotifCmd::DeleteRecipient {
                id,
                reply: reply_tx,
            })
            .map_err(|_| NotificationError::ChannelClosed)?;
        reply_rx
            .await
            .map_err(|_| NotificationError::ChannelClosed)?
    }

    pub async fn list_recipients(&self) -> Vec<AlarmRecipient> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .cmd_tx
            .send(NotifCmd::ListRecipients { reply: reply_tx });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn create_rule(
        &self,
        recipient_id: i64,
        min_severity: &str,
        device_filter: &str,
        alarm_type_filter: &str,
        schedule_id: Option<i64>,
        escalation_tier: u8,
        escalation_delay_mins: u32,
        notify_on_clear: bool,
    ) -> Result<i64, NotificationError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(NotifCmd::CreateRule {
                recipient_id,
                min_severity: min_severity.to_string(),
                device_filter: device_filter.to_string(),
                alarm_type_filter: alarm_type_filter.to_string(),
                schedule_id,
                escalation_tier,
                escalation_delay_mins,
                notify_on_clear,
                reply: reply_tx,
            })
            .map_err(|_| NotificationError::ChannelClosed)?;
        reply_rx
            .await
            .map_err(|_| NotificationError::ChannelClosed)?
    }

    pub async fn update_rule(
        &self,
        id: i64,
        min_severity: &str,
        device_filter: &str,
        alarm_type_filter: &str,
        schedule_id: Option<i64>,
        escalation_tier: u8,
        escalation_delay_mins: u32,
        notify_on_clear: bool,
        enabled: bool,
    ) -> Result<(), NotificationError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(NotifCmd::UpdateRule {
                id,
                min_severity: min_severity.to_string(),
                device_filter: device_filter.to_string(),
                alarm_type_filter: alarm_type_filter.to_string(),
                schedule_id,
                escalation_tier,
                escalation_delay_mins,
                notify_on_clear,
                enabled,
                reply: reply_tx,
            })
            .map_err(|_| NotificationError::ChannelClosed)?;
        reply_rx
            .await
            .map_err(|_| NotificationError::ChannelClosed)?
    }

    pub async fn delete_rule(&self, id: i64) -> Result<(), NotificationError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(NotifCmd::DeleteRule {
                id,
                reply: reply_tx,
            })
            .map_err(|_| NotificationError::ChannelClosed)?;
        reply_rx
            .await
            .map_err(|_| NotificationError::ChannelClosed)?
    }

    pub async fn list_rules(&self) -> Vec<RoutingRule> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(NotifCmd::ListRules { reply: reply_tx });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn list_rules_for_recipient(&self, recipient_id: i64) -> Vec<RoutingRule> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(NotifCmd::ListRulesForRecipient {
            recipient_id,
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn create_shelving(
        &self,
        alarm_config_id: Option<i64>,
        device_id: Option<String>,
        shelved_by: &str,
        reason: &str,
        expires_ms: Option<i64>,
    ) -> Result<i64, NotificationError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(NotifCmd::CreateShelving {
                alarm_config_id,
                device_id,
                shelved_by: shelved_by.to_string(),
                reason: reason.to_string(),
                expires_ms,
                reply: reply_tx,
            })
            .map_err(|_| NotificationError::ChannelClosed)?;
        reply_rx
            .await
            .map_err(|_| NotificationError::ChannelClosed)?
    }

    pub async fn delete_shelving(&self, id: i64) -> Result<(), NotificationError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(NotifCmd::DeleteShelving {
                id,
                reply: reply_tx,
            })
            .map_err(|_| NotificationError::ChannelClosed)?;
        reply_rx
            .await
            .map_err(|_| NotificationError::ChannelClosed)?
    }

    pub async fn list_active_shelving(&self) -> Vec<AlarmShelving> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .cmd_tx
            .send(NotifCmd::ListActiveShelving { reply: reply_tx });
        reply_rx.await.unwrap_or_default()
    }

    /// Returns true if the given alarm config or device is currently shelved.
    /// Matches on alarm_config_id (exact), device_id (exact), or wildcard shelving
    /// (both NULL = shelves everything).
    pub async fn is_shelved(
        &self,
        alarm_config_id: Option<i64>,
        device_id: Option<String>,
    ) -> bool {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(NotifCmd::IsShelved {
            alarm_config_id,
            device_id,
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or(false)
    }

    pub async fn clean_expired_shelving(&self) -> u32 {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .cmd_tx
            .send(NotifCmd::CleanExpiredShelving { reply: reply_tx });
        reply_rx.await.unwrap_or(0)
    }

    pub async fn insert_notification(
        &self,
        alarm_id: i64,
        recipient_id: i64,
        rule_id: i64,
        channel_type: ChannelType,
        address: &str,
    ) -> Result<i64, NotificationError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(NotifCmd::InsertNotification {
                alarm_id,
                recipient_id,
                rule_id,
                channel_type,
                address: address.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| NotificationError::ChannelClosed)?;
        reply_rx
            .await
            .map_err(|_| NotificationError::ChannelClosed)?
    }

    pub async fn update_notification_status(
        &self,
        id: i64,
        status: DeliveryStatus,
        error: Option<String>,
        next_retry_ms: Option<i64>,
    ) -> Result<(), NotificationError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(NotifCmd::UpdateNotificationStatus {
                id,
                status,
                error,
                next_retry_ms,
                reply: reply_tx,
            })
            .map_err(|_| NotificationError::ChannelClosed)?;
        reply_rx
            .await
            .map_err(|_| NotificationError::ChannelClosed)?
    }

    /// Increment the attempt count for a notification record (call before each retry send).
    pub fn increment_attempt_count(&self, id: i64) {
        let _ = self.cmd_tx.send(NotifCmd::IncrementAttemptCount { id });
    }

    pub async fn get_pending_retries(&self) -> Vec<NotificationRecord> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .cmd_tx
            .send(NotifCmd::GetPendingRetries { reply: reply_tx });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn query_notification_log(&self, limit: i64) -> Vec<NotificationRecord> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(NotifCmd::QueryNotificationLog {
            limit,
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn count_failed_recent(&self, since_ms: i64) -> i64 {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(NotifCmd::CountFailedRecent {
            since_ms,
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or(0)
    }
}

// ----------------------------------------------------------------
// SQLite schema & thread
// ----------------------------------------------------------------

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    label: "initial notification schema",
    sql: "
CREATE TABLE IF NOT EXISTS recipient (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    name           TEXT NOT NULL,
    channel_type   TEXT NOT NULL,
    address        TEXT NOT NULL,
    channel_config TEXT NOT NULL DEFAULT '{}',
    enabled        INTEGER NOT NULL DEFAULT 1,
    created_ms     INTEGER NOT NULL,
    updated_ms     INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS routing_rule (
    id                    INTEGER PRIMARY KEY AUTOINCREMENT,
    recipient_id          INTEGER NOT NULL REFERENCES recipient(id) ON DELETE CASCADE,
    min_severity          TEXT NOT NULL DEFAULT 'info',
    device_filter         TEXT NOT NULL DEFAULT '',
    alarm_type_filter     TEXT NOT NULL DEFAULT '',
    schedule_id           INTEGER,
    escalation_tier       INTEGER NOT NULL DEFAULT 0,
    escalation_delay_mins INTEGER NOT NULL DEFAULT 0,
    notify_on_clear       INTEGER NOT NULL DEFAULT 1,
    enabled               INTEGER NOT NULL DEFAULT 1,
    created_ms            INTEGER NOT NULL,
    updated_ms            INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS alarm_shelving (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    alarm_config_id INTEGER,
    device_id       TEXT,
    shelved_by      TEXT NOT NULL,
    reason          TEXT NOT NULL DEFAULT '',
    expires_ms      INTEGER,
    created_ms      INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_shelving_device ON alarm_shelving(device_id);
CREATE INDEX IF NOT EXISTS idx_shelving_config ON alarm_shelving(alarm_config_id);

CREATE TABLE IF NOT EXISTS notification_log (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    alarm_id      INTEGER NOT NULL,
    recipient_id  INTEGER NOT NULL,
    rule_id       INTEGER NOT NULL,
    channel_type  TEXT NOT NULL,
    address       TEXT NOT NULL,
    status        TEXT NOT NULL DEFAULT 'pending',
    attempt_count INTEGER NOT NULL DEFAULT 1,
    last_error    TEXT,
    created_ms    INTEGER NOT NULL,
    updated_ms    INTEGER NOT NULL,
    next_retry_ms INTEGER
);
CREATE INDEX IF NOT EXISTS idx_notif_log_status ON notification_log(status);
CREATE INDEX IF NOT EXISTS idx_notif_log_time ON notification_log(created_ms);
",
}];

fn run_sqlite_thread(db_path: &Path, mut cmd_rx: mpsc::UnboundedReceiver<NotifCmd>) {
    let conn = rusqlite::Connection::open(db_path).expect("failed to open notifications database");
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;",
    )
    .expect("failed to set pragmas");
    run_migrations(&conn, "notifications", MIGRATIONS)
        .expect("notifications: schema migration failed");

    while let Some(cmd) = cmd_rx.blocking_recv() {
        match cmd {
            // ---- Recipients ----
            NotifCmd::CreateRecipient {
                name,
                channel_type,
                address,
                channel_config,
                reply,
            } => {
                let result =
                    create_recipient_db(&conn, &name, &channel_type, &address, &channel_config);
                let _ = reply.send(result);
            }
            NotifCmd::UpdateRecipient {
                id,
                name,
                channel_type,
                address,
                channel_config,
                enabled,
                reply,
            } => {
                let result = update_recipient_db(
                    &conn,
                    id,
                    &name,
                    &channel_type,
                    &address,
                    &channel_config,
                    enabled,
                );
                let _ = reply.send(result);
            }
            NotifCmd::DeleteRecipient { id, reply } => {
                let result = delete_recipient_db(&conn, id);
                let _ = reply.send(result);
            }
            NotifCmd::ListRecipients { reply } => {
                let _ = reply.send(list_recipients_db(&conn));
            }
            // ---- Routing rules ----
            NotifCmd::CreateRule {
                recipient_id,
                min_severity,
                device_filter,
                alarm_type_filter,
                schedule_id,
                escalation_tier,
                escalation_delay_mins,
                notify_on_clear,
                reply,
            } => {
                let result = create_rule_db(
                    &conn,
                    recipient_id,
                    &min_severity,
                    &device_filter,
                    &alarm_type_filter,
                    schedule_id,
                    escalation_tier,
                    escalation_delay_mins,
                    notify_on_clear,
                );
                let _ = reply.send(result);
            }
            NotifCmd::UpdateRule {
                id,
                min_severity,
                device_filter,
                alarm_type_filter,
                schedule_id,
                escalation_tier,
                escalation_delay_mins,
                notify_on_clear,
                enabled,
                reply,
            } => {
                let result = update_rule_db(
                    &conn,
                    id,
                    &min_severity,
                    &device_filter,
                    &alarm_type_filter,
                    schedule_id,
                    escalation_tier,
                    escalation_delay_mins,
                    notify_on_clear,
                    enabled,
                );
                let _ = reply.send(result);
            }
            NotifCmd::DeleteRule { id, reply } => {
                let result = delete_rule_db(&conn, id);
                let _ = reply.send(result);
            }
            NotifCmd::ListRules { reply } => {
                let _ = reply.send(list_rules_db(&conn, None));
            }
            NotifCmd::ListRulesForRecipient {
                recipient_id,
                reply,
            } => {
                let _ = reply.send(list_rules_db(&conn, Some(recipient_id)));
            }
            // ---- Shelving ----
            NotifCmd::CreateShelving {
                alarm_config_id,
                device_id,
                shelved_by,
                reason,
                expires_ms,
                reply,
            } => {
                let result = create_shelving_db(
                    &conn,
                    alarm_config_id,
                    device_id.as_deref(),
                    &shelved_by,
                    &reason,
                    expires_ms,
                );
                let _ = reply.send(result);
            }
            NotifCmd::DeleteShelving { id, reply } => {
                let result = delete_shelving_db(&conn, id);
                let _ = reply.send(result);
            }
            NotifCmd::ListActiveShelving { reply } => {
                let _ = reply.send(list_active_shelving_db(&conn));
            }
            NotifCmd::IsShelved {
                alarm_config_id,
                device_id,
                reply,
            } => {
                let _ = reply.send(is_shelved_db(&conn, alarm_config_id, device_id.as_deref()));
            }
            NotifCmd::CleanExpiredShelving { reply } => {
                let _ = reply.send(clean_expired_shelving_db(&conn));
            }
            // ---- Notification log ----
            NotifCmd::InsertNotification {
                alarm_id,
                recipient_id,
                rule_id,
                channel_type,
                address,
                reply,
            } => {
                let result = insert_notification_db(
                    &conn,
                    alarm_id,
                    recipient_id,
                    rule_id,
                    &channel_type,
                    &address,
                );
                let _ = reply.send(result);
            }
            NotifCmd::UpdateNotificationStatus {
                id,
                status,
                error,
                next_retry_ms,
                reply,
            } => {
                let result = update_notification_status_db(
                    &conn,
                    id,
                    &status,
                    error.as_deref(),
                    next_retry_ms,
                );
                let _ = reply.send(result);
            }
            NotifCmd::IncrementAttemptCount { id } => {
                let _ = conn.execute(
                    "UPDATE notification_log SET attempt_count = attempt_count + 1 WHERE id = ?1",
                    rusqlite::params![id],
                );
            }
            NotifCmd::GetPendingRetries { reply } => {
                let _ = reply.send(get_pending_retries_db(&conn));
            }
            NotifCmd::QueryNotificationLog { limit, reply } => {
                let _ = reply.send(query_notification_log_db(&conn, limit));
            }
            NotifCmd::CountFailedRecent { since_ms, reply } => {
                let _ = reply.send(count_failed_recent_db(&conn, since_ms));
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

// ---- Recipients ----

fn create_recipient_db(
    conn: &rusqlite::Connection,
    name: &str,
    channel_type: &ChannelType,
    address: &str,
    channel_config: &str,
) -> Result<i64, NotificationError> {
    let ts = now_ms();
    conn.execute(
        "INSERT INTO recipient (name, channel_type, address, channel_config, enabled, created_ms, updated_ms)
         VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6)",
        rusqlite::params![name, channel_type.as_str(), address, channel_config, ts, ts],
    )
    .map_err(|e| NotificationError::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

fn update_recipient_db(
    conn: &rusqlite::Connection,
    id: i64,
    name: &str,
    channel_type: &ChannelType,
    address: &str,
    channel_config: &str,
    enabled: bool,
) -> Result<(), NotificationError> {
    let ts = now_ms();
    let rows = conn
        .execute(
            "UPDATE recipient SET name = ?1, channel_type = ?2, address = ?3, channel_config = ?4, enabled = ?5, updated_ms = ?6 WHERE id = ?7",
            rusqlite::params![name, channel_type.as_str(), address, channel_config, enabled as i32, ts, id],
        )
        .map_err(|e| NotificationError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(NotificationError::NotFound);
    }
    Ok(())
}

fn delete_recipient_db(conn: &rusqlite::Connection, id: i64) -> Result<(), NotificationError> {
    let rows = conn
        .execute("DELETE FROM recipient WHERE id = ?1", rusqlite::params![id])
        .map_err(|e| NotificationError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(NotificationError::NotFound);
    }
    Ok(())
}

fn list_recipients_db(conn: &rusqlite::Connection) -> Vec<AlarmRecipient> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT id, name, channel_type, address, channel_config, enabled FROM recipient ORDER BY id",
        )
        .unwrap();
    let rows = stmt
        .query_map([], |row| {
            let ct_str: String = row.get(2)?;
            Ok(AlarmRecipient {
                id: row.get(0)?,
                name: row.get(1)?,
                channel_type: ChannelType::from_str(&ct_str).unwrap_or(ChannelType::Webhook),
                address: row.get(3)?,
                channel_config: row.get(4)?,
                enabled: row.get::<_, i32>(5)? != 0,
            })
        })
        .unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

// ---- Routing rules ----

fn create_rule_db(
    conn: &rusqlite::Connection,
    recipient_id: i64,
    min_severity: &str,
    device_filter: &str,
    alarm_type_filter: &str,
    schedule_id: Option<i64>,
    escalation_tier: u8,
    escalation_delay_mins: u32,
    notify_on_clear: bool,
) -> Result<i64, NotificationError> {
    let ts = now_ms();
    conn.execute(
        "INSERT INTO routing_rule (recipient_id, min_severity, device_filter, alarm_type_filter, schedule_id, escalation_tier, escalation_delay_mins, notify_on_clear, enabled, created_ms, updated_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, ?9, ?10)",
        rusqlite::params![
            recipient_id,
            min_severity,
            device_filter,
            alarm_type_filter,
            schedule_id,
            escalation_tier as i32,
            escalation_delay_mins as i32,
            notify_on_clear as i32,
            ts,
            ts,
        ],
    )
    .map_err(|e| NotificationError::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

fn update_rule_db(
    conn: &rusqlite::Connection,
    id: i64,
    min_severity: &str,
    device_filter: &str,
    alarm_type_filter: &str,
    schedule_id: Option<i64>,
    escalation_tier: u8,
    escalation_delay_mins: u32,
    notify_on_clear: bool,
    enabled: bool,
) -> Result<(), NotificationError> {
    let ts = now_ms();
    let rows = conn
        .execute(
            "UPDATE routing_rule SET min_severity = ?1, device_filter = ?2, alarm_type_filter = ?3, schedule_id = ?4, escalation_tier = ?5, escalation_delay_mins = ?6, notify_on_clear = ?7, enabled = ?8, updated_ms = ?9 WHERE id = ?10",
            rusqlite::params![
                min_severity,
                device_filter,
                alarm_type_filter,
                schedule_id,
                escalation_tier as i32,
                escalation_delay_mins as i32,
                notify_on_clear as i32,
                enabled as i32,
                ts,
                id,
            ],
        )
        .map_err(|e| NotificationError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(NotificationError::NotFound);
    }
    Ok(())
}

fn delete_rule_db(conn: &rusqlite::Connection, id: i64) -> Result<(), NotificationError> {
    let rows = conn
        .execute(
            "DELETE FROM routing_rule WHERE id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| NotificationError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(NotificationError::NotFound);
    }
    Ok(())
}

fn parse_rule_row(row: &rusqlite::Row) -> rusqlite::Result<RoutingRule> {
    Ok(RoutingRule {
        id: row.get(0)?,
        recipient_id: row.get(1)?,
        min_severity: row.get(2)?,
        device_filter: row.get(3)?,
        alarm_type_filter: row.get(4)?,
        schedule_id: row.get(5)?,
        escalation_tier: row.get::<_, i32>(6)? as u8,
        escalation_delay_mins: row.get::<_, i32>(7)? as u32,
        notify_on_clear: row.get::<_, i32>(8)? != 0,
        enabled: row.get::<_, i32>(9)? != 0,
    })
}

fn list_rules_db(conn: &rusqlite::Connection, recipient_id: Option<i64>) -> Vec<RoutingRule> {
    match recipient_id {
        Some(rid) => {
            let mut stmt = conn
                .prepare_cached(
                    "SELECT id, recipient_id, min_severity, device_filter, alarm_type_filter, schedule_id, escalation_tier, escalation_delay_mins, notify_on_clear, enabled FROM routing_rule WHERE recipient_id = ?1 ORDER BY id",
                )
                .unwrap();
            let rows = stmt
                .query_map(rusqlite::params![rid], parse_rule_row)
                .unwrap();
            rows.filter_map(|r| r.ok()).collect()
        }
        None => {
            let mut stmt = conn
                .prepare_cached(
                    "SELECT id, recipient_id, min_severity, device_filter, alarm_type_filter, schedule_id, escalation_tier, escalation_delay_mins, notify_on_clear, enabled FROM routing_rule ORDER BY id",
                )
                .unwrap();
            let rows = stmt.query_map([], parse_rule_row).unwrap();
            rows.filter_map(|r| r.ok()).collect()
        }
    }
}

// ---- Shelving ----

fn create_shelving_db(
    conn: &rusqlite::Connection,
    alarm_config_id: Option<i64>,
    device_id: Option<&str>,
    shelved_by: &str,
    reason: &str,
    expires_ms: Option<i64>,
) -> Result<i64, NotificationError> {
    let ts = now_ms();
    conn.execute(
        "INSERT INTO alarm_shelving (alarm_config_id, device_id, shelved_by, reason, expires_ms, created_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![alarm_config_id, device_id, shelved_by, reason, expires_ms, ts],
    )
    .map_err(|e| NotificationError::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

fn delete_shelving_db(conn: &rusqlite::Connection, id: i64) -> Result<(), NotificationError> {
    let rows = conn
        .execute(
            "DELETE FROM alarm_shelving WHERE id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| NotificationError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(NotificationError::NotFound);
    }
    Ok(())
}

fn list_active_shelving_db(conn: &rusqlite::Connection) -> Vec<AlarmShelving> {
    let now = now_ms();
    let mut stmt = conn
        .prepare_cached(
            "SELECT id, alarm_config_id, device_id, shelved_by, reason, expires_ms, created_ms FROM alarm_shelving WHERE expires_ms IS NULL OR expires_ms > ?1 ORDER BY id",
        )
        .unwrap();
    let rows = stmt
        .query_map(rusqlite::params![now], |row| {
            Ok(AlarmShelving {
                id: row.get(0)?,
                alarm_config_id: row.get(1)?,
                device_id: row.get(2)?,
                shelved_by: row.get(3)?,
                reason: row.get(4)?,
                expires_ms: row.get(5)?,
                created_ms: row.get(6)?,
            })
        })
        .unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

/// Check if an alarm is shelved. Matches if any active shelving entry covers the
/// given alarm_config_id or device_id (NULL fields in the shelving row act as wildcards).
fn is_shelved_db(
    conn: &rusqlite::Connection,
    alarm_config_id: Option<i64>,
    device_id: Option<&str>,
) -> bool {
    let now = now_ms();
    let result: bool = conn
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM alarm_shelving
                WHERE (expires_ms IS NULL OR expires_ms > ?1)
                  AND (alarm_config_id IS NULL OR alarm_config_id = ?2)
                  AND (device_id IS NULL OR device_id = ?3)
            )",
            rusqlite::params![now, alarm_config_id, device_id],
            |row| row.get(0),
        )
        .unwrap_or(false);
    result
}

fn clean_expired_shelving_db(conn: &rusqlite::Connection) -> u32 {
    let now = now_ms();
    conn.execute(
        "DELETE FROM alarm_shelving WHERE expires_ms IS NOT NULL AND expires_ms <= ?1",
        rusqlite::params![now],
    )
    .unwrap_or(0) as u32
}

// ---- Notification log ----

fn insert_notification_db(
    conn: &rusqlite::Connection,
    alarm_id: i64,
    recipient_id: i64,
    rule_id: i64,
    channel_type: &ChannelType,
    address: &str,
) -> Result<i64, NotificationError> {
    let ts = now_ms();
    conn.execute(
        "INSERT INTO notification_log (alarm_id, recipient_id, rule_id, channel_type, address, status, attempt_count, created_ms, updated_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, 'pending', 1, ?6, ?7)",
        rusqlite::params![alarm_id, recipient_id, rule_id, channel_type.as_str(), address, ts, ts],
    )
    .map_err(|e| NotificationError::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

fn update_notification_status_db(
    conn: &rusqlite::Connection,
    id: i64,
    status: &DeliveryStatus,
    error: Option<&str>,
    next_retry_ms: Option<i64>,
) -> Result<(), NotificationError> {
    let ts = now_ms();
    let rows = conn
        .execute(
            "UPDATE notification_log SET status = ?1, last_error = ?2, next_retry_ms = ?3, updated_ms = ?4 WHERE id = ?5",
            rusqlite::params![status.as_str(), error, next_retry_ms, ts, id],
        )
        .map_err(|e| NotificationError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(NotificationError::NotFound);
    }
    Ok(())
}

fn parse_notification_row(row: &rusqlite::Row) -> rusqlite::Result<NotificationRecord> {
    let ct_str: String = row.get(4)?;
    let st_str: String = row.get(6)?;
    Ok(NotificationRecord {
        id: row.get(0)?,
        alarm_id: row.get(1)?,
        recipient_id: row.get(2)?,
        rule_id: row.get(3)?,
        channel_type: ChannelType::from_str(&ct_str).unwrap_or(ChannelType::Webhook),
        address: row.get(5)?,
        status: DeliveryStatus::from_str(&st_str).unwrap_or(DeliveryStatus::Pending),
        attempt_count: row.get::<_, i32>(7)? as u32,
        last_error: row.get(8)?,
        created_ms: row.get(9)?,
        updated_ms: row.get(10)?,
        next_retry_ms: row.get(11)?,
    })
}

fn get_pending_retries_db(conn: &rusqlite::Connection) -> Vec<NotificationRecord> {
    let now = now_ms();
    let mut stmt = conn
        .prepare_cached(
            "SELECT id, alarm_id, recipient_id, rule_id, channel_type, address, status, attempt_count, last_error, created_ms, updated_ms, next_retry_ms
             FROM notification_log
             WHERE status IN ('pending', 'retrying') AND (next_retry_ms IS NULL OR next_retry_ms <= ?1)
             ORDER BY created_ms",
        )
        .unwrap();
    let rows = stmt
        .query_map(rusqlite::params![now], parse_notification_row)
        .unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

fn query_notification_log_db(conn: &rusqlite::Connection, limit: i64) -> Vec<NotificationRecord> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT id, alarm_id, recipient_id, rule_id, channel_type, address, status, attempt_count, last_error, created_ms, updated_ms, next_retry_ms
             FROM notification_log
             ORDER BY created_ms DESC
             LIMIT ?1",
        )
        .unwrap();
    let rows = stmt
        .query_map(rusqlite::params![limit], parse_notification_row)
        .unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

fn count_failed_recent_db(conn: &rusqlite::Connection, since_ms: i64) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM notification_log WHERE status = 'failed' AND created_ms >= ?1",
        rusqlite::params![since_ms],
        |row| row.get(0),
    )
    .unwrap_or(0)
}

// ----------------------------------------------------------------
// Constructor
// ----------------------------------------------------------------

pub fn start_notification_store_with_path(db_path: &Path) -> NotificationStore {
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let path_clone = db_path.to_path_buf();
    std::thread::Builder::new()
        .name("notification-sqlite".into())
        .spawn(move || run_sqlite_thread(&path_clone, cmd_rx))
        .expect("failed to spawn notification SQLite thread");
    NotificationStore { cmd_tx }
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_db_path() -> std::path::PathBuf {
        let n = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let tid = std::thread::current().id();
        let dir = std::env::temp_dir().join("opencrate-notif-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("notif-test-{n}-{tid:?}.db"));
        // Clean up any leftover files from previous runs
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
        path
    }

    #[tokio::test]
    async fn test_recipient_crud() {
        let db_path = temp_db_path();
        let store = start_notification_store_with_path(&db_path);

        // Create
        let id = store
            .create_recipient(
                "Ops Team",
                ChannelType::Webhook,
                "https://hooks.example.com/alerts",
                "{}",
            )
            .await
            .unwrap();
        assert!(id > 0);

        // List
        let recipients = store.list_recipients().await;
        assert_eq!(recipients.len(), 1);
        assert_eq!(recipients[0].name, "Ops Team");
        assert_eq!(recipients[0].channel_type, ChannelType::Webhook);
        assert!(recipients[0].enabled);

        // Update
        store
            .update_recipient(
                id,
                "Ops Team v2",
                ChannelType::Email,
                "ops@example.com",
                "{\"smtp\":true}",
                false,
            )
            .await
            .unwrap();
        let recipients = store.list_recipients().await;
        assert_eq!(recipients[0].name, "Ops Team v2");
        assert_eq!(recipients[0].channel_type, ChannelType::Email);
        assert_eq!(recipients[0].address, "ops@example.com");
        assert!(!recipients[0].enabled);

        // Delete
        store.delete_recipient(id).await.unwrap();
        let recipients = store.list_recipients().await;
        assert!(recipients.is_empty());

        // Delete non-existent
        let err = store.delete_recipient(999).await;
        assert!(err.is_err());

        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn test_routing_rule_crud() {
        let db_path = temp_db_path();
        let store = start_notification_store_with_path(&db_path);

        let recip_id = store
            .create_recipient("Test", ChannelType::Sms, "+1555000111", "{}")
            .await
            .unwrap();

        // Create rule
        let rule_id = store
            .create_rule(recip_id, "warning", "ahu-1,ahu-2", "", None, 0, 0, true)
            .await
            .unwrap();
        assert!(rule_id > 0);

        // List all
        let rules = store.list_rules().await;
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].recipient_id, recip_id);
        assert_eq!(rules[0].min_severity, "warning");
        assert_eq!(rules[0].device_filter, "ahu-1,ahu-2");
        assert!(rules[0].notify_on_clear);

        // List for recipient
        let rules = store.list_rules_for_recipient(recip_id).await;
        assert_eq!(rules.len(), 1);

        // List for non-existent recipient
        let rules = store.list_rules_for_recipient(999).await;
        assert!(rules.is_empty());

        // Update rule
        store
            .update_rule(
                rule_id,
                "critical",
                "",
                "high_limit",
                Some(42),
                1,
                15,
                false,
                true,
            )
            .await
            .unwrap();
        let rules = store.list_rules().await;
        assert_eq!(rules[0].min_severity, "critical");
        assert_eq!(rules[0].alarm_type_filter, "high_limit");
        assert_eq!(rules[0].schedule_id, Some(42));
        assert_eq!(rules[0].escalation_tier, 1);
        assert_eq!(rules[0].escalation_delay_mins, 15);
        assert!(!rules[0].notify_on_clear);

        // Delete rule
        store.delete_rule(rule_id).await.unwrap();
        let rules = store.list_rules().await;
        assert!(rules.is_empty());

        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn test_shelving_crud() {
        let db_path = temp_db_path();
        let store = start_notification_store_with_path(&db_path);

        // Create shelving for a specific alarm config
        let sh_id = store
            .create_shelving(Some(10), None, "admin", "Maintenance window", None)
            .await
            .unwrap();
        assert!(sh_id > 0);

        // Check is_shelved
        let shelved = store.is_shelved(Some(10), None).await;
        assert!(shelved, "alarm_config_id=10 should be shelved");

        let shelved = store.is_shelved(Some(99), None).await;
        assert!(!shelved, "alarm_config_id=99 should not be shelved");

        // List active
        let active = store.list_active_shelving().await;
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].shelved_by, "admin");

        // Delete shelving
        store.delete_shelving(sh_id).await.unwrap();
        let shelved = store.is_shelved(Some(10), None).await;
        assert!(!shelved, "should no longer be shelved after delete");

        // Create expired shelving and clean it
        let _expired_id = store
            .create_shelving(Some(20), None, "admin", "Short shelve", Some(1))
            .await
            .unwrap();
        // expires_ms=1 is in the past
        let cleaned = store.clean_expired_shelving().await;
        assert_eq!(cleaned, 1);
        let active = store.list_active_shelving().await;
        assert!(active.is_empty());

        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn test_notification_log() {
        let db_path = temp_db_path();
        let store = start_notification_store_with_path(&db_path);

        // Insert notification
        let notif_id = store
            .insert_notification(
                100,
                1,
                1,
                ChannelType::Webhook,
                "https://hooks.example.com/alerts",
            )
            .await
            .unwrap();
        assert!(notif_id > 0);

        // Should appear in pending retries
        let pending = store.get_pending_retries().await;
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].alarm_id, 100);
        assert_eq!(pending[0].status, DeliveryStatus::Pending);
        assert_eq!(pending[0].attempt_count, 1);

        // Update to delivered
        store
            .update_notification_status(notif_id, DeliveryStatus::Delivered, None, None)
            .await
            .unwrap();

        // Should no longer be pending
        let pending = store.get_pending_retries().await;
        assert!(pending.is_empty());

        // Query log
        let log = store.query_notification_log(10).await;
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].status, DeliveryStatus::Delivered);
        assert_eq!(log[0].attempt_count, 1); // first attempt, delivered

        // Insert a failed notification
        let notif_id2 = store
            .insert_notification(101, 1, 1, ChannelType::Email, "ops@example.com")
            .await
            .unwrap();
        store
            .update_notification_status(
                notif_id2,
                DeliveryStatus::Failed,
                Some("SMTP connection refused".to_string()),
                None,
            )
            .await
            .unwrap();

        // Count failed recent
        let count = store.count_failed_recent(0).await;
        assert_eq!(count, 1);

        let _ = std::fs::remove_file(&db_path);
    }
}
