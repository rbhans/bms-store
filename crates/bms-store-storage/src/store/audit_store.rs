use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot, watch};

// ----------------------------------------------------------------
// Public types
// ----------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditAction {
    WritePoint,
    AcknowledgeAlarm,
    AcknowledgeAllAlarms,
    AcceptDevice,
    IgnoreDevice,
    ScanNetwork,
    CreateSchedule,
    UpdateSchedule,
    DeleteSchedule,
    CreateAssignment,
    DeleteAssignment,
    CreateProgram,
    UpdateProgram,
    DeleteProgram,
    EnableProgram,
    DisableProgram,
    CreateUser,
    UpdateUser,
    DeleteUser,
    ChangePassword,
    ChangeRolePermission,
    Login,
    Logout,
    CreateEntity,
    UpdateEntity,
    DeleteEntity,
    SetTag,
    RemoveTag,
    ShelveAlarm,
    UnshelveAlarm,
    TestNotification,
    ConfigureMqttBroker,
    DeleteMqttBroker,
    TestMqttConnection,
    StartCommissioning,
    VerifyCommissionItem,
    FailCommissionItem,
    DeferCommissionItem,
    SignOffCommissioning,
    CreateReport,
    UpdateReport,
    DeleteReport,
    RunReport,
    CreateReportSchedule,
    UpdateReportSchedule,
    DeleteReportSchedule,
    CreateUtilityRate,
    UpdateUtilityRate,
    DeleteUtilityRate,
    CreateEnergyMeter,
    UpdateEnergyMeter,
    DeleteEnergyMeter,
    CreateBaseline,
    DeleteBaseline,
    CreateWebhook,
    UpdateWebhook,
    DeleteWebhook,
    TestWebhook,
    CreateFddRule,
    UpdateFddRule,
    DeleteFddRule,
    CreateFddBinding,
    DeleteFddBinding,
    AcknowledgeFddFault,
    CreateExportConnector,
    UpdateExportConnector,
    DeleteExportConnector,
    TestExportConnector,
    RunExportBackfill,
    CreateCloudBridge,
    UpdateCloudBridge,
    DeleteCloudBridge,
    TestCloudBridge,
}

