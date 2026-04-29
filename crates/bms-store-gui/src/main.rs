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

    // Parse --project <path> before handing control to Dioxus (which swallows argv).
    // The resolved path is forwarded via an env var so the App component can read it
    // without needing a custom prop chain through the Dioxus launch machinery.
    let args: Vec<String> = std::env::args().collect();
    if let Some(project_path) = args
        .iter()
        .position(|a| a == "--project")
        .and_then(|i| args.get(i + 1))
    {
        // Resolve to an absolute path so the App component gets a stable value
        // regardless of the working directory when the env var is read later.
        let abs = std::fs::canonicalize(project_path)
            .unwrap_or_else(|_| std::path::PathBuf::from(project_path));
        // SAFETY: this runs before Dioxus spawns any threads.
        unsafe { std::env::set_var("BMS_STORE_GUI_PROJECT", &abs) };
        tracing::info!(path = %abs.display(), "--project flag parsed");
    }

    let window = WindowBuilder::new()
        .with_title("bms-store")
        .with_inner_size(dioxus::desktop::LogicalSize::new(1280.0, 800.0));

    LaunchBuilder::desktop()
        .with_cfg(Config::new().with_window(window))
        .launch(App);
}
