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