impl AuditAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::WritePoint => "write_point",
            Self::AcknowledgeAlarm => "acknowledge_alarm",
            Self::AcknowledgeAllAlarms => "acknowledge_all_alarms",
            Self::AcceptDevice => "accept_device",
            Self::IgnoreDevice => "ignore_device",
            Self::ScanNetwork => "scan_network",
            Self::CreateSchedule => "create_schedule",
            Self::UpdateSchedule => "update_schedule",
            Self::DeleteSchedule => "delete_schedule",
            Self::CreateAssignment => "create_assignment",
            Self::DeleteAssignment => "delete_assignment",
            Self::CreateProgram => "create_program",
            Self::UpdateProgram => "update_program",
            Self::DeleteProgram => "delete_program",
            Self::EnableProgram => "enable_program",
            Self::DisableProgram => "disable_program",
            Self::CreateUser => "create_user",
            Self::UpdateUser => "update_user",
            Self::DeleteUser => "delete_user",
            Self::ChangePassword => "change_password",
            Self::ChangeRolePermission => "change_role_permission",
            Self::Login => "login",
            Self::Logout => "logout",
            Self::CreateEntity => "create_entity",
            Self::UpdateEntity => "update_entity",
            Self::DeleteEntity => "delete_entity",
            Self::SetTag => "set_tag",
            Self::RemoveTag => "remove_tag",
            Self::ShelveAlarm => "shelve_alarm",
            Self::UnshelveAlarm => "unshelve_alarm",
            Self::TestNotification => "test_notification",
            Self::ConfigureMqttBroker => "configure_mqtt_broker",
            Self::DeleteMqttBroker => "delete_mqtt_broker",
            Self::TestMqttConnection => "test_mqtt_connection",
            Self::StartCommissioning => "start_commissioning",
            Self::VerifyCommissionItem => "verify_commission_item",
            Self::FailCommissionItem => "fail_commission_item",
            Self::DeferCommissionItem => "defer_commission_item",
            Self::SignOffCommissioning => "sign_off_commissioning",
            Self::CreateReport => "create_report",
            Self::UpdateReport => "update_report",
            Self::DeleteReport => "delete_report",
            Self::RunReport => "run_report",
            Self::CreateReportSchedule => "create_report_schedule",
            Self::UpdateReportSchedule => "update_report_schedule",
            Self::DeleteReportSchedule => "delete_report_schedule",
            Self::CreateUtilityRate => "create_utility_rate",
            Self::UpdateUtilityRate => "update_utility_rate",
            Self::DeleteUtilityRate => "delete_utility_rate",
            Self::CreateEnergyMeter => "create_energy_meter",
            Self::UpdateEnergyMeter => "update_energy_meter",
            Self::DeleteEnergyMeter => "delete_energy_meter",
            Self::CreateBaseline => "create_baseline",
            Self::DeleteBaseline => "delete_baseline",
            Self::CreateWebhook => "create_webhook",
            Self::UpdateWebhook => "update_webhook",
            Self::DeleteWebhook => "delete_webhook",
            Self::TestWebhook => "test_webhook",
            Self::CreateFddRule => "create_fdd_rule",
            Self::UpdateFddRule => "update_fdd_rule",
            Self::DeleteFddRule => "delete_fdd_rule",
            Self::CreateFddBinding => "create_fdd_binding",
            Self::DeleteFddBinding => "delete_fdd_binding",
            Self::AcknowledgeFddFault => "acknowledge_fdd_fault",
            Self::CreateExportConnector => "create_export_connector",
            Self::UpdateExportConnector => "update_export_connector",
            Self::DeleteExportConnector => "delete_export_connector",
            Self::TestExportConnector => "test_export_connector",
            Self::RunExportBackfill => "run_export_backfill",
            Self::CreateCloudBridge => "create_cloud_bridge",
            Self::UpdateCloudBridge => "update_cloud_bridge",
            Self::DeleteCloudBridge => "delete_cloud_bridge",
            Self::TestCloudBridge => "test_cloud_bridge",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::WritePoint => "Write Point",
            Self::AcknowledgeAlarm => "Acknowledge Alarm",
            Self::AcknowledgeAllAlarms => "Acknowledge All Alarms",
            Self::AcceptDevice => "Accept Device",
            Self::IgnoreDevice => "Ignore Device",
            Self::ScanNetwork => "Scan Network",
            Self::CreateSchedule => "Create Schedule",
            Self::UpdateSchedule => "Update Schedule",
            Self::DeleteSchedule => "Delete Schedule",
            Self::CreateAssignment => "Create Assignment",
            Self::DeleteAssignment => "Delete Assignment",
            Self::CreateProgram => "Create Program",
            Self::UpdateProgram => "Update Program",
            Self::DeleteProgram => "Delete Program",
            Self::EnableProgram => "Enable Program",
            Self::DisableProgram => "Disable Program",
            Self::CreateUser => "Create User",
            Self::UpdateUser => "Update User",
            Self::DeleteUser => "Delete User",
            Self::ChangePassword => "Change Password",
            Self::ChangeRolePermission => "Change Role Permission",
            Self::Login => "Login",
            Self::Logout => "Logout",
            Self::CreateEntity => "Create Entity",
            Self::UpdateEntity => "Update Entity",
            Self::DeleteEntity => "Delete Entity",
            Self::SetTag => "Set Tag",
            Self::RemoveTag => "Remove Tag",
            Self::ShelveAlarm => "Shelve Alarm",
            Self::UnshelveAlarm => "Unshelve Alarm",
            Self::TestNotification => "Test Notification",
            Self::ConfigureMqttBroker => "Configure MQTT Broker",
            Self::DeleteMqttBroker => "Delete MQTT Broker",
            Self::TestMqttConnection => "Test MQTT Connection",
            Self::StartCommissioning => "Start Commissioning",
            Self::VerifyCommissionItem => "Verify Commission Item",
            Self::FailCommissionItem => "Fail Commission Item",
            Self::DeferCommissionItem => "Defer Commission Item",
            Self::SignOffCommissioning => "Sign Off Commissioning",
            Self::CreateReport => "Create Report",
            Self::UpdateReport => "Update Report",
            Self::DeleteReport => "Delete Report",
            Self::RunReport => "Run Report",
            Self::CreateReportSchedule => "Create Report Schedule",
            Self::UpdateReportSchedule => "Update Report Schedule",
            Self::DeleteReportSchedule => "Delete Report Schedule",
            Self::CreateUtilityRate => "Create Utility Rate",
            Self::UpdateUtilityRate => "Update Utility Rate",
            Self::DeleteUtilityRate => "Delete Utility Rate",
            Self::CreateEnergyMeter => "Create Energy Meter",
            Self::UpdateEnergyMeter => "Update Energy Meter",
            Self::DeleteEnergyMeter => "Delete Energy Meter",
            Self::CreateBaseline => "Create Baseline",
            Self::DeleteBaseline => "Delete Baseline",
            Self::CreateWebhook => "Create Webhook",
            Self::UpdateWebhook => "Update Webhook",
            Self::DeleteWebhook => "Delete Webhook",
            Self::TestWebhook => "Test Webhook",
            Self::CreateFddRule => "Create FDD Rule",
            Self::UpdateFddRule => "Update FDD Rule",
            Self::DeleteFddRule => "Delete FDD Rule",
            Self::CreateFddBinding => "Create FDD Binding",
            Self::DeleteFddBinding => "Delete FDD Binding",
            Self::AcknowledgeFddFault => "Acknowledge FDD Fault",
            Self::CreateExportConnector => "Create Export Connector",
            Self::UpdateExportConnector => "Update Export Connector",
            Self::DeleteExportConnector => "Delete Export Connector",
            Self::TestExportConnector => "Test Export Connector",
            Self::RunExportBackfill => "Run Export Backfill",
            Self::CreateCloudBridge => "Create Cloud Bridge",
            Self::UpdateCloudBridge => "Update Cloud Bridge",
            Self::DeleteCloudBridge => "Delete Cloud Bridge",
            Self::TestCloudBridge => "Test Cloud Bridge",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "write_point" => Some(Self::WritePoint),
            "acknowledge_alarm" => Some(Self::AcknowledgeAlarm),
            "acknowledge_all_alarms" => Some(Self::AcknowledgeAllAlarms),
            "accept_device" => Some(Self::AcceptDevice),
            "ignore_device" => Some(Self::IgnoreDevice),
            "scan_network" => Some(Self::ScanNetwork),
            "create_schedule" => Some(Self::CreateSchedule),
            "update_schedule" => Some(Self::UpdateSchedule),
            "delete_schedule" => Some(Self::DeleteSchedule),
            "create_assignment" => Some(Self::CreateAssignment),
            "delete_assignment" => Some(Self::DeleteAssignment),
            "create_program" => Some(Self::CreateProgram),
            "update_program" => Some(Self::UpdateProgram),
            "delete_program" => Some(Self::DeleteProgram),
            "enable_program" => Some(Self::EnableProgram),
            "disable_program" => Some(Self::DisableProgram),
            "create_user" => Some(Self::CreateUser),
            "update_user" => Some(Self::UpdateUser),
            "delete_user" => Some(Self::DeleteUser),
            "change_password" => Some(Self::ChangePassword),
            "change_role_permission" => Some(Self::ChangeRolePermission),
            "login" => Some(Self::Login),
            "logout" => Some(Self::Logout),
            "create_entity" => Some(Self::CreateEntity),
            "update_entity" => Some(Self::UpdateEntity),
            "delete_entity" => Some(Self::DeleteEntity),
            "set_tag" => Some(Self::SetTag),
            "remove_tag" => Some(Self::RemoveTag),
            "shelve_alarm" => Some(Self::ShelveAlarm),
            "unshelve_alarm" => Some(Self::UnshelveAlarm),
            "test_notification" => Some(Self::TestNotification),
            "configure_mqtt_broker" => Some(Self::ConfigureMqttBroker),
            "delete_mqtt_broker" => Some(Self::DeleteMqttBroker),
            "test_mqtt_connection" => Some(Self::TestMqttConnection),
            "start_commissioning" => Some(Self::StartCommissioning),
            "verify_commission_item" => Some(Self::VerifyCommissionItem),
            "fail_commission_item" => Some(Self::FailCommissionItem),
            "defer_commission_item" => Some(Self::DeferCommissionItem),
            "sign_off_commissioning" => Some(Self::SignOffCommissioning),
            "create_report" => Some(Self::CreateReport),
            "update_report" => Some(Self::UpdateReport),
            "delete_report" => Some(Self::DeleteReport),
            "run_report" => Some(Self::RunReport),
            "create_report_schedule" => Some(Self::CreateReportSchedule),
            "update_report_schedule" => Some(Self::UpdateReportSchedule),
            "delete_report_schedule" => Some(Self::DeleteReportSchedule),
            "create_utility_rate" => Some(Self::CreateUtilityRate),
            "update_utility_rate" => Some(Self::UpdateUtilityRate),
            "delete_utility_rate" => Some(Self::DeleteUtilityRate),
            "create_energy_meter" => Some(Self::CreateEnergyMeter),
            "update_energy_meter" => Some(Self::UpdateEnergyMeter),
            "delete_energy_meter" => Some(Self::DeleteEnergyMeter),
            "create_baseline" => Some(Self::CreateBaseline),
            "delete_baseline" => Some(Self::DeleteBaseline),
            "create_webhook" => Some(Self::CreateWebhook),
            "update_webhook" => Some(Self::UpdateWebhook),
            "delete_webhook" => Some(Self::DeleteWebhook),
            "test_webhook" => Some(Self::TestWebhook),
            "create_fdd_rule" => Some(Self::CreateFddRule),
            "update_fdd_rule" => Some(Self::UpdateFddRule),
            "delete_fdd_rule" => Some(Self::DeleteFddRule),
            "create_fdd_binding" => Some(Self::CreateFddBinding),
            "delete_fdd_binding" => Some(Self::DeleteFddBinding),
            "acknowledge_fdd_fault" => Some(Self::AcknowledgeFddFault),
            "create_export_connector" => Some(Self::CreateExportConnector),
            "update_export_connector" => Some(Self::UpdateExportConnector),
            "delete_export_connector" => Some(Self::DeleteExportConnector),
            "test_export_connector" => Some(Self::TestExportConnector),
            "run_export_backfill" => Some(Self::RunExportBackfill),
            "create_cloud_bridge" => Some(Self::CreateCloudBridge),
            "update_cloud_bridge" => Some(Self::UpdateCloudBridge),
            "delete_cloud_bridge" => Some(Self::DeleteCloudBridge),
            "test_cloud_bridge" => Some(Self::TestCloudBridge),
            _ => None,
        }
    }

    pub fn all() -> &'static [AuditAction] {
        &[
            Self::WritePoint,
            Self::AcknowledgeAlarm,
            Self::AcknowledgeAllAlarms,
            Self::AcceptDevice,
            Self::IgnoreDevice,
            Self::ScanNetwork,
            Self::CreateSchedule,
            Self::UpdateSchedule,
            Self::DeleteSchedule,
            Self::CreateAssignment,
            Self::DeleteAssignment,
            Self::CreateProgram,
            Self::UpdateProgram,
            Self::DeleteProgram,
            Self::EnableProgram,
            Self::DisableProgram,
            Self::CreateUser,
            Self::UpdateUser,
            Self::DeleteUser,
            Self::ChangePassword,
            Self::ChangeRolePermission,
            Self::Login,
            Self::Logout,
            Self::CreateEntity,
            Self::UpdateEntity,
            Self::DeleteEntity,
            Self::SetTag,
            Self::RemoveTag,
            Self::ShelveAlarm,
            Self::UnshelveAlarm,
            Self::TestNotification,
            Self::ConfigureMqttBroker,
            Self::DeleteMqttBroker,
            Self::TestMqttConnection,
            Self::StartCommissioning,
            Self::VerifyCommissionItem,
            Self::FailCommissionItem,
            Self::DeferCommissionItem,
            Self::SignOffCommissioning,
            Self::CreateFddRule,
            Self::UpdateFddRule,
            Self::DeleteFddRule,
            Self::CreateFddBinding,
            Self::DeleteFddBinding,
            Self::AcknowledgeFddFault,
            Self::CreateExportConnector,
            Self::UpdateExportConnector,
            Self::DeleteExportConnector,
            Self::TestExportConnector,
            Self::RunExportBackfill,
            Self::CreateCloudBridge,
            Self::UpdateCloudBridge,
            Self::DeleteCloudBridge,
            Self::TestCloudBridge,
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: i64,
    pub user_id: String,
    pub username: String,
    pub action: AuditAction,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub details: Option<String>,
    pub result: AuditResult,
    pub error_message: Option<String>,
    pub timestamp_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AuditResult {
    Success,
    Failure,
}

impl AuditResult {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "failure" => Self::Failure,
            _ => Self::Success,
        }
    }
}

