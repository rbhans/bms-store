use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use tokio::sync::{mpsc, watch};

use crate::config::profile::PointValue;
use crate::event::bus::EventBus;
use crate::store::point_store::PointStore;

use super::db::{run_sqlite_thread, EngineData, ScheduleCmd};
use super::engine::run_schedule_engine;
use super::types::*;

// ----------------------------------------------------------------
// ScheduleStore — async handle to the SQLite thread
// ----------------------------------------------------------------

#[derive(Clone)]
pub struct ScheduleStore {
    pub(super) cmd_tx: mpsc::UnboundedSender<ScheduleCmd>,
    pub(super) config_version_tx: watch::Sender<u64>,
    pub(super) config_version_rx: watch::Receiver<u64>,
    pub(super) event_bus: Option<EventBus>,
}

impl ScheduleStore {
    pub fn with_event_bus(mut self, bus: EventBus) -> Self {
        self.event_bus = Some(bus);
        self
    }

    pub async fn create_schedule(
        &self,
        name: &str,
        description: &str,
        value_type: ScheduleValueType,
        default_value: PointValue,
        weekly: WeeklySchedule,
    ) -> Result<ScheduleId, ScheduleError> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.cmd_tx
            .send(ScheduleCmd::CreateSchedule {
                name: name.to_string(),
                description: description.to_string(),
                value_type,
                default_value,
                weekly,
                reply: reply_tx,
            })
            .map_err(|_| ScheduleError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| ScheduleError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_config_version();
        }
        result
    }

    pub async fn update_schedule(
        &self,
        id: ScheduleId,
        name: &str,
        description: &str,
        default_value: PointValue,
        enabled: bool,
        weekly: WeeklySchedule,
    ) -> Result<(), ScheduleError> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.cmd_tx
            .send(ScheduleCmd::UpdateSchedule {
                id,
                name: name.to_string(),
                description: description.to_string(),
                default_value,
                enabled,
                weekly,
                reply: reply_tx,
            })
            .map_err(|_| ScheduleError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| ScheduleError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_config_version();
        }
        result
    }

    pub async fn delete_schedule(&self, id: ScheduleId) -> Result<(), ScheduleError> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.cmd_tx
            .send(ScheduleCmd::DeleteSchedule {
                id,
                reply: reply_tx,
            })
            .map_err(|_| ScheduleError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| ScheduleError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_config_version();
        }
        result
    }

    pub async fn list_schedules(&self) -> Vec<Schedule> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        let _ = self
            .cmd_tx
            .send(ScheduleCmd::ListSchedules { reply: reply_tx });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn get_schedule(&self, id: ScheduleId) -> Option<Schedule> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        let _ = self.cmd_tx.send(ScheduleCmd::GetSchedule {
            id,
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or(None)
    }

    pub async fn create_exception_group(
        &self,
        name: &str,
        description: &str,
        entries: Vec<DateSpec>,
    ) -> Result<ExceptionGroupId, ScheduleError> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.cmd_tx
            .send(ScheduleCmd::CreateExceptionGroup {
                name: name.to_string(),
                description: description.to_string(),
                entries,
                reply: reply_tx,
            })
            .map_err(|_| ScheduleError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| ScheduleError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_config_version();
        }
        result
    }

    pub async fn update_exception_group(
        &self,
        id: ExceptionGroupId,
        name: &str,
        description: &str,
        entries: Vec<DateSpec>,
    ) -> Result<(), ScheduleError> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.cmd_tx
            .send(ScheduleCmd::UpdateExceptionGroup {
                id,
                name: name.to_string(),
                description: description.to_string(),
                entries,
                reply: reply_tx,
            })
            .map_err(|_| ScheduleError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| ScheduleError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_config_version();
        }
        result
    }

    pub async fn delete_exception_group(&self, id: ExceptionGroupId) -> Result<(), ScheduleError> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.cmd_tx
            .send(ScheduleCmd::DeleteExceptionGroup {
                id,
                reply: reply_tx,
            })
            .map_err(|_| ScheduleError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| ScheduleError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_config_version();
        }
        result
    }

    pub async fn list_exception_groups(&self) -> Vec<ExceptionGroup> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        let _ = self
            .cmd_tx
            .send(ScheduleCmd::ListExceptionGroups { reply: reply_tx });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn add_exception(
        &self,
        schedule_id: ScheduleId,
        group_id: Option<ExceptionGroupId>,
        name: &str,
        date_spec: DateSpec,
        slots: DaySlots,
        use_default: bool,
    ) -> Result<i64, ScheduleError> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.cmd_tx
            .send(ScheduleCmd::AddException {
                schedule_id,
                group_id,
                name: name.to_string(),
                date_spec,
                slots,
                use_default,
                reply: reply_tx,
            })
            .map_err(|_| ScheduleError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| ScheduleError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_config_version();
        }
        result
    }

    pub async fn remove_exception(&self, id: i64) -> Result<(), ScheduleError> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.cmd_tx
            .send(ScheduleCmd::RemoveException {
                id,
                reply: reply_tx,
            })
            .map_err(|_| ScheduleError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| ScheduleError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_config_version();
        }
        result
    }

    pub async fn list_exceptions(&self, schedule_id: ScheduleId) -> Vec<ScheduleException> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        let _ = self.cmd_tx.send(ScheduleCmd::ListExceptions {
            schedule_id,
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn create_assignment(
        &self,
        schedule_id: ScheduleId,
        device_id: &str,
        point_id: &str,
        priority: i32,
    ) -> Result<AssignmentId, ScheduleError> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.cmd_tx
            .send(ScheduleCmd::CreateAssignment {
                schedule_id,
                device_id: device_id.to_string(),
                point_id: point_id.to_string(),
                priority,
                reply: reply_tx,
            })
            .map_err(|_| ScheduleError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| ScheduleError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_config_version();
        }
        result
    }

    pub async fn create_assignments_batch(
        &self,
        schedule_id: ScheduleId,
        entries: &[(String, String)],
        priority: i32,
    ) -> Result<Vec<AssignmentId>, ScheduleError> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.cmd_tx
            .send(ScheduleCmd::CreateAssignmentsBatch {
                schedule_id,
                entries: entries.to_vec(),
                priority,
                reply: reply_tx,
            })
            .map_err(|_| ScheduleError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| ScheduleError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_config_version();
        }
        result
    }

    pub async fn delete_assignment(&self, id: AssignmentId) -> Result<(), ScheduleError> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.cmd_tx
            .send(ScheduleCmd::DeleteAssignment {
                id,
                reply: reply_tx,
            })
            .map_err(|_| ScheduleError::ChannelClosed)?;
        let result = reply_rx.await.map_err(|_| ScheduleError::ChannelClosed)?;
        if result.is_ok() {
            self.bump_config_version();
        }
        result
    }

    pub async fn list_assignments_for_schedule(
        &self,
        schedule_id: ScheduleId,
    ) -> Vec<ScheduleAssignment> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        let _ = self.cmd_tx.send(ScheduleCmd::ListAssignmentsForSchedule {
            schedule_id,
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn get_assignments_for_point(
        &self,
        device_id: &str,
        point_id: &str,
    ) -> Vec<ScheduleAssignment> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        let _ = self.cmd_tx.send(ScheduleCmd::GetAssignmentsForPoint {
            device_id: device_id.to_string(),
            point_id: point_id.to_string(),
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn query_log(
        &self,
        device_id: &str,
        point_id: &str,
        limit: i64,
    ) -> Vec<ScheduleLogEntry> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        let _ = self.cmd_tx.send(ScheduleCmd::QueryLog {
            device_id: device_id.to_string(),
            point_id: point_id.to_string(),
            limit,
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_default()
    }

    pub fn subscribe_config_changes(&self) -> watch::Receiver<u64> {
        self.config_version_rx.clone()
    }

    pub(super) fn bump_config_version(&self) {
        let current = *self.config_version_rx.borrow();
        let _ = self.config_version_tx.send(current + 1);
    }

    // Internal: insert log entry
    pub(super) fn insert_log(
        &self,
        assignment_id: AssignmentId,
        device_id: &str,
        point_id: &str,
        value_json: &str,
        reason: &str,
        timestamp_ms: i64,
    ) {
        let _ = self.cmd_tx.send(ScheduleCmd::InsertLog {
            assignment_id,
            device_id: device_id.to_string(),
            point_id: point_id.to_string(),
            value_json: value_json.to_string(),
            reason: reason.to_string(),
            timestamp_ms,
        });
    }

    // Internal: load engine data
    pub(super) async fn load_engine_data(&self) -> EngineData {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        let _ = self
            .cmd_tx
            .send(ScheduleCmd::LoadEngineData { reply: reply_tx });
        reply_rx.await.unwrap_or_else(|_| EngineData {
            schedules: Vec::new(),
            assignments: Vec::new(),
            exceptions: Vec::new(),
        })
    }

    /// Find points with multiple schedule assignments (potential conflicts).
    pub async fn get_conflicts(&self) -> Vec<ScheduleConflict> {
        // Load all assignments
        let data = self.load_engine_data().await;
        let mut by_point: HashMap<(String, String), Vec<ScheduleAssignment>> = HashMap::new();
        for a in data.assignments {
            if a.enabled {
                by_point
                    .entry((a.device_id.clone(), a.point_id.clone()))
                    .or_default()
                    .push(a);
            }
        }

        by_point
            .into_iter()
            .filter(|(_, v)| v.len() > 1)
            .map(|((device_id, point_id), assignments)| ScheduleConflict {
                device_id,
                point_id,
                assignments,
            })
            .collect()
    }
}

// ----------------------------------------------------------------
// Public startup function
// ----------------------------------------------------------------

/// Start the schedule system. Returns a `ScheduleStore` handle.
pub fn start_schedule_engine(store: &PointStore) -> ScheduleStore {
    let db_dir = Path::new("data");
    if !db_dir.exists() {
        std::fs::create_dir_all(db_dir).expect("failed to create data directory");
    }
    start_schedule_engine_with_path(store, &db_dir.join("schedules.db"), None)
}

pub fn start_schedule_engine_with_path(
    store: &PointStore,
    db_path: &Path,
    shutdown: Option<tokio_util::sync::CancellationToken>,
) -> ScheduleStore {
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (config_version_tx, config_version_rx) = watch::channel(0u64);

    let sched_store = ScheduleStore {
        cmd_tx,
        config_version_tx,
        config_version_rx,
        event_bus: None,
    };

    // Start SQLite thread
    let path_clone = db_path.to_path_buf();
    std::thread::Builder::new()
        .name("schedule-sqlite".into())
        .spawn(move || run_sqlite_thread(&path_clone, cmd_rx))
        .expect("failed to spawn schedule SQLite thread");

    // Start engine task
    let engine_store = store.clone();
    let engine_sched = sched_store.clone();
    tokio::spawn(async move {
        // Small delay to let SQLite thread initialize
        tokio::time::sleep(Duration::from_millis(100)).await;
        run_schedule_engine(engine_store, engine_sched, shutdown).await;
    });

    sched_store
}
