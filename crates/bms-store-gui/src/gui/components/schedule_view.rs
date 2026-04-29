use std::collections::HashSet;

use dioxus::prelude::*;

use bms_store_storage::auth::Permission;
use bms_store_bridges::bridge::bacnet::BacnetNetworks;
use bms_store_storage::config::profile::PointKind;
use bms_store_storage::config::profile::PointValue;
use crate::gui::state::AppState;
use bms_store_storage::store::schedule_store::{
    compute_preview, empty_weekly, template_24_7, template_extended_hours, template_office_hours,
    template_retail, template_school, template_warehouse, uk_bank_holidays, us_federal_holidays,
    DateSpec, DaySlots, ExceptionGroup, Ordinal, Schedule, ScheduleAssignment, ScheduleConflict,
    ScheduleException, ScheduleId, ScheduleLogEntry, ScheduleValueType, TimeOfDay, TimeSlot,
};
use rustbac_client::schedule::{CalendarEntry, TimeValue};

// ----------------------------------------------------------------
// Tab state
// ----------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum ScheduleTab {
    Weekly,
    Exceptions,
    Assignments,
    Log,
    Preview,
}

// ----------------------------------------------------------------
// ScheduleView — 3-pane layout (browser | tabs | properties)
// ----------------------------------------------------------------

#[component]
pub fn ScheduleView() -> Element {
    let mut tab = use_signal(|| ScheduleTab::Weekly);
    let selected_schedule: Signal<Option<ScheduleId>> = use_signal(|| None);
    let refresh_counter = use_signal(|| 0u64);
    let current_tab = *tab.read();
    let sel_id = *selected_schedule.read();

    rsx! {
        ScheduleBrowser {
            selected_schedule,
            refresh_counter,
        }
        div { class: "main-content",
            if sel_id.is_some() {
                div { class: "schedule-tabs",
                    button {
                        class: if current_tab == ScheduleTab::Weekly { "schedule-tab active" } else { "schedule-tab" },
                        onclick: move |_| tab.set(ScheduleTab::Weekly),
                        "Weekly"
                    }
                    button {
                        class: if current_tab == ScheduleTab::Exceptions { "schedule-tab active" } else { "schedule-tab" },
                        onclick: move |_| tab.set(ScheduleTab::Exceptions),
                        "Exceptions"
                    }
                    button {
                        class: if current_tab == ScheduleTab::Assignments { "schedule-tab active" } else { "schedule-tab" },
                        onclick: move |_| tab.set(ScheduleTab::Assignments),
                        "Assignments"
                    }
                    button {
                        class: if current_tab == ScheduleTab::Log { "schedule-tab active" } else { "schedule-tab" },
                        onclick: move |_| tab.set(ScheduleTab::Log),
                        "Log"
                    }
                    button {
                        class: if current_tab == ScheduleTab::Preview { "schedule-tab active" } else { "schedule-tab" },
                        onclick: move |_| tab.set(ScheduleTab::Preview),
                        "Preview"
                    }
                }
                div { class: "schedule-tab-content",
                    match current_tab {
                        ScheduleTab::Weekly => rsx! { WeeklyTab { schedule_id: sel_id.unwrap(), refresh_counter } },
                        ScheduleTab::Exceptions => rsx! { ExceptionsTab { schedule_id: sel_id.unwrap(), refresh_counter } },
                        ScheduleTab::Assignments => rsx! { AssignmentsTab { schedule_id: sel_id.unwrap(), refresh_counter } },
                        ScheduleTab::Log => rsx! { LogTab { schedule_id: sel_id.unwrap() } },
                        ScheduleTab::Preview => rsx! { PreviewTab { schedule_id: sel_id.unwrap() } },
                    }
                }
            } else {
                div { class: "schedule-empty",
                    h3 { "Schedules" }
                    p { "Select a schedule from the browser or create a new one." }
                }
            }
        }
        if sel_id.is_some() {
            SchedulePropertiesPanel {
                schedule_id: sel_id.unwrap(),
                selected_schedule,
                refresh_counter,
            }
        }
    }
}

// ----------------------------------------------------------------
// Left pane: Schedule browser
// ----------------------------------------------------------------