/// Builder for audit log entries — reduces boilerplate at call sites.
pub struct AuditEntryBuilder {
    pub user_id: String,
    pub username: String,
    pub action: AuditAction,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub details: Option<String>,
    pub result: AuditResult,
    pub error_message: Option<String>,
}

impl AuditEntryBuilder {
    pub fn new(action: AuditAction, resource_type: &str) -> Self {
        Self {
            user_id: String::new(),
            username: String::new(),
            action,
            resource_type: resource_type.to_string(),
            resource_id: None,
            details: None,
            result: AuditResult::Success,
            error_message: None,
        }
    }

    pub fn resource_id(mut self, id: &str) -> Self {
        self.resource_id = Some(id.to_string());
        self
    }

    pub fn details(mut self, d: &str) -> Self {
        self.details = Some(d.to_string());
        self
    }

    pub fn failure(mut self, msg: &str) -> Self {
        self.result = AuditResult::Failure;
        self.error_message = Some(msg.to_string());
        self
    }
}

#[derive(Debug, Clone, Default)]
pub struct AuditQuery {
    pub user_id: Option<String>,
    pub action: Option<AuditAction>,
    pub resource_type: Option<String>,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    #[error("database error: {0}")]
    Db(String),
    #[error("channel closed")]
    ChannelClosed,
}

