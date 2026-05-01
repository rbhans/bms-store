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

    // Capture panics to a file so users can report them without backtrace setup.
    // The file lives next to the project data dir for easy retrieval.
    install_panic_hook();

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

/// Install a panic hook that writes panic info + best-effort backtrace to
/// `~/.bms-store-gui/last-panic.log`. Helps debugging when the GUI's overlay
/// only shows "CapturedPanic" without a backtrace.
fn install_panic_hook() {
    // Force backtrace on for richer diagnostics.
    if std::env::var_os("RUST_BACKTRACE").is_none() {
        // SAFETY: runs before any other thread.
        unsafe { std::env::set_var("RUST_BACKTRACE", "1") };
    }

    let log_path = home_panic_log_path();
    if let Some(parent) = log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Always run the default hook (prints to stderr) so terminal users see it.
        default_hook(info);

        // Write a structured record to the log file.
        let payload = format!(
            "=== panic at {} ===\n{info}\n\nbacktrace:\n{:?}\n\n",
            chrono_like_now(),
            std::backtrace::Backtrace::force_capture()
        );
        // Append; ignore failures.
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            use std::io::Write;
            let _ = f.write_all(payload.as_bytes());
        }
        eprintln!("(panic also written to {})", log_path.display());
    }));
}

fn home_panic_log_path() -> std::path::PathBuf {
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    home.join(".bms-store-gui").join("last-panic.log")
}

/// Lightweight ISO-ish timestamp without pulling in chrono just for this.
fn chrono_like_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("epoch-secs={now}")
}
