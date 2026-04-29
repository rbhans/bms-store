use dioxus::prelude::*;

use crate::gui::state::AppState;
use bms_store_storage::weather::config::{TemperatureUnit, WeatherConfig};
use bms_store_storage::weather::model::*;

#[component]
pub fn WeatherView() -> Element {
    let state = use_context::<AppState>();
    let weather = state.weather_data.read().clone();
    let weather_svc = state.weather_service.clone();
    let mut settings_open = use_signal(|| false);
    let mut saving = use_signal(|| false);

    let config_res = use_resource(move || {
        let svc = weather_svc.clone();
        async move { svc.config().await }
    });
    let config = config_res.read().clone().unwrap_or_default();
    let temp_unit = config.temperature_unit;
    let has_location = config.location.is_some();

    rsx! {
        div { class: "weather-view",
            // Header with title, refresh button, and settings toggle
            div { class: "weather-view-header",
                h2 { "Weather" }
                div { class: "weather-header-actions",
                    RefreshButton {}
                    button {
                        class: if *settings_open.read() { "toolbar-btn active" } else { "toolbar-btn" },
                        title: "Settings",
                        onclick: move |_| settings_open.toggle(),
                        svg {
                            width: "18",
                            height: "18",
                            view_box: "0 0 24 24",
                            fill: "currentColor",
                            path { d: "M19.14 12.94c.04-.3.06-.61.06-.94 0-.32-.02-.64-.07-.94l2.03-1.58a.49.49 0 00.12-.61l-1.92-3.32a.49.49 0 00-.59-.22l-2.39.96c-.5-.38-1.03-.7-1.62-.94l-.36-2.54a.484.484 0 00-.48-.41h-3.84c-.24 0-.43.17-.47.41l-.36 2.54c-.59.24-1.13.57-1.62.94l-2.39-.96a.49.49 0 00-.59.22L2.74 8.87c-.12.21-.08.47.12.61l2.03 1.58c-.05.3-.07.62-.07.94s.02.64.07.94l-2.03 1.58a.49.49 0 00-.12.61l1.92 3.32c.12.22.37.29.59.22l2.39-.96c.5.38 1.03.7 1.62.94l.36 2.54c.05.24.24.41.48.41h3.84c.24 0 .44-.17.47-.41l.36-2.54c.59-.24 1.13-.56 1.62-.94l2.39.96c.22.08.47 0 .59-.22l1.92-3.32c.12-.22.07-.47-.12-.61l-2.01-1.58zM12 15.6A3.6 3.6 0 1112 8.4a3.6 3.6 0 010 7.2z" }
                        }
                    }
                }
            }

            if *settings_open.read() {
                WeatherSettings {
                    on_save: move |pending: PendingSave| {
                        settings_open.set(false);
                        saving.set(true);
                        // Spawn from WeatherView scope so it survives settings unmount
                        let svc = state.weather_service.clone();
                        let paths = state.project_paths.clone();
                        let mut weather_data = state.weather_data;
                        spawn(async move {
                            // Auto-geocode if the user typed a city but didn't click Lookup
                            let final_location = if pending.location.is_none() && !pending.zip_code.is_empty() {
                                svc.geocode_zip(&pending.zip_code).await.ok()
                            } else {
                                pending.location
                            };

                            let config = WeatherConfig {
                                location: final_location,
                                zip_code: pending.zip_code,
                                openweathermap_api_key: pending.owm_key,
                                weatherapi_api_key: pending.wa_key,
                                visual_crossing_api_key: pending.vc_key,
                                enabled_sources: pending.enabled_sources,
                                temperature_unit: pending.temp_unit,
                                ..WeatherConfig::default()
                            };

                            config.save(&paths.data_dir);
                            svc.update_config(config).await;
                            svc.force_refresh().await;
                            if let Some(data) = svc.latest().await {
                                weather_data.set(Some(data));
                            }
                            saving.set(false);
                        });
                    },
                    on_cancel: move |_| settings_open.set(false),
                }
            }

            if *saving.read() {
                div { class: "view-placeholder",
                    h3 { "Fetching weather..." }
                }
            } else if let Some(data) = weather {
                div { class: "weather-content",
                    CurrentConditionsPanel { data: data.clone(), temp_unit }

                    // Hourly forecast chart (48h)
                    if !data.hourly.is_empty() {
                        HourlyChart { hourly: data.hourly.clone(), temp_unit }
                    }

                    // Daily forecast list (7d)
                    if !data.daily.is_empty() {
                        DailyForecastList { daily: data.daily.clone(), temp_unit }
                    }

                    // Source status
                    SourceStatus { data: data.clone() }
                }
            } else if !has_location {
                div { class: "view-placeholder",
                    h3 { "No Location Configured" }
                    p { "Open Settings to enter your location coordinates." }
                }
            } else {
                div { class: "view-placeholder",
                    h3 { "Loading weather data..." }
                }
            }
        }
    }
}