// ----------------------------------------------------------------
// Commands sent to the SQLite thread
// ----------------------------------------------------------------

enum AuditCmd {
    Log {
        entry: AuditEntry,
        reply: oneshot::Sender<Result<i64, AuditError>>,
    },
    Query {
        query: AuditQuery,
        reply: oneshot::Sender<Result<Vec<AuditEntry>, AuditError>>,
    },
    Count {
        query: AuditQuery,
        reply: oneshot::Sender<Result<i64, AuditError>>,
    },
}

// ----------------------------------------------------------------
// Store handle
// ----------------------------------------------------------------

#[derive(Clone)]
pub struct AuditStore {
    cmd_tx: mpsc::UnboundedSender<AuditCmd>,
    #[allow(dead_code)] // kept alive so the watch channel stays open
    version_tx: watch::Sender<u64>,
    version_rx: watch::Receiver<u64>,
}

impl PartialEq for AuditStore {
    fn eq(&self, _: &Self) -> bool {
        true // singleton
    }
}

impl AuditStore {
    /// Log an audit entry, filling in user_id/username from the builder.
    pub async fn log(&self, entry: AuditEntry) -> Result<i64, AuditError> {
        let (reply, rx) = oneshot::channel();
        self.cmd_tx
            .send(AuditCmd::Log { entry, reply })
            .map_err(|_| AuditError::ChannelClosed)?;
        rx.await.map_err(|_| AuditError::ChannelClosed)?
    }

