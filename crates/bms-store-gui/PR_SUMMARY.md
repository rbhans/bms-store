# bms-store GUI — PR Summary

Ports the opencrate-bms Dioxus desktop GUI into the bms-store workspace as a
new `bms-store-gui` crate, removing graphical-only features and reconciling
the platform-init layer to use bms-store-storage + bms-store-bridges boot APIs.

## What's in

- New `crates/bms-store-gui` Dioxus 0.6 desktop app (`cargo run -p bms-store-gui -- --project ./demo-data`).
- Compiles cleanly, boots cleanly against demo-data (entity store, point store, bridges all loaded).
- 17 Config tabs (incl. Atlas).
- Smoke test: `cargo test -p bms-store-gui --test smoke`.

## What's out

- Floor plan canvas, site map (Mapbox), status dashboards, weather widget chrome.
- Multi-site supervisor views (cross-site alarms/energy, remote-site forms, supervisor gate). The `AppPhase` enum form is preserved so a `Multi(...)` variant can be reintroduced cleanly.
- Cloud sync settings.

## Deferred (post-v0 follow-ups)

- TDD redesigns of programming view (canvas → code editor + function picker), trend chart (drop multi-overlay), schedule view (drop visual timeline if any), weather view (further chrome strip). Tasks 15–18 in the original plan.
- Several `unimplemented!()` stubs in `plugin_manager.rs` for wasm-plugins UI (the `wasmtime` runtime isn't pulled in for desktop).

## Stats

- Commits: 21
- LOC added: 47184
- LOC removed: 120
- Net: +47064
- Files added: 69
- Files modified: 2

## Verification

- `cargo build -p bms-store-gui` ✅
- `cargo test -p bms-store-gui --test smoke` ✅
- `cargo build --workspace --release` ✅
- Manual launch + boot ✅ (storage runtime + bridges initialize, ProjectLauncher + main views render)