#[component]
fn RefreshButton() -> Element {
    let state = use_context::<AppState>();
    let mut refreshing = use_signal(|| false);

    rsx! {
        button {
            class: "toolbar-btn",
            title: "Refresh Now",
            disabled: *refreshing.read(),
            onclick: move |_| {
                let svc = state.weather_service.clone();
                let mut weather_data = state.weather_data;
                refreshing.set(true);
                spawn(async move {
                    svc.force_refresh().await;
                    if let Some(data) = svc.latest().await {
                        weather_data.set(Some(data));
                    }
                    refreshing.set(false);
                });
            },
            svg {
                width: "18",
                height: "18",
                view_box: "0 0 24 24",
                fill: "currentColor",
                path { d: "M17.65 6.35A7.958 7.958 0 0012 4c-4.42 0-7.99 3.58-7.99 8s3.57 8 7.99 8c3.73 0 6.84-2.55 7.73-6h-2.08A5.99 5.99 0 0112 18c-3.31 0-6-2.69-6-6s2.69-6 6-6c1.66 0 3.14.69 4.22 1.78L13 11h7V4l-2.35 2.35z" }
            }
        }
    }
}

#[component]
fn CurrentConditionsPanel(data: WeatherData, temp_unit: TemperatureUnit) -> Element {
    let c = &data.current;
    let temp = temp_unit.convert(c.temperature.avg);
    let feels = temp_unit.convert(c.feels_like.avg);
    let loc_name = data.location.name.as_deref().unwrap_or("Current Location");

    rsx! {
        div { class: "weather-current-panel",
            div { class: "weather-current-main",
                svg {
                    class: "weather-current-icon",
                    width: "48",
                    height: "48",
                    view_box: "0 0 24 24",
                    fill: "currentColor",
                    path { d: "{c.condition.icon_path()}" }
                }
                div { class: "weather-current-temp",
                    span { class: "weather-temp-value", "{temp:.0}" }
                    span { class: "weather-temp-unit", "{temp_unit.suffix()}" }
                }
                div { class: "weather-current-condition",
                    span { class: "weather-condition-text", "{c.condition.label()}" }
                    span { class: "weather-location-text", "{loc_name}" }
                }
            }
            div { class: "weather-current-details",
                WeatherDetail { label: "Feels Like", value: format!("{feels:.0}{}", temp_unit.suffix()) }
                WeatherDetail { label: "Humidity", value: format!("{:.0}%", c.humidity.avg) }
                WeatherDetail { label: "Wind", value: format!("{:.0} km/h", c.wind_speed.avg) }
                WeatherDetail { label: "Pressure", value: format!("{:.0} hPa", c.pressure.avg) }
                if let Some(ref uv) = c.uv_index {
                    WeatherDetail { label: "UV Index", value: format!("{:.0}", uv.avg) }
                }
                WeatherDetail {
                    label: "Sources",
                    value: format!("{}", data.sources_available.len()),
                }
            }
            if c.temperature.source_count > 1 {
                div { class: "weather-source-range",
                    "Temperature range across sources: {temp_unit.convert(c.temperature.min):.0}–{temp_unit.convert(c.temperature.max):.0}{temp_unit.suffix()}"
                }
            }
        }
    }
}

#[component]
fn WeatherDetail(label: &'static str, value: String) -> Element {
    rsx! {
        div { class: "weather-detail-item",
            span { class: "weather-detail-label", "{label}" }
            span { class: "weather-detail-value", "{value}" }
        }
    }
}