    /// Convenience: log from a builder, auto-populating user fields.
    pub async fn log_action(
        &self,
        user_id: &str,
        username: &str,
        builder: AuditEntryBuilder,
    ) -> Result<i64, AuditError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let entry = AuditEntry {
            id: 0,
            user_id: user_id.to_string(),
            username: username.to_string(),
            action: builder.action,
            resource_type: builder.resource_type,
            resource_id: builder.resource_id,
            details: builder.details,
            result: builder.result,
            error_message: builder.error_message,
            timestamp_ms: now,
        };
        self.log(entry).await
    }

    pub async fn query(&self, query: AuditQuery) -> Result<Vec<AuditEntry>, AuditError> {
        let (reply, rx) = oneshot::channel();
        self.cmd_tx
            .send(AuditCmd::Query { query, reply })
            .map_err(|_| AuditError::ChannelClosed)?;
        rx.await.map_err(|_| AuditError::ChannelClosed)?
    }

    pub async fn count(&self, query: AuditQuery) -> Result<i64, AuditError> {
        let (reply, rx) = oneshot::channel();
        self.cmd_tx
            .send(AuditCmd::Count { query, reply })
            .map_err(|_| AuditError::ChannelClosed)?;
        rx.await.map_err(|_| AuditError::ChannelClosed)?
    }

    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.version_rx.clone()
    }
}

// ----------------------------------------------------------------
// SQLite thread
// ----------------------------------------------------------------

use super::migration::{run_migrations, Migration};

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    label: "initial audit schema",
    sql: "
