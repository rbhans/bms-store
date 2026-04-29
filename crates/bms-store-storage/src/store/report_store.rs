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
pub enum ReportType {
    EnergySummary,
    AlarmSummary,
    ComfortCompliance,
    EquipmentRuntime,
    Custom,
}

impl ReportType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::EnergySummary => "energy_summary",
            Self::AlarmSummary => "alarm_summary",
            Self::ComfortCompliance => "comfort_compliance",
            Self::EquipmentRuntime => "equipment_runtime",
            Self::Custom => "custom",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "energy_summary" => Some(Self::EnergySummary),
            "alarm_summary" => Some(Self::AlarmSummary),
            "comfort_compliance" => Some(Self::ComfortCompliance),
            "equipment_runtime" => Some(Self::EquipmentRuntime),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::EnergySummary => "Energy Summary",
            Self::AlarmSummary => "Alarm Summary",
            Self::ComfortCompliance => "Comfort Compliance",
            Self::EquipmentRuntime => "Equipment Runtime",
            Self::Custom => "Custom",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportFrequency {
    Daily,
    Weekly,
    Monthly,
}

impl ReportFrequency {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "daily" => Some(Self::Daily),
            "weekly" => Some(Self::Weekly),
            "monthly" => Some(Self::Monthly),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Daily => "Daily",
            Self::Weekly => "Weekly",
            Self::Monthly => "Monthly",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Running,
    Completed,
    Failed,
}

impl ExecutionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "running" => Some(Self::Running),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

/// Time range for a report — relative or absolute.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeRangeKind {
    Last24Hours,
    Last7Days,
    Last30Days,
    LastMonth,
    Custom { start_ms: i64, end_ms: i64 },
}

impl TimeRangeKind {
    /// Resolve to absolute (start_ms, end_ms) based on current time.
    pub fn resolve(&self) -> (i64, i64) {
        let now = now_ms();
        match self {
            Self::Last24Hours => (now - 86_400_000, now),
            Self::Last7Days => (now - 7 * 86_400_000, now),
            Self::Last30Days => (now - 30 * 86_400_000, now),
            Self::LastMonth => {
                // Calendar month: from 1st of last month to 1st of this month
                // Approximation: 30 days back (good enough for v1)
                (now - 30 * 86_400_000, now)
            }
            Self::Custom { start_ms, end_ms } => (*start_ms, *end_ms),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Last24Hours => "Last 24 Hours",
            Self::Last7Days => "Last 7 Days",
            Self::Last30Days => "Last 30 Days",
            Self::LastMonth => "Last Month",
            Self::Custom { .. } => "Custom Range",
        }
    }
}

/// How to select points for a report section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointSelector {
    Explicit(Vec<PointRef>),
    ByTag(String),
    ByParentNode(String),
    ByDevices(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PointRef {
    pub device_id: String,
    pub point_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Type of data a report section displays.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionType {
    HistorySummary,
    AlarmSummary,
    AlarmList,
    CurrentValues,
    RuntimeSummary,
    EnergyConsumption,
    DemandSummary,
}

impl SectionType {
    pub fn label(&self) -> &'static str {
        match self {
            Self::HistorySummary => "History Summary",
            Self::AlarmSummary => "Alarm Summary",
            Self::AlarmList => "Alarm List",
            Self::CurrentValues => "Current Values",
            Self::RuntimeSummary => "Runtime Summary",
            Self::EnergyConsumption => "Energy Consumption",
            Self::DemandSummary => "Demand Summary",
        }
    }
}

/// Aggregation granularity for history data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregationMode {
    Raw,
    Hourly,
    Daily,
}

impl AggregationMode {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Raw => "Raw",
            Self::Hourly => "Hourly",
            Self::Daily => "Daily",
        }
    }
}

/// A single section within a report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportSection {
    pub title: String,
    pub section_type: SectionType,
    pub point_selector: PointSelector,
    #[serde(default = "default_aggregation")]
    pub aggregation: AggregationMode,
}

fn default_aggregation() -> AggregationMode {
    AggregationMode::Raw
}

/// Full report configuration — stored as JSON in report_definition.config_json.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportConfig {
    pub time_range: TimeRangeKind,
    pub sections: Vec<ReportSection>,
}

/// A report definition row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportDefinition {
    pub id: i64,
    pub name: String,
    pub report_type: ReportType,
    pub config: ReportConfig,
    pub created_ms: i64,
    pub updated_ms: i64,
}

/// A recipient for scheduled report delivery.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportRecipient {
    pub email: String,
    pub name: String,
}

/// A report schedule row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportSchedule {
    pub id: i64,
    pub report_id: i64,
    pub frequency: ReportFrequency,
    pub day_of_week: Option<u8>,
    pub day_of_month: Option<u8>,
    pub hour: u8,
    pub minute: u8,
    pub timezone_offset_mins: i32,
    pub enabled: bool,
    pub recipients: Vec<ReportRecipient>,
    pub last_run_ms: Option<i64>,
    pub next_run_ms: Option<i64>,
}

/// A report execution row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportExecution {
    pub id: i64,
    pub report_id: i64,
    pub schedule_id: Option<i64>,
    pub status: ExecutionStatus,
    pub triggered_by: String,
    pub started_ms: i64,
    pub completed_ms: Option<i64>,
    pub report_html: Option<String>,
    pub error_message: Option<String>,
    pub delivery_status: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ReportError {
    #[error("database error: {0}")]
    Db(String),
    #[error("channel closed")]
    ChannelClosed,
    #[error("not found")]
    NotFound,
    #[error("invalid config: {0}")]
    InvalidConfig(String),
}

// ----------------------------------------------------------------
// Commands sent to the SQLite thread
// ----------------------------------------------------------------

