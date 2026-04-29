use super::*;
use crate::config::profile::PointValue;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

fn temp_db_path() -> std::path::PathBuf {
    let n = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join("opencrate-schedule-test");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(format!("schedule-test-{n}.db"))
}

#[tokio::test]
async fn schedule_crud() {
    let db_path = temp_db_path();
    let store = PointStore::new();
    let sched_store = start_schedule_engine_with_path(&store, &db_path, None);

    // Create
    let weekly = template_office_hours(PointValue::Bool(true), PointValue::Bool(false));
    let id = sched_store
        .create_schedule(
            "Office Hours",
            "Standard M-F schedule",
            ScheduleValueType::Binary,
            PointValue::Bool(false),
            weekly.clone(),
        )
        .await
        .unwrap();
    assert!(id > 0);

    // List
    let schedules = sched_store.list_schedules().await;
    assert_eq!(schedules.len(), 1);
    assert_eq!(schedules[0].name, "Office Hours");
    assert_eq!(schedules[0].value_type, ScheduleValueType::Binary);

    // Get
    let sched = sched_store.get_schedule(id).await.unwrap();
    assert_eq!(sched.name, "Office Hours");

    // Update
    sched_store
        .update_schedule(
            id,
            "Updated Hours",
            "Changed name",
            PointValue::Bool(false),
            true,
            weekly,
        )
        .await
        .unwrap();
    let sched = sched_store.get_schedule(id).await.unwrap();
    assert_eq!(sched.name, "Updated Hours");

    // Delete
    sched_store.delete_schedule(id).await.unwrap();
    let schedules = sched_store.list_schedules().await;
    assert!(schedules.is_empty());

    let _ = std::fs::remove_file(&db_path);
}

#[test]
fn weekly_json_roundtrip() {
    let weekly = template_office_hours(PointValue::Bool(true), PointValue::Bool(false));
    let json = serde_json::to_string(&weekly).unwrap();
    let parsed: WeeklySchedule = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed[0].0.len(), 2); // Monday has 2 slots
    assert_eq!(parsed[5].0.len(), 0); // Saturday has 0 slots
}

#[test]
fn time_of_day_ordering() {
    let t1 = TimeOfDay::new(6, 0);
    let t2 = TimeOfDay::new(18, 0);
    let t3 = TimeOfDay::new(6, 30);
    assert!(t1 < t2);
    assert!(t1 < t3);
    assert_eq!(t1.total_minutes(), 360);
    assert_eq!(t2.total_minutes(), 1080);
}

#[test]
fn resolve_thanksgiving_2026() {
    // 4th Thursday in November 2026
    let spec = DateSpec::Relative {
        ordinal: Ordinal::Fourth,
        weekday: 3, // Thursday = 3 (0=Mon)
        month: 11,
    };
    let result = time::resolve_date_spec(&spec, 2026);
    assert_eq!(result, Some((11, 26))); // Nov 26, 2026
}

#[test]
fn resolve_memorial_day_2026() {
    // Last Monday in May 2026
    let spec = DateSpec::Relative {
        ordinal: Ordinal::Last,
        weekday: 0, // Monday = 0
        month: 5,
    };
    let result = time::resolve_date_spec(&spec, 2026);
    assert_eq!(result, Some((5, 25))); // May 25, 2026
}

#[test]
fn resolve_christmas() {
    let spec = DateSpec::Fixed { month: 12, day: 25 };
    assert_eq!(time::resolve_date_spec(&spec, 2026), Some((12, 25)));
    assert_eq!(time::resolve_date_spec(&spec, 2030), Some((12, 25)));
}

#[test]
fn fixed_year_only_matches_its_year() {
    let spec = DateSpec::FixedYear {
        year: 2026,
        month: 4,
        day: 18,
    };
    assert_eq!(time::resolve_date_spec(&spec, 2026), Some((4, 18)));
    assert_eq!(time::resolve_date_spec(&spec, 2027), None);
}

