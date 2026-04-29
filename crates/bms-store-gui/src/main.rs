//! bms-store-gui desktop launcher.
//!
//! All logic lives in the library crate (`bms_store_gui`); this binary only
//! initializes tracing and launches the Dioxus desktop app.

use dioxus::desktop::{Config, WindowBuilder};
use dioxus::prelude::*;
use tracing_subscriber::EnvFilter;

use bms_store_gui::gui::app::App;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let window = WindowBuilder::new()
        .with_title("bms-store")
        .with_inner_size(dioxus::desktop::LogicalSize::new(1280.0, 800.0));

    LaunchBuilder::desktop()
        .with_cfg(Config::new().with_window(window))
        .launch(App);
}