enum ReportCmd {
    // Definition CRUD
    CreateDefinition {
        name: String,
        report_type: ReportType,
        config_json: String,
        reply: oneshot::Sender<Result<i64, ReportError>>,
    },
    UpdateDefinition {
        id: i64,
        name: String,
        report_type: ReportType,
        config_json: String,
        reply: oneshot::Sender<Result<(), ReportError>>,
    },
    DeleteDefinition {
        id: i64,
        reply: oneshot::Sender<Result<(), ReportError>>,
    },
    GetDefinition {
        id: i64,
        reply: oneshot::Sender<Result<ReportDefinition, ReportError>>,
    },
    ListDefinitions {
        reply: oneshot::Sender<Vec<ReportDefinition>>,
    },
    // Schedule CRUD
    CreateSchedule {
        report_id: i64,
        frequency: ReportFrequency,
        day_of_week: Option<u8>,
        day_of_month: Option<u8>,
        hour: u8,
        minute: u8,
        timezone_offset_mins: i32,
        recipients_json: String,
        reply: oneshot::Sender<Result<i64, ReportError>>,
    },
    UpdateSchedule {
        id: i64,
        frequency: ReportFrequency,
        day_of_week: Option<u8>,
        day_of_month: Option<u8>,
        hour: u8,
        minute: u8,
        timezone_offset_mins: i32,
        enabled: bool,
        recipients_json: String,
        reply: oneshot::Sender<Result<(), ReportError>>,
    },
    DeleteSchedule {
        id: i64,
        reply: oneshot::Sender<Result<(), ReportError>>,
    },
    ListSchedules {
        report_id: Option<i64>,
        reply: oneshot::Sender<Vec<ReportSchedule>>,
    },
    GetDueSchedules {
        now_ms: i64,
        reply: oneshot::Sender<Vec<ReportSchedule>>,
    },
    UpdateScheduleLastRun {
        id: i64,
        last_run_ms: i64,
        next_run_ms: i64,
        reply: oneshot::Sender<Result<(), ReportError>>,
    },
    // Execution
    InsertExecution {
        report_id: i64,
        schedule_id: Option<i64>,
        triggered_by: String,
        started_ms: i64,
        reply: oneshot::Sender<Result<i64, ReportError>>,
    },
    UpdateExecution {
        id: i64,
        status: ExecutionStatus,
        completed_ms: Option<i64>,
        report_html: Option<String>,
        error_message: Option<String>,
        delivery_status: Option<String>,
        reply: oneshot::Sender<Result<(), ReportError>>,
    },
    ListExecutions {
        report_id: Option<i64>,
        limit: i64,
        reply: oneshot::Sender<Vec<ReportExecution>>,
    },
    GetExecution {
        id: i64,
        reply: oneshot::Sender<Result<ReportExecution, ReportError>>,
    },
    // Config key-value
    GetConfig {
        key: String,
        reply: oneshot::Sender<Option<String>>,
    },
    SetConfig {
        key: String,
        value: String,
        reply: oneshot::Sender<Result<(), ReportError>>,
    },
}

// ----------------------------------------------------------------
// ReportStore — async handle to the SQLite thread
// ----------------------------------------------------------------

#[derive(Clone)]
pub struct ReportStore {
    cmd_tx: mpsc::UnboundedSender<ReportCmd>,
}

impl ReportStore {
    // ---- Definition CRUD ----

    pub async fn create_definition(
        &self,
        name: &str,
        report_type: ReportType,
        config: &ReportConfig,
    ) -> Result<i64, ReportError> {
        let config_json =
            serde_json::to_string(config).map_err(|e| ReportError::InvalidConfig(e.to_string()))?;
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(ReportCmd::CreateDefinition {
                name: name.to_string(),
                report_type,
                config_json,
                reply: reply_tx,
            })
            .map_err(|_| ReportError::ChannelClosed)?;
        reply_rx.await.map_err(|_| ReportError::ChannelClosed)?
    }

    pub async fn update_definition(
        &self,
        id: i64,
        name: &str,
        report_type: ReportType,
        config: &ReportConfig,
    ) -> Result<(), ReportError> {
        let config_json =
            serde_json::to_string(config).map_err(|e| ReportError::InvalidConfig(e.to_string()))?;
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(ReportCmd::UpdateDefinition {
                id,
                name: name.to_string(),
                report_type,
                config_json,
                reply: reply_tx,
            })
            .map_err(|_| ReportError::ChannelClosed)?;
        reply_rx.await.map_err(|_| ReportError::ChannelClosed)?
    }