#[component]
fn ScheduleBrowser(
    selected_schedule: Signal<Option<ScheduleId>>,
    refresh_counter: Signal<u64>,
) -> Element {
    let state = use_context::<AppState>();
    let user_can_write = state.has_permission(Permission::ManageSchedules);
    let _refresh = *refresh_counter.read();
    let mut schedules = use_signal(Vec::<Schedule>::new);
    let mut conflicts = use_signal(Vec::<ScheduleConflict>::new);
    let mut show_new = use_signal(|| false);
    let mut new_name = use_signal(String::new);
    let mut new_type = use_signal(|| "binary".to_string());
    let mut new_template = use_signal(|| "none".to_string());
    let mut status = use_signal(|| Option::<String>::None);

    let sched_store = state.schedule_store.clone();
    let _ = use_resource(move || {
        let ss = sched_store.clone();
        let _r = *refresh_counter.read();
        async move {
            let s = ss.list_schedules().await;
            let c = ss.get_conflicts().await;
            schedules.set(s);
            conflicts.set(c);
        }
    });

    let sel_id = *selected_schedule.read();

    rsx! {
        div { class: "sidebar dash-device-browser",
            div { class: "details-header",
                span { "Schedules" }
                if !conflicts.read().is_empty() {
                    span {
                        class: "schedule-conflict-badge",
                        title: "Points with multiple schedules",
                        "{conflicts.read().len()}"
                    }
                }
            }
            div { class: "sidebar-content",
                for sched in schedules.read().iter() {
                    {
                        let id = sched.id;
                        let is_selected = sel_id == Some(id);
                        let name = sched.name.clone();
                        let vtype = sched.value_type.label().to_string();
                        let enabled = sched.enabled;
                        rsx! {
                            div {
                                class: if is_selected { "schedule-list-item selected" } else { "schedule-list-item" },
                                onclick: move |_| selected_schedule.set(Some(id)),
                                div { class: "schedule-list-name",
                                    if !enabled {
                                        span { class: "schedule-disabled-icon", title: "Disabled", "||" }
                                    }
                                    "{name}"
                                }
                                span { class: "schedule-list-badge", "{vtype}" }
                            }
                        }
                    }
                }
                if schedules.read().is_empty() {
                    div { class: "schedule-empty-list", "No schedules yet" }
                }
            }

            // New schedule form
            div { class: "schedule-browser-footer",
                if *show_new.read() {
                    div { class: "schedule-new-form",
                        input {
                            class: "sidebar-search-input",
                            r#type: "text",
                            placeholder: "Schedule name...",
                            value: "{new_name.read()}",
                            oninput: move |evt| new_name.set(evt.value()),
                        }
                        div { class: "schedule-new-row",
                            select {
                                class: "schedule-select",
                                value: "{new_type.read()}",
                                onchange: move |evt| new_type.set(evt.value()),
                                option { value: "binary", "Binary" }
                                option { value: "analog", "Analog" }
                                option { value: "multistate", "Multistate" }
                            }
                            select {
                                class: "schedule-select",
                                value: "{new_template.read()}",
                                onchange: move |evt| new_template.set(evt.value()),
                                option { value: "none", "Blank" }
                                option { value: "office", "Office Hours" }
                                option { value: "extended", "Extended Hours" }
                                option { value: "24_7", "24/7" }
                                option { value: "retail", "Retail" }
                                option { value: "school", "School" }
                                option { value: "warehouse", "Warehouse" }
                            }
                        }
                        div { class: "schedule-new-row",
                            button {
                                class: "alarm-save-btn",
                                onclick: {
                                    let ss = state.schedule_store.clone();
                                    let audit_state = state.clone();
                                    move |_| {
                                        let name = new_name.read().trim().to_string();
                                        if name.is_empty() {
                                            status.set(Some("Name required".into()));
                                            return;
                                        }
                                        let vtype_str = new_type.read().clone();
                                        let vtype = ScheduleValueType::from_str(&vtype_str)
                                            .unwrap_or(ScheduleValueType::Binary);
                                        let (default_val, on_val, off_val) = match vtype {
                                            ScheduleValueType::Binary => (
                                                PointValue::Bool(false),
                                                PointValue::Bool(true),
                                                PointValue::Bool(false),
                                            ),
                                            ScheduleValueType::Analog => (
                                                PointValue::Float(0.0),
                                                PointValue::Float(72.0),
                                                PointValue::Float(55.0),
                                            ),
                                            ScheduleValueType::Multistate => (
                                                PointValue::Integer(0),
                                                PointValue::Integer(1),
                                                PointValue::Integer(0),
                                            ),
                                        };
                                        let tmpl = new_template.read().clone();
                                        let weekly = match tmpl.as_str() {
                                            "office" => template_office_hours(on_val, off_val),
                                            "extended" => template_extended_hours(on_val, off_val),
                                            "24_7" => template_24_7(on_val),
                                            "retail" => template_retail(on_val, off_val),
                                            "school" => template_school(on_val, off_val),
                                            "warehouse" => template_warehouse(on_val, off_val),
                                            _ => empty_weekly(),
                                        };
                                        let ss = ss.clone();
                                        let dv = default_val.clone();
                                        let audit_state = audit_state.clone();
                                        spawn(async move {
                                            match ss.create_schedule(&name, "", vtype, dv, weekly).await {
                                                Ok(id) => {
                                                    audit_state.audit(
                                                        bms_store_storage::store::audit_store::AuditEntryBuilder::new(
                                                            bms_store_storage::store::audit_store::AuditAction::CreateSchedule, "schedule",
                                                        ).resource_id(&format!("{id}")).details(&name),
                                                    );
                                                    selected_schedule.set(Some(id));
                                                    show_new.set(false);
                                                    new_name.set(String::new());
                                                    new_template.set("none".into());
                                                    status.set(None);
                                                    { let v = *refresh_counter.peek(); refresh_counter.set(v + 1); }
                                                }
                                                Err(e) => status.set(Some(format!("{e}"))),
                                            }
                                        });
                                    }
                                },
                                "Create"
                            }
                            button {
                                class: "alarm-cancel-btn",
                                onclick: move |_| {
                                    show_new.set(false);
                                    status.set(None);
                                },
                                "Cancel"
                            }
                        }
                        if let Some(ref msg) = *status.read() {
                            div { class: "alarm-form-error", "{msg}" }
                        }
                    }
                } else {
                    if user_can_write {
                        button {
                            class: "alarm-add-btn",
                            style: "width: 100%;",
                            onclick: move |_| show_new.set(true),
                            "+ New Schedule"
                        }
                    }
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Center: Weekly tab
// ----------------------------------------------------------------

const DAY_NAMES: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

#[component]
fn WeeklyTab(schedule_id: ScheduleId, refresh_counter: Signal<u64>) -> Element {
    let state = use_context::<AppState>();
    let mut schedule = use_signal(|| Option::<Schedule>::None);

    let ss = state.schedule_store.clone();
    let _ = use_resource(move || {
        let ss = ss.clone();
        let _r = *refresh_counter.read();
        async move {
            schedule.set(ss.get_schedule(schedule_id).await);
        }
    });

    let Some(ref sched) = *schedule.read() else {
        return rsx! { div { class: "schedule-loading", "Loading..." } };
    };

    let weekly = sched.weekly.clone();
    let _sched_name = sched.name.clone();

    rsx! {
        BacnetScheduleSync { schedule_id, refresh_counter }
        div { class: "schedule-weekly",
            div { class: "schedule-weekly-header",
                h3 { "Weekly Schedule" }
                button {
                    class: "schedule-copy-btn",
                    title: "Copy Monday to all weekdays",
                    onclick: {
                        let ss = state.schedule_store.clone();
                        let sched = sched.clone();
                        move |_| {
                            let ss = ss.clone();
                            let mut s = sched.clone();
                            let monday_slots = s.weekly[0].clone();
                            for day in 1..5 {
                                s.weekly[day] = monday_slots.clone();
                            }
                            spawn(async move {
                                let _ = ss.update_schedule(
                                    s.id, &s.name, &s.description,
                                    s.default_value, s.enabled, s.weekly,
                                ).await;
                                { let v = *refresh_counter.peek(); refresh_counter.set(v + 1); }
                            });
                        }
                    },
                    "Copy Mon to Weekdays"
                }
            }
            for (day_idx, day_name) in DAY_NAMES.iter().enumerate() {
                WeeklyDayRow {
                    schedule_id,
                    day_idx: day_idx as u8,
                    day_name: day_name.to_string(),
                    slots: weekly[day_idx].clone(),
                    refresh_counter,
                }
            }
        }
    }
}

#[component]
fn WeeklyDayRow(
    schedule_id: ScheduleId,
    day_idx: u8,
    day_name: String,
    slots: DaySlots,
    refresh_counter: Signal<u64>,
) -> Element {
    let state = use_context::<AppState>();
    let mut adding = use_signal(|| false);
    let mut new_time = use_signal(|| "08:00".to_string());
    let mut new_value = use_signal(|| "true".to_string());

    rsx! {
        div { class: "schedule-day-row",
            div { class: "schedule-day-name", "{day_name}" }
            div { class: "schedule-day-slots",
                for (slot_idx, slot) in slots.0.iter().enumerate() {
                    {
                        let time_str = format!("{:02}:{:02}", slot.time.hour, slot.time.minute);
                        let val_str = format_point_value(&slot.value);
                        rsx! {
                            div { class: "schedule-slot",
                                span { class: "schedule-slot-time", "{time_str}" }
                                span { class: "schedule-slot-value", "{val_str}" }
                                button {
                                    class: "schedule-slot-delete",
                                    title: "Remove slot",
                                    onclick: {
                                        let ss = state.schedule_store.clone();
                                        move |_| {
                                            let ss = ss.clone();
                                            let si = slot_idx;
                                            let di = day_idx as usize;
                                            let sid = schedule_id;
                                            spawn(async move {
                                                if let Some(mut sched) = ss.get_schedule(sid).await {
                                                    sched.weekly[di].0.remove(si);
                                                    let _ = ss.update_schedule(
                                                        sched.id, &sched.name, &sched.description,
                                                        sched.default_value, sched.enabled, sched.weekly,
                                                    ).await;
                                                    { let v = *refresh_counter.peek(); refresh_counter.set(v + 1); }
                                                }
                                            });
                                        }
                                    },
                                    "x"
                                }
                            }
                        }
                    }
                }
                if slots.0.is_empty() {
                    span { class: "schedule-no-slots", "No slots (default value)" }
                }
                if *adding.read() {
                    div { class: "schedule-slot-add-form",
                        input {
                            class: "schedule-time-input",
                            r#type: "time",
                            value: "{new_time.read()}",
                            oninput: move |evt| new_time.set(evt.value()),
                        }
                        input {
                            class: "schedule-value-input",
                            r#type: "text",
                            placeholder: "value",
                            value: "{new_value.read()}",
                            oninput: move |evt| new_value.set(evt.value()),
                        }
                        button {
                            class: "alarm-save-btn",
                            onclick: {
                                let ss = state.schedule_store.clone();
                                move |_| {
                                    let time_str = new_time.read().clone();
                                    let val_str = new_value.read().clone();
                                    let parts: Vec<&str> = time_str.split(':').collect();
                                    if parts.len() != 2 { return; }
                                    let hour: u8 = parts[0].parse().unwrap_or(0);
                                    let minute: u8 = parts[1].parse().unwrap_or(0);
                                    let value = parse_point_value(&val_str);
                                    let ss = ss.clone();
                                    let di = day_idx as usize;
                                    let sid = schedule_id;
                                    spawn(async move {
                                        if let Some(mut sched) = ss.get_schedule(sid).await {
                                            sched.weekly[di].0.push(TimeSlot {
                                                time: TimeOfDay::new(hour, minute),
                                                value,
                                            });
                                            sched.weekly[di].sort();
                                            let _ = ss.update_schedule(
                                                sched.id, &sched.name, &sched.description,
                                                sched.default_value, sched.enabled, sched.weekly,
                                            ).await;
                                            { let v = *refresh_counter.peek(); refresh_counter.set(v + 1); }
                                        }
                                    });
                                    adding.set(false);
                                }
                            },
                            "Add"
                        }
                        button {
                            class: "alarm-cancel-btn",
                            onclick: move |_| adding.set(false),
                            "Cancel"
                        }
                    }
                } else {
                    button {
                        class: "schedule-add-slot-btn",
                        onclick: move |_| adding.set(true),
                        "+"
                    }
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Center: Exceptions tab
// ----------------------------------------------------------------

#[component]
fn ExceptionsTab(schedule_id: ScheduleId, refresh_counter: Signal<u64>) -> Element {
    let state = use_context::<AppState>();
    let mut exceptions = use_signal(Vec::<ScheduleException>::new);
    let mut groups = use_signal(Vec::<ExceptionGroup>::new);
    let mut show_add = use_signal(|| false);
    let mut new_name = use_signal(|| "Holiday".to_string());
    let mut new_month = use_signal(|| "1".to_string());
    let mut new_day = use_signal(|| "1".to_string());
    let mut new_use_default = use_signal(|| true);
    let mut show_import = use_signal(|| false);

    let ss = state.schedule_store.clone();
    let _ = use_resource(move || {
        let ss = ss.clone();
        let _r = *refresh_counter.read();
        async move {
            exceptions.set(ss.list_exceptions(schedule_id).await);
            groups.set(ss.list_exception_groups().await);
        }
    });

    rsx! {
        div { class: "schedule-exceptions",
            div { class: "schedule-section-header",
                h3 { "Exceptions" }
                div { class: "schedule-header-actions",
                    button {
                        class: "alarm-add-btn",
                        onclick: move |_| show_import.set(true),
                        "Import Holidays"
                    }
                    button {
                        class: "alarm-add-btn",
                        onclick: move |_| show_add.set(true),
                        "+ Add Exception"
                    }
                }
            }

            if *show_import.read() {
                div { class: "schedule-import-form",
                    button {
                        class: "alarm-save-btn",
                        onclick: {
                            let ss = state.schedule_store.clone();
                            move |_| {
                                let ss = ss.clone();
                                let sid = schedule_id;
                                spawn(async move {
                                    let entries = us_federal_holidays();
                                    let gid = ss.create_exception_group(
                                        "US Federal Holidays", "Standard US federal holidays", entries.clone()
                                    ).await.ok();
                                    for entry in &entries {
                                        let name = describe_date_spec(entry);
                                        let _ = ss.add_exception(
                                            sid, gid, &name, entry.clone(),
                                            DaySlots::default(), true,
                                        ).await;
                                    }
                                    { let v = *refresh_counter.peek(); refresh_counter.set(v + 1); }
                                    show_import.set(false);
                                });
                            }
                        },
                        "US Federal"
                    }
                    button {
                        class: "alarm-save-btn",
                        onclick: {
                            let ss = state.schedule_store.clone();
                            move |_| {
                                let ss = ss.clone();
                                let sid = schedule_id;
                                spawn(async move {
                                    let entries = uk_bank_holidays();
                                    let gid = ss.create_exception_group(
                                        "UK Bank Holidays", "UK bank holidays", entries.clone()
                                    ).await.ok();
                                    for entry in &entries {
                                        let name = describe_date_spec(entry);
                                        let _ = ss.add_exception(
                                            sid, gid, &name, entry.clone(),
                                            DaySlots::default(), true,
                                        ).await;
                                    }
                                    { let v = *refresh_counter.peek(); refresh_counter.set(v + 1); }
                                    show_import.set(false);
                                });
                            }
                        },
                        "UK Bank"
                    }
                    button {
                        class: "alarm-cancel-btn",
                        onclick: move |_| show_import.set(false),
                        "Cancel"
                    }
                }
            }

            if *show_add.read() {
                div { class: "schedule-exception-add",
                    div { class: "alarm-form-row",
                        label { "Name" }
                        input {
                            class: "schedule-value-input",
                            r#type: "text",
                            value: "{new_name.read()}",
                            oninput: move |evt| new_name.set(evt.value()),
                        }
                    }
                    div { class: "alarm-form-row",
                        label { "Month" }
                        input {
                            class: "schedule-value-input",
                            r#type: "number",
                            min: "1", max: "12",
                            value: "{new_month.read()}",
                            oninput: move |evt| new_month.set(evt.value()),
                        }
                    }
                    div { class: "alarm-form-row",
                        label { "Day" }
                        input {
                            class: "schedule-value-input",
                            r#type: "number",
                            min: "1", max: "31",
                            value: "{new_day.read()}",
                            oninput: move |evt| new_day.set(evt.value()),
                        }
                    }
                    div { class: "alarm-form-row",
                        label { "Use Default" }
                        input {
                            class: "schedule-checkbox",
                            r#type: "checkbox",
                            checked: *new_use_default.read(),
                            onchange: move |evt| new_use_default.set(evt.checked()),
                        }
                    }
                    div { class: "schedule-new-row",
                        button {
                            class: "alarm-save-btn",
                            onclick: {
                                let ss = state.schedule_store.clone();
                                move |_| {
                                    let name = new_name.read().clone();
                                    let month: u8 = new_month.read().parse().unwrap_or(1);
                                    let day: u8 = new_day.read().parse().unwrap_or(1);
                                    let use_def = *new_use_default.read();
                                    let ss = ss.clone();
                                    let sid = schedule_id;
                                    spawn(async move {
                                        let _ = ss.add_exception(
                                            sid, None, &name,
                                            DateSpec::Fixed { month, day },
                                            DaySlots::default(), use_def,
                                        ).await;
                                        { let v = *refresh_counter.peek(); refresh_counter.set(v + 1); }
                                        show_add.set(false);
                                    });
                                }
                            },
                            "Save"
                        }
                        button {
                            class: "alarm-cancel-btn",
                            onclick: move |_| show_add.set(false),
                            "Cancel"
                        }
                    }
                }
            }

            table { class: "alarm-table",
                thead {
                    tr {
                        th { "Name" }
                        th { "Date" }
                        th { "Slots" }
                        th { "Default" }
                        th { "" }
                    }
                }
                tbody {
                    for exc in exceptions.read().iter() {
                        {
                            let eid = exc.id;
                            let ename = exc.name.clone();
                            let date_desc = describe_date_spec(&exc.date_spec);
                            let slot_count = exc.slots.0.len();
                            let use_def = if exc.use_default { "Yes" } else { "No" };
                            rsx! {
                                tr { class: "alarm-row",
                                    td { "{ename}" }
                                    td { "{date_desc}" }
                                    td { "{slot_count}" }
                                    td { "{use_def}" }
                                    td {
                                        button {
                                            class: "alarm-delete-btn",
                                            onclick: {
                                                let ss = state.schedule_store.clone();
                                                move |_| {
                                                    let ss = ss.clone();
                                                    spawn(async move {
                                                        let _ = ss.remove_exception(eid).await;
                                                        { let v = *refresh_counter.peek(); refresh_counter.set(v + 1); }
                                                    });
                                                }
                                            },
                                            "Del"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if exceptions.read().is_empty() {
                div { class: "schedule-empty-list", "No exceptions defined" }
            }
        }
    }
}

// ----------------------------------------------------------------
// Center: Assignments tab
// ----------------------------------------------------------------

#[component]
fn AssignmentsTab(schedule_id: ScheduleId, refresh_counter: Signal<u64>) -> Element {
    let state = use_context::<AppState>();
    let mut assignments = use_signal(Vec::<ScheduleAssignment>::new);
    let mut show_browser = use_signal(|| false);
    let mut selected_points: Signal<HashSet<(String, String)>> = use_signal(HashSet::new);
    let search = use_signal(String::new);
    let mut priority_str = use_signal(|| "12".to_string());

    let ss = state.schedule_store.clone();
    let _ = use_resource(move || {
        let ss = ss.clone();
        let _r = *refresh_counter.read();
        async move {
            assignments.set(ss.list_assignments_for_schedule(schedule_id).await);
        }
    });

    rsx! {
        div { class: "schedule-assignments",
            div { class: "schedule-section-header",
                h3 { "Assignments" }
                button {
                    class: "alarm-add-btn",
                    onclick: move |_| { let v = *show_browser.peek(); show_browser.set(!v); },
                    if *show_browser.read() { "Close Browser" } else { "Assign Points" }
                }
            }

            if *show_browser.read() {
                div { class: "schedule-assign-browser",
                    div { class: "schedule-assign-controls",
                        label { "Priority: " }
                        input {
                            class: "schedule-value-input",
                            r#type: "number",
                            min: "1", max: "16",
                            value: "{priority_str.read()}",
                            oninput: move |evt| priority_str.set(evt.value()),
                            style: "width: 50px;",
                        }
                        span { class: "schedule-selected-count",
                            "{selected_points.read().len()} points selected"
                        }
                        button {
                            class: "alarm-save-btn",
                            onclick: {
                                let ss = state.schedule_store.clone();
                                move |_| {
                                    let pts: Vec<(String, String)> = selected_points.read().iter().cloned().collect();
                                    if pts.is_empty() { return; }
                                    let priority: i32 = priority_str.read().parse().unwrap_or(12);
                                    let ss = ss.clone();
                                    let sid = schedule_id;
                                    spawn(async move {
                                        let _ = ss.create_assignments_batch(sid, &pts, priority).await;
                                        selected_points.set(HashSet::new());
                                        { let v = *refresh_counter.peek(); refresh_counter.set(v + 1); }
                                    });
                                }
                            },
                            "Assign Selected"
                        }
                    }
                    AssignPointBrowser { selected_points, search }
                }
            }

            table { class: "alarm-table",
                thead {
                    tr {
                        th { "Device" }
                        th { "Point" }
                        th { "Priority" }
                        th { "" }
                    }
                }
                tbody {
                    for a in assignments.read().iter() {
                        {
                            let aid = a.id;
                            let dev = a.device_id.clone();
                            let pt = a.point_id.clone();
                            let pri = a.priority;
                            rsx! {
                                tr { class: "alarm-row",
                                    td { "{dev}" }
                                    td { "{pt}" }
                                    td { "{pri}" }
                                    td {
                                        button {
                                            class: "alarm-delete-btn",
                                            onclick: {
                                                let ss = state.schedule_store.clone();
                                                move |_| {
                                                    let ss = ss.clone();
                                                    spawn(async move {
                                                        let _ = ss.delete_assignment(aid).await;
                                                        { let v = *refresh_counter.peek(); refresh_counter.set(v + 1); }
                                                    });
                                                }
                                            },
                                            "Del"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if assignments.read().is_empty() {
                div { class: "schedule-empty-list", "No assignments. Assign points to activate this schedule." }
            }
        }
    }
}

/// Point browser for bulk assignment (reuses alarm browser pattern).
#[component]
fn AssignPointBrowser(
    selected_points: Signal<HashSet<(String, String)>>,
    search: Signal<String>,
) -> Element {
    let state = use_context::<AppState>();
    let query = search.read().clone();

    rsx! {
        div { class: "schedule-point-browser",
            div { class: "sidebar-search",
                input {
                    class: "sidebar-search-input",
                    r#type: "text",
                    placeholder: "Search...",
                    value: "{query}",
                    oninput: move |evt| search.set(evt.value()),
                }
            }
            div { class: "alarm-browser-actions",
                button {
                    class: "alarm-browser-action-btn",
                    onclick: {
                        let devices = state.loaded.devices.clone();
                        let q = query.clone();
                        move |_| {
                            selected_points.set(collect_points(&devices, &q, None));
                        }
                    },
                    "All"
                }
                button {
                    class: "alarm-browser-action-btn",
                    onclick: {
                        let devices = state.loaded.devices.clone();
                        let q = query.clone();
                        move |_| {
                            selected_points.set(collect_points(&devices, &q, Some(PointKind::Binary)));
                        }
                    },
                    "Binary"
                }
                button {
                    class: "alarm-browser-action-btn",
                    onclick: move |_| selected_points.set(HashSet::new()),
                    "Clear"
                }
            }
            div { class: "schedule-point-list",
                for dev in state.loaded.devices.iter() {
                    {
                        let dev_id = dev.instance_id.clone();
                        let q_lower = query.to_lowercase();
                        let visible: Vec<_> = dev.profile.points.iter()
                            .filter(|p| {
                                query.is_empty()
                                    || p.id.to_lowercase().contains(&q_lower)
                                    || p.name.to_lowercase().contains(&q_lower)
                                    || dev_id.to_lowercase().contains(&q_lower)
                            })
                            .collect();
                        if visible.is_empty() {
                            rsx! {}
                        } else {
                            rsx! {
                                div { class: "dash-device-node",
                                    div { class: "tree-node-row",
                                        span { class: "tree-label", "{dev_id}" }
                                    }
                                    div { class: "dash-point-list",
                                        for pt in visible {
                                            {
                                                let key = (dev_id.clone(), pt.id.clone());
                                                let is_checked = selected_points.read().contains(&key);
                                                let pt_id = pt.id.clone();
                                                let pt_name = pt.name.clone();
                                                let did = dev_id.clone();
                                                rsx! {
                                                    div { class: "alarm-browser-point",
                                                        input {
                                                            r#type: "checkbox",
                                                            checked: is_checked,
                                                            onchange: move |_| {
                                                                let k = (did.clone(), pt_id.clone());
                                                                let mut pts = selected_points.read().clone();
                                                                if pts.contains(&k) {
                                                                    pts.remove(&k);
                                                                } else {
                                                                    pts.insert(k);
                                                                }
                                                                selected_points.set(pts);
                                                            },
                                                        }
                                                        span { "{pt_name}" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn collect_points(
    devices: &[bms_store_storage::config::loader::LoadedDevice],
    query: &str,
    kind_filter: Option<PointKind>,
) -> HashSet<(String, String)> {
    let mut set = HashSet::new();
    let q = query.to_lowercase();
    for dev in devices {
        for pt in &dev.profile.points {
            if let Some(ref kind) = kind_filter {
                if pt.kind != *kind {
                    continue;
                }
            }
            if !query.is_empty()
                && !pt.id.to_lowercase().contains(&q)
                && !pt.name.to_lowercase().contains(&q)
                && !dev.instance_id.to_lowercase().contains(&q)
            {
                continue;
            }
            set.insert((dev.instance_id.clone(), pt.id.clone()));
        }
    }
    set
}

// ----------------------------------------------------------------
// Center: Log tab
// ----------------------------------------------------------------

#[component]
fn LogTab(schedule_id: ScheduleId) -> Element {
    let state = use_context::<AppState>();
    let mut logs = use_signal(Vec::<ScheduleLogEntry>::new);
    let mut assignments = use_signal(Vec::<ScheduleAssignment>::new);

    let ss = state.schedule_store.clone();
    let _ = use_resource(move || {
        let ss = ss.clone();
        async move {
            let assigns = ss.list_assignments_for_schedule(schedule_id).await;
            let mut all_logs = Vec::new();
            for a in &assigns {
                let mut l = ss.query_log(&a.device_id, &a.point_id, 50).await;
                all_logs.append(&mut l);
            }
            all_logs.sort_by(|a, b| b.timestamp_ms.cmp(&a.timestamp_ms));
            all_logs.truncate(100);
            logs.set(all_logs);
            assignments.set(assigns);
        }
    });

    rsx! {
        div { class: "schedule-log",
            h3 { "Schedule Log" }
            table { class: "alarm-table",
                thead {
                    tr {
                        th { "Time" }
                        th { "Device" }
                        th { "Point" }
                        th { "Value" }
                        th { "Reason" }
                    }
                }
                tbody {
                    for entry in logs.read().iter() {
                        {
                            let time_str = format_time_ms(entry.timestamp_ms);
                            let dev = entry.device_id.clone();
                            let pt = entry.point_id.clone();
                            let val = entry.value_json.clone();
                            let reason = entry.reason.clone();
                            rsx! {
                                tr { class: "alarm-row",
                                    td { "{time_str}" }
                                    td { "{dev}" }
                                    td { "{pt}" }
                                    td { "{val}" }
                                    td { "{reason}" }
                                }
                            }
                        }
                    }
                }
            }
            if logs.read().is_empty() {
                div { class: "schedule-empty-list", "No log entries yet" }
            }
        }
    }
}

// ----------------------------------------------------------------
// Center: Preview tab (Phase 3a)
// ----------------------------------------------------------------

#[component]
fn PreviewTab(schedule_id: ScheduleId) -> Element {
    let state = use_context::<AppState>();
    let mut schedule = use_signal(|| Option::<Schedule>::None);
    let mut exceptions = use_signal(Vec::<ScheduleException>::new);

    let ss = state.schedule_store.clone();
    let _ = use_resource(move || {
        let ss = ss.clone();
        async move {
            schedule.set(ss.get_schedule(schedule_id).await);
            exceptions.set(ss.list_exceptions(schedule_id).await);
        }
    });

    let Some(ref sched) = *schedule.read() else {
        return rsx! { div { class: "schedule-loading", "Loading..." } };
    };

    // Get today's date for preview start
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    #[repr(C)]
    #[derive(Default)]
    struct TmPreview {
        tm_sec: i32,
        tm_min: i32,
        tm_hour: i32,
        tm_mday: i32,
        tm_mon: i32,
        tm_year: i32,
        tm_wday: i32,
        tm_yday: i32,
        tm_isdst: i32,
        tm_gmtoff: i64,
        tm_zone: *const i8,
    }
    extern "C" {
        fn localtime_r(time: *const i64, result: *mut TmPreview) -> *mut TmPreview;
    }
    let mut tm = TmPreview::default();
    unsafe { localtime_r(&now, &mut tm) };
    let year = tm.tm_year + 1900;
    let month = (tm.tm_mon + 1) as u8;
    let day = tm.tm_mday as u8;

    let exc_list = exceptions.read().clone();
    let preview = compute_preview(sched, &exc_list, year, month, day);

    rsx! {
        div { class: "schedule-preview",
            h3 { "7-Day Preview" }
            for (day_offset, blocks) in preview.iter().enumerate() {
                {
                    let day_label = if day_offset == 0 {
                        "Today".to_string()
                    } else {
                        format!("+{day_offset}d")
                    };
                    rsx! {
                        div { class: "schedule-preview-day",
                            div { class: "schedule-preview-label", "{day_label}" }
                            div { class: "schedule-preview-timeline",
                                for block in blocks {
                                    {
                                        let start_min = block.start.total_minutes() as f64;
                                        let end_min = block.end.total_minutes() as f64 + 1.0;
                                        let left_pct = start_min / 1440.0 * 100.0;
                                        let width_pct = (end_min - start_min) / 1440.0 * 100.0;
                                        let is_on = match &block.value {
                                            PointValue::Bool(b) => *b,
                                            PointValue::Float(f) => *f > 0.0,
                                            PointValue::Integer(i) => *i > 0,
                                        };
                                        let class = if is_on { "schedule-preview-block on" } else { "schedule-preview-block off" };
                                        let title = format!(
                                            "{:02}:{:02}-{:02}:{:02} {} ({})",
                                            block.start.hour, block.start.minute,
                                            block.end.hour, block.end.minute,
                                            format_point_value(&block.value),
                                            block.source,
                                        );
                                        rsx! {
                                            div {
                                                class: class,
                                                style: "left: {left_pct}%; width: {width_pct}%;",
                                                title: "{title}",
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // Time axis
            div { class: "schedule-preview-axis",
                span { "0:00" }
                span { "6:00" }
                span { "12:00" }
                span { "18:00" }
                span { "24:00" }
            }
        }
    }
}

// ----------------------------------------------------------------
// Right pane: Properties panel
// ----------------------------------------------------------------

#[component]
fn SchedulePropertiesPanel(
    schedule_id: ScheduleId,
    selected_schedule: Signal<Option<ScheduleId>>,
    refresh_counter: Signal<u64>,
) -> Element {
    let state = use_context::<AppState>();
    let mut schedule = use_signal(|| Option::<Schedule>::None);
    let mut edit_name = use_signal(String::new);
    let mut edit_desc = use_signal(String::new);
    let mut edit_default = use_signal(String::new);
    let mut edit_enabled = use_signal(|| true);
    let mut confirm_delete = use_signal(|| false);
    let mut status = use_signal(|| Option::<String>::None);

    let ss = state.schedule_store.clone();
    let _ = use_resource(move || {
        let ss = ss.clone();
        let _r = *refresh_counter.read();
        async move {
            if let Some(s) = ss.get_schedule(schedule_id).await {
                edit_name.set(s.name.clone());
                edit_desc.set(s.description.clone());
                edit_default.set(format_point_value(&s.default_value));
                edit_enabled.set(s.enabled);
                schedule.set(Some(s));
            }
        }
    });

    let Some(ref sched) = *schedule.read() else {
        return rsx! {};
    };

    let vtype_label = sched.value_type.label();

    rsx! {
        div { class: "details-pane schedule-properties",
            div { class: "details-header",
                span { "Properties" }
            }
            div { class: "schedule-props-body",
                div { class: "alarm-form-row",
                    label { "Name" }
                    input {
                        class: "schedule-value-input",
                        r#type: "text",
                        value: "{edit_name.read()}",
                        oninput: move |evt| edit_name.set(evt.value()),
                    }
                }
                div { class: "alarm-form-row",
                    label { "Description" }
                    textarea {
                        class: "schedule-textarea",
                        value: "{edit_desc.read()}",
                        oninput: move |evt| edit_desc.set(evt.value()),
                    }
                }
                div { class: "alarm-form-row",
                    label { "Type" }
                    span { class: "schedule-type-label", "{vtype_label}" }
                }
                div { class: "alarm-form-row",
                    label { "Default" }
                    input {
                        class: "schedule-value-input",
                        r#type: "text",
                        value: "{edit_default.read()}",
                        oninput: move |evt| edit_default.set(evt.value()),
                    }
                }
                div { class: "alarm-form-row",
                    label { "Enabled" }
                    input {
                        class: "schedule-checkbox",
                        r#type: "checkbox",
                        checked: *edit_enabled.read(),
                        onchange: move |evt| edit_enabled.set(evt.checked()),
                    }
                }

                div { class: "schedule-prop-actions",
                    button {
                        class: "alarm-save-btn",
                        onclick: {
                            let ss = state.schedule_store.clone();
                            let sched = sched.clone();
                            move |_| {
                                let name = edit_name.read().clone();
                                let desc = edit_desc.read().clone();
                                let default_str = edit_default.read().clone();
                                let enabled = *edit_enabled.read();
                                let default_value = parse_point_value_typed(&default_str, &sched.value_type);
                                let ss = ss.clone();
                                let s = sched.clone();
                                spawn(async move {
                                    match ss.update_schedule(
                                        s.id, &name, &desc, default_value, enabled, s.weekly,
                                    ).await {
                                        Ok(()) => {
                                            status.set(None);
                                            { let v = *refresh_counter.peek(); refresh_counter.set(v + 1); }
                                        }
                                        Err(e) => status.set(Some(format!("{e}"))),
                                    }
                                });
                            }
                        },
                        "Save"
                    }

                    if *confirm_delete.read() {
                        div { class: "schedule-delete-confirm",
                            span { "Delete this schedule?" }
                            button {
                                class: "alarm-delete-btn confirm",
                                onclick: {
                                    let ss = state.schedule_store.clone();
                                    let del_audit = state.clone();
                                    move |_| {
                                        let ss = ss.clone();
                                        let sid = schedule_id;
                                        let audit_state = del_audit.clone();
                                        spawn(async move {
                                            let _ = ss.delete_schedule(sid).await;
                                            audit_state.audit(
                                                bms_store_storage::store::audit_store::AuditEntryBuilder::new(
                                                    bms_store_storage::store::audit_store::AuditAction::DeleteSchedule, "schedule",
                                                ).resource_id(&format!("{sid}")),
                                            );
                                            selected_schedule.set(None);
                                            { let v = *refresh_counter.peek(); refresh_counter.set(v + 1); }
                                        });
                                    }
                                },
                                "Confirm"
                            }
                            button {
                                class: "alarm-cancel-btn",
                                onclick: move |_| confirm_delete.set(false),
                                "No"
                            }
                        }
                    } else {
                        button {
                            class: "alarm-delete-btn",
                            onclick: move |_| confirm_delete.set(true),
                            "Delete"
                        }
                    }
                }

                if let Some(ref msg) = *status.read() {
                    div { class: "alarm-form-error", "{msg}" }
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------

fn format_point_value(v: &PointValue) -> String {
    match v {
        PointValue::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        PointValue::Float(f) => format!("{f}"),
        PointValue::Integer(i) => format!("{i}"),
    }
}

fn parse_point_value(s: &str) -> PointValue {
    let s = s.trim();
    if s.eq_ignore_ascii_case("true") {
        return PointValue::Bool(true);
    }
    if s.eq_ignore_ascii_case("false") {
        return PointValue::Bool(false);
    }
    if let Ok(i) = s.parse::<i64>() {
        return PointValue::Integer(i);
    }
    if let Ok(f) = s.parse::<f64>() {
        return PointValue::Float(f);
    }
    PointValue::Float(0.0)
}

fn parse_point_value_typed(s: &str, vtype: &ScheduleValueType) -> PointValue {
    let s = s.trim();
    match vtype {
        ScheduleValueType::Binary => PointValue::Bool(s.eq_ignore_ascii_case("true") || s == "1"),
        ScheduleValueType::Analog => PointValue::Float(s.parse().unwrap_or(0.0)),
        ScheduleValueType::Multistate => PointValue::Integer(s.parse().unwrap_or(0)),
    }
}

fn describe_date_spec(spec: &DateSpec) -> String {
    match spec {
        DateSpec::Fixed { month, day } => {
            let month_name = month_abbr(*month);
            format!("{month_name} {day}")
        }
        DateSpec::FixedYear { year, month, day } => {
            let month_name = month_abbr(*month);
            format!("{month_name} {day}, {year}")
        }
        DateSpec::Relative {
            ordinal,
            weekday,
            month,
        } => {
            let ord = match ordinal {
                Ordinal::First => "1st",
                Ordinal::Second => "2nd",
                Ordinal::Third => "3rd",
                Ordinal::Fourth => "4th",
                Ordinal::Last => "Last",
            };
            let wd = weekday_name(*weekday);
            let mn = month_abbr(*month);
            format!("{ord} {wd} in {mn}")
        }
    }
}

fn month_abbr(m: u8) -> &'static str {
    match m {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "???",
    }
}

fn weekday_name(wd: u8) -> &'static str {
    match wd {
        0 => "Monday",
        1 => "Tuesday",
        2 => "Wednesday",
        3 => "Thursday",
        4 => "Friday",
        5 => "Saturday",
        6 => "Sunday",
        _ => "???",
    }
}

// ----------------------------------------------------------------
// BACnet Schedule Sync
// ----------------------------------------------------------------

const BACNET_DAY_NAMES: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

fn format_time_value(tv: &TimeValue) -> String {
    let t = &tv.time;
    format!(
        "{:02}:{:02}:{:02} = {:?}",
        t.hour, t.minute, t.second, tv.value
    )
}

#[component]
fn BacnetScheduleSync(schedule_id: ScheduleId, refresh_counter: Signal<u64>) -> Element {
    let state = use_context::<AppState>();
    let mut expanded = use_signal(|| false);
    let mut device_instance = use_signal(|| "0".to_string());
    let mut schedule_instance = use_signal(|| "0".to_string());
    let mut status_msg = use_signal(|| Option::<String>::None);
    let mut busy = use_signal(|| false);
    let mut read_result = use_signal(|| Option::<Vec<Vec<TimeValue>>>::None);
    let mut calendar_instance = use_signal(|| "0".to_string());
    let mut default_result = use_signal(|| Option::<String>::None);
    let mut calendar_result = use_signal(|| Option::<Vec<String>>::None);
    let mut exception_result = use_signal(|| Option::<String>::None);

    if !*expanded.read() {
        return rsx! {
            div { class: "schedule-bacnet-sync-toggle",
                button {
                    class: "schedule-tab",
                    onclick: move |_| expanded.set(true),
                    "BACnet Sync"
                }
            }
        };
    }

    rsx! {
        div { class: "schedule-bacnet-sync",
            div { class: "schedule-bacnet-sync-header",
                h4 { "BACnet Schedule Sync" }
                button {
                    class: "schedule-copy-btn",
                    onclick: move |_| {
                        expanded.set(false);
                        read_result.set(None);
                        default_result.set(None);
                        calendar_result.set(None);
                        exception_result.set(None);
                        status_msg.set(None);
                    },
                    "Close"
                }
            }
            div { class: "schedule-bacnet-sync-inputs",
                div { class: "alarm-form-row",
                    label { "Device Instance" }
                    input {
                        class: "schedule-value-input",
                        r#type: "text",
                        value: "{device_instance.read()}",
                        oninput: move |evt| device_instance.set(evt.value()),
                    }
                }
                div { class: "alarm-form-row",
                    label { "Schedule Instance" }
                    input {
                        class: "schedule-value-input",
                        r#type: "text",
                        value: "{schedule_instance.read()}",
                        oninput: move |evt| schedule_instance.set(evt.value()),
                    }
                }
                div { class: "alarm-form-row",
                    label { "Calendar Instance" }
                    input {
                        class: "schedule-value-input",
                        r#type: "text",
                        value: "{calendar_instance.read()}",
                        oninput: move |evt| calendar_instance.set(evt.value()),
                    }
                }
            }
            div { class: "schedule-bacnet-sync-actions",
                button {
                    class: "alarm-save-btn",
                    disabled: *busy.read(),
                    onclick: {
                        let bridge_handle = state.bacnet_handle();
                        move |_| {
                            let dev: u32 = match device_instance.read().parse() {
                                Ok(v) => v,
                                Err(_) => { status_msg.set(Some("Invalid device instance".into())); return; }
                            };
                            let sched_inst: u32 = match schedule_instance.read().parse() {
                                Ok(v) => v,
                                Err(_) => { status_msg.set(Some("Invalid schedule instance".into())); return; }
                            };
                            let bridge_handle = bridge_handle.clone();
                            busy.set(true);
                            status_msg.set(Some("Reading...".into()));
                            spawn(async move {
                                let guard = bridge_handle.lock().await;
                                let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                                if let Some(bridge) = nets.bridge_for_device(dev) {
                                    match bridge.read_schedule(dev, sched_inst).await {
                                        Ok(week) => {
                                            status_msg.set(Some(format!("Read OK: {} days", week.len())));
                                            read_result.set(Some(week));
                                        }
                                        Err(e) => {
                                            status_msg.set(Some(format!("Read error: {e}")));
                                            read_result.set(None);
                                        }
                                    }
                                } else {
                                    status_msg.set(Some("BACnet bridge not connected".into()));
                                }
                                drop(guard);
                                busy.set(false);
                            });
                        }
                    },
                    if *busy.read() { "Reading..." } else { "Read from Device" }
                }
                button {
                    class: "alarm-save-btn",
                    disabled: *busy.read(),
                    onclick: {
                        let bridge_handle = state.bacnet_handle();
                        let ss = state.schedule_store.clone();
                        move |_| {
                            let dev: u32 = match device_instance.read().parse() {
                                Ok(v) => v,
                                Err(_) => { status_msg.set(Some("Invalid device instance".into())); return; }
                            };
                            let sched_inst: u32 = match schedule_instance.read().parse() {
                                Ok(v) => v,
                                Err(_) => { status_msg.set(Some("Invalid schedule instance".into())); return; }
                            };
                            let bridge_handle = bridge_handle.clone();
                            let ss = ss.clone();
                            let sid = schedule_id;
                            busy.set(true);
                            status_msg.set(Some("Writing...".into()));
                            spawn(async move {
                                // Load the current schedule and convert to BACnet TimeValue format
                                let sched = ss.get_schedule(sid).await;
                                let Some(sched) = sched else {
                                    status_msg.set(Some("Schedule not found".into()));
                                    busy.set(false);
                                    return;
                                };

                                // Convert internal weekly (Mon-Sun, 7 days) to BACnet (Sun-Sat, 7 days)
                                let mut bacnet_week: Vec<Vec<TimeValue>> = Vec::with_capacity(7);
                                // BACnet day 0 = Sunday = internal day 6
                                bacnet_week.push(convert_day_slots_to_time_values(&sched.weekly[6]));
                                // BACnet days 1-6 = Mon-Sat = internal days 0-5
                                for day_idx in 0..6 {
                                    bacnet_week.push(convert_day_slots_to_time_values(&sched.weekly[day_idx]));
                                }

                                let guard = bridge_handle.lock().await;
                                let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                                if let Some(bridge) = nets.bridge_for_device(dev) {
                                    match bridge.write_schedule(dev, sched_inst, &bacnet_week).await {
                                        Ok(()) => {
                                            status_msg.set(Some("Write OK".into()));
                                        }
                                        Err(e) => {
                                            status_msg.set(Some(format!("Write error: {e}")));
                                        }
                                    }
                                } else {
                                    status_msg.set(Some("BACnet bridge not connected".into()));
                                }
                                drop(guard);
                                busy.set(false);
                            });
                        }
                    },
                    if *busy.read() { "Writing..." } else { "Write to Device" }
                }
                button {
                    class: "alarm-save-btn",
                    disabled: *busy.read(),
                    onclick: {
                        let bridge_handle = state.bacnet_handle();
                        move |_| {
                            let dev: u32 = match device_instance.read().parse() {
                                Ok(v) => v,
                                Err(_) => { status_msg.set(Some("Invalid device instance".into())); return; }
                            };
                            let sched_inst: u32 = match schedule_instance.read().parse() {
                                Ok(v) => v,
                                Err(_) => { status_msg.set(Some("Invalid schedule instance".into())); return; }
                            };
                            let bridge_handle = bridge_handle.clone();
                            busy.set(true);
                            status_msg.set(Some("Reading default...".into()));
                            spawn(async move {
                                let guard = bridge_handle.lock().await;
                                let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                                if let Some(bridge) = nets.bridge_for_device(dev) {
                                    match bridge.read_schedule_default(dev, sched_inst).await {
                                        Ok(value) => {
                                            status_msg.set(Some("Read default OK".into()));
                                            default_result.set(Some(format!("{:?}", value)));
                                        }
                                        Err(e) => {
                                            status_msg.set(Some(format!("Read default error: {e}")));
                                            default_result.set(None);
                                        }
                                    }
                                } else {
                                    status_msg.set(Some("BACnet bridge not connected".into()));
                                }
                                drop(guard);
                                busy.set(false);
                            });
                        }
                    },
                    if *busy.read() { "Reading..." } else { "Read Default" }
                }
                button {
                    class: "alarm-save-btn",
                    disabled: *busy.read(),
                    onclick: {
                        let bridge_handle = state.bacnet_handle();
                        move |_| {
                            let dev: u32 = match device_instance.read().parse() {
                                Ok(v) => v,
                                Err(_) => { status_msg.set(Some("Invalid device instance".into())); return; }
                            };
                            let cal_inst: u32 = match calendar_instance.read().parse() {
                                Ok(v) => v,
                                Err(_) => { status_msg.set(Some("Invalid calendar instance".into())); return; }
                            };
                            let bridge_handle = bridge_handle.clone();
                            busy.set(true);
                            status_msg.set(Some("Reading calendar...".into()));
                            spawn(async move {
                                let guard = bridge_handle.lock().await;
                                let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                                if let Some(bridge) = nets.bridge_for_device(dev) {
                                    match bridge.read_calendar(dev, cal_inst).await {
                                        Ok(entries) => {
                                            status_msg.set(Some(format!("Read calendar OK: {} entries", entries.len())));
                                            let formatted: Vec<String> = entries.iter().map(|e| format_calendar_entry(e)).collect();
                                            calendar_result.set(Some(formatted));
                                        }
                                        Err(e) => {
                                            status_msg.set(Some(format!("Read calendar error: {e}")));
                                            calendar_result.set(None);
                                        }
                                    }
                                } else {
                                    status_msg.set(Some("BACnet bridge not connected".into()));
                                }
                                drop(guard);
                                busy.set(false);
                            });
                        }
                    },
                    if *busy.read() { "Reading..." } else { "Read Calendar" }
                }
                button {
                    class: "alarm-save-btn",
                    disabled: *busy.read(),
                    onclick: {
                        let bridge_handle = state.bacnet_handle();
                        move |_| {
                            let dev: u32 = match device_instance.read().parse() {
                                Ok(v) => v,
                                Err(_) => { status_msg.set(Some("Invalid device instance".into())); return; }
                            };
                            let sched_inst: u32 = match schedule_instance.read().parse() {
                                Ok(v) => v,
                                Err(_) => { status_msg.set(Some("Invalid schedule instance".into())); return; }
                            };
                            let bridge_handle = bridge_handle.clone();
                            busy.set(true);
                            status_msg.set(Some("Reading exceptions...".into()));
                            spawn(async move {
                                let guard = bridge_handle.lock().await;
                                let nets = guard.as_any().downcast_ref::<BacnetNetworks>().unwrap();
                                if let Some(bridge) = nets.bridge_for_device(dev) {
                                    match bridge.read_exception_schedule(dev, sched_inst).await {
                                        Ok(value) => {
                                            status_msg.set(Some("Read exceptions OK".into()));
                                            exception_result.set(Some(format!("{:?}", value)));
                                        }
                                        Err(e) => {
                                            status_msg.set(Some(format!("Read exceptions error: {e}")));
                                            exception_result.set(None);
                                        }
                                    }
                                } else {
                                    status_msg.set(Some("BACnet bridge not connected".into()));
                                }
                                drop(guard);
                                busy.set(false);
                            });
                        }
                    },
                    if *busy.read() { "Reading..." } else { "Read Exceptions" }
                }
            }
            if let Some(ref msg) = *status_msg.read() {
                div { class: "schedule-bacnet-status",
                    "{msg}"
                }
            }
            if let Some(ref week) = *read_result.read() {
                div { class: "schedule-bacnet-result",
                    h4 { "BACnet Weekly Schedule (raw)" }
                    for (day_idx, day) in week.iter().enumerate() {
                        div { class: "schedule-bacnet-day",
                            strong {
                                "{BACNET_DAY_NAMES.get(day_idx).unwrap_or(&\"?\")}: "
                            }
                            if day.is_empty() {
                                span { class: "schedule-empty-day", "(no entries)" }
                            }
                            for tv in day.iter() {
                                div { class: "schedule-bacnet-tv",
                                    "{format_time_value(tv)}"
                                }
                            }
                        }
                    }
                }
            }
            if let Some(ref val) = *default_result.read() {
                div { class: "schedule-bacnet-result",
                    h4 { "Schedule Default Value" }
                    div { "{val}" }
                }
            }
            if let Some(ref entries) = *calendar_result.read() {
                div { class: "schedule-bacnet-result",
                    h4 { "Calendar Entries" }
                    for entry in entries.iter() {
                        div { "{entry}" }
                    }
                }
            }
            if let Some(ref val) = *exception_result.read() {
                div { class: "schedule-bacnet-result",
                    h4 { "Exception Schedule" }
                    div { "{val}" }
                }
            }
        }
    }
}

/// Format a CalendarEntry for display.
fn format_calendar_entry(entry: &CalendarEntry) -> String {
    match entry {
        CalendarEntry::Date(d) => {
            format!(
                "Date: {}/{}/{}",
                d.year_since_1900 as u16 + 1900,
                d.month,
                d.day
            )
        }
        CalendarEntry::Range(r) => {
            format!(
                "Range: {}/{}/{} - {}/{}/{}",
                r.start.year_since_1900 as u16 + 1900,
                r.start.month,
                r.start.day,
                r.end.year_since_1900 as u16 + 1900,
                r.end.month,
                r.end.day,
            )
        }
        CalendarEntry::WeekNDay {
            month,
            week_of_month,
            day_of_week,
        } => {
            format!("WeekNDay: month={month}, week={week_of_month}, day={day_of_week}")
        }
    }
}

/// Convert internal DaySlots to BACnet TimeValue entries.
fn convert_day_slots_to_time_values(slots: &DaySlots) -> Vec<TimeValue> {
    use rustbac_client::ClientDataValue;
    use rustbac_core::types::Time;

    slots
        .0
        .iter()
        .map(|slot| {
            let time = Time {
                hour: slot.time.hour,
                minute: slot.time.minute,
                second: 0,
                hundredths: 0,
            };
            let value = match slot.value {
                PointValue::Bool(b) => ClientDataValue::Enumerated(if b { 1 } else { 0 }),
                PointValue::Integer(i) => ClientDataValue::Unsigned(i as u32),
                PointValue::Float(f) => ClientDataValue::Real(f as f32),
            };
            TimeValue { time, value }
        })
        .collect()
}

// ----------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------

fn format_time_ms(ms: i64) -> String {
    #[repr(C)]
    #[derive(Default)]
    struct Tm {
        tm_sec: i32,
        tm_min: i32,
        tm_hour: i32,
        tm_mday: i32,
        tm_mon: i32,
        tm_year: i32,
        tm_wday: i32,
        tm_yday: i32,
        tm_isdst: i32,
        tm_gmtoff: i64,
        tm_zone: *const i8,
    }
    extern "C" {
        fn localtime_r(time: *const i64, result: *mut Tm) -> *mut Tm;
    }

    let epoch_secs = ms / 1000;
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let mut tm = Tm::default();
    unsafe { localtime_r(&epoch_secs, &mut tm) };

    let mut now_tm = Tm::default();
    unsafe { localtime_r(&now_secs, &mut now_tm) };

    let hour = tm.tm_hour;
    let min = tm.tm_min;
    let sec = tm.tm_sec;

    if tm.tm_year == now_tm.tm_year && tm.tm_yday == now_tm.tm_yday {
        format!("{hour:02}:{min:02}:{sec:02}")
    } else {
        const MONTHS: [&str; 12] = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        let mon = MONTHS.get(tm.tm_mon as usize).unwrap_or(&"???");
        let day = tm.tm_mday;
        format!("{mon} {day} {hour:02}:{min:02}:{sec:02}")
    }
}
