use dioxus::prelude::*;

use crate::auth::Permission;
use crate::gui::state::AppState;
use crate::reporting::templates::template_for_type;
use crate::store::report_store::{
    ExecutionStatus, ReportConfig, ReportDefinition, ReportExecution, ReportFrequency,
    ReportRecipient, ReportSchedule, ReportType, TimeRangeKind,
};

// ----------------------------------------------------------------
// Sub-tabs
// ----------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum ReportTab {
    Templates,
    Schedules,
    History,
}

// ----------------------------------------------------------------
// ReportView — main component
// ----------------------------------------------------------------

#[component]
pub fn ReportView() -> Element {
    let state = use_context::<AppState>();
    let can_manage = state.has_permission(Permission::ManageReports);
    let mut tab = use_signal(|| ReportTab::Templates);
    let current_tab = *tab.read();

    rsx! {
        div { class: "report-view",
            div { class: "report-tabs",
                button {
                    class: if current_tab == ReportTab::Templates { "report-tab active" } else { "report-tab" },
                    onclick: move |_| tab.set(ReportTab::Templates),
                    "Templates"
                }
                button {
                    class: if current_tab == ReportTab::Schedules { "report-tab active" } else { "report-tab" },
                    onclick: move |_| tab.set(ReportTab::Schedules),
                    "Schedules"
                }
                button {
                    class: if current_tab == ReportTab::History { "report-tab active" } else { "report-tab" },
                    onclick: move |_| tab.set(ReportTab::History),
                    "History"
                }
            }
            div { class: "report-tab-content",
                match current_tab {
                    ReportTab::Templates => rsx! { TemplatesTab { can_manage } },
                    ReportTab::Schedules => rsx! { SchedulesTab { can_manage } },
                    ReportTab::History => rsx! { HistoryTab { can_manage } },
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Templates Tab
// ----------------------------------------------------------------

#[component]
fn TemplatesTab(can_manage: bool) -> Element {
    let state = use_context::<AppState>();
    let rs = state.report_store.clone();

    let mut definitions: Signal<Vec<ReportDefinition>> = use_signal(Vec::new);
    let mut editing: Signal<Option<ReportDefinition>> = use_signal(|| None);
    let mut show_form: Signal<bool> = use_signal(|| false);
    let mut status_msg: Signal<String> = use_signal(String::new);

    // Form state
    let mut form_name: Signal<String> = use_signal(String::new);
    let mut form_type: Signal<String> = use_signal(|| "energy_summary".to_string());
    let mut form_time_range: Signal<String> = use_signal(|| "last_7_days".to_string());

    // Load definitions
    {
        let rs = rs.clone();
        let _ = use_resource(move || {
            let rs = rs.clone();
            async move {
                definitions.set(rs.list_definitions().await);
            }
        });
    }

    rsx! {
        div { class: "templates-section",
            div { class: "section-header",
                h3 { "Report Templates" }
                if can_manage {
                    button {
                        class: "btn btn-primary",
                        onclick: move |_| {
                            editing.set(None);
                            form_name.set(String::new());
                            form_type.set("energy_summary".to_string());
                            form_time_range.set("last_7_days".to_string());
                            show_form.set(true);
                        },
                        "+ New Report"
                    }
                }
            }

            if !status_msg.read().is_empty() {
                div { class: "status-bar", "{status_msg}" }
            }

            if *show_form.read() {
                div { class: "report-form",
                    h4 { if editing.read().is_some() { "Edit Report" } else { "New Report" } }
                    div { class: "form-row",
                        label { "Name" }
                        input {
                            r#type: "text",
                            value: "{form_name}",
                            oninput: move |evt| form_name.set(evt.value()),
                            placeholder: "e.g., Weekly Energy Summary",
                        }
                    }
                    div { class: "form-row",
                        label { "Type" }
                        select {
                            value: "{form_type}",
                            onchange: move |evt| form_type.set(evt.value()),
                            option { value: "energy_summary", "Energy Summary" }
                            option { value: "alarm_summary", "Alarm Summary" }
                            option { value: "comfort_compliance", "Comfort Compliance" }
                            option { value: "equipment_runtime", "Equipment Runtime" }
                            option { value: "custom", "Custom" }
                        }
                    }
                    div { class: "form-row",
                        label { "Time Range" }
                        select {
                            value: "{form_time_range}",
                            onchange: move |evt| form_time_range.set(evt.value()),
                            option { value: "last_24_hours", "Last 24 Hours" }
                            option { value: "last_7_days", "Last 7 Days" }
                            option { value: "last_30_days", "Last 30 Days" }
                            option { value: "last_month", "Last Month" }
                        }
                    }
                    div { class: "form-actions",
                        button {
                            class: "btn btn-primary",
                            onclick: {
                                let rs = rs.clone();
                                let audit = state.audit_store.clone();
                                let current_user = state.current_user;
                                move |_| {
                                    let rs = rs.clone();
                                    let audit = audit.clone();
                                    let name = form_name.read().clone();
                                    let rt_str = form_type.read().clone();
                                    let tr_str = form_time_range.read().clone();
                                    let edit_id = editing.read().as_ref().map(|d| d.id);
                                    spawn(async move {
                                        let report_type = ReportType::from_str(&rt_str).unwrap_or(ReportType::Custom);
                                        let time_range = match tr_str.as_str() {
                                            "last_24_hours" => TimeRangeKind::Last24Hours,
                                            "last_30_days" => TimeRangeKind::Last30Days,
                                            "last_month" => TimeRangeKind::LastMonth,
                                            _ => TimeRangeKind::Last7Days,
                                        };
                                        let mut config = template_for_type(&report_type);
                                        config.time_range = time_range;

                                        let (result, action) = if let Some(id) = edit_id {
                                            (rs.update_definition(id, &name, report_type, &config).await.map(|_| id),
                                             crate::store::audit_store::AuditAction::UpdateReport)
                                        } else {
                                            (rs.create_definition(&name, report_type, &config).await,
                                             crate::store::audit_store::AuditAction::CreateReport)
                                        };

                                        match result {
                                            Ok(id) => {
                                                if let Some(ref user) = *current_user.read() {
                                                    let builder = crate::store::audit_store::AuditEntryBuilder::new(action, "report")
                                                        .resource_id(&id.to_string());
                                                    let _ = audit.log_action(&user.id, &user.username, builder).await;
                                                }
                                                status_msg.set("Report saved.".to_string());
                                                show_form.set(false);
                                                editing.set(None);
                                                definitions.set(rs.list_definitions().await);
                                            }
                                            Err(e) => status_msg.set(format!("Error: {e}")),
                                        }
                                    });
                                }
                            },
                            "Save"
                        }
                        button {
                            class: "btn",
                            onclick: move |_| show_form.set(false),
                            "Cancel"
                        }
                    }
                }
            }

            // Definition list
            table { class: "data-table",
                thead {
                    tr {
                        th { "Name" }
                        th { "Type" }
                        th { "Sections" }
                        th { "Time Range" }
                        if can_manage { th { "Actions" } }
                    }
                }
                tbody {
                    for def in definitions.read().iter() {
                        {
                            let def_id = def.id;
                            let def_name = def.name.clone();
                            let type_label = def.report_type.label().to_string();
                            let section_count = def.config.sections.len();
                            let range_label = def.config.time_range.label().to_string();
                            let rt_str = def.report_type.as_str().to_string();
                            rsx! {
                                tr {
                                    td { "{def_name}" }
                                    td {
                                        span { class: "badge badge-report-type", "{type_label}" }
                                    }
                                    td { "{section_count}" }
                                    td { "{range_label}" }
                                    if can_manage {
                                        td { class: "action-cell",
                                            button {
                                                class: "btn btn-sm",
                                                onclick: {
                                                    let name = def_name.clone();
                                                    let rt = rt_str.clone();
                                                    let did = def_id;
                                                    move |_| {
                                                        form_name.set(name.clone());
                                                        form_type.set(rt.clone());
                                                        editing.set(Some(ReportDefinition {
                                                            id: did,
                                                            name: name.clone(),
                                                            report_type: ReportType::from_str(&rt).unwrap_or(ReportType::Custom),
                                                            config: ReportConfig {
                                                                time_range: TimeRangeKind::Last7Days,
                                                                sections: vec![],
                                                            },
                                                            created_ms: 0,
                                                            updated_ms: 0,
                                                        }));
                                                        show_form.set(true);
                                                    }
                                                },
                                                "Edit"
                                            }
                                            button {
                                                class: "btn btn-sm btn-danger",
                                                onclick: {
                                                    let rs = rs.clone();
                                                    let audit = state.audit_store.clone();
                                                    let current_user = state.current_user;
                                                    move |_| {
                                                        let rs = rs.clone();
                                                        let audit = audit.clone();
                                                        spawn(async move {
                                                            let _ = rs.delete_definition(def_id).await;
                                                            if let Some(ref user) = *current_user.read() {
                                                                let builder = crate::store::audit_store::AuditEntryBuilder::new(
                                                                    crate::store::audit_store::AuditAction::DeleteReport, "report",
                                                                ).resource_id(&def_id.to_string());
                                                                let _ = audit.log_action(&user.id, &user.username, builder).await;
                                                            }
                                                            definitions.set(rs.list_definitions().await);
                                                        });
                                                    }
                                                },
                                                "Delete"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if definitions.read().is_empty() && !*show_form.read() {
                p { class: "empty-state", "No report templates defined. Click '+ New Report' to create one." }
            }
        }
    }
}

// ----------------------------------------------------------------
// Schedules Tab
// ----------------------------------------------------------------

#[component]
fn SchedulesTab(can_manage: bool) -> Element {
    let state = use_context::<AppState>();
    let rs = state.report_store.clone();

    let mut schedules: Signal<Vec<ReportSchedule>> = use_signal(Vec::new);
    let mut definitions: Signal<Vec<ReportDefinition>> = use_signal(Vec::new);
    let mut show_form: Signal<bool> = use_signal(|| false);
    let mut status_msg: Signal<String> = use_signal(String::new);

    // Form state
    let mut form_report_id: Signal<i64> = use_signal(|| 0);
    let mut form_frequency: Signal<String> = use_signal(|| "daily".to_string());
    let mut form_day_of_week: Signal<u8> = use_signal(|| 0);
    let mut form_hour: Signal<u8> = use_signal(|| 6);
    let mut form_minute: Signal<u8> = use_signal(|| 0);
    let mut form_email: Signal<String> = use_signal(String::new);
    let mut form_recip_name: Signal<String> = use_signal(String::new);

    // Load data
    {
        let rs = rs.clone();
        let _ = use_resource(move || {
            let rs = rs.clone();
            async move {
                schedules.set(rs.list_schedules(None).await);
                definitions.set(rs.list_definitions().await);
            }
        });
    }

    rsx! {
        div { class: "schedules-section",
            div { class: "section-header",
                h3 { "Report Schedules" }
                if can_manage {
                    button {
                        class: "btn btn-primary",
                        onclick: move |_| {
                            show_form.set(true);
                            form_frequency.set("daily".to_string());
                            form_hour.set(6);
                            form_minute.set(0);
                            form_email.set(String::new());
                            form_recip_name.set(String::new());
                            // Default to first definition if available
                            if let Some(first) = definitions.read().first() {
                                form_report_id.set(first.id);
                            }
                        },
                        "+ Add Schedule"
                    }
                }
            }

            if !status_msg.read().is_empty() {
                div { class: "status-bar", "{status_msg}" }
            }

            if *show_form.read() {
                div { class: "report-form",
                    h4 { "New Schedule" }
                    div { class: "form-row",
                        label { "Report" }
                        select {
                            onchange: move |evt| {
                                if let Ok(id) = evt.value().parse::<i64>() {
                                    form_report_id.set(id);
                                }
                            },
                            for def in definitions.read().iter() {
                                option { value: "{def.id}", "{def.name}" }
                            }
                        }
                    }
                    div { class: "form-row",
                        label { "Frequency" }
                        select {
                            value: "{form_frequency}",
                            onchange: move |evt| form_frequency.set(evt.value()),
                            option { value: "daily", "Daily" }
                            option { value: "weekly", "Weekly" }
                            option { value: "monthly", "Monthly" }
                        }
                    }
                    if *form_frequency.read() == "weekly" {
                        div { class: "form-row",
                            label { "Day" }
                            select {
                                onchange: move |evt| {
                                    if let Ok(d) = evt.value().parse::<u8>() { form_day_of_week.set(d); }
                                },
                                option { value: "0", "Monday" }
                                option { value: "1", "Tuesday" }
                                option { value: "2", "Wednesday" }
                                option { value: "3", "Thursday" }
                                option { value: "4", "Friday" }
                                option { value: "5", "Saturday" }
                                option { value: "6", "Sunday" }
                            }
                        }
                    }
                    div { class: "form-row",
                        label { "Time (hour:minute)" }
                        div { style: "display:flex;gap:4px;",
                            input {
                                r#type: "number",
                                min: "0",
                                max: "23",
                                value: "{form_hour}",
                                style: "width:60px;",
                                oninput: move |evt| {
                                    if let Ok(h) = evt.value().parse::<u8>() { form_hour.set(h.min(23)); }
                                },
                            }
                            span { ":" }
                            input {
                                r#type: "number",
                                min: "0",
                                max: "59",
                                value: "{form_minute}",
                                style: "width:60px;",
                                oninput: move |evt| {
                                    if let Ok(m) = evt.value().parse::<u8>() { form_minute.set(m.min(59)); }
                                },
                            }
                        }
                    }
                    div { class: "form-row",
                        label { "Recipient Email" }
                        input {
                            r#type: "email",
                            value: "{form_email}",
                            oninput: move |evt| form_email.set(evt.value()),
                            placeholder: "ops@example.com",
                        }
                    }
                    div { class: "form-row",
                        label { "Recipient Name" }
                        input {
                            r#type: "text",
                            value: "{form_recip_name}",
                            oninput: move |evt| form_recip_name.set(evt.value()),
                            placeholder: "Operations Team",
                        }
                    }
                    div { class: "form-actions",
                        button {
                            class: "btn btn-primary",
                            onclick: {
                                let rs = rs.clone();
                                move |_| {
                                    let rs = rs.clone();
                                    let report_id = *form_report_id.read();
                                    let freq_str = form_frequency.read().clone();
                                    let dow = *form_day_of_week.read();
                                    let hour = *form_hour.read();
                                    let minute = *form_minute.read();
                                    let email = form_email.read().clone();
                                    let name = form_recip_name.read().clone();
                                    spawn(async move {
                                        let freq = ReportFrequency::from_str(&freq_str).unwrap_or(ReportFrequency::Daily);
                                        let day_of_week = if freq == ReportFrequency::Weekly { Some(dow) } else { None };
                                        let recipients = if email.is_empty() {
                                            vec![]
                                        } else {
                                            vec![ReportRecipient { email, name }]
                                        };
                                        match rs.create_schedule(report_id, freq, day_of_week, None, hour, minute, 0, &recipients).await {
                                            Ok(_) => {
                                                status_msg.set("Schedule created.".to_string());
                                                show_form.set(false);
                                                schedules.set(rs.list_schedules(None).await);
                                            }
                                            Err(e) => status_msg.set(format!("Error: {e}")),
                                        }
                                    });
                                }
                            },
                            "Save"
                        }
                        button {
                            class: "btn",
                            onclick: move |_| show_form.set(false),
                            "Cancel"
                        }
                    }
                }
            }

            // Schedule list
            table { class: "data-table",
                thead {
                    tr {
                        th { "Report" }
                        th { "Frequency" }
                        th { "Time" }
                        th { "Recipients" }
                        th { "Enabled" }
                        th { "Next Run" }
                        if can_manage { th { "Actions" } }
                    }
                }
                tbody {
                    for sched in schedules.read().iter() {
                        {
                            let sched_id = sched.id;
                            let report_id = sched.report_id;
                            let freq_label = sched.frequency.label().to_string();
                            let time_str = format!("{:02}:{:02}", sched.hour, sched.minute);
                            let recip_count = sched.recipients.len();
                            let enabled = sched.enabled;
                            let next_run = sched.next_run_ms.map(|ms| format_timestamp(ms)).unwrap_or_else(|| "—".to_string());
                            // Find report name
                            let report_name = definitions.read().iter()
                                .find(|d| d.id == report_id)
                                .map(|d| d.name.clone())
                                .unwrap_or_else(|| format!("Report #{report_id}"));
                            rsx! {
                                tr {
                                    td { "{report_name}" }
                                    td { "{freq_label}" }
                                    td { "{time_str}" }
                                    td { "{recip_count}" }
                                    td {
                                        span {
                                            class: if enabled { "badge badge-success" } else { "badge badge-muted" },
                                            if enabled { "Yes" } else { "No" }
                                        }
                                    }
                                    td { "{next_run}" }
                                    if can_manage {
                                        td { class: "action-cell",
                                            button {
                                                class: "btn btn-sm btn-danger",
                                                onclick: {
                                                    let rs = rs.clone();
                                                    move |_| {
                                                        let rs = rs.clone();
                                                        spawn(async move {
                                                            let _ = rs.delete_schedule(sched_id).await;
                                                            schedules.set(rs.list_schedules(None).await);
                                                        });
                                                    }
                                                },
                                                "Delete"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if schedules.read().is_empty() && !*show_form.read() {
                p { class: "empty-state", "No schedules configured. Click '+ Add Schedule' to set up automated report delivery." }
            }
        }
    }
}

// ----------------------------------------------------------------
// History Tab
// ----------------------------------------------------------------

#[component]
fn HistoryTab(can_manage: bool) -> Element {
    let state = use_context::<AppState>();
    let rs = state.report_store.clone();

    let mut executions: Signal<Vec<ReportExecution>> = use_signal(Vec::new);
    let mut definitions: Signal<Vec<ReportDefinition>> = use_signal(Vec::new);
    let mut status_msg: Signal<String> = use_signal(String::new);
    let mut preview_html: Signal<Option<String>> = use_signal(|| None);

    // Clone stores for use in closures
    let history_store = state.history_store.clone();
    let alarm_store = state.alarm_store.clone();
    let point_store = state.store.clone();
    let node_store = state.node_store.clone();

    // Load data
    {
        let rs = rs.clone();
        let _ = use_resource(move || {
            let rs = rs.clone();
            async move {
                executions.set(rs.list_executions(None, 100).await);
                definitions.set(rs.list_definitions().await);
            }
        });
    }

    rsx! {
        div { class: "history-section",
            div { class: "section-header",
                h3 { "Execution History" }
                if can_manage {
                    // Run Now dropdown
                    for def in definitions.read().iter() {
                        {
                            let def_id = def.id;
                            let def_name = def.name.clone();
                            let rs = rs.clone();
                            rsx! {
                                button {
                                    class: "btn btn-sm",
                                    title: "Run {def_name} now",
                                    onclick: {
                                        let rs = rs.clone();
                                        let def_name = def_name.clone();
                                        let hs = history_store.clone();
                                        let als = alarm_store.clone();
                                        let ps = point_store.clone();
                                        let ns = node_store.clone();
                                        let audit = state.audit_store.clone();
                                        let current_user = state.current_user;
                                        move |_| {
                                            let rs = rs.clone();
                                            let name = def_name.clone();
                                            let audit = audit.clone();
                                            let engine = crate::reporting::engine::ReportEngine::new(
                                                hs.clone(), als.clone(), ps.clone(), ns.clone(),
                                            );
                                            spawn(async move {
                                                status_msg.set(format!("Running '{name}'..."));
                                                match engine.run_report(&rs, def_id, None, "manual").await {
                                                    Ok((_exec_id, status)) => {
                                                        let msg = if status == ExecutionStatus::Completed {
                                                            format!("'{name}' completed.")
                                                        } else {
                                                            format!("'{name}' failed.")
                                                        };
                                                        status_msg.set(msg);
                                                    }
                                                    Err(e) => {
                                                        status_msg.set(format!("Error: {e}"));
                                                    }
                                                }
                                                // Audit log
                                                if let Some(ref user) = *current_user.read() {
                                                    let builder = crate::store::audit_store::AuditEntryBuilder::new(
                                                        crate::store::audit_store::AuditAction::RunReport,
                                                        "report",
                                                    ).resource_id(&def_id.to_string());
                                                    let _ = audit.log_action(&user.id, &user.username, builder).await;
                                                }
                                                executions.set(rs.list_executions(None, 100).await);
                                            });
                                        }
                                    },
                                    "Run {def_name}"
                                }
                            }
                        }
                    }
                }
            }

            if !status_msg.read().is_empty() {
                div { class: "status-bar", "{status_msg}" }
            }

            // Preview modal
            if let Some(html) = preview_html.read().as_ref() {
                div { class: "report-preview-modal",
                    div { class: "report-preview-header",
                        h3 { "Report Preview" }
                        button {
                            class: "btn btn-sm",
                            onclick: move |_| preview_html.set(None),
                            "Close"
                        }
                    }
                    div {
                        class: "report-preview-content",
                        dangerous_inner_html: "{html}",
                    }
                }
            }

            // Execution table
            table { class: "data-table",
                thead {
                    tr {
                        th { "Report" }
                        th { "Triggered By" }
                        th { "Status" }
                        th { "Started" }
                        th { "Delivery" }
                        th { "Actions" }
                    }
                }
                tbody {
                    for exec in executions.read().iter() {
                        {
                            let _exec_id = exec.id;
                            let report_id = exec.report_id;
                            let triggered_by = exec.triggered_by.clone();
                            let status_str = exec.status.as_str().to_string();
                            let started = format_timestamp(exec.started_ms);
                            let delivery = exec.delivery_status.clone().unwrap_or_else(|| "—".to_string());
                            let has_html = exec.report_html.is_some();
                            let html_content = exec.report_html.clone();
                            let error_msg = exec.error_message.clone();
                            let report_name = definitions.read().iter()
                                .find(|d| d.id == report_id)
                                .map(|d| d.name.clone())
                                .unwrap_or_else(|| format!("#{report_id}"));
                            let status_class = match exec.status {
                                ExecutionStatus::Running => "badge badge-info",
                                ExecutionStatus::Completed => "badge badge-success",
                                ExecutionStatus::Failed => "badge badge-danger",
                            };
                            rsx! {
                                tr {
                                    td { "{report_name}" }
                                    td { "{triggered_by}" }
                                    td {
                                        span { class: "{status_class}", "{status_str}" }
                                        if let Some(ref err) = error_msg {
                                            span { class: "error-hint", title: "{err}", " !" }
                                        }
                                    }
                                    td { "{started}" }
                                    td { "{delivery}" }
                                    td { class: "action-cell",
                                        if has_html {
                                            button {
                                                class: "btn btn-sm",
                                                onclick: move |_| {
                                                    preview_html.set(html_content.clone());
                                                },
                                                "View"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if executions.read().is_empty() {
                p { class: "empty-state", "No report executions yet. Use 'Run' buttons above to generate a report." }
            }
        }
    }
}

// ----------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------

fn format_timestamp(ms: i64) -> String {
    let secs = ms / 1000;
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let mins = (time_secs % 3600) / 60;

    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{:02}-{d:02} {hours:02}:{mins:02}", m)
}
