use dioxus::prelude::*;

use crate::gui::state::{ActiveView, AppState};
use bms_store_storage::weather::config::TemperatureUnit;

/// Compact weather display for the toolbar: condition icon + temperature + humidity.
/// Clickable to navigate to the full Weather view.
/// Hidden when no location is configured.
#[component]
pub fn WeatherWidget() -> Element {
    let mut state = use_context::<AppState>();
    let weather = state.weather_data.read().clone();

    let Some(data) = weather else {
        return rsx! {};
    };

    let weather_svc = state.weather_service.clone();
    let unit = use_resource(move || {
        let svc = weather_svc.clone();
        async move { svc.config().await.temperature_unit }
    });
    let temp_unit = unit.read().clone().unwrap_or(TemperatureUnit::Fahrenheit);

    let temp = temp_unit.convert(data.current.temperature.avg);
    let humidity = data.current.humidity.avg;
    let condition = data.current.condition;
    let icon_path = condition.icon_path();
    let source_count = data.sources_available.len();

    rsx! {
        button {
            class: "weather-widget-btn",
            title: "{condition.label()} — {source_count} source(s)",
            onclick: move |_| {
                state.active_view.set(ActiveView::Weather);
                state.selected_point.set(None);
                state.detail_open.set(false);
            },

            svg {
                class: "weather-widget-icon",
                width: "18",
                height: "18",
                view_box: "0 0 24 24",
                fill: "currentColor",
                path { d: "{icon_path}" }
            }

            span { class: "weather-widget-temp",
                "{temp:.0}{temp_unit.suffix()}"
            }

            span { class: "weather-widget-humidity",
                "{humidity:.0}%"
            }
        }
    }
}
