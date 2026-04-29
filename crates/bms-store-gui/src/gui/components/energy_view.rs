use dioxus::prelude::*;

use crate::auth::Permission;
use crate::gui::state::AppState;
use bms_store_storage::store::energy_store::{EnergyMeter, StoredRollup, UtilityRate};

// ----------------------------------------------------------------
// Energy View — sub-tabbed: Dashboard | Meters | Rates | Baselines
// ----------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum EnergyTab {
    Dashboard,
    Meters,
    Rates,
    Baselines,
}

impl EnergyTab {
    fn label(&self) -> &'static str {
        match self {
            Self::Dashboard => "Dashboard",
            Self::Meters => "Meters",
            Self::Rates => "Rates",
            Self::Baselines => "Baselines",
        }
    }
}

#[component]
pub fn EnergyView() -> Element {
    let mut tab = use_signal(|| EnergyTab::Dashboard);
    let current = *tab.read();

    rsx! {
        div { class: "energy-view",
            // Sub-tab bar
            div { class: "energy-tab-bar",
                for t in [EnergyTab::Dashboard, EnergyTab::Meters, EnergyTab::Rates, EnergyTab::Baselines] {
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
                    EnergyTab::Dashboard => rsx! { EnergyDashboard {} },
                    EnergyTab::Meters => rsx! { MeterList {} },
                    EnergyTab::Rates => rsx! { RateList {} },
                    EnergyTab::Baselines => rsx! { BaselineList {} },
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Dashboard — summary cards + rollup table
// ----------------------------------------------------------------

#[component]
fn EnergyDashboard() -> Element {
    let state = use_context::<AppState>();
    let energy_store = state.energy_store.clone();

    let meters = use_resource(move || {
        let es = energy_store.clone();
        async move { es.list_meters().await }
    });

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let today_start = crate::energy::consumption::day_start_ms(now_ms);
    let week_start = today_start - 7 * 86_400_000;

    // Fetch rollups for all meters over last 7 days
    let energy_store2 = state.energy_store.clone();
    let rollups = use_resource(move || {
        let es = energy_store2.clone();
        let meter_list = meters.cloned().unwrap_or_default();
        async move {
            let mut all = Vec::new();
            for meter in &meter_list {
                let r = es
                    .query_rollups(meter.id, "daily", week_start, today_start + 86_400_000)
                    .await;
                all.extend(r);
            }
            all
        }
    });

    let meter_list = meters.cloned().unwrap_or_default();
    let rollup_list = rollups.cloned().unwrap_or_default();

    // Compute summary metrics
    let total_kwh: f64 = rollup_list.iter().map(|r| r.consumption_kwh).sum();
    let total_cost: f64 = rollup_list.iter().map(|r| r.cost).sum();
    let peak_demand = rollup_list
        .iter()
        .map(|r| r.peak_demand_kw)
        .fold(0.0f64, f64::max);
    let total_hours = rollup_list.len() as f64 * 24.0;
    let load_factor = if peak_demand > 0.0 && total_hours > 0.0 {
        (total_kwh / total_hours) / peak_demand * 100.0
    } else {
        0.0
    };

    rsx! {
        div { class: "energy-dashboard",
            // Summary cards
            div { class: "energy-summary-cards",
                div { class: "energy-card",
                    div { class: "energy-card-label", "Total Consumption (7d)" }
                    div { class: "energy-card-value", "{total_kwh:.1} kWh" }
                }
                div { class: "energy-card",
                    div { class: "energy-card-label", "Total Cost (7d)" }
                    div { class: "energy-card-value", "${total_cost:.2}" }
                }
                div { class: "energy-card",
                    div { class: "energy-card-label", "Peak Demand" }
                    div { class: "energy-card-value", "{peak_demand:.1} kW" }
                }
                div { class: "energy-card",
                    div { class: "energy-card-label", "Load Factor" }
                    div { class: "energy-card-value", "{load_factor:.1}%" }
                }
            }

            if meter_list.is_empty() {
                div { class: "energy-empty",
                    p { "No energy meters configured." }
                    p { "Go to the Meters tab to add a power measurement point as an energy meter." }
                }
            } else {
                // Daily consumption table
                h3 { "Daily Consumption (Last 7 Days)" }
                table { class: "energy-table",
                    thead {
                        tr {
                            th { "Date" }
                            th { "Meter" }
                            th { "kWh" }
                            th { "Peak kW" }
                            th { "Avg kW" }
                            th { "Cost" }
                            th { "HDD" }
                            th { "CDD" }
                        }
                    }
                    tbody {
                        for rollup in &rollup_list {
                            {
                                let date_str = format_date(rollup.period_start_ms);
                                let meter_name = meter_list
                                    .iter()
                                    .find(|m| m.id == rollup.meter_id)
                                    .map(|m| m.name.as_str())
                                    .unwrap_or("?");
                                rsx! {
                                    tr {
                                        td { "{date_str}" }
                                        td { "{meter_name}" }
                                        td { "{rollup.consumption_kwh:.1}" }
                                        td { "{rollup.peak_demand_kw:.1}" }
                                        td { "{rollup.avg_kw:.2}" }
                                        td { "${rollup.cost:.2}" }
                                        td { "{rollup.hdd:.1}" }
                                        td { "{rollup.cdd:.1}" }
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
// Meters — CRUD
// ----------------------------------------------------------------

#[component]
fn MeterList() -> Element {
    let state = use_context::<AppState>();
    let energy_store = state.energy_store.clone();
    let can_manage = state.has_permission(Permission::ManageEnergy);

    let mut version = use_signal(|| 0u64);
    let ver = *version.read();

    let meters = use_resource(move || {
        let es = energy_store.clone();
        let _v = ver; // reactive dependency
        async move { es.list_meters().await }
    });

    let energy_store2 = state.energy_store.clone();
    let rates = use_resource(move || {
        let es = energy_store2.clone();
        async move { es.list_rates().await }
    });

    let meter_list = meters.cloned().unwrap_or_default();
    let rate_list = rates.cloned().unwrap_or_default();

    // Form state
    let mut name = use_signal(String::new);
    let mut node_id = use_signal(String::new);
    let mut meter_type = use_signal(|| "electric".to_string());
    let mut unit = use_signal(|| "kW".to_string());
    let mut rate_id = use_signal(|| 0i64);

    let energy_store3 = state.energy_store.clone();
    let create_meter = move |_| {
        let es = energy_store3.clone();
        let n = name.read().clone();
        let nid = node_id.read().clone();
        let mt = meter_type.read().clone();
        let u = unit.read().clone();
        let rid = *rate_id.read();
        spawn(async move {
            let rate = if rid > 0 { Some(rid) } else { None };
            let _ = es.create_meter(&n, &nid, None, rate, &mt, &u).await;
        });
        name.set(String::new());
        node_id.set(String::new());
        version.set(ver + 1);
    };

    rsx! {
        div { class: "energy-meters",
            h3 { "Energy Meters" }

            if can_manage {
                div { class: "energy-form",
                    input {
                        class: "energy-input",
                        placeholder: "Meter name",
                        value: "{name}",
                        oninput: move |e| name.set(e.value()),
                    }
                    input {
                        class: "energy-input",
                        placeholder: "Node ID (device/point)",
                        value: "{node_id}",
                        oninput: move |e| node_id.set(e.value()),
                    }
                    select {
                        class: "energy-select",
                        value: "{meter_type}",
                        onchange: move |e| meter_type.set(e.value()),
                        option { value: "electric", "Electric" }
                        option { value: "gas", "Gas" }
                        option { value: "water", "Water" }
                        option { value: "steam", "Steam" }
                    }
                    select {
                        class: "energy-select",
                        value: "{unit}",
                        onchange: move |e| unit.set(e.value()),
                        option { value: "kW", "kW" }
                        option { value: "W", "W" }
                        option { value: "BTU/hr", "BTU/hr" }
                        option { value: "kBTU/hr", "kBTU/hr" }
                    }
                    select {
                        class: "energy-select",
                        value: "{rate_id}",
                        onchange: move |e| { let _ = e.value().parse::<i64>().map(|v| rate_id.set(v)); },
                        option { value: "0", "No rate assigned" }
                        for rate in &rate_list {
                            option { value: "{rate.id}", "{rate.name}" }
                        }
                    }
                    button {
                        class: "energy-btn energy-btn-primary",
                        onclick: create_meter,
                        "Add Meter"
                    }
                }
            }

            table { class: "energy-table",
                thead {
                    tr {
                        th { "Name" }
                        th { "Node ID" }
                        th { "Type" }
                        th { "Unit" }
                        th { "Rate" }
                        if can_manage {
                            th { "Actions" }
                        }
                    }
                }
                tbody {
                    for meter in &meter_list {
                        {
                            let rate_name = meter
                                .utility_rate_id
                                .and_then(|rid| rate_list.iter().find(|r| r.id == rid))
                                .map(|r| r.name.as_str())
                                .unwrap_or("—");
                            let mid = meter.id;
                            let es = state.energy_store.clone();
                            rsx! {
                                tr {
                                    td { "{meter.name}" }
                                    td { class: "monospace", "{meter.node_id}" }
                                    td { "{meter.meter_type}" }
                                    td { "{meter.unit}" }
                                    td { "{rate_name}" }
                                    if can_manage {
                                        td {
                                            button {
                                                class: "energy-btn energy-btn-danger",
                                                onclick: move |_| {
                                                    let es = es.clone();
                                                    spawn(async move { let _ = es.delete_meter(mid).await; });
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

// ----------------------------------------------------------------
// Rates — CRUD
// ----------------------------------------------------------------

#[component]
fn RateList() -> Element {
    let state = use_context::<AppState>();
    let energy_store = state.energy_store.clone();
    let can_manage = state.has_permission(Permission::ManageEnergy);

    let mut version = use_signal(|| 0u64);
    let ver = *version.read();

    let rates = use_resource(move || {
        let es = energy_store.clone();
        let _v = ver;
        async move { es.list_rates().await }
    });

    let rate_list = rates.cloned().unwrap_or_default();

    // Form state
    let mut name = use_signal(String::new);
    let mut rate_type = use_signal(|| "flat".to_string());
    let mut energy_rate = use_signal(|| "0.12".to_string());
    let mut demand_rate = use_signal(|| "0".to_string());
    let mut currency = use_signal(|| "USD".to_string());

    let energy_store2 = state.energy_store.clone();
    let create_rate = move |_| {
        let es = energy_store2.clone();
        let n = name.read().clone();
        let rt = rate_type.read().clone();
        let er: f64 = energy_rate.read().parse().unwrap_or(0.12);
        let dr: f64 = demand_rate.read().parse().unwrap_or(0.0);
        let cur = currency.read().clone();
        // Serialize the correct RateConfig shape for each rate type.
        let config = match rt.as_str() {
            "tou" => serde_json::json!({
                "type": "tou",
                "periods": [
                    {"name": "on_peak", "rate": er, "weekday_start_hour": 12, "weekday_end_hour": 20, "weekend": false},
                    {"name": "off_peak", "rate": er * 0.5, "weekday_start_hour": 0, "weekday_end_hour": 12, "weekend": false},
                    {"name": "weekend", "rate": er * 0.6, "weekday_start_hour": 0, "weekday_end_hour": 24, "weekend": true},
                ],
                "demand_rate": dr,
            }),
            "tiered" => serde_json::json!({
                "type": "tiered",
                "tiers": [
                    {"up_to_kwh": 500.0, "rate": er * 0.8},
                    {"up_to_kwh": 1000.0, "rate": er},
                    {"up_to_kwh": 1e18, "rate": er * 1.5},
                ],
                "demand_rate": dr,
            }),
            "demand" => serde_json::json!({
                "type": "demand",
                "energy_rate": er,
                "demand_tiers": [
                    {"up_to_kw": 100.0, "rate": dr},
                    {"up_to_kw": 1e18, "rate": dr * 1.5},
                ],
                "ratchet_pct": 0.0,
            }),
            _ => serde_json::json!({
                "type": "flat",
                "energy_rate": er,
                "demand_rate": dr,
            }),
        }
        .to_string();
        spawn(async move {
            let _ = es.create_rate(&n, &rt, &config, &cur).await;
        });
        name.set(String::new());
        version.set(ver + 1);
    };

    rsx! {
        div { class: "energy-rates",
            h3 { "Utility Rates" }

            if can_manage {
                div { class: "energy-form",
                    input {
                        class: "energy-input",
                        placeholder: "Rate name",
                        value: "{name}",
                        oninput: move |e| name.set(e.value()),
                    }
                    select {
                        class: "energy-select",
                        value: "{rate_type}",
                        onchange: move |e| rate_type.set(e.value()),
                        option { value: "flat", "Flat Rate" }
                        option { value: "tou", "Time-of-Use" }
                        option { value: "tiered", "Tiered" }
                        option { value: "demand", "Demand-Based" }
                    }
                    input {
                        class: "energy-input energy-input-sm",
                        placeholder: "$/kWh",
                        value: "{energy_rate}",
                        oninput: move |e| energy_rate.set(e.value()),
                    }
                    input {
                        class: "energy-input energy-input-sm",
                        placeholder: "$/kW demand",
                        value: "{demand_rate}",
                        oninput: move |e| demand_rate.set(e.value()),
                    }
                    select {
                        class: "energy-select energy-select-sm",
                        value: "{currency}",
                        onchange: move |e| currency.set(e.value()),
                        option { value: "USD", "USD" }
                        option { value: "EUR", "EUR" }
                        option { value: "GBP", "GBP" }
                        option { value: "CAD", "CAD" }
                    }
                    button {
                        class: "energy-btn energy-btn-primary",
                        onclick: create_rate,
                        "Add Rate"
                    }
                }
            }

            table { class: "energy-table",
                thead {
                    tr {
                        th { "Name" }
                        th { "Type" }
                        th { "Currency" }
                        th { "Config" }
                        if can_manage {
                            th { "Actions" }
                        }
                    }
                }
                tbody {
                    for rate in &rate_list {
                        {
                            let rid = rate.id;
                            let es = state.energy_store.clone();
                            let config_preview = rate.config.chars().take(60).collect::<String>();
                            rsx! {
                                tr {
                                    td { "{rate.name}" }
                                    td {
                                        span { class: "energy-badge", "{rate.rate_type}" }
                                    }
                                    td { "{rate.currency}" }
                                    td { class: "monospace energy-config-preview", "{config_preview}" }
                                    if can_manage {
                                        td {
                                            button {
                                                class: "energy-btn energy-btn-danger",
                                                onclick: move |_| {
                                                    let es = es.clone();
                                                    spawn(async move { let _ = es.delete_rate(rid).await; });
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

// ----------------------------------------------------------------
// Baselines — list + create
// ----------------------------------------------------------------

#[component]
fn BaselineList() -> Element {
    let state = use_context::<AppState>();
    let energy_store = state.energy_store.clone();
    let can_manage = state.has_permission(Permission::ManageEnergy);

    let meters = use_resource(move || {
        let es = energy_store.clone();
        async move { es.list_meters().await }
    });

    let meter_list = meters.cloned().unwrap_or_default();

    rsx! {
        div { class: "energy-baselines",
            h3 { "Energy Baselines" }

            if meter_list.is_empty() {
                p { "Add energy meters first to create baselines." }
            } else {
                for meter in &meter_list {
                    {
                        let mid = meter.id;
                        let mname = meter.name.clone();
                        rsx! {
                            BaselineMeterSection { meter_id: mid, meter_name: mname, can_manage: can_manage }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn BaselineMeterSection(meter_id: i64, meter_name: String, can_manage: bool) -> Element {
    let state = use_context::<AppState>();
    let energy_store = state.energy_store.clone();

    let mut version = use_signal(|| 0u64);
    let ver = *version.read();
    let mid = meter_id;

    let baselines = use_resource(move || {
        let es = energy_store.clone();
        let _v = ver;
        async move { es.list_baselines(mid).await }
    });

    let baseline_list = baselines.cloned().unwrap_or_default();

    // Create form state
    let mut bl_name = use_signal(String::new);
    let mut bl_type = use_signal(|| "degree_day".to_string());
    let mut bl_days = use_signal(|| "90".to_string());

    let energy_store2 = state.energy_store.clone();
    let create_baseline = move |_| {
        let es = energy_store2.clone();
        let n = bl_name.read().clone();
        let bt = bl_type.read().clone();
        let days: i64 = bl_days.read().parse().unwrap_or(90);
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        let end = crate::energy::consumption::day_start_ms(now_ms);
        let start = end - days * 86_400_000;
        let config = serde_json::json!({
            "base_temp_f": 65.0,
            "period_days": days,
        })
        .to_string();
        spawn(async move {
            let _ = es.create_baseline(mid, &n, &bt, &config, start, end).await;
        });
        bl_name.set(String::new());
        version.set(ver + 1);
    };

    rsx! {
        div { class: "energy-baseline-section",
            h4 { "{meter_name}" }

            if can_manage {
                div { class: "energy-form energy-form-inline",
                    input {
                        class: "energy-input",
                        placeholder: "Baseline name",
                        value: "{bl_name}",
                        oninput: move |e| bl_name.set(e.value()),
                    }
                    select {
                        class: "energy-select",
                        value: "{bl_type}",
                        onchange: move |e| bl_type.set(e.value()),
                        option { value: "degree_day", "Degree-Day" }
                        option { value: "fixed", "Fixed Period" }
                        option { value: "schedule", "Schedule-Based" }
                    }
                    input {
                        class: "energy-input energy-input-sm",
                        placeholder: "Days back",
                        value: "{bl_days}",
                        oninput: move |e| bl_days.set(e.value()),
                    }
                    button {
                        class: "energy-btn energy-btn-primary energy-btn-sm",
                        onclick: create_baseline,
                        "Create"
                    }
                }
            }

            if baseline_list.is_empty() {
                p { class: "energy-empty-sm", "No baselines defined for this meter." }
            }

            for bl in &baseline_list {
                {
                    let bid = bl.id;
                    let es = state.energy_store.clone();
                    rsx! {
                        div { class: "energy-baseline-row",
                            span { class: "energy-baseline-name", "{bl.name}" }
                            span { class: "energy-badge", "{bl.baseline_type}" }
                            span { class: "energy-baseline-period",
                                "{format_date(bl.start_ms)} — {format_date(bl.end_ms)}"
                            }
                            if can_manage {
                                button {
                                    class: "energy-btn energy-btn-danger energy-btn-sm",
                                    onclick: move |_| {
                                        let es = es.clone();
                                        spawn(async move { let _ = es.delete_baseline(bid).await; });
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

// ----------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------

fn format_date(ms: i64) -> String {
    let secs = ms / 1000;
    let days_since_epoch = secs / 86400;
    // Simple date formatting without chrono.
    let (y, m, d) = days_to_ymd(days_since_epoch);
    format!("{y:04}-{m:02}-{d:02}")
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
