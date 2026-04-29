# Task 13 Boot Smoke Report

## Build

`cargo build -p bms-store-gui` **PASSED** in 34s (dev profile).
61 warnings; 0 errors. Warning categories:
- `unexpected_cfg` for undeclared features (`api`, `cloud`, `atlas`, `desktop`, `wasm-plugins`)
- Dead-code / unused-import cleanup noise

## Launch Behavior

**Platform:** macOS (arm64, Darwin 25.4.0). A Cocoa display is present.

Command run:
```
RUST_LOG=trace ./target/debug/bms-store-gui
```

Result: **window opened cleanly**. The binary reached:
1. `applicationDidFinishLaunching` (Cocoa app delegate fired)
2. `Creating new window` (tao window created, 1280x800)
3. `ProjectLauncher` component rendered — Dioxus scope tree built, signal
   subscriptions established, project registry loaded (empty on this machine)
4. Process killed after ~8 s; exit via `windowDidResignKey` — no panic.

No panic, no error log line, no abort. The binary ran until SIGTERM.

## `--project ./demo-data` flag

The `--project` flag is **not implemented** in `main.rs`. The binary ignores
unknown CLI args (Dioxus swallows them). The app always opens the
`ProjectLauncher` GUI on startup; `init_platform` is only called after the
user selects or creates a project interactively.

Therefore the demo-data path was never exercised from the CLI in this task.

## `init_platform` static analysis (no runtime path reached)

Platform init lives in `crates/bms-store-gui/src/extracted/platform.rs`.
It calls `bms_store_storage::config::loader::resolve_scenario`, then starts
every store (`node_store`, `history_store`, `alarm_store`, etc.) and both
protocol bridges (BACnet, Modbus). All store constructors and bridge start
calls compile and link cleanly against the workspace versions of
`bms-store-storage` and `bms-store-bridges`. No missing symbols at link time.

## Findings for Task 14

1. **`--project <path>` CLI flag missing** — `main.rs` does not parse args.
   To auto-open demo-data on launch, add a `clap`/`std::env::args` check
   in `main.rs` that calls `ProjectPaths::from_root(path)` and passes it
   to the `App` component (or sets the initial `AppPhase::Single`).

2. **Undeclared Cargo features** — `#[cfg(feature = "cloud")]`,
   `#[cfg(feature = "atlas")]`, `#[cfg(feature = "desktop")]`, etc. produce
   `unexpected_cfg` warnings. These are dead-cfg stubs from the original
   monorepo. Task 14 should either declare them in `Cargo.toml` or strip
   the cfg guards entirely.

3. **`init_platform` vs `bms-store-storage::boot`** — `platform.rs` wires
   stores directly (not via a `boot` helper). If Task 14 wants to align with
   a `bms-store-storage::boot::boot_storage` entry point, the stores started
   in `init_platform` (lines 320–360) need to match that API. No runtime
   mismatch observed yet (stores link cleanly), but parity check is needed.

## Did the app reach "stores started"?

Not from CLI. The window opened at the ProjectLauncher stage (pre-project
selection). `init_platform` was not exercised at runtime in this smoke test.

---

# Task 14 Update — platform.rs reconciled with bms-store-* boot APIs

## What changed

### `extracted/platform.rs` (819 → 173 LOC)

- **`init_platform` rewritten** to delegate entirely to:
  1. `bms_store_storage::boot::boot_project_with_shutdown(paths.root, shutdown)` for all stores
  2. `bms_store_bridges::boot::boot_bridges(&storage)` for BACnet, Modbus, discovery, plugins
- Composes `SharedPlatform` directly from the two runtimes' fields — no per-store startup logic remains in this file.
- `init_platform_legacy` stripped (CLI-mode artifact, not needed in GUI).
- Intermediate `Platform`, `ModelState`, `AutomationState`, `BridgeHandles`, `SharedBridgeHandles` types removed — `init_platform` now returns `(SharedPlatform, BridgeStartReport)` directly.
- `BridgeStartReport` and `BridgeStartStatus` are re-exported from `bms_store_bridges::boot` so callers (`app.rs`, `site_context.rs`) see no import changes.
- `SharedPlatform` now includes `override_store`, `user_store`, `audit_store` (all present in `StorageRuntime`) and `plugin_registry` (from `BridgeRuntime`).

### `gui/app.rs`

- Updated `ProjectGate` to match new 2-tuple return: `Ok((platform, report))` instead of `Ok((platform, bridges, report))`.
- `App` component checks `BMS_STORE_GUI_PROJECT` env var on first render; if set, jumps straight to `AppPhase::Single(ProjectPaths::from_root(path))` instead of showing the launcher.

### `main.rs`

- Added `--project <path>` CLI arg parsing via `std::env::args` before Dioxus launch.
- Resolves the path to absolute via `fs::canonicalize`, then stores in `BMS_STORE_GUI_PROJECT` env var.
- Logs the parsed path at INFO level.

## Build

`cargo build -p bms-store-gui` — **PASSED** in 4.3s. 50 warnings (same pre-existing categories), 0 errors.

## Smoke test: `RUST_LOG=info cargo run -p bms-store-gui -- --project ./demo-data`

Log output confirmed (12-second window, no panic, clean exit on SIGTERM):

```
INFO bms_store_gui: --project flag parsed path=.../demo-data
INFO bms_store_gui::extracted::platform: Booting storage layer… project=.../demo-data
INFO bms_store_storage::store::migration: Applied schema migration store="nodes" version=1 …
INFO bms_store_storage::store::migration: Applied schema migration store="history" version=1 …
INFO bms_store_storage::store::migration: Applied schema migration store="alarms" version=1 …
[… 20+ migration lines, all stores …]
INFO bms_store_storage::boot: bms-store storage runtime booted project=.../demo-data devices=5 points=91
INFO bms_store_gui::extracted::platform: Booting bridge layer…
INFO bms_store_bridges::bridge::modbus: Modbus: no devices configured
INFO bms_store_bridges::boot: bms-store bridge runtime booted bacnet_networks=0 modbus_ok=true
INFO bms_store_storage::reporting::scheduler: Report scheduler started
```

All stores started. No panic. Boot reached full platform init with demo-data (5 devices, 91 points).