#[component]
fn HourlyChart(hourly: Vec<HourlyForecast>, temp_unit: TemperatureUnit) -> Element {
    let w = 900.0f64;
    let h = 220.0f64;
    let pad_l = 45.0;
    let pad_r = 10.0;
    let pad_t = 20.0;
    let pad_b = 40.0;
    let chart_w = w - pad_l - pad_r;
    let chart_h = h - pad_t - pad_b;

    let n = hourly.len();
    if n < 2 {
        return rsx! {};
    }

    // Temperature range
    let all_min: f64 = hourly
        .iter()
        .map(|h| temp_unit.convert(h.temperature.min))
        .fold(f64::INFINITY, f64::min);
    let all_max: f64 = hourly
        .iter()
        .map(|h| temp_unit.convert(h.temperature.max))
        .fold(f64::NEG_INFINITY, f64::max);
    let range = (all_max - all_min).max(1.0);
    let y_min = all_min - range * 0.1;
    let y_max = all_max + range * 0.1;
    let y_range = y_max - y_min;

    // Build SVG paths
    let mut avg_points = String::new();
    let mut min_poly = String::new();
    let mut max_poly = String::new();

    for (i, h_data) in hourly.iter().enumerate() {
        let x = pad_l + (i as f64 / (n - 1) as f64) * chart_w;
        let avg = temp_unit.convert(h_data.temperature.avg);
        let min = temp_unit.convert(h_data.temperature.min);
        let max = temp_unit.convert(h_data.temperature.max);

        let y_avg = pad_t + (1.0 - (avg - y_min) / y_range) * chart_h;
        let y_min_pt = pad_t + (1.0 - (min - y_min) / y_range) * chart_h;
        let y_max_pt = pad_t + (1.0 - (max - y_min) / y_range) * chart_h;

        if i == 0 {
            avg_points.push_str(&format!("M{x:.1},{y_avg:.1}"));
            min_poly.push_str(&format!("M{x:.1},{y_min_pt:.1}"));
            max_poly.push_str(&format!("M{x:.1},{y_max_pt:.1}"));
        } else {
            avg_points.push_str(&format!(" L{x:.1},{y_avg:.1}"));
            min_poly.push_str(&format!(" L{x:.1},{y_min_pt:.1}"));
            max_poly.push_str(&format!(" L{x:.1},{y_max_pt:.1}"));
        }
    }

    // Shade band between min and max (closed polygon)
    let mut band = String::new();
    // Forward along max
    for (i, h_data) in hourly.iter().enumerate() {
        let x = pad_l + (i as f64 / (n - 1) as f64) * chart_w;
        let max = temp_unit.convert(h_data.temperature.max);
        let y = pad_t + (1.0 - (max - y_min) / y_range) * chart_h;
        if i == 0 {
            band.push_str(&format!("M{x:.1},{y:.1}"));
        } else {
            band.push_str(&format!(" L{x:.1},{y:.1}"));
        }
    }
    // Reverse along min
    for (i, h_data) in hourly.iter().enumerate().rev() {
        let x = pad_l + (i as f64 / (n - 1) as f64) * chart_w;
        let min = temp_unit.convert(h_data.temperature.min);
        let y = pad_t + (1.0 - (min - y_min) / y_range) * chart_h;
        band.push_str(&format!(" L{x:.1},{y:.1}"));
    }
    band.push_str(" Z");

    // X-axis labels (every 6 hours)
    let x_labels: Vec<(f64, String)> = hourly
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 6 == 0)
        .map(|(i, h_data)| {
            let x = pad_l + (i as f64 / (n - 1) as f64) * chart_w;
            let hour = ((h_data.hour_ms / 3_600_000) % 24) as u32;
            let label = if hour == 0 {
                "12a".to_string()
            } else if hour < 12 {
                format!("{hour}a")
            } else if hour == 12 {
                "12p".to_string()
            } else {
                format!("{}p", hour - 12)
            };
            (x, label)
        })
        .collect();

    // Y-axis labels
    let y_step = nice_step(y_range, 4);
    let y_start = (y_min / y_step).ceil() * y_step;
    let mut y_labels = Vec::new();
    let mut y_val = y_start;
    while y_val <= y_max {
        let y = pad_t + (1.0 - (y_val - y_min) / y_range) * chart_h;
        y_labels.push((y, format!("{y_val:.0}")));
        y_val += y_step;
    }

    rsx! {
        div { class: "weather-section",
            h3 { class: "weather-section-title", "48-Hour Forecast" }
            div { class: "weather-chart-scroll",
                svg {
                    class: "weather-hourly-chart",
                    width: "{w}",
                    height: "{h}",
                    view_box: "0 0 {w} {h}",

                    // Grid lines
                    for (y, _label) in &y_labels {
                        line {
                            x1: "{pad_l}",
                            y1: "{y}",
                            x2: "{w - pad_r}",
                            y2: "{y}",
                            stroke: "#333",
                            stroke_width: "0.5",
                        }
                    }

                    // Shaded band (min–max across sources)
                    path {
                        d: "{band}",
                        fill: "#5B9BD5",
                        fill_opacity: "0.15",
                    }

                    // Average temperature line
                    path {
                        d: "{avg_points}",
                        fill: "none",
                        stroke: "#5B9BD5",
                        stroke_width: "2",
                    }

                    // Y-axis labels
                    for (y, label) in &y_labels {
                        text {
                            x: "{pad_l - 5.0}",
                            y: "{y + 4.0}",
                            text_anchor: "end",
                            fill: "#999",
                            font_size: "11",
                            "{label}"
                        }
                    }

                    // X-axis labels
                    for (x, label) in &x_labels {
                        text {
                            x: "{x}",
                            y: "{h - pad_b + 18.0}",
                            text_anchor: "middle",
                            fill: "#999",
                            font_size: "11",
                            "{label}"
                        }
                    }
                }
            }
        }
    }
}