    pub async fn delete_definition(&self, id: i64) -> Result<(), ReportError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(ReportCmd::DeleteDefinition {
                id,
                reply: reply_tx,
            })
            .map_err(|_| ReportError::ChannelClosed)?;
        reply_rx.await.map_err(|_| ReportError::ChannelClosed)?
    }

    pub async fn get_definition(&self, id: i64) -> Result<ReportDefinition, ReportError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(ReportCmd::GetDefinition {
                id,
                reply: reply_tx,
            })
            .map_err(|_| ReportError::ChannelClosed)?;
        reply_rx.await.map_err(|_| ReportError::ChannelClosed)?
    }

    pub async fn list_definitions(&self) -> Vec<ReportDefinition> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .cmd_tx
            .send(ReportCmd::ListDefinitions { reply: reply_tx });
        reply_rx.await.unwrap_or_default()
    }

    // ---- Schedule CRUD ----

    pub async fn create_schedule(
        &self,
        report_id: i64,
        frequency: ReportFrequency,
        day_of_week: Option<u8>,
        day_of_month: Option<u8>,
        hour: u8,
        minute: u8,
        timezone_offset_mins: i32,
        recipients: &[ReportRecipient],
    ) -> Result<i64, ReportError> {
        let recipients_json = serde_json::to_string(recipients)
            .map_err(|e| ReportError::InvalidConfig(e.to_string()))?;
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(ReportCmd::CreateSchedule {
                report_id,
                frequency,
                day_of_week,
                day_of_month,
                hour,
                minute,
                timezone_offset_mins,
                recipients_json,
                reply: reply_tx,
            })
            .map_err(|_| ReportError::ChannelClosed)?;
        reply_rx.await.map_err(|_| ReportError::ChannelClosed)?
    }

    pub async fn update_schedule(
        &self,
        id: i64,
        frequency: ReportFrequency,
        day_of_week: Option<u8>,
        day_of_month: Option<u8>,
        hour: u8,
        minute: u8,
        timezone_offset_mins: i32,
        enabled: bool,
        recipients: &[ReportRecipient],
    ) -> Result<(), ReportError> {
        let recipients_json = serde_json::to_string(recipients)
            .map_err(|e| ReportError::InvalidConfig(e.to_string()))?;
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(ReportCmd::UpdateSchedule {
                id,
                frequency,
                day_of_week,
                day_of_month,
                hour,
                minute,
                timezone_offset_mins,
                enabled,
                recipients_json,
                reply: reply_tx,
            })
            .map_err(|_| ReportError::ChannelClosed)?;
        reply_rx.await.map_err(|_| ReportError::ChannelClosed)?
    }

    pub async fn delete_schedule(&self, id: i64) -> Result<(), ReportError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(ReportCmd::DeleteSchedule {
                id,
                reply: reply_tx,
            })
            .map_err(|_| ReportError::ChannelClosed)?;
        reply_rx.await.map_err(|_| ReportError::ChannelClosed)?
    }

    pub async fn list_schedules(&self, report_id: Option<i64>) -> Vec<ReportSchedule> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(ReportCmd::ListSchedules {
            report_id,
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn get_due_schedules(&self, now_ms: i64) -> Vec<ReportSchedule> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(ReportCmd::GetDueSchedules {
            now_ms,
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn update_schedule_last_run(
        &self,
        id: i64,
        last_run_ms: i64,
        next_run_ms: i64,
    ) -> Result<(), ReportError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(ReportCmd::UpdateScheduleLastRun {
                id,
                last_run_ms,
                next_run_ms,
                reply: reply_tx,
            })
            .map_err(|_| ReportError::ChannelClosed)?;
        reply_rx.await.map_err(|_| ReportError::ChannelClosed)?
    }

    // ---- Execution ----

    pub async fn insert_execution(
        &self,
        report_id: i64,
        schedule_id: Option<i64>,
        triggered_by: &str,
        started_ms: i64,
    ) -> Result<i64, ReportError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(ReportCmd::InsertExecution {
                report_id,
                schedule_id,
                triggered_by: triggered_by.to_string(),
                started_ms,
                reply: reply_tx,
            })
            .map_err(|_| ReportError::ChannelClosed)?;
        reply_rx.await.map_err(|_| ReportError::ChannelClosed)?
    }

    pub async fn update_execution(
        &self,
        id: i64,
        status: ExecutionStatus,
        completed_ms: Option<i64>,
        report_html: Option<String>,
        error_message: Option<String>,
        delivery_status: Option<String>,
    ) -> Result<(), ReportError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(ReportCmd::UpdateExecution {
                id,
                status,
                completed_ms,
                report_html,
                error_message,
                delivery_status,
                reply: reply_tx,
            })
            .map_err(|_| ReportError::ChannelClosed)?;
        reply_rx.await.map_err(|_| ReportError::ChannelClosed)?
    }

    pub async fn list_executions(
        &self,
        report_id: Option<i64>,
        limit: i64,
    ) -> Vec<ReportExecution> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(ReportCmd::ListExecutions {
            report_id,
            limit,
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn get_execution(&self, id: i64) -> Result<ReportExecution, ReportError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(ReportCmd::GetExecution {
                id,
                reply: reply_tx,
            })
            .map_err(|_| ReportError::ChannelClosed)?;
        reply_rx.await.map_err(|_| ReportError::ChannelClosed)?
    }

    // ---- Config key-value ----

    pub async fn get_config(&self, key: &str) -> Option<String> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(ReportCmd::GetConfig {
            key: key.to_string(),
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or(None)
    }

    pub async fn set_config(&self, key: &str, value: &str) -> Result<(), ReportError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(ReportCmd::SetConfig {
                key: key.to_string(),
                value: value.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| ReportError::ChannelClosed)?;
        reply_rx.await.map_err(|_| ReportError::ChannelClosed)?
    }
}

// ----------------------------------------------------------------
// SQLite schema & thread
// ----------------------------------------------------------------

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        label: "initial reporting schema",
        sql: "
CREATE TABLE IF NOT EXISTS report_definition (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL,
    report_type TEXT NOT NULL,
    config_json TEXT NOT NULL DEFAULT '{}',
    created_ms  INTEGER NOT NULL,
    updated_ms  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS report_schedule (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
    report_id            INTEGER NOT NULL REFERENCES report_definition(id) ON DELETE CASCADE,
    frequency            TEXT NOT NULL,
    day_of_week          INTEGER,
    day_of_month         INTEGER,
    hour                 INTEGER NOT NULL DEFAULT 6,
    minute               INTEGER NOT NULL DEFAULT 0,
    timezone_offset_mins INTEGER NOT NULL DEFAULT 0,
    enabled              INTEGER NOT NULL DEFAULT 1,
    recipients_json      TEXT NOT NULL DEFAULT '[]',
    last_run_ms          INTEGER,
    next_run_ms          INTEGER
);
CREATE INDEX IF NOT EXISTS idx_schedule_report ON report_schedule(report_id);
CREATE INDEX IF NOT EXISTS idx_schedule_next_run ON report_schedule(next_run_ms);

CREATE TABLE IF NOT EXISTS report_execution (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    report_id       INTEGER NOT NULL REFERENCES report_definition(id) ON DELETE CASCADE,
    schedule_id     INTEGER REFERENCES report_schedule(id) ON DELETE SET NULL,
    status          TEXT NOT NULL,
    triggered_by    TEXT NOT NULL,
    started_ms      INTEGER NOT NULL,
    completed_ms    INTEGER,
    report_html     TEXT,
    error_message   TEXT,
    delivery_status TEXT
);
CREATE INDEX IF NOT EXISTS idx_execution_report ON report_execution(report_id);
CREATE INDEX IF NOT EXISTS idx_execution_started ON report_execution(started_ms);
",
    },
    Migration {
        version: 2,
        label: "add report_config key-value table for SMTP settings",
        sql: "
CREATE TABLE IF NOT EXISTS report_config (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL DEFAULT ''
);
",
    },
];

fn run_sqlite_thread(db_path: &Path, mut cmd_rx: mpsc::UnboundedReceiver<ReportCmd>) {
    let conn = rusqlite::Connection::open(db_path).expect("failed to open reports database");
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;",
    )
    .expect("failed to set pragmas");
    run_migrations(&conn, "reports", MIGRATIONS).expect("reports: schema migration failed");

    while let Some(cmd) = cmd_rx.blocking_recv() {
        match cmd {
            // ---- Definitions ----
            ReportCmd::CreateDefinition {
                name,
                report_type,
                config_json,
                reply,
            } => {
                let _ = reply.send(create_definition_db(
                    &conn,
                    &name,
                    &report_type,
                    &config_json,
                ));
            }
            ReportCmd::UpdateDefinition {
                id,
                name,
                report_type,
                config_json,
                reply,
            } => {
                let _ = reply.send(update_definition_db(
                    &conn,
                    id,
                    &name,
                    &report_type,
                    &config_json,
                ));
            }
            ReportCmd::DeleteDefinition { id, reply } => {
                let _ = reply.send(delete_definition_db(&conn, id));
            }
            ReportCmd::GetDefinition { id, reply } => {
                let _ = reply.send(get_definition_db(&conn, id));
            }
            ReportCmd::ListDefinitions { reply } => {
                let _ = reply.send(list_definitions_db(&conn));
            }
            // ---- Schedules ----
            ReportCmd::CreateSchedule {
                report_id,
                frequency,
                day_of_week,
                day_of_month,
                hour,
                minute,
                timezone_offset_mins,
                recipients_json,
                reply,
            } => {
                let _ = reply.send(create_schedule_db(
                    &conn,
                    report_id,
                    &frequency,
                    day_of_week,
                    day_of_month,
                    hour,
                    minute,
                    timezone_offset_mins,
                    &recipients_json,
                ));
            }
            ReportCmd::UpdateSchedule {
                id,
                frequency,
                day_of_week,
                day_of_month,
                hour,
                minute,
                timezone_offset_mins,
                enabled,
                recipients_json,
                reply,
            } => {
                let _ = reply.send(update_schedule_db(
                    &conn,
                    id,
                    &frequency,
                    day_of_week,
                    day_of_month,
                    hour,
                    minute,
                    timezone_offset_mins,
                    enabled,
                    &recipients_json,
                ));
            }
            ReportCmd::DeleteSchedule { id, reply } => {
                let _ = reply.send(delete_schedule_db(&conn, id));
            }
            ReportCmd::ListSchedules { report_id, reply } => {
                let _ = reply.send(list_schedules_db(&conn, report_id));
            }
            ReportCmd::GetDueSchedules { now_ms, reply } => {
                let _ = reply.send(get_due_schedules_db(&conn, now_ms));
            }
            ReportCmd::UpdateScheduleLastRun {
                id,
                last_run_ms,
                next_run_ms,
                reply,
            } => {
                let _ = reply.send(update_schedule_last_run_db(
                    &conn,
                    id,
                    last_run_ms,
                    next_run_ms,
                ));
            }
            // ---- Executions ----
            ReportCmd::InsertExecution {
                report_id,
                schedule_id,
                triggered_by,
                started_ms,
                reply,
            } => {
                let _ = reply.send(insert_execution_db(
                    &conn,
                    report_id,
                    schedule_id,
                    &triggered_by,
                    started_ms,
                ));
            }
            ReportCmd::UpdateExecution {
                id,
                status,
                completed_ms,
                report_html,
                error_message,
                delivery_status,
                reply,
            } => {
                let _ = reply.send(update_execution_db(
                    &conn,
                    id,
                    &status,
                    completed_ms,
                    report_html.as_deref(),
                    error_message.as_deref(),
                    delivery_status.as_deref(),
                ));
            }
            ReportCmd::ListExecutions {
                report_id,
                limit,
                reply,
            } => {
                let _ = reply.send(list_executions_db(&conn, report_id, limit));
            }
            ReportCmd::GetExecution { id, reply } => {
                let _ = reply.send(get_execution_db(&conn, id));
            }
            // ---- Config ----
            ReportCmd::GetConfig { key, reply } => {
                let val: Option<String> = conn
                    .query_row(
                        "SELECT value FROM report_config WHERE key = ?1",
                        rusqlite::params![key],
                        |row| row.get(0),
                    )
                    .ok();
                let _ = reply.send(val);
            }
            ReportCmd::SetConfig { key, value, reply } => {
                let result = conn
                    .execute(
                        "INSERT INTO report_config (key, value) VALUES (?1, ?2)
                         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                        rusqlite::params![key, value],
                    )
                    .map(|_| ())
                    .map_err(|e| ReportError::Db(e.to_string()));
                let _ = reply.send(result);
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

// ---- Definitions ----

fn create_definition_db(
    conn: &rusqlite::Connection,
    name: &str,
    report_type: &ReportType,
    config_json: &str,
) -> Result<i64, ReportError> {
    let ts = now_ms();
    conn.execute(
        "INSERT INTO report_definition (name, report_type, config_json, created_ms, updated_ms)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![name, report_type.as_str(), config_json, ts, ts],
    )
    .map_err(|e| ReportError::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

fn update_definition_db(
    conn: &rusqlite::Connection,
    id: i64,
    name: &str,
    report_type: &ReportType,
    config_json: &str,
) -> Result<(), ReportError> {
    let ts = now_ms();
    let rows = conn
        .execute(
            "UPDATE report_definition SET name = ?1, report_type = ?2, config_json = ?3, updated_ms = ?4 WHERE id = ?5",
            rusqlite::params![name, report_type.as_str(), config_json, ts, id],
        )
        .map_err(|e| ReportError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(ReportError::NotFound);
    }
    Ok(())
}

fn delete_definition_db(conn: &rusqlite::Connection, id: i64) -> Result<(), ReportError> {
    let rows = conn
        .execute(
            "DELETE FROM report_definition WHERE id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| ReportError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(ReportError::NotFound);
    }
    Ok(())
}

fn parse_definition_row(row: &rusqlite::Row) -> rusqlite::Result<ReportDefinition> {
    let rt_str: String = row.get(2)?;
    let config_str: String = row.get(3)?;
    let config: ReportConfig = serde_json::from_str(&config_str).unwrap_or(ReportConfig {
        time_range: TimeRangeKind::Last24Hours,
        sections: vec![],
    });
    Ok(ReportDefinition {
        id: row.get(0)?,
        name: row.get(1)?,
        report_type: ReportType::from_str(&rt_str).unwrap_or(ReportType::Custom),
        config,
        created_ms: row.get(4)?,
        updated_ms: row.get(5)?,
    })
}

fn get_definition_db(
    conn: &rusqlite::Connection,
    id: i64,
) -> Result<ReportDefinition, ReportError> {
    conn.query_row(
        "SELECT id, name, report_type, config_json, created_ms, updated_ms FROM report_definition WHERE id = ?1",
        rusqlite::params![id],
        parse_definition_row,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => ReportError::NotFound,
        other => ReportError::Db(other.to_string()),
    })
}

fn list_definitions_db(conn: &rusqlite::Connection) -> Vec<ReportDefinition> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT id, name, report_type, config_json, created_ms, updated_ms FROM report_definition ORDER BY name",
        )
        .unwrap();
    let rows = stmt.query_map([], parse_definition_row).unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

// ---- Schedules ----

fn create_schedule_db(
    conn: &rusqlite::Connection,
    report_id: i64,
    frequency: &ReportFrequency,
    day_of_week: Option<u8>,
    day_of_month: Option<u8>,
    hour: u8,
    minute: u8,
    timezone_offset_mins: i32,
    recipients_json: &str,
) -> Result<i64, ReportError> {
    let next_run = compute_next_run_ms(
        frequency,
        day_of_week,
        day_of_month,
        hour,
        minute,
        timezone_offset_mins,
        now_ms(),
    );
    conn.execute(
        "INSERT INTO report_schedule (report_id, frequency, day_of_week, day_of_month, hour, minute, timezone_offset_mins, enabled, recipients_json, next_run_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8, ?9)",
        rusqlite::params![
            report_id, frequency.as_str(), day_of_week.map(|v| v as i32), day_of_month.map(|v| v as i32),
            hour as i32, minute as i32, timezone_offset_mins, recipients_json, next_run,
        ],
    )
    .map_err(|e| ReportError::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

fn update_schedule_db(
    conn: &rusqlite::Connection,
    id: i64,
    frequency: &ReportFrequency,
    day_of_week: Option<u8>,
    day_of_month: Option<u8>,
    hour: u8,
    minute: u8,
    timezone_offset_mins: i32,
    enabled: bool,
    recipients_json: &str,
) -> Result<(), ReportError> {
    let next_run = compute_next_run_ms(
        frequency,
        day_of_week,
        day_of_month,
        hour,
        minute,
        timezone_offset_mins,
        now_ms(),
    );
    let rows = conn
        .execute(
            "UPDATE report_schedule SET frequency = ?1, day_of_week = ?2, day_of_month = ?3, hour = ?4, minute = ?5, timezone_offset_mins = ?6, enabled = ?7, recipients_json = ?8, next_run_ms = ?9 WHERE id = ?10",
            rusqlite::params![
                frequency.as_str(), day_of_week.map(|v| v as i32), day_of_month.map(|v| v as i32),
                hour as i32, minute as i32, timezone_offset_mins, enabled as i32, recipients_json, next_run, id,
            ],
        )
        .map_err(|e| ReportError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(ReportError::NotFound);
    }
    Ok(())
}

fn delete_schedule_db(conn: &rusqlite::Connection, id: i64) -> Result<(), ReportError> {
    let rows = conn
        .execute(
            "DELETE FROM report_schedule WHERE id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| ReportError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(ReportError::NotFound);
    }
    Ok(())
}

fn parse_schedule_row(row: &rusqlite::Row) -> rusqlite::Result<ReportSchedule> {
    let freq_str: String = row.get(2)?;
    let recip_str: String = row.get(9)?;
    let recipients: Vec<ReportRecipient> = serde_json::from_str(&recip_str).unwrap_or_default();
    Ok(ReportSchedule {
        id: row.get(0)?,
        report_id: row.get(1)?,
        frequency: ReportFrequency::from_str(&freq_str).unwrap_or(ReportFrequency::Daily),
        day_of_week: row.get::<_, Option<i32>>(3)?.map(|v| v as u8),
        day_of_month: row.get::<_, Option<i32>>(4)?.map(|v| v as u8),
        hour: row.get::<_, i32>(5)? as u8,
        minute: row.get::<_, i32>(6)? as u8,
        timezone_offset_mins: row.get(7)?,
        enabled: row.get::<_, i32>(8)? != 0,
        recipients,
        last_run_ms: row.get(10)?,
        next_run_ms: row.get(11)?,
    })
}

const SCHEDULE_SELECT: &str =
    "SELECT id, report_id, frequency, day_of_week, day_of_month, hour, minute, timezone_offset_mins, enabled, recipients_json, last_run_ms, next_run_ms FROM report_schedule";

fn list_schedules_db(conn: &rusqlite::Connection, report_id: Option<i64>) -> Vec<ReportSchedule> {
    match report_id {
        Some(rid) => {
            let sql = format!("{SCHEDULE_SELECT} WHERE report_id = ?1 ORDER BY id");
            let mut stmt = conn.prepare_cached(&sql).unwrap();
            let rows = stmt
                .query_map(rusqlite::params![rid], parse_schedule_row)
                .unwrap();
            rows.filter_map(|r| r.ok()).collect()
        }
        None => {
            let sql = format!("{SCHEDULE_SELECT} ORDER BY id");
            let mut stmt = conn.prepare_cached(&sql).unwrap();
            let rows = stmt.query_map([], parse_schedule_row).unwrap();
            rows.filter_map(|r| r.ok()).collect()
        }
    }
}

fn get_due_schedules_db(conn: &rusqlite::Connection, now_ms: i64) -> Vec<ReportSchedule> {
    let sql = format!(
        "{SCHEDULE_SELECT} WHERE enabled = 1 AND next_run_ms IS NOT NULL AND next_run_ms <= ?1"
    );
    let mut stmt = conn.prepare_cached(&sql).unwrap();
    let rows = stmt
        .query_map(rusqlite::params![now_ms], parse_schedule_row)
        .unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

fn update_schedule_last_run_db(
    conn: &rusqlite::Connection,
    id: i64,
    last_run_ms: i64,
    next_run_ms: i64,
) -> Result<(), ReportError> {
    let rows = conn
        .execute(
            "UPDATE report_schedule SET last_run_ms = ?1, next_run_ms = ?2 WHERE id = ?3",
            rusqlite::params![last_run_ms, next_run_ms, id],
        )
        .map_err(|e| ReportError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(ReportError::NotFound);
    }
    Ok(())
}

// ---- Executions ----

fn insert_execution_db(
    conn: &rusqlite::Connection,
    report_id: i64,
    schedule_id: Option<i64>,
    triggered_by: &str,
    started_ms: i64,
) -> Result<i64, ReportError> {
    conn.execute(
        "INSERT INTO report_execution (report_id, schedule_id, status, triggered_by, started_ms)
         VALUES (?1, ?2, 'running', ?3, ?4)",
        rusqlite::params![report_id, schedule_id, triggered_by, started_ms],
    )
    .map_err(|e| ReportError::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

fn update_execution_db(
    conn: &rusqlite::Connection,
    id: i64,
    status: &ExecutionStatus,
    completed_ms: Option<i64>,
    report_html: Option<&str>,
    error_message: Option<&str>,
    delivery_status: Option<&str>,
) -> Result<(), ReportError> {
    let rows = conn
        .execute(
            "UPDATE report_execution SET status = ?1, completed_ms = ?2, report_html = ?3, error_message = ?4, delivery_status = ?5 WHERE id = ?6",
            rusqlite::params![status.as_str(), completed_ms, report_html, error_message, delivery_status, id],
        )
        .map_err(|e| ReportError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(ReportError::NotFound);
    }
    Ok(())
}

fn parse_execution_row(row: &rusqlite::Row) -> rusqlite::Result<ReportExecution> {
    let st_str: String = row.get(3)?;
    Ok(ReportExecution {
        id: row.get(0)?,
        report_id: row.get(1)?,
        schedule_id: row.get(2)?,
        status: ExecutionStatus::from_str(&st_str).unwrap_or(ExecutionStatus::Failed),
        triggered_by: row.get(4)?,
        started_ms: row.get(5)?,
        completed_ms: row.get(6)?,
        report_html: row.get(7)?,
        error_message: row.get(8)?,
        delivery_status: row.get(9)?,
    })
}

const EXECUTION_SELECT: &str =
    "SELECT id, report_id, schedule_id, status, triggered_by, started_ms, completed_ms, report_html, error_message, delivery_status FROM report_execution";

fn list_executions_db(
    conn: &rusqlite::Connection,
    report_id: Option<i64>,
    limit: i64,
) -> Vec<ReportExecution> {
    match report_id {
        Some(rid) => {
            let sql = format!(
                "{EXECUTION_SELECT} WHERE report_id = ?1 ORDER BY started_ms DESC LIMIT ?2"
            );
            let mut stmt = conn.prepare_cached(&sql).unwrap();
            let rows = stmt
                .query_map(rusqlite::params![rid, limit], parse_execution_row)
                .unwrap();
            rows.filter_map(|r| r.ok()).collect()
        }
        None => {
            let sql = format!("{EXECUTION_SELECT} ORDER BY started_ms DESC LIMIT ?1");
            let mut stmt = conn.prepare_cached(&sql).unwrap();
            let rows = stmt
                .query_map(rusqlite::params![limit], parse_execution_row)
                .unwrap();
            rows.filter_map(|r| r.ok()).collect()
        }
    }
}

fn get_execution_db(conn: &rusqlite::Connection, id: i64) -> Result<ReportExecution, ReportError> {
    let sql = format!("{EXECUTION_SELECT} WHERE id = ?1");
    conn.query_row(&sql, rusqlite::params![id], parse_execution_row)
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => ReportError::NotFound,
            other => ReportError::Db(other.to_string()),
        })
}

// ---- Schedule helpers ----

/// Compute the next run time for a schedule based on the current time.
pub fn compute_next_run_ms(
    frequency: &ReportFrequency,
    day_of_week: Option<u8>,
    day_of_month: Option<u8>,
    hour: u8,
    minute: u8,
    tz_offset_mins: i32,
    from_ms: i64,
) -> i64 {
    // Target time-of-day in UTC (offset from local)
    let target_minutes_utc = (hour as i32 * 60 + minute as i32) - tz_offset_mins;

    // Start of current UTC day
    let day_ms: i64 = 86_400_000;
    let today_start = (from_ms / day_ms) * day_ms;
    let target_today = today_start + (target_minutes_utc as i64 * 60_000);

    match frequency {
        ReportFrequency::Daily => {
            if target_today > from_ms {
                target_today
            } else {
                target_today + day_ms
            }
        }
        ReportFrequency::Weekly => {
            // day_of_week: 0=Mon..6=Sun
            let dow = day_of_week.unwrap_or(0) as i64;
            // Calculate current day of week (0=Thu for epoch, adjust)
            // Unix epoch (Jan 1, 1970) was a Thursday = 3
            let current_day_index = (from_ms / day_ms) % 7; // 0=Thu, 1=Fri, ...
            let current_dow = (current_day_index + 3) % 7; // Convert: 0=Mon, 1=Tue, ...
            let mut days_until = dow - current_dow;
            if days_until < 0 {
                days_until += 7;
            }
            let candidate =
                today_start + days_until * day_ms + (target_minutes_utc as i64 * 60_000);
            if candidate > from_ms {
                candidate
            } else {
                candidate + 7 * day_ms
            }
        }
        ReportFrequency::Monthly => {
            // Simple: advance to the target day_of_month (capped at 28) in current or next month
            let dom = day_of_month.unwrap_or(1).min(28) as i64;
            // Approximate: use 30-day months
            let days_into_epoch = from_ms / day_ms;
            // Rough month start: find day 1 of current ~30-day period
            let approx_month_day = (days_into_epoch % 30) + 1;
            let candidate_offset = (dom - approx_month_day) * day_ms;
            let candidate = today_start + candidate_offset + (target_minutes_utc as i64 * 60_000);
            if candidate > from_ms {
                candidate
            } else {
                candidate + 30 * day_ms
            }
        }
    }
}

// ----------------------------------------------------------------
// Constructor
// ----------------------------------------------------------------

pub fn start_report_store_with_path(db_path: &Path) -> ReportStore {
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let path_clone = db_path.to_path_buf();
    std::thread::Builder::new()
        .name("report-sqlite".into())
        .spawn(move || run_sqlite_thread(&path_clone, cmd_rx))
        .expect("failed to spawn report SQLite thread");
    ReportStore { cmd_tx }
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
        let dir = std::env::temp_dir().join("opencrate-report-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("report-test-{n}-{tid:?}.db"));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
        path
    }

    #[tokio::test]
    async fn test_definition_crud() {
        let db_path = temp_db_path();
        let store = start_report_store_with_path(&db_path);

        let config = ReportConfig {
            time_range: TimeRangeKind::Last24Hours,
            sections: vec![ReportSection {
                title: "Energy".to_string(),
                section_type: SectionType::HistorySummary,
                point_selector: PointSelector::ByTag("power".to_string()),
                aggregation: AggregationMode::Daily,
            }],
        };

        // Create
        let id = store
            .create_definition("Daily Energy", ReportType::EnergySummary, &config)
            .await
            .unwrap();
        assert!(id > 0);

        // Get
        let def = store.get_definition(id).await.unwrap();
        assert_eq!(def.name, "Daily Energy");
        assert_eq!(def.report_type, ReportType::EnergySummary);
        assert_eq!(def.config.sections.len(), 1);

        // List
        let defs = store.list_definitions().await;
        assert_eq!(defs.len(), 1);

        // Update
        let config2 = ReportConfig {
            time_range: TimeRangeKind::Last7Days,
            sections: vec![],
        };
        store
            .update_definition(id, "Weekly Energy", ReportType::EnergySummary, &config2)
            .await
            .unwrap();
        let def = store.get_definition(id).await.unwrap();
        assert_eq!(def.name, "Weekly Energy");
        assert_eq!(def.config.time_range, TimeRangeKind::Last7Days);

        // Delete
        store.delete_definition(id).await.unwrap();
        let defs = store.list_definitions().await;
        assert!(defs.is_empty());

        // Delete non-existent
        assert!(store.delete_definition(999).await.is_err());

        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn test_schedule_crud() {
        let db_path = temp_db_path();
        let store = start_report_store_with_path(&db_path);

        let config = ReportConfig {
            time_range: TimeRangeKind::Last24Hours,
            sections: vec![],
        };
        let report_id = store
            .create_definition("Test Report", ReportType::Custom, &config)
            .await
            .unwrap();

        let recipients = vec![ReportRecipient {
            email: "ops@example.com".to_string(),
            name: "Ops".to_string(),
        }];

        // Create schedule
        let sched_id = store
            .create_schedule(
                report_id,
                ReportFrequency::Daily,
                None,
                None,
                6,
                0,
                0,
                &recipients,
            )
            .await
            .unwrap();
        assert!(sched_id > 0);

        // List
        let scheds = store.list_schedules(Some(report_id)).await;
        assert_eq!(scheds.len(), 1);
        assert_eq!(scheds[0].frequency, ReportFrequency::Daily);
        assert_eq!(scheds[0].hour, 6);
        assert!(scheds[0].enabled);
        assert_eq!(scheds[0].recipients.len(), 1);
        assert_eq!(scheds[0].recipients[0].email, "ops@example.com");
        assert!(scheds[0].next_run_ms.is_some());

        // Update
        store
            .update_schedule(
                sched_id,
                ReportFrequency::Weekly,
                Some(0),
                None,
                8,
                30,
                -300,
                true,
                &recipients,
            )
            .await
            .unwrap();
        let scheds = store.list_schedules(None).await;
        assert_eq!(scheds[0].frequency, ReportFrequency::Weekly);
        assert_eq!(scheds[0].day_of_week, Some(0));
        assert_eq!(scheds[0].hour, 8);
        assert_eq!(scheds[0].minute, 30);

        // Delete
        store.delete_schedule(sched_id).await.unwrap();
        let scheds = store.list_schedules(None).await;
        assert!(scheds.is_empty());

        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn test_execution_lifecycle() {
        let db_path = temp_db_path();
        let store = start_report_store_with_path(&db_path);

        let config = ReportConfig {
            time_range: TimeRangeKind::Last24Hours,
            sections: vec![],
        };
        let report_id = store
            .create_definition("Test", ReportType::Custom, &config)
            .await
            .unwrap();

        let started = now_ms();
        let exec_id = store
            .insert_execution(report_id, None, "manual", started)
            .await
            .unwrap();
        assert!(exec_id > 0);

        // Should be running
        let exec = store.get_execution(exec_id).await.unwrap();
        assert_eq!(exec.status, ExecutionStatus::Running);
        assert_eq!(exec.triggered_by, "manual");
        assert!(exec.report_html.is_none());

        // Complete it
        let completed = now_ms();
        store
            .update_execution(
                exec_id,
                ExecutionStatus::Completed,
                Some(completed),
                Some("<html>Report</html>".to_string()),
                None,
                Some("sent".to_string()),
            )
            .await
            .unwrap();

        let exec = store.get_execution(exec_id).await.unwrap();
        assert_eq!(exec.status, ExecutionStatus::Completed);
        assert_eq!(exec.report_html.as_deref(), Some("<html>Report</html>"));
        assert_eq!(exec.delivery_status.as_deref(), Some("sent"));

        // List
        let execs = store.list_executions(Some(report_id), 10).await;
        assert_eq!(execs.len(), 1);

        // Insert a failed one
        let exec_id2 = store
            .insert_execution(report_id, None, "schedule", started)
            .await
            .unwrap();
        store
            .update_execution(
                exec_id2,
                ExecutionStatus::Failed,
                Some(completed),
                None,
                Some("timeout".to_string()),
                None,
            )
            .await
            .unwrap();
        let exec = store.get_execution(exec_id2).await.unwrap();
        assert_eq!(exec.status, ExecutionStatus::Failed);
        assert_eq!(exec.error_message.as_deref(), Some("timeout"));

        let execs = store.list_executions(None, 50).await;
        assert_eq!(execs.len(), 2);

        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn test_due_schedules() {
        let db_path = temp_db_path();
        let store = start_report_store_with_path(&db_path);

        let config = ReportConfig {
            time_range: TimeRangeKind::Last24Hours,
            sections: vec![],
        };
        let report_id = store
            .create_definition("Test", ReportType::Custom, &config)
            .await
            .unwrap();

        let recipients = vec![];
        let _ = store
            .create_schedule(
                report_id,
                ReportFrequency::Daily,
                None,
                None,
                6,
                0,
                0,
                &recipients,
            )
            .await
            .unwrap();

        // Query far in the future — should find the schedule
        let future_ms = now_ms() + 365 * 86_400_000;
        let due = store.get_due_schedules(future_ms).await;
        assert_eq!(due.len(), 1);

        // Query in the past — should find nothing
        let due = store.get_due_schedules(0).await;
        assert!(due.is_empty());

        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn test_cascade_delete() {
        let db_path = temp_db_path();
        let store = start_report_store_with_path(&db_path);

        let config = ReportConfig {
            time_range: TimeRangeKind::Last24Hours,
            sections: vec![],
        };
        let report_id = store
            .create_definition("Test", ReportType::Custom, &config)
            .await
            .unwrap();

        // Create schedule + execution
        let _ = store
            .create_schedule(report_id, ReportFrequency::Daily, None, None, 6, 0, 0, &[])
            .await
            .unwrap();
        let _ = store
            .insert_execution(report_id, None, "manual", now_ms())
            .await
            .unwrap();

        // Delete definition — should cascade
        store.delete_definition(report_id).await.unwrap();
        let scheds = store.list_schedules(Some(report_id)).await;
        assert!(scheds.is_empty());
        let execs = store.list_executions(Some(report_id), 50).await;
        assert!(execs.is_empty());

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn test_compute_next_run_daily() {
        let from = 1711411200000_i64; // 2024-03-26 00:00:00 UTC
        let next = compute_next_run_ms(&ReportFrequency::Daily, None, None, 6, 0, 0, from);
        // Should be 6:00 UTC same day
        assert_eq!(next, from + 6 * 3_600_000);

        // If already past 6:00, should be next day
        let from_late = from + 7 * 3_600_000; // 07:00 UTC
        let next = compute_next_run_ms(&ReportFrequency::Daily, None, None, 6, 0, 0, from_late);
        assert_eq!(next, from + 86_400_000 + 6 * 3_600_000);
    }
}