CREATE TABLE IF NOT EXISTS audit_log (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id         TEXT NOT NULL,
    username        TEXT NOT NULL,
    action          TEXT NOT NULL,
    resource_type   TEXT NOT NULL,
    resource_id     TEXT,
    details         TEXT,
    result          TEXT NOT NULL DEFAULT 'success',
    error_message   TEXT,
    timestamp_ms    INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_audit_time ON audit_log(timestamp_ms DESC);
CREATE INDEX IF NOT EXISTS idx_audit_user ON audit_log(user_id, timestamp_ms DESC);
CREATE INDEX IF NOT EXISTS idx_audit_action ON audit_log(action, timestamp_ms DESC);
",
}];

fn run_sqlite_thread(
    db_path: &Path,
    rx: mpsc::UnboundedReceiver<AuditCmd>,
    version_tx: watch::Sender<u64>,
) {
    let conn = rusqlite::Connection::open(db_path).expect("failed to open audit DB");
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
        .expect("failed to set pragmas");
    run_migrations(&conn, "audit", MIGRATIONS).expect("audit: schema migration failed");

    let mut rx = rx;
    let mut version: u64 = 0;

    while let Some(cmd) = rx.blocking_recv() {
        match cmd {
            AuditCmd::Log { entry, reply } => {
                let result = conn.execute(
                    "INSERT INTO audit_log (user_id, username, action, resource_type, resource_id, details, result, error_message, timestamp_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    rusqlite::params![
                        entry.user_id,
                        entry.username,
                        entry.action.as_str(),
                        entry.resource_type,
                        entry.resource_id,
                        entry.details,
                        entry.result.as_str(),
                        entry.error_message,
                        entry.timestamp_ms,
                    ],
                );
                match result {
                    Ok(_) => {
                        let id = conn.last_insert_rowid();
                        version += 1;
                        let _ = version_tx.send(version);
                        let _ = reply.send(Ok(id));
                    }
                    Err(e) => {
                        let _ = reply.send(Err(AuditError::Db(e.to_string())));
                    }
                }
            }
            AuditCmd::Query { query, reply } => {
                let _ = reply.send(query_entries(&conn, &query));
            }
            AuditCmd::Count { query, reply } => {
                let _ = reply.send(count_entries(&conn, &query));
            }
        }
    }
}

fn query_entries(
    conn: &rusqlite::Connection,
    q: &AuditQuery,
) -> Result<Vec<AuditEntry>, AuditError> {
    let mut sql = String::from(
        "SELECT id, user_id, username, action, resource_type, resource_id, details, result, error_message, timestamp_ms
         FROM audit_log WHERE 1=1",
    );
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(ref uid) = q.user_id {
        sql.push_str(" AND user_id = ?");
        params.push(Box::new(uid.clone()));
    }
    if let Some(ref action) = q.action {
        sql.push_str(" AND action = ?");
        params.push(Box::new(action.as_str().to_string()));
    }
    if let Some(ref rt) = q.resource_type {
        sql.push_str(" AND resource_type = ?");
        params.push(Box::new(rt.clone()));
    }
    if let Some(start) = q.start_ms {
        sql.push_str(" AND timestamp_ms >= ?");
        params.push(Box::new(start));
    }
    if let Some(end) = q.end_ms {
        sql.push_str(" AND timestamp_ms <= ?");
        params.push(Box::new(end));
    }

    sql.push_str(" ORDER BY timestamp_ms DESC");

    if let Some(limit) = q.limit {
        sql.push_str(" LIMIT ?");
        params.push(Box::new(limit));
    }
    if let Some(offset) = q.offset {
        sql.push_str(" OFFSET ?");
        params.push(Box::new(offset));
    }

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| AuditError::Db(e.to_string()))?;
    let rows = stmt
        .query_map(param_refs.as_slice(), |row| {
            Ok(AuditEntry {
                id: row.get(0)?,
                user_id: row.get(1)?,
                username: row.get(2)?,
                action: AuditAction::from_str(&row.get::<_, String>(3)?)
                    .unwrap_or(AuditAction::WritePoint),
                resource_type: row.get(4)?,
                resource_id: row.get(5)?,
                details: row.get(6)?,
                result: AuditResult::from_str(&row.get::<_, String>(7)?),
                error_message: row.get(8)?,
                timestamp_ms: row.get(9)?,
            })
        })
        .map_err(|e| AuditError::Db(e.to_string()))?;

    let mut entries = Vec::new();
    for row in rows {
        entries.push(row.map_err(|e| AuditError::Db(e.to_string()))?);
    }
    Ok(entries)
}

