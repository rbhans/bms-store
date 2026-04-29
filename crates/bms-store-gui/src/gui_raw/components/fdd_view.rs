use dioxus::prelude::*;

use crate::auth::Permission;
use crate::fdd::model::{
    FddBinding, FddCategory, FddFault, FddFaultEvent, FddFaultState, FddHistoryQuery, FddRule,
    FddSeverity,
};
use crate::gui::state::AppState;

// ----------------------------------------------------------------
// FDD View — sub-tabbed: Dashboard | Rules | Bindings | History
// ----------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum FddSubTab {
    Dashboard,
    Rules,
    Bindings,
    History,
}

impl FddSubTab {
    fn label(&self) -> &'static str {
        match self {
            Self::Dashboard => "Dashboard",
            Self::Rules => "Rules",
            Self::Bindings => "Bindings",
            Self::History => "History",
        }
    }
}

#[component]
pub fn FddView() -> Element {
    let mut tab = use_signal(|| FddSubTab::Dashboard);
    let current = *tab.read();

    rsx! {
        div { class: "energy-view",
            // Sub-tab bar (reuse energy-tab-bar CSS)
            div { class: "energy-tab-bar",
                for t in [FddSubTab::Dashboard, FddSubTab::Rules, FddSubTab::Bindings, FddSubTab::History] {
                    button {
                        class: if current == t { "energy-tab-btn active" } else { "energy-tab-btn" },
                        onclick: move |_| tab.set(t),
                        "{t.label()}"
                    }
                }
            }

            // Tab content
            div { class: "energy-tab-content",
                match current {
                    FddSubTab::Dashboard => rsx! { FddDashboard {} },
                    FddSubTab::Rules => rsx! { FddRuleList {} },
                    FddSubTab::Bindings => rsx! { FddBindingList {} },
                    FddSubTab::History => rsx! { FddHistoryView {} },
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Dashboard — summary cards + active faults table
// ----------------------------------------------------------------

#[component]
fn FddDashboard() -> Element {
    let state = use_context::<AppState>();
    let fdd_store = state.fdd_store.clone();
    let can_manage = state.has_permission(Permission::ManageFdd);

    let mut version = use_signal(|| 0u64);
    let ver = *version.read();

    let faults = use_resource(move || {
        let fs = fdd_store.clone();
        let _v = ver;
        async move { fs.get_active_faults().await }
    });

    let fault_list = faults.cloned().unwrap_or_default();

    // Summary counts
    let total = fault_list.len();
    let warning_count = fault_list
        .iter()
        .filter(|f| f.severity == FddSeverity::Warning)
        .count();
    let critical_count = fault_list
        .iter()
        .filter(|f| f.severity == FddSeverity::Critical)
        .count();
    let info_count = fault_list
        .iter()
        .filter(|f| f.severity == FddSeverity::Info)
        .count();
    let acked_count = fault_list
        .iter()
        .filter(|f| f.state == FddFaultState::Acknowledged)
        .count();

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    rsx! {
        div { class: "energy-dashboard",
            // Summary cards
            div { class: "energy-summary-cards",
                div { class: "energy-card",
                    div { class: "energy-card-label", "Active Faults" }
                    div { class: "energy-card-value", "{total}" }
                }
                div { class: "energy-card",
                    div { class: "energy-card-label", style: "color: #f44336;", "Critical" }
                    div { class: "energy-card-value", style: "color: #f44336;", "{critical_count}" }
                }
                div { class: "energy-card",
                    div { class: "energy-card-label", style: "color: #ff9800;", "Warning" }
                    div { class: "energy-card-value", style: "color: #ff9800;", "{warning_count}" }
                }
                div { class: "energy-card",
                    div { class: "energy-card-label", "Info" }
                    div { class: "energy-card-value", "{info_count}" }
                }
                div { class: "energy-card",
                    div { class: "energy-card-label", "Acknowledged" }
                    div { class: "energy-card-value", "{acked_count}" }
                }
            }

            if fault_list.is_empty() {
                div { class: "energy-empty",
                    p { "No active faults detected." }
                    p { "Configure FDD rules and bind them to equipment to start detecting faults." }
                }
            } else {
                h3 { "Active Faults" }
                table { class: "energy-table",
                    thead {
                        tr {
                            th { "Equipment" }
                            th { "Rule" }
                            th { "Severity" }
                            th { "State" }
                            th { "Duration" }
                            th { "Guidance" }
                            if can_manage {
                                th { "Actions" }
                            }
                        }
                    }
                    tbody {
                        for fault in &fault_list {
                            {
                                let fid = fault.id;
                                let sev_color = severity_color(&fault.severity);
                                let duration = format_duration(now_ms - fault.detected_ms);
                                let guidance_preview: String = fault.guidance.chars().take(80).collect();
                                let state_label = fault.state.key();
                                let fs = state.fdd_store.clone();
                                rsx! {
                                    tr {
                                        td { class: "monospace", "{fault.equip_id}" }
                                        td { "{fault.rule_name}" }
                                        td {
                                            span {
                                                class: "energy-badge",
                                                style: "background: {sev_color};",
                                                "{fault.severity.key()}"
                                            }
                                        }
                                        td { "{state_label}" }
                                        td { "{duration}" }
                                        td { class: "energy-config-preview", "{guidance_preview}" }
                                        if can_manage {
                                            td {
                                                if fault.state == FddFaultState::Active {
                                                    button {
                                                        class: "energy-btn energy-btn-primary energy-btn-sm",
                                                        onclick: move |_| {
                                                            let fs = fs.clone();
                                                            spawn(async move { let _ = fs.acknowledge_fault(fid).await; });
                                                            version.set(ver + 1);
                                                        },
                                                        "Ack"
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

// ----------------------------------------------------------------
// Rules — table with expandable rows
// ----------------------------------------------------------------

#[component]
fn FddRuleList() -> Element {
    let state = use_context::<AppState>();
    let fdd_store = state.fdd_store.clone();
    let can_manage = state.has_permission(Permission::ManageFdd);

    let mut version = use_signal(|| 0u64);
    let ver = *version.read();

    let rules = use_resource(move || {
        let fs = fdd_store.clone();
        let _v = ver;
        async move { fs.list_rules().await }
    });

    let rule_list = rules.cloned().unwrap_or_default();

    let mut expanded_id = use_signal(|| Option::<i64>::None);

    rsx! {
        div { class: "energy-meters",
            h3 { "FDD Rules" }

            table { class: "energy-table",
                thead {
                    tr {
                        th { "Name" }
                        th { "Category" }
                        th { "Equip Tags" }
                        th { "Severity" }
                        th { "Built-in" }
                        th { "Enabled" }
                        if can_manage {
                            th { "Actions" }
                        }
                    }
                }
                tbody {
                    for rule in &rule_list {
                        {
                            let rid = rule.id;
                            let is_expanded = *expanded_id.read() == Some(rid);
                            let sev_color = severity_color(&rule.severity);
                            let cat_color = category_color(&rule.category);
                            let tags_str = rule.equip_tags.join(", ");
                            let enabled = rule.enabled;
                            let is_builtin = rule.builtin;
                            let fs = state.fdd_store.clone();
                            let rule_name = rule.name.clone();
                            let rule_desc = rule.description.clone();
                            let rule_guidance = rule.guidance.clone();
                            let condition_json = serde_json::to_string_pretty(&rule.condition)
                                .unwrap_or_else(|_| "{}".to_string());

                            rsx! {
                                tr {
                                    onclick: move |_| {
                                        if *expanded_id.read() == Some(rid) {
                                            expanded_id.set(None);
                                        } else {
                                            expanded_id.set(Some(rid));
                                        }
                                    },
                                    style: "cursor: pointer;",
                                    td { "{rule_name}" }
                                    td {
                                        span {
                                            class: "energy-badge",
                                            style: "background: {cat_color};",
                                            "{rule.category.key()}"
                                        }
                                    }
                                    td { class: "monospace", "{tags_str}" }
                                    td {
                                        span {
                                            class: "energy-badge",
                                            style: "background: {sev_color};",
                                            "{rule.severity.key()}"
                                        }
                                    }
                                    td {
                                        if is_builtin {
                                            span { class: "energy-badge", "built-in" }
                                        }
                                    }
                                    td {
                                        if enabled { "Yes" } else { "No" }
                                    }
                                    if can_manage {
                                        td {
                                            // Toggle enabled
                                            {
                                                let fs2 = fs.clone();
                                                let new_enabled = !enabled;
                                                // We need all the rule fields for update_rule
                                                let r_name = rule.name.clone();
                                                let r_desc = rule.description.clone();
                                                let r_cat = rule.category;
                                                let r_tags = rule.equip_tags.clone();
                                                let r_sev = rule.severity;
                                                let r_cond = rule.condition.clone();
                                                let r_guid = rule.guidance.clone();
                                                let r_cc = rule.confirmation_count;
                                                rsx! {
                                                    button {
                                                        class: "energy-btn energy-btn-sm",
                                                        onclick: move |evt| {
                                                            evt.stop_propagation();
                                                            let fs2 = fs2.clone();
                                                            let n = r_name.clone();
                                                            let d = r_desc.clone();
                                                            let c = r_cat;
                                                            let t = r_tags.clone();
                                                            let s = r_sev;
                                                            let co = r_cond.clone();
                                                            let g = r_guid.clone();
                                                            let cc = r_cc;
                                                            spawn(async move {
                                                                let _ = fs2.update_rule(rid, &n, &d, &c, &t, &s, &co, &g, new_enabled, cc).await;
                                                            });
                                                            version.set(ver + 1);
                                                        },
                                                        if enabled { "Disable" } else { "Enable" }
                                                    }
                                                }
                                            }
                                            if !is_builtin {
                                                {
                                                    let fs3 = fs.clone();
                                                    rsx! {
                                                        button {
                                                            class: "energy-btn energy-btn-danger energy-btn-sm",
                                                            onclick: move |evt| {
                                                                evt.stop_propagation();
                                                                let fs3 = fs3.clone();
                                                                spawn(async move { let _ = fs3.delete_rule(rid).await; });
                                                                version.set(ver + 1);
                                                            },
                                                            "Delete"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                // Expanded detail row
                                if is_expanded {
                                    tr { class: "energy-baseline-section",
                                        td { colspan: "7",
                                            div { style: "padding: 8px 12px;",
                                                p { style: "margin: 0 0 6px 0; color: #aaa;",
                                                    strong { "Description: " }
                                                    "{rule_desc}"
                                                }
                                                p { style: "margin: 0 0 6px 0; color: #aaa;",
                                                    strong { "Guidance: " }
                                                    "{rule_guidance}"
                                                }
                                                pre {
                                                    style: "margin: 0; font-size: 11px; color: #888; white-space: pre-wrap; max-height: 200px; overflow-y: auto;",
                                                    "{condition_json}"
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

// ----------------------------------------------------------------
// Bindings — table + auto-bind
// ----------------------------------------------------------------

#[component]
fn FddBindingList() -> Element {
    let state = use_context::<AppState>();
    let fdd_store = state.fdd_store.clone();
    let node_store = state.node_store.clone();
    let can_manage = state.has_permission(Permission::ManageFdd);

    let mut version = use_signal(|| 0u64);
    let ver = *version.read();
    let mut auto_bind_msg = use_signal(|| Option::<String>::None);

    let fdd_store2 = state.fdd_store.clone();
    let bindings = use_resource(move || {
        let fs = fdd_store2.clone();
        let _v = ver;
        async move { fs.list_bindings(None, None).await }
    });

    let fdd_store3 = state.fdd_store.clone();
    let rules = use_resource(move || {
        let fs = fdd_store3.clone();
        let _v = ver;
        async move { fs.list_rules().await }
    });

    let binding_list = bindings.cloned().unwrap_or_default();
    let rule_list = rules.cloned().unwrap_or_default();

    // Build a rule_id -> rule_name map
    let rule_names: std::collections::HashMap<i64, String> =
        rule_list.iter().map(|r| (r.id, r.name.clone())).collect();

    // Auto-bind handler
    let fdd_store_ab = state.fdd_store.clone();
    let node_store_ab = state.node_store.clone();
    let auto_bind = move |_| {
        let fs = fdd_store_ab.clone();
        let ns = node_store_ab.clone();
        spawn(async move {
            let rules = fs.list_rules().await;
            let existing = fs.list_bindings(None, None).await;
            // Get all equip nodes
            let equip_nodes = ns.find_by_tag("equip", None).await;
            let mut created = 0u32;
            for equip in &equip_nodes {
                let equip_tags: Vec<&str> = equip.tags.keys().map(|s| s.as_str()).collect();
                for rule in &rules {
                    if !rule.enabled {
                        continue;
                    }
                    // Check if rule's equip_tags are a subset of the equip's tags
                    let matches = rule
                        .equip_tags
                        .iter()
                        .all(|t| equip_tags.contains(&t.as_str()));
                    if !matches {
                        continue;
                    }
                    // Check if binding already exists
                    let already = existing
                        .iter()
                        .any(|b| b.rule_id == rule.id && b.equip_id == equip.id);
                    if already {
                        continue;
                    }
                    let _ = fs.create_binding(rule.id, &equip.id, true, None).await;
                    created += 1;
                }
            }
            auto_bind_msg.set(Some(format!("Created {created} new binding(s).")));
        });
        version.set(ver + 1);
    };

    rsx! {
        div { class: "energy-meters",
            h3 { "FDD Bindings" }

            if can_manage {
                div { class: "energy-form",
                    button {
                        class: "energy-btn energy-btn-primary",
                        onclick: auto_bind,
                        "Auto-Bind All"
                    }
                    if let Some(ref msg) = *auto_bind_msg.read() {
                        span { style: "margin-left: 12px; color: #66bb6a;", "{msg}" }
                    }
                }
            }

            table { class: "energy-table",
                thead {
                    tr {
                        th { "Equipment" }
                        th { "Rule" }
                        th { "Enabled" }
                        if can_manage {
                            th { "Actions" }
                        }
                    }
                }
                tbody {
                    for binding in &binding_list {
                        {
                            let bid = binding.id;
                            let rule_name = rule_names
                                .get(&binding.rule_id)
                                .cloned()
                                .unwrap_or_else(|| format!("Rule #{}", binding.rule_id));
                            let enabled = binding.enabled;
                            let fs = state.fdd_store.clone();
                            let config_ov = binding.config_overrides.clone();
                            rsx! {
                                tr {
                                    td { class: "monospace", "{binding.equip_id}" }
                                    td { "{rule_name}" }
                                    td { if enabled { "Yes" } else { "No" } }
                                    if can_manage {
                                        td {
                                            {
                                                let fs2 = fs.clone();
                                                let new_enabled = !enabled;
                                                let co = config_ov.clone();
                                                rsx! {
                                                    button {
                                                        class: "energy-btn energy-btn-sm",
                                                        onclick: move |_| {
                                                            let fs2 = fs2.clone();
                                                            let co = co.clone();
                                                            spawn(async move {
                                                                let _ = fs2.update_binding(bid, new_enabled, co.as_deref()).await;
                                                            });
                                                            version.set(ver + 1);
                                                        },
                                                        if enabled { "Disable" } else { "Enable" }
                                                    }
                                                }
                                            }
                                            {
                                                let fs3 = fs.clone();
                                                rsx! {
                                                    button {
                                                        class: "energy-btn energy-btn-danger energy-btn-sm",
                                                        onclick: move |_| {
                                                            let fs3 = fs3.clone();
                                                            spawn(async move { let _ = fs3.delete_binding(bid).await; });
                                                            version.set(ver + 1);
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
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// History — filtered fault event log
// ----------------------------------------------------------------

#[component]
fn FddHistoryView() -> Element {
    let state = use_context::<AppState>();
    let fdd_store = state.fdd_store.clone();

    let mut version = use_signal(|| 0u64);
    let ver = *version.read();

    // Filter state
    let mut filter_equip = use_signal(String::new);
    let mut filter_rule_id = use_signal(String::new);
    let mut filter_severity = use_signal(String::new);
    let mut filter_limit = use_signal(|| "100".to_string());

    // Build query from current filters
    let fdd_store2 = state.fdd_store.clone();
    let events = use_resource(move || {
        let fs = fdd_store2.clone();
        let _v = ver;
        let equip = filter_equip.read().clone();
        let rid_str = filter_rule_id.read().clone();
        let sev = filter_severity.read().clone();
        let lim_str = filter_limit.read().clone();
        async move {
            let query = FddHistoryQuery {
                equip_id: if equip.is_empty() { None } else { Some(equip) },
                rule_id: rid_str.parse::<i64>().ok(),
                severity: if sev.is_empty() { None } else { Some(sev) },
                start_ms: None,
                end_ms: None,
                limit: Some(lim_str.parse::<u32>().unwrap_or(100)),
            };
            fs.query_history(query).await
        }
    });

    let event_list = events.cloned().unwrap_or_default();

    rsx! {
        div { class: "energy-meters",
            h3 { "Fault History" }

            div { class: "energy-form",
                input {
                    class: "energy-input",
                    placeholder: "Equipment ID",
                    value: "{filter_equip}",
                    oninput: move |e| filter_equip.set(e.value()),
                }
                input {
                    class: "energy-input energy-input-sm",
                    placeholder: "Rule ID",
                    value: "{filter_rule_id}",
                    oninput: move |e| filter_rule_id.set(e.value()),
                }
                select {
                    class: "energy-select",
                    value: "{filter_severity}",
                    onchange: move |e| filter_severity.set(e.value()),
                    option { value: "", "All Severities" }
                    option { value: "info", "Info" }
                    option { value: "warning", "Warning" }
                    option { value: "critical", "Critical" }
                }
                input {
                    class: "energy-input energy-input-sm",
                    placeholder: "Limit",
                    value: "{filter_limit}",
                    oninput: move |e| filter_limit.set(e.value()),
                }
                button {
                    class: "energy-btn energy-btn-primary",
                    onclick: move |_| version.set(ver + 1),
                    "Apply"
                }
                button {
                    class: "energy-btn",
                    onclick: move |_| {
                        filter_equip.set(String::new());
                        filter_rule_id.set(String::new());
                        filter_severity.set(String::new());
                        filter_limit.set("100".to_string());
                        version.set(ver + 1);
                    },
                    "Clear"
                }
            }

            if event_list.is_empty() {
                p { class: "energy-empty-sm", "No fault history events match the current filters." }
            } else {
                table { class: "energy-table",
                    thead {
                        tr {
                            th { "Timestamp" }
                            th { "Equipment" }
                            th { "Rule ID" }
                            th { "Severity" }
                            th { "Transition" }
                            th { "Note" }
                        }
                    }
                    tbody {
                        for event in &event_list {
                            {
                                let ts = format_timestamp(event.timestamp_ms);
                                let sev_color = severity_color_str(&event.severity);
                                let note = event.note.as_deref().unwrap_or("");
                                rsx! {
                                    tr {
                                        td { class: "monospace", "{ts}" }
                                        td { class: "monospace", "{event.equip_id}" }
                                        td { "{event.rule_id}" }
                                        td {
                                            span {
                                                class: "energy-badge",
                                                style: "background: {sev_color};",
                                                "{event.severity}"
                                            }
                                        }
                                        td { "{event.from_state} → {event.to_state}" }
                                        td { class: "energy-config-preview", "{note}" }
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

// ----------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------

fn severity_color(sev: &FddSeverity) -> &'static str {
    match sev {
        FddSeverity::Info => "#2196f3",
        FddSeverity::Warning => "#ff9800",
        FddSeverity::Critical => "#f44336",
    }
}

fn severity_color_str(sev: &str) -> &'static str {
    match sev {
        "critical" => "#f44336",
        "warning" => "#ff9800",
        "info" => "#2196f3",
        _ => "#666",
    }
}

fn category_color(cat: &FddCategory) -> &'static str {
    match cat {
        FddCategory::SensorValidation => "#7c4dff",
        FddCategory::Ahu => "#00bcd4",
        FddCategory::Vav => "#4caf50",
        FddCategory::ChillerPlant => "#2196f3",
        FddCategory::HeatPump => "#ff5722",
        FddCategory::Economizer => "#8bc34a",
        FddCategory::General => "#9e9e9e",
    }
}

fn format_duration(ms: i64) -> String {
    if ms < 0 {
        return "—".to_string();
    }
    let secs = ms / 1000;
    let mins = secs / 60;
    let hours = mins / 60;
    let days = hours / 24;
    if days > 0 {
        format!("{}d {}h", days, hours % 24)
    } else if hours > 0 {
        format!("{}h {}m", hours, mins % 60)
    } else if mins > 0 {
        format!("{}m", mins)
    } else {
        format!("{}s", secs)
    }
}

fn format_timestamp(ms: i64) -> String {
    let secs = ms / 1000;
    let days_since_epoch = secs / 86400;
    let day_secs = secs % 86400;
    let (y, m, d) = days_to_ymd(days_since_epoch);
    let hh = day_secs / 3600;
    let mm = (day_secs % 3600) / 60;
    let ss = day_secs % 60;
    format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02}:{ss:02}")
}

fn days_to_ymd(mut days: i64) -> (i32, u32, u32) {
    days += 719468;
    let era = if days >= 0 { days } else { days - 146096 } / 146097;
    let doe = (days - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}