#[test]
fn evaluate_point_weekly() {
    let schedule = Schedule {
        id: 1,
        name: "Test".to_string(),
        description: String::new(),
        value_type: ScheduleValueType::Binary,
        default_value: PointValue::Bool(false),
        enabled: true,
        weekly: template_office_hours(PointValue::Bool(true), PointValue::Bool(false)),
        created_ms: 0,
        updated_ms: 0,
    };

    // Monday at 10:00 → occupied (true)
    let now = time::LocalTime {
        year: 2026,
        month: 3,
        day: 9, // Monday
        weekday: 0,
        hour: 10,
        minute: 0,
    };
    assert_eq!(
        engine::evaluate_point_value(&schedule, &[], &now),
        PointValue::Bool(true)
    );

    // Monday at 20:00 → unoccupied (false)
    let now = time::LocalTime {
        year: 2026,
        month: 3,
        day: 9,
        weekday: 0,
        hour: 20,
        minute: 0,
    };
    assert_eq!(
        engine::evaluate_point_value(&schedule, &[], &now),
        PointValue::Bool(false)
    );

    // Monday at 05:00 → before first slot → default (false)
    let now = time::LocalTime {
        year: 2026,
        month: 3,
        day: 9,
        weekday: 0,
        hour: 5,
        minute: 0,
    };
    assert_eq!(
        engine::evaluate_point_value(&schedule, &[], &now),
        PointValue::Bool(false)
    );

    // Saturday → no slots → default (false)
    let now = time::LocalTime {
        year: 2026,
        month: 3,
        day: 14,
        weekday: 5,
        hour: 10,
        minute: 0,
    };
    assert_eq!(
        engine::evaluate_point_value(&schedule, &[], &now),
        PointValue::Bool(false)
    );
}

#[test]
fn exception_overrides_weekly() {
    let schedule = Schedule {
        id: 1,
        name: "Test".to_string(),
        description: String::new(),
        value_type: ScheduleValueType::Binary,
        default_value: PointValue::Bool(false),
        enabled: true,
        weekly: template_office_hours(PointValue::Bool(true), PointValue::Bool(false)),
        created_ms: 0,
        updated_ms: 0,
    };

    // Holiday exception: Christmas = no slots (use default)
    let exception = ScheduleException {
        id: 1,
        schedule_id: 1,
        group_id: None,
        name: "Christmas".to_string(),
        date_spec: DateSpec::Fixed { month: 12, day: 25 },
        slots: DaySlots::default(),
        use_default: true,
        created_ms: 0,
    };

    // Dec 25 (Thursday) at 10:00 — would normally be occupied, but exception overrides
    let now = time::LocalTime {
        year: 2025,
        month: 12,
        day: 25,
        weekday: 3,
        hour: 10,
        minute: 0,
    };
    assert_eq!(
        engine::evaluate_point_value(&schedule, &[exception], &now),
        PointValue::Bool(false) // default
    );
}