fn nice_step(range: f64, target_ticks: usize) -> f64 {
    let raw = range / target_ticks as f64;
    let mag = 10.0f64.powf(raw.log10().floor());
    let norm = raw / mag;
    let step = if norm <= 1.5 {
        1.0
    } else if norm <= 3.5 {
        2.0
    } else if norm <= 7.5 {
        5.0
    } else {
        10.0
    };
    step * mag
}

#[component]
fn DailyForecastList(daily: Vec<DailyForecast>, temp_unit: TemperatureUnit) -> Element {
    rsx! {
        div { class: "weather-section",
            h3 { class: "weather-section-title", "7-Day Forecast" }
            div { class: "weather-daily-list",
                for d in &daily {
                    DailyRow { data: d.clone(), temp_unit }
                }
            }
        }
    }
}

#[component]
fn DailyRow(data: DailyForecast, temp_unit: TemperatureUnit) -> Element {
    let high = temp_unit.convert(data.temp_high.avg);
    let low = temp_unit.convert(data.temp_low.avg);
    let precip = data.precip_probability.avg;

    // Simple day name from timestamp
    let day_name = day_of_week(data.date_ms);

    rsx! {
        div { class: "weather-daily-row",
            span { class: "weather-daily-day", "{day_name}" }
            svg {
                class: "weather-daily-icon",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "currentColor",
                path { d: "{data.condition.icon_path()}" }
            }
            span { class: "weather-daily-low", "{low:.0}{temp_unit.suffix()}" }
            div { class: "weather-daily-bar-wrap",
                div {
                    class: "weather-daily-bar",
                    // Bar width represents the temperature range
                }
            }
            span { class: "weather-daily-high", "{high:.0}{temp_unit.suffix()}" }
            if precip > 5.0 {
                span { class: "weather-daily-precip", "{precip:.0}%" }
            }
            span { class: "weather-daily-condition", "{data.condition.label()}" }
        }
    }
}

fn day_of_week(ms: i64) -> &'static str {
    // Days since epoch mod 7. 1970-01-01 was Thursday (4).
    let days = ms / 86_400_000;
    let dow = ((days % 7) + 4) % 7; // 0=Sun, 1=Mon, ...
    match dow {
        0 => "Sun",
        1 => "Mon",
        2 => "Tue",
        3 => "Wed",
        4 => "Thu",
        5 => "Fri",
        6 => "Sat",
        _ => "???",
    }
}

