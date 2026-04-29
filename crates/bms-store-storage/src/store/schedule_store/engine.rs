use std::collections::HashMap;
use std::time::Duration;

use crate::config::profile::PointValue;
use crate::store::point_store::{PointKey, PointStore};

use super::db::{now_ms, EngineData};
use super::store::ScheduleStore;
use super::time::{local_time_now, resolve_date_spec, LocalTime};
use super::types::*;

// ----------------------------------------------------------------
// Schedule evaluation helpers
// ----------------------------------------------------------------

/// Given a schedule's weekly slots, exceptions, and the current local time,
/// determine the active value for a point.
pub(super) fn evaluate_point_value(
    schedule: &Schedule,
    exceptions: &[ScheduleException],
    now: &LocalTime,
) -> PointValue {
    // 1. Check if any exception matches today
    let effective_slots = get_effective_slots(schedule, exceptions, now);

    // 2. Find the active slot at the current time
    resolve_value_from_slots(effective_slots, now, &schedule.default_value)
}

/// Get the effective day slots for today, considering exceptions.
fn get_effective_slots<'a>(
    schedule: &'a Schedule,
    exceptions: &'a [ScheduleException],
    now: &LocalTime,
) -> &'a DaySlots {
    // Check exceptions (later ones override earlier ones)
    for exc in exceptions.iter().rev() {
        if exc.use_default {
            // "use default" means use the schedule's default value for the whole day
            // We'll return empty slots so the default value applies
            if date_spec_matches_today(&exc.date_spec, now) {
                // Return a reference to empty slots — use_default means no slots active
                // Actually we need to check if it matches. If use_default, we still
                // want the "no slots" behavior which falls through to default_value.
                static EMPTY: DaySlots = DaySlots(Vec::new());
                return &EMPTY;
            }
        }
        if date_spec_matches_today(&exc.date_spec, now) {
            return &exc.slots;
        }
    }

    // No exception matches — use weekly schedule
    &schedule.weekly[now.weekday as usize]
}

/// Check if a DateSpec matches today.
pub(super) fn date_spec_matches_today(spec: &DateSpec, now: &LocalTime) -> bool {
    match resolve_date_spec(spec, now.year) {
        Some((m, d)) => m == now.month && d == now.day,
        None => false,
    }
}

/// Given day slots and current time, find the active value.
/// Scans slots in reverse to find the most recent transition.
fn resolve_value_from_slots(
    slots: &DaySlots,
    now: &LocalTime,
    default_value: &PointValue,
) -> PointValue {
    let now_minutes = now.hour as u16 * 60 + now.minute as u16;

    // Find the last slot whose time is <= now
    let mut best: Option<&TimeSlot> = None;
    for slot in &slots.0 {
        if slot.time.total_minutes() <= now_minutes {
            best = Some(slot);
        } else {
            break; // slots are sorted ascending
        }
    }

    match best {
        Some(slot) => slot.value.clone(),
        None => default_value.clone(),
    }
}

// ----------------------------------------------------------------
// Schedule engine
// ----------------------------------------------------------------

/// Tracks the last value written by the engine for each point.
#[derive(Debug, Clone)]
struct LastWrite {
    assignment_id: AssignmentId,
    value: PointValue,
    #[allow(dead_code)]
    priority: i32,
}

pub(super) async fn run_schedule_engine(
    store: PointStore,
    sched_store: ScheduleStore,
    shutdown: Option<tokio_util::sync::CancellationToken>,
) {
    // Load initial data
    let mut data = sched_store.load_engine_data().await;
    let mut last_writes: HashMap<(String, String), LastWrite> = HashMap::new();

    // Startup recovery: evaluate all assignments immediately
    let now = local_time_now();
    evaluate_all_assignments(&data, &now, &store, &sched_store, &mut last_writes);

    let mut minute_ticker = tokio::time::interval(Duration::from_secs(60));
    minute_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut config_rx = sched_store.subscribe_config_changes();
    let mut last_day = now.day;

    loop {
        tokio::select! {
            _ = minute_ticker.tick() => {
                let now = local_time_now();
                if now.day != last_day {
                    // Day rollover — reload data to re-resolve exceptions
                    data = sched_store.load_engine_data().await;
                    last_day = now.day;
                }
                evaluate_all_assignments(&data, &now, &store, &sched_store, &mut last_writes);
            }
            Ok(_) = config_rx.changed() => {
                // Reload from DB, re-evaluate immediately
                data = sched_store.load_engine_data().await;
                let now = local_time_now();
                evaluate_all_assignments(&data, &now, &store, &sched_store, &mut last_writes);
            }
            _ = async {
                match &shutdown {
                    Some(t) => t.cancelled().await,
                    None => std::future::pending().await,
                }
            } => {
                tracing::info!("Schedule engine shutting down");
                break;
            }
        }
    }
}

fn evaluate_all_assignments(
    data: &EngineData,
    now: &LocalTime,
    store: &PointStore,
    sched_store: &ScheduleStore,
    last_writes: &mut HashMap<(String, String), LastWrite>,
) {
    // Group assignments by point
    let mut point_assignments: HashMap<(String, String), Vec<(&ScheduleAssignment, &Schedule)>> =
        HashMap::new();

    for assignment in &data.assignments {
        if !assignment.enabled {
            continue;
        }
        // Find the schedule
        if let Some(schedule) = data
            .schedules
            .iter()
            .find(|s| s.id == assignment.schedule_id)
        {
            if !schedule.enabled {
                continue;
            }
            let key = (assignment.device_id.clone(), assignment.point_id.clone());
            point_assignments
                .entry(key)
                .or_default()
                .push((assignment, schedule));
        }
    }

    for ((device_id, point_id), mut assignments) in point_assignments {
        // Sort by priority (lowest number = highest precedence)
        assignments.sort_by_key(|(a, _)| a.priority);

        // Evaluate the highest-priority assignment
        if let Some((assignment, schedule)) = assignments.first() {
            // Get exceptions for this schedule
            let exceptions: Vec<&ScheduleException> = data
                .exceptions
                .iter()
                .filter(|e| e.schedule_id == schedule.id)
                .collect();

            let exc_refs: Vec<ScheduleException> = exceptions.into_iter().cloned().collect();

            let value = evaluate_point_value(schedule, &exc_refs, now);

            let point_key = (device_id.clone(), point_id.clone());

            // Only write if value changed from last write
            let should_write = match last_writes.get(&point_key) {
                Some(lw) => lw.value != value || lw.assignment_id != assignment.id,
                None => true,
            };

            if should_write {
                let pk = PointKey {
                    device_instance_id: device_id.clone(),
                    point_id: point_id.clone(),
                };
                store.set(pk, value.clone());

                let value_json =
                    serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
                let reason = format!("schedule:{}", schedule.name);
                sched_store.insert_log(
                    assignment.id,
                    &device_id,
                    &point_id,
                    &value_json,
                    &reason,
                    now_ms(),
                );

                last_writes.insert(
                    point_key,
                    LastWrite {
                        assignment_id: assignment.id,
                        value,
                        priority: assignment.priority,
                    },
                );
            }
        }
    }
}