fn count_entries(conn: &rusqlite::Connection, q: &AuditQuery) -> Result<i64, AuditError> {
    let mut sql = String::from("SELECT COUNT(*) FROM audit_log WHERE 1=1");
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(ref uid) = q.user_id {
        sql.push_str(" AND user_id = ?");
        params.push(Box::new(uid.clone()));
    }
    if let Some(ref action) = q.action {
        sql.push_str(" AND action = ?");
        params.push(Box::new(action.as_str().to_string()));
    }
    if let Some(ref rt) = q.resource_type {
        sql.push_str(" AND resource_type = ?");
        params.push(Box::new(rt.clone()));
    }
    if let Some(start) = q.start_ms {
        sql.push_str(" AND timestamp_ms >= ?");
        params.push(Box::new(start));
    }
    if let Some(end) = q.end_ms {
        sql.push_str(" AND timestamp_ms <= ?");
        params.push(Box::new(end));
    }

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let count: i64 = conn
        .query_row(&sql, param_refs.as_slice(), |row| row.get(0))
        .map_err(|e| AuditError::Db(e.to_string()))?;
    Ok(count)
}

// ----------------------------------------------------------------
// Constructor
// ----------------------------------------------------------------

pub fn start_audit_store_with_path(db_path: &Path) -> AuditStore {
    let path_clone = db_path.to_path_buf();
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (version_tx, version_rx) = watch::channel(0u64);
    let vtx = version_tx.clone();

    std::thread::Builder::new()
        .name("audit-sqlite".into())
        .spawn(move || run_sqlite_thread(&path_clone, cmd_rx, vtx))
        .expect("failed to spawn audit SQLite thread");

    AuditStore {
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
    use super::*;

    fn now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    }

    fn test_store(name: &str) -> AuditStore {
        let db_path = std::path::PathBuf::from(format!("/tmp/test_audit_{name}.db"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).ok();
        }
        start_audit_store_with_path(&db_path)
    }

    #[tokio::test]
    async fn log_and_query() {
        let store = test_store("log_and_query");

        let entry = AuditEntry {
            id: 0,
            user_id: "u1".into(),
            username: "admin".into(),
            action: AuditAction::WritePoint,
            resource_type: "point".into(),
            resource_id: Some("dev1/temp".into()),
            details: Some("72.5 → 74.0".into()),
            result: AuditResult::Success,
            error_message: None,
            timestamp_ms: now_ms(),
        };
        let id = store.log(entry).await.unwrap();
        assert!(id > 0);

        let entries = store.query(AuditQuery::default()).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].username, "admin");
        assert_eq!(entries[0].action, AuditAction::WritePoint);
    }

    #[tokio::test]
    async fn query_filters() {
        let store = test_store("query_filters");

        let builder_a = AuditEntryBuilder::new(AuditAction::WritePoint, "point")
            .resource_id("dev1/temp")
            .details("set to 72");
        store.log_action("u1", "admin", builder_a).await.unwrap();

        let builder_b =
            AuditEntryBuilder::new(AuditAction::AcknowledgeAlarm, "alarm").resource_id("alarm-1");
        store.log_action("u2", "operator", builder_b).await.unwrap();

        // Filter by action
        let q = AuditQuery {
            action: Some(AuditAction::WritePoint),
            ..Default::default()
        };
        let entries = store.query(q).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].username, "admin");

        // Filter by user
        let q = AuditQuery {
            user_id: Some("u2".into()),
            ..Default::default()
        };
        let entries = store.query(q).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].action, AuditAction::AcknowledgeAlarm);

        // Count
        let count = store.count(AuditQuery::default()).await.unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn log_failure() {
        let store = test_store("log_failure");

        let builder = AuditEntryBuilder::new(AuditAction::WritePoint, "point")
            .resource_id("dev1/temp")
            .failure("Bridge error: timeout");
        store.log_action("u1", "admin", builder).await.unwrap();

        let entries = store.query(AuditQuery::default()).await.unwrap();
        assert_eq!(entries[0].result, AuditResult::Failure);
        assert_eq!(
            entries[0].error_message.as_deref(),
            Some("Bridge error: timeout")
        );
    }

    #[tokio::test]
    async fn version_increments() {
        let store = test_store("version_increments");
        let mut rx = store.subscribe();

        let builder = AuditEntryBuilder::new(AuditAction::Login, "session");
        store.log_action("u1", "admin", builder).await.unwrap();

        rx.changed().await.unwrap();
        assert_eq!(*rx.borrow(), 1);
    }
}