#[component]
fn SourceStatus(data: WeatherData) -> Element {
    let mut expanded = use_signal(|| false);

    rsx! {
        div { class: "weather-section weather-source-section",
            button {
                class: "weather-source-toggle",
                onclick: move |_| expanded.toggle(),
                if *expanded.read() {
                    "Source Status ▾"
                } else {
                    "Source Status ▸"
                }
            }

            if *expanded.read() {
                div { class: "weather-source-list",
                    for source in &data.sources_available {
                        div { class: "weather-source-item success",
                            span { class: "weather-source-dot success" }
                            span { "{source.label()}" }
                        }
                    }
                    for (source, error) in &data.sources_failed {
                        div { class: "weather-source-item failed",
                            span { class: "weather-source-dot failed" }
                            span { "{source.label()}" }
                            span { class: "weather-source-error", "{error}" }
                        }
                    }

                    div { class: "weather-source-time",
                        "Last updated: {format_time(data.last_updated_ms)}"
                    }
                }
            }
        }
    }
}

fn format_time(ms: i64) -> String {
    let secs = ms / 1000;
    let hours = (secs / 3600) % 24;
    let minutes = (secs / 60) % 60;
    format!("{hours:02}:{minutes:02} UTC")
}

// ---- Settings panel ----

/// Data emitted by WeatherSettings on save — parent handles async work.
#[derive(Clone)]
struct PendingSave {
    location: Option<WeatherLocation>,
    zip_code: String,
    owm_key: Option<String>,
    wa_key: Option<String>,
    vc_key: Option<String>,
    enabled_sources: Vec<WeatherSource>,
    temp_unit: TemperatureUnit,
}