#[tokio::test]
async fn assignment_crud() {
    let db_path = temp_db_path();
    let store = PointStore::new();
    let sched_store = start_schedule_engine_with_path(&store, &db_path, None);

    let weekly = template_office_hours(PointValue::Bool(true), PointValue::Bool(false));
    let sched_id = sched_store
        .create_schedule(
            "Office",
            "",
            ScheduleValueType::Binary,
            PointValue::Bool(false),
            weekly,
        )
        .await
        .unwrap();

    // Create assignment
    let assign_id = sched_store
        .create_assignment(sched_id, "ahu-1", "occ_mode", 12)
        .await
        .unwrap();
    assert!(assign_id > 0);

    // List for schedule
    let assigns = sched_store.list_assignments_for_schedule(sched_id).await;
    assert_eq!(assigns.len(), 1);
    assert_eq!(assigns[0].device_id, "ahu-1");

    // Get for point
    let assigns = sched_store
        .get_assignments_for_point("ahu-1", "occ_mode")
        .await;
    assert_eq!(assigns.len(), 1);

    // Delete
    sched_store.delete_assignment(assign_id).await.unwrap();
    let assigns = sched_store.list_assignments_for_schedule(sched_id).await;
    assert!(assigns.is_empty());

    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn priority_resolution() {
    let db_path = temp_db_path();
    let store = PointStore::new();
    store.set(
        PointKey {
            device_instance_id: "ahu-1".into(),
            point_id: "occ_mode".into(),
        },
        PointValue::Bool(false),
    );
    let sched_store = start_schedule_engine_with_path(&store, &db_path, None);

    // Schedule A: priority 10, value = true (always on)
    let weekly_a = template_24_7(PointValue::Bool(true));
    let id_a = sched_store
        .create_schedule(
            "Always On",
            "",
            ScheduleValueType::Binary,
            PointValue::Bool(true),
            weekly_a,
        )
        .await
        .unwrap();
    sched_store
        .create_assignment(id_a, "ahu-1", "occ_mode", 10)
        .await
        .unwrap();

    // Schedule B: priority 14, value = false (always off)
    let weekly_b = template_24_7(PointValue::Bool(false));
    let id_b = sched_store
        .create_schedule(
            "Always Off",
            "",
            ScheduleValueType::Binary,
            PointValue::Bool(false),
            weekly_b,
        )
        .await
        .unwrap();
    sched_store
        .create_assignment(id_b, "ahu-1", "occ_mode", 14)
        .await
        .unwrap();

    // Wait for engine to evaluate
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Priority 10 (Always On) should win
    let val = store.get(&PointKey {
        device_instance_id: "ahu-1".into(),
        point_id: "occ_mode".into(),
    });
    assert_eq!(val.unwrap().value, PointValue::Bool(true));

    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn engine_writes_on_startup() {
    let db_path = temp_db_path();
    let store = PointStore::new();
    store.set(
        PointKey {
            device_instance_id: "ahu-1".into(),
            point_id: "occ_mode".into(),
        },
        PointValue::Integer(0),
    );

    // Create schedule with 24/7 value
    let sched_store = start_schedule_engine_with_path(&store, &db_path, None);
    let weekly = template_24_7(PointValue::Integer(1));
    let id = sched_store
        .create_schedule(
            "24/7 Occupied",
            "",
            ScheduleValueType::Multistate,
            PointValue::Integer(0),
            weekly,
        )
        .await
        .unwrap();
    sched_store
        .create_assignment(id, "ahu-1", "occ_mode", 12)
        .await
        .unwrap();

    // Wait for engine startup recovery
    tokio::time::sleep(Duration::from_millis(500)).await;

    let val = store.get(&PointKey {
        device_instance_id: "ahu-1".into(),
        point_id: "occ_mode".into(),
    });
    assert_eq!(val.unwrap().value, PointValue::Integer(1));

    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn exception_group_crud() {
    let db_path = temp_db_path();
    let store = PointStore::new();
    let sched_store = start_schedule_engine_with_path(&store, &db_path, None);

    let entries = us_federal_holidays();
    let id = sched_store
        .create_exception_group(
            "US Federal Holidays",
            "Standard US holidays",
            entries.clone(),
        )
        .await
        .unwrap();
    assert!(id > 0);

    let groups = sched_store.list_exception_groups().await;
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].name, "US Federal Holidays");
    assert_eq!(groups[0].entries.len(), entries.len());

    sched_store.delete_exception_group(id).await.unwrap();
    let groups = sched_store.list_exception_groups().await;
    assert!(groups.is_empty());

    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn schedule_log_entries() {
    let db_path = temp_db_path();
    let store = PointStore::new();
    store.set(
        PointKey {
            device_instance_id: "ahu-1".into(),
            point_id: "occ_mode".into(),
        },
        PointValue::Bool(false),
    );
    let sched_store = start_schedule_engine_with_path(&store, &db_path, None);

    let weekly = template_24_7(PointValue::Bool(true));
    let id = sched_store
        .create_schedule(
            "Test",
            "",
            ScheduleValueType::Binary,
            PointValue::Bool(false),
            weekly,
        )
        .await
        .unwrap();
    sched_store
        .create_assignment(id, "ahu-1", "occ_mode", 12)
        .await
        .unwrap();

    // Wait for engine to write
    tokio::time::sleep(Duration::from_millis(500)).await;

    let logs = sched_store.query_log("ahu-1", "occ_mode", 10).await;
    assert!(!logs.is_empty(), "should have log entries");
    assert!(logs[0].reason.contains("schedule:"));

    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn batch_assignment_create() {
    let db_path = temp_db_path();
    let store = PointStore::new();
    let sched_store = start_schedule_engine_with_path(&store, &db_path, None);

    let weekly = template_office_hours(PointValue::Bool(true), PointValue::Bool(false));
    let sched_id = sched_store
        .create_schedule(
            "Office",
            "",
            ScheduleValueType::Binary,
            PointValue::Bool(false),
            weekly,
        )
        .await
        .unwrap();

    let entries = vec![
        ("ahu-1".to_string(), "occ_mode".to_string()),
        ("ahu-2".to_string(), "occ_mode".to_string()),
        ("vav-1".to_string(), "occ_mode".to_string()),
    ];
    let ids = sched_store
        .create_assignments_batch(sched_id, &entries, 12)
        .await
        .unwrap();
    assert_eq!(ids.len(), 3);

    let assigns = sched_store.list_assignments_for_schedule(sched_id).await;
    assert_eq!(assigns.len(), 3);

    let _ = std::fs::remove_file(&db_path);
}

#[test]
fn template_schedules_structure() {
    let on = PointValue::Bool(true);
    let off = PointValue::Bool(false);

    let office = template_office_hours(on.clone(), off.clone());
    assert_eq!(office[0].0.len(), 2); // Monday: 2 slots
    assert_eq!(office[5].0.len(), 0); // Saturday: empty

    let h24 = template_24_7(on.clone());
    assert_eq!(h24[0].0.len(), 1); // Every day: 1 slot at midnight
    assert_eq!(h24[6].0.len(), 1);

    let retail = template_retail(on.clone(), off.clone());
    assert_eq!(retail[5].0.len(), 2); // Saturday: 2 slots
    assert_eq!(retail[6].0.len(), 2); // Sunday: 2 slots (different hours)
}

#[test]
fn date_spec_serde_roundtrip() {
    let specs = vec![
        DateSpec::Fixed { month: 1, day: 1 },
        DateSpec::FixedYear {
            year: 2026,
            month: 4,
            day: 18,
        },
        DateSpec::Relative {
            ordinal: Ordinal::Fourth,
            weekday: 3,
            month: 11,
        },
    ];
    let json = serde_json::to_string(&specs).unwrap();
    let parsed: Vec<DateSpec> = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.len(), 3);
}

#[test]
fn preview_basic() {
    let schedule = Schedule {
        id: 1,
        name: "Test".to_string(),
        description: String::new(),
        value_type: ScheduleValueType::Binary,
        default_value: PointValue::Bool(false),
        enabled: true,
        weekly: template_office_hours(PointValue::Bool(true), PointValue::Bool(false)),
        created_ms: 0,
        updated_ms: 0,
    };

    // Start from Monday March 9 2026
    let preview = compute_preview(&schedule, &[], 2026, 3, 9);
    // Monday should have 3 blocks: default (00:00-05:59), on (06:00-17:59), off (18:00-23:59)
    assert_eq!(preview[0].len(), 3);
    // Saturday (index 5) should have 1 block (all default)
    assert_eq!(preview[5].len(), 1);
}