#[component]
fn WeatherSettings(on_save: EventHandler<PendingSave>, on_cancel: EventHandler<()>) -> Element {
    let state = use_context::<AppState>();
    let weather_svc = state.weather_service.clone();

    let config_res = use_resource(move || {
        let svc = weather_svc.clone();
        async move { svc.config().await }
    });
    let initial_config = config_res.read().clone().unwrap_or_default();

    let mut zip_code = use_signal(|| initial_config.zip_code.clone());
    let mut geo_status = use_signal(|| Option::<String>::None);
    let mut geo_error = use_signal(|| Option::<String>::None);
    let mut resolved_location = use_signal(|| initial_config.location.clone());
    let mut owm_key = use_signal(|| {
        initial_config
            .openweathermap_api_key
            .clone()
            .unwrap_or_default()
    });
    let mut wa_key = use_signal(|| {
        initial_config
            .weatherapi_api_key
            .clone()
            .unwrap_or_default()
    });
    let mut vc_key = use_signal(|| {
        initial_config
            .visual_crossing_api_key
            .clone()
            .unwrap_or_default()
    });
    let mut temp_unit = use_signal(|| initial_config.temperature_unit);

    // Source toggles
    let open_meteo = use_signal(|| {
        initial_config
            .enabled_sources
            .contains(&WeatherSource::OpenMeteo)
    });
    let nws = use_signal(|| initial_config.enabled_sources.contains(&WeatherSource::Nws));
    let owm = use_signal(|| {
        initial_config
            .enabled_sources
            .contains(&WeatherSource::OpenWeatherMap)
    });
    let wa = use_signal(|| {
        initial_config
            .enabled_sources
            .contains(&WeatherSource::WeatherApi)
    });
    let vc = use_signal(|| {
        initial_config
            .enabled_sources
            .contains(&WeatherSource::VisualCrossing)
    });

    let lookup_svc = state.weather_service.clone();

    rsx! {
        div { class: "weather-settings",
            h3 { "Weather Settings" }

            div { class: "weather-settings-group",
                label { "Location" }
                div { class: "weather-settings-row",
                    input {
                        class: "input-bg input-border",
                        r#type: "text",
                        placeholder: "Zip code or city name",
                        value: "{zip_code.read()}",
                        oninput: move |e| {
                            zip_code.set(e.value());
                            geo_error.set(None);
                            geo_status.set(None);
                        },
                    }
                    button {
                        class: "btn",
                        disabled: zip_code.read().is_empty(),
                        onclick: move |_| {
                            let svc = lookup_svc.clone();
                            let zip = zip_code.read().clone();
                            geo_status.set(Some("Looking up...".to_string()));
                            geo_error.set(None);
                            spawn(async move {
                                match svc.geocode_zip(&zip).await {
                                    Ok(loc) => {
                                        let label = loc.name.clone().unwrap_or_else(|| format!("{:.2}, {:.2}", loc.lat, loc.lon));
                                        geo_status.set(Some(label));
                                        resolved_location.set(Some(loc));
                                    }
                                    Err(e) => {
                                        geo_status.set(None);
                                        geo_error.set(Some(e));
                                    }
                                }
                            });
                        },
                        "Lookup"
                    }
                }
                if let Some(ref status) = *geo_status.read() {
                    span { class: "weather-geo-status", "{status}" }
                }
                if let Some(ref err) = *geo_error.read() {
                    span { class: "weather-geo-error", "{err}" }
                }
                if let Some(ref loc) = *resolved_location.read() {
                    span { class: "weather-geo-coords",
                        "{loc.lat:.4}, {loc.lon:.4}"
                    }
                }
            }

            div { class: "weather-settings-group",
                label { "Temperature Unit" }
                div { class: "weather-settings-row",
                    button {
                        class: if *temp_unit.read() == TemperatureUnit::Fahrenheit { "btn btn-primary" } else { "btn" },
                        onclick: move |_| temp_unit.set(TemperatureUnit::Fahrenheit),
                        "Fahrenheit"
                    }
                    button {
                        class: if *temp_unit.read() == TemperatureUnit::Celsius { "btn btn-primary" } else { "btn" },
                        onclick: move |_| temp_unit.set(TemperatureUnit::Celsius),
                        "Celsius"
                    }
                }
            }

            div { class: "weather-settings-group",
                label { "Sources" }
                SourceToggle { label: "Open-Meteo (no key required)", checked: open_meteo }
                SourceToggle { label: "NWS (US only, no key)", checked: nws }
                SourceToggle { label: "OpenWeatherMap", checked: owm }
                if *owm.read() {
                    input {
                        class: "input-bg input-border",
                        r#type: "text",
                        placeholder: "OpenWeatherMap API Key",
                        value: "{owm_key.read()}",
                        oninput: move |e| owm_key.set(e.value()),
                    }
                }
                SourceToggle { label: "WeatherAPI", checked: wa }
                if *wa.read() {
                    input {
                        class: "input-bg input-border",
                        r#type: "text",
                        placeholder: "WeatherAPI Key",
                        value: "{wa_key.read()}",
                        oninput: move |e| wa_key.set(e.value()),
                    }
                }
                SourceToggle { label: "Visual Crossing", checked: vc }
                if *vc.read() {
                    input {
                        class: "input-bg input-border",
                        r#type: "text",
                        placeholder: "Visual Crossing API Key",
                        value: "{vc_key.read()}",
                        oninput: move |e| vc_key.set(e.value()),
                    }
                }
            }

            div { class: "weather-settings-actions",
                button {
                    class: "btn btn-primary",
                    onclick: move |_| {
                        let mut enabled = Vec::new();
                        if *open_meteo.read() { enabled.push(WeatherSource::OpenMeteo); }
                        if *nws.read() { enabled.push(WeatherSource::Nws); }
                        if *owm.read() { enabled.push(WeatherSource::OpenWeatherMap); }
                        if *wa.read() { enabled.push(WeatherSource::WeatherApi); }
                        if *vc.read() { enabled.push(WeatherSource::VisualCrossing); }

                        on_save.call(PendingSave {
                            location: resolved_location.read().clone(),
                            zip_code: zip_code.read().clone(),
                            owm_key: {
                                let k = owm_key.read().clone();
                                if k.is_empty() { None } else { Some(k) }
                            },
                            wa_key: {
                                let k = wa_key.read().clone();
                                if k.is_empty() { None } else { Some(k) }
                            },
                            vc_key: {
                                let k = vc_key.read().clone();
                                if k.is_empty() { None } else { Some(k) }
                            },
                            enabled_sources: enabled,
                            temp_unit: *temp_unit.read(),
                        });
                    },
                    "Save"
                }
                button {
                    class: "btn",
                    onclick: move |_| on_cancel.call(()),
                    "Cancel"
                }
            }
        }
    }
}

#[component]
fn SourceToggle(label: &'static str, checked: Signal<bool>) -> Element {
    let is_checked = *checked.read();
    rsx! {
        label { class: "weather-source-check",
            input {
                r#type: "checkbox",
                checked: is_checked,
                onchange: move |e| checked.set(e.checked()),
            }
            span { "{label}" }
        }
    }
}
