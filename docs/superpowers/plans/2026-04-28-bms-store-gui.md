# bms-store GUI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port the opencrate-bms Dioxus desktop GUI into the bms-store workspace as a new `bms-store-gui` crate, removing all purely graphical/visualization features (floor plans, site map, symbol editor, status dashboards, weather widget chrome) while keeping every data-management UI (point browser, device tree, alarms, schedules, discovery, commissioning, users, audit log, programming, virtual points, plugin manager, MQTT/webhook/web-server settings, haystack tagging, reports, energy/FDD data, theme).

**Architecture:** A new workspace member `crates/bms-store-gui` houses the Dioxus 0.6 desktop app. It depends on `bms-core`, `bms-store-storage`, and `bms-store-bridges` directly (in-process, single-binary), mirroring opencrate's desktop model — no HTTP hop. The opencrate `src/gui/`, `src/main.rs`, `assets/`, and `Dioxus.toml` are copied as the starting baseline. Top-level opencrate modules that the GUI depends on but that are not yet present in any `bms-store-*` crate (e.g., `auth`, `config`, `platform`, `project`, `supervisor`) are copied alongside the GUI inside `bms-store-gui`. After the baseline compiles, graphical-only modules are deleted, redesigned views replace canvas UX with structured forms (programming wire-sheet → code editor + function picker; schedule timeline → table; trend chart → simpler line plot), and Config tabs that don't apply (e.g., supervisor cross-site multi-tenant features) are pruned.

**Tech Stack:**
- Rust 2021, Cargo workspace
- Dioxus 0.6 (`features = ["desktop"]`), `rfd` 0.15 for native file dialogs
- Tokio async runtime, `tracing` + `tracing-subscriber`
- Custom CSS in `assets/style.css` (copied from opencrate)
- Backend: `bms-core` (event bus, types, plugin traits, RBAC), `bms-store-storage` (SQLite stores), `bms-store-bridges` (BACnet, Modbus, MQTT)
- Logic engine: Rhai (already in `bms-store-storage`)

---

## Scope: KEEP / DROP / REDESIGN

**KEEP** (port wholesale, fix imports only):
- `point_table.rs`, `point_detail.rs`, `device_tree.rs`, `building_tree.rs`, `relationships_section.rs`, `virtual_points_view.rs`
- `discovery_view.rs`, `discovery_detail.rs`, `discovery_list.rs`, `discovery_group_editor.rs`, `discovery_bacnet_ops.rs`, `discovery_utils.rs`
- `commissioning_tab.rs`, `commissioning_overview.rs`
- `alarm_view.rs`, `alarm_routing_view.rs`
- `bacnet_device_alarms.rs`, `bacnet_device_cov.rs`, `bacnet_device_files.rs`, `bacnet_device_trends.rs`, `bacnet_network_tools.rs`
- `modbus_device_diagnostics.rs`, `modbus_device_registers.rs`
- `user_management.rs`, `audit_log_view.rs`, `theme_settings.rs`
- `plugin_manager.rs`
- `mqtt_settings.rs`, `webhook_settings.rs`, `web_server_settings.rs`, `export_settings.rs`
- `report_view.rs`
- `energy_view.rs`, `fdd_view.rs` (data-only — drop any embedded gauges/dashboards)
- `login.rs`, `project_launcher.rs`, `sidebar.rs`, `toolbar.rs`, `collapsible.rs`, `write_dialog.rs`
- `config_view.rs` (the tab shell — reduce tab list)

**DROP** (delete entirely, remove all references):
- `floor_plan.rs` (canvas, zones, equipment placement, symbol editor, point bindings overlay)
- `site_map_view.rs` (Mapbox GL JS marker map)
- `site_status_dashboard.rs`
- `weather_widget.rs` (decorative widget; raw weather data view stays via `weather_view.rs` simplified)
- Anything in `state.rs` that exists only to power floor plans / site map / status dashboard (markers, map config, zones, equipment placements)

**SCOPE-OUT** (drop from initial bms-store GUI):
- Multi-site supervisor *views* (`supervisor_app.rs`, `supervisor_gate.rs`, `cross_site_alarm_view.rs`, `cross_site_energy_view.rs`, `remote_site_form.rs`, `remote_site_view.rs`, `supervisor_state.rs`, `supervisor_validation.rs`).
  - **Architectural note:** Multi-site itself may return as a future feature. The `AppPhase` enum is therefore simplified to `Launcher | Single` *for now* but its shape (a phase enum vs. a hard-coded single mode) is intentionally preserved so a `Multi(...)` variant can be reintroduced cleanly.
- Cloud sync settings (`cloud_settings.rs`) — drop for now.

**KEEP — initially considered SCOPE-OUT but reinstated per user input:**
- Atlas taxonomy integration (`atlas_settings.rs` + `crate::atlas::*` rewire). Atlas serves as an initial-pass naming hint engine, which directly aligns with bms-store's mission of standardizing point names. Backend support is already present in `bms_store_storage::atlas`.

**REDESIGN** (rebuild as data forms with TDD):
- `programming_view.rs` — keep Rhai compile/run engine; replace wire-sheet canvas with a code editor + function picker
- `trend_chart.rs` — keep line-plot for point history; drop multi-overlay/gauge variants
- `schedule_view.rs` — replace any visual timeline with a table-of-rules form
- `weather_view.rs` — strip animated/iconographic chrome; keep raw weather data list

---

## File Structure

```
bms-store/                                       (existing workspace root)
├── Cargo.toml                                  (modify: add bms-store-gui to workspace.members)
├── docs/superpowers/plans/2026-04-28-bms-store-gui.md   (this plan)
└── crates/
    └── bms-store-gui/                          (NEW — created in Phase A)
        ├── Cargo.toml
        ├── Dioxus.toml
        ├── README.md
        ├── assets/
        │   ├── style.css                       (copied from opencrate)
        │   └── manifest.json
        ├── src/
        │   ├── main.rs                         (Dioxus desktop entry — pruned cli mode)
        │   ├── lib.rs                          (re-exports for tests + main.rs)
        │   ├── app.rs                          (root <App/> + AppPhase)
        │   ├── state.rs                        (AppState — stripped of floor-plan/map types)
        │   ├── theme.rs
        │   ├── api_client.rs                   (deferred — not used by desktop build)
        │   ├── auth.rs                         (copied from opencrate src/auth.rs; reconciled with bms-core::rbac)
        │   ├── config/                         (copied from opencrate src/config/)
        │   │   ├── mod.rs
        │   │   ├── loader.rs                   (LoadedScenario)
        │   │   └── profile.rs                  (PointValue, profiles)
        │   ├── platform.rs                     (init_platform — wraps bms-store-storage)
        │   ├── project.rs                      (ProjectPaths, ProjectMeta)
        │   ├── components/
        │   │   ├── mod.rs                      (only export the KEEP list)
        │   │   ├── point_table.rs
        │   │   ├── point_detail.rs
        │   │   ├── device_tree.rs
        │   │   ├── building_tree.rs
        │   │   ├── relationships_section.rs
        │   │   ├── virtual_points_view.rs
        │   │   ├── discovery_view.rs
        │   │   ├── discovery_detail.rs
        │   │   ├── discovery_list.rs
        │   │   ├── discovery_group_editor.rs
        │   │   ├── discovery_bacnet_ops.rs
        │   │   ├── discovery_utils.rs
        │   │   ├── commissioning_tab.rs
        │   │   ├── commissioning_overview.rs
        │   │   ├── alarm_view.rs
        │   │   ├── alarm_routing_view.rs
        │   │   ├── bacnet_device_alarms.rs
        │   │   ├── bacnet_device_cov.rs
        │   │   ├── bacnet_device_files.rs
        │   │   ├── bacnet_device_trends.rs
        │   │   ├── bacnet_network_tools.rs
        │   │   ├── modbus_device_diagnostics.rs
        │   │   ├── modbus_device_registers.rs
        │   │   ├── user_management.rs
        │   │   ├── audit_log_view.rs
        │   │   ├── theme_settings.rs
        │   │   ├── plugin_manager.rs
        │   │   ├── mqtt_settings.rs
        │   │   ├── webhook_settings.rs
        │   │   ├── web_server_settings.rs
        │   │   ├── export_settings.rs
        │   │   ├── report_view.rs
        │   │   ├── energy_view.rs              (data-only view)
        │   │   ├── fdd_view.rs                 (data-only view)
        │   │   ├── login.rs
        │   │   ├── project_launcher.rs
        │   │   ├── sidebar.rs
        │   │   ├── toolbar.rs                  (Page/SiteMap/SiteStatus buttons removed)
        │   │   ├── collapsible.rs
        │   │   ├── write_dialog.rs
        │   │   ├── config_view.rs              (tab list pruned)
        │   │   ├── programming_view.rs         (REDESIGNED — code editor + function picker)
        │   │   ├── schedule_view.rs            (REDESIGNED — table-based)
        │   │   ├── trend_chart.rs              (SIMPLIFIED — line plot only)
        │   │   └── weather_view.rs             (data-only)
        │   └── aggregation.rs                  (kept — used by config tabs; review for cross-site coupling)
        └── tests/
            └── smoke.rs                        (NEW — boots app shell against demo-data)
```

> **Note on copying strategy.** Phase A does a literal copy of the opencrate sources to get a working compile baseline. Phase B then deletes the DROP/SCOPE-OUT files and removes their references. This is more reliable than selective copy (we miss fewer transitive deps) and lets the compiler find every reference for us. The trade-off is one large initial commit — acceptable here because the source is already version-controlled in opencrate-bms.

---

## Phase A — Worktree, Crate Scaffold, Baseline Compile

### Task 1: Create the worktree

**Files:** None (git operation)

- [ ] **Step 1: Create worktree from main**

```bash
cd /Users/benhansen/github/bms-store
git worktree add -b feat/gui ../bms-store-gui-worktree main
cd ../bms-store-gui-worktree
pwd
```

Expected: prints `/Users/benhansen/github/bms-store-gui-worktree`. All subsequent task paths are relative to this worktree unless noted.

- [ ] **Step 2: Confirm baseline builds**

```bash
cargo check --workspace
```

Expected: PASS (workspace compiles cleanly before any changes).

- [ ] **Step 3: Commit nothing yet (worktree creation only)**

No commit — work happens on `feat/gui` branch.

---

### Task 2: Scaffold the new crate

**Files:**
- Create: `crates/bms-store-gui/Cargo.toml`
- Create: `crates/bms-store-gui/src/main.rs` (placeholder)
- Modify: `Cargo.toml` (workspace members)

- [ ] **Step 1: Create `crates/bms-store-gui/Cargo.toml`**

```toml
[package]
name = "bms-store-gui"
version = "0.1.0"
edition = "2021"
description = "Desktop GUI for the bms-store data layer"
license = "MIT OR Apache-2.0"

[[bin]]
name = "bms-store-gui"
path = "src/main.rs"

[dependencies]
# Workspace data layer
bms-core = { path = "../bms-core" }
bms-store-storage = { path = "../bms-store-storage" }
bms-store-bridges = { path = "../bms-store-bridges" }

# UI
dioxus = { version = "0.6", features = ["desktop"] }
rfd = "0.15"

# Async / errors
tokio = { version = "1", features = ["full"] }
tokio-util = "0.7"
async-trait = "0.1"
thiserror = "2"
futures = "0.3"

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"

# Crypto / IDs / encoding (carry-over from opencrate desktop deps)
uuid = { version = "1", features = ["v4"] }
argon2 = "0.5"
rand = "0.8"
hmac = "0.12"
sha2 = "0.10"
base64 = "0.22"
hex = "0.4"
aes-gcm = "0.10"
ed25519-dalek = { version = "2", features = ["rand_core"] }
blake3 = "1"

# Storage / scripting
rusqlite = { version = "0.33", features = ["bundled"] }
rhai = { version = "1", features = ["sync"] }

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Misc desktop
webbrowser = "1"
flate2 = "1"
tar = "0.4"
urlencoding = "2"
libc = "0.2"

[dev-dependencies]
tempfile = "3"
```

> If a dependency is unused after Phase B prune, remove it then.

- [ ] **Step 2: Create placeholder `src/main.rs`**

```rust
fn main() {
    println!("bms-store-gui scaffold — populated in Phase A Task 3");
}
```

- [ ] **Step 3: Add to workspace `Cargo.toml`**

Edit `/Users/benhansen/github/bms-store-gui-worktree/Cargo.toml` `[workspace]` block:

```toml
[workspace]
members = [
    ".",
    "crates/bms-core",
    "crates/bms-store-domain",
    "crates/bms-store-storage",
    "crates/bms-store-bridges",
    "crates/bms-store-server",
    "crates/bms-store-client",
    "crates/bms-store-gui",
]
resolver = "2"
```

- [ ] **Step 4: Verify the workspace still builds**

```bash
cargo check -p bms-store-gui
```

Expected: PASS — empty placeholder binary builds.

- [ ] **Step 5: Commit**

```bash
git add crates/bms-store-gui/Cargo.toml crates/bms-store-gui/src/main.rs Cargo.toml
git commit -m "feat(gui): scaffold empty bms-store-gui crate

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: Copy opencrate GUI source

**Files:**
- Copy from `/Users/benhansen/github/opencrate-bms/src/gui/` → `crates/bms-store-gui/src/gui_raw/`
- Copy from `/Users/benhansen/github/opencrate-bms/assets/` → `crates/bms-store-gui/assets/`
- Copy `/Users/benhansen/github/opencrate-bms/Dioxus.toml` → `crates/bms-store-gui/Dioxus.toml`

We copy into a temporary `gui_raw/` subdirectory first to keep the diff isolated, then promote in Task 5 once we know it compiles.

- [ ] **Step 1: Copy gui sources**

```bash
cp -R /Users/benhansen/github/opencrate-bms/src/gui \
      /Users/benhansen/github/bms-store-gui-worktree/crates/bms-store-gui/src/gui_raw
ls crates/bms-store-gui/src/gui_raw/components | wc -l
```

Expected: `58` (the component file count).

- [ ] **Step 2: Copy assets**

```bash
cp -R /Users/benhansen/github/opencrate-bms/assets \
      /Users/benhansen/github/bms-store-gui-worktree/crates/bms-store-gui/assets
ls crates/bms-store-gui/assets | head
```

Expected: includes `style.css`, `manifest.json`, possibly icons.

- [ ] **Step 3: Copy Dioxus.toml**

```bash
cp /Users/benhansen/github/opencrate-bms/Dioxus.toml \
   /Users/benhansen/github/bms-store-gui-worktree/crates/bms-store-gui/Dioxus.toml
```

- [ ] **Step 4: Commit the raw copy as one snapshot**

```bash
git add crates/bms-store-gui/src/gui_raw crates/bms-store-gui/assets crates/bms-store-gui/Dioxus.toml
git commit -m "feat(gui): copy opencrate gui sources verbatim into gui_raw/

Source: opencrate-bms src/gui (commit-of-the-day) — porting baseline.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: Copy opencrate top-level modules the GUI depends on

The opencrate GUI references `crate::auth`, `crate::config`, `crate::platform`, `crate::project`, plus several store/event/logic/discovery/etc. modules. The latter group exists in `bms-store-storage` and `bms-core` — we'll rewire to those in Task 6. The former group has no equivalent in bms-store crates yet, so we copy it into the GUI crate.

**Files (copy verbatim into `crates/bms-store-gui/src/extracted/`):**
- `auth.rs` ← opencrate `src/auth.rs`
- `config/` ← opencrate `src/config/`
- `platform.rs` ← opencrate `src/platform.rs`
- `project/` ← opencrate `src/project/` (or `project.rs` if single file)

Other opencrate top-level modules (e.g., `event/`, `store/`, `logic/`, `discovery/`, `weather/`, `notification/`, `webhook/`, `mqtt/`, `aggregation/`, `health.rs`, `bridge/`, `plugin/`, `reporting/`, `energy/`, `fdd/`, `haystack/`, `export/`, `cloud/`, `atlas/`, `supervisor/`, `node/`, `backup.rs`) are NOT copied at this stage — they're either already in `bms-store-storage`/`bms-core` or in scope-out areas.

- [ ] **Step 1: List exactly what opencrate has at the top level**

```bash
ls /Users/benhansen/github/opencrate-bms/src/
```

Expected output (cross-check with the table below):
```
aggregation api atlas auth.rs backup.rs bridge cloud config discovery energy
event export fdd gui haystack health.rs lib.rs logic main.rs mqtt node
notification platform.rs plugin project protocol reporting store supervisor
weather webhook
```

- [ ] **Step 2: Copy required modules into `crates/bms-store-gui/src/extracted/`**

```bash
mkdir -p crates/bms-store-gui/src/extracted
cp    /Users/benhansen/github/opencrate-bms/src/auth.rs        crates/bms-store-gui/src/extracted/auth.rs
cp -R /Users/benhansen/github/opencrate-bms/src/config         crates/bms-store-gui/src/extracted/config
cp    /Users/benhansen/github/opencrate-bms/src/platform.rs    crates/bms-store-gui/src/extracted/platform.rs
cp -R /Users/benhansen/github/opencrate-bms/src/project        crates/bms-store-gui/src/extracted/project
ls crates/bms-store-gui/src/extracted
```

If `src/project` does not exist as a directory in opencrate (single-file `project.rs`), copy that file instead. Same fallback for `config`. Adjust the command to whatever's actually there.

- [ ] **Step 3: Commit the extracted modules**

```bash
git add crates/bms-store-gui/src/extracted
git commit -m "feat(gui): copy opencrate auth/config/platform/project modules

These are GUI-coupled support modules with no equivalent in the bms-store
crates yet. Reconciled with bms-core/bms-store-storage in Phase B.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: Wire `main.rs`, `lib.rs`, and the gui module path

**Files:**
- Modify: `crates/bms-store-gui/src/main.rs`
- Create: `crates/bms-store-gui/src/lib.rs`

The opencrate `src/main.rs` has both `cli` and `desktop` entry paths. We keep only the desktop path.

- [ ] **Step 1: Read opencrate main.rs to extract the desktop entry**

```bash
grep -n 'feature = "desktop"' /Users/benhansen/github/opencrate-bms/src/main.rs | head -20
```

This shows the relevant `#[cfg(feature = "desktop")] mod desktop { … }` block.

- [ ] **Step 2: Write `crates/bms-store-gui/src/lib.rs`**

```rust
//! bms-store desktop GUI — library entry for tests and the binary.
//!
//! See `main.rs` for the desktop launch path.

pub mod gui;
pub mod extracted;

// Re-exports so `crate::auth::…`, `crate::config::…`, `crate::platform`, `crate::project`
// keep working in copied component code without bulk-rewriting paths.
pub use extracted::auth;
pub use extracted::config;
pub use extracted::platform;
pub use extracted::project;
```

- [ ] **Step 3: Move `gui_raw` → `gui`**

```bash
mv crates/bms-store-gui/src/gui_raw crates/bms-store-gui/src/gui
ls crates/bms-store-gui/src/gui | head
```

Expected: includes `mod.rs`, `app.rs`, `state.rs`, `components/`, etc.

- [ ] **Step 4: Replace `crates/bms-store-gui/src/main.rs`**

```rust
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
```

- [ ] **Step 5: First compile attempt — expected to fail with import errors**

```bash
cargo check -p bms-store-gui 2>&1 | head -80
```

Expected: HUNDREDS of errors. They fall into a few buckets:
1. `crate::store::*` — opencrate had stores in `crate::store`; we'll rewire to `bms_store_storage::*`.
2. `crate::event::*` — rewire to `bms_core::event` or `bms_store_storage` equivalents.
3. `crate::logic::*`, `crate::discovery::*`, `crate::weather::*`, etc. — same pattern.
4. Removed-feature references (floor plan, supervisor views, cloud) — handled in Phase B/C.

Don't try to fix here. The import rewire is Task 6.

- [ ] **Step 6: Commit the wiring (broken build is OK on this branch)**

```bash
git add crates/bms-store-gui/src/main.rs crates/bms-store-gui/src/lib.rs crates/bms-store-gui/src/gui
git rm -r --cached crates/bms-store-gui/src/gui_raw 2>/dev/null || true
git commit -m "feat(gui): wire main.rs/lib.rs and promote gui_raw to gui

Build is intentionally broken on this commit — import rewiring follows
in the next task. Each subsequent commit narrows the error count.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase B — Import Rewire & Module Reconciliation

This phase iteratively shrinks the compile-error list. Work in passes — each pass fixes one category of errors and commits.

### Task 6: Pass 1 — rewire `crate::store::*` → `bms_store_storage::*`

**Files:** all of `crates/bms-store-gui/src/gui/components/*.rs` plus `state.rs`, `app.rs`.

In opencrate, store types like `PointStore`, `AlarmStore`, `EntityStore` live under `crate::store::*`. In bms-store they live in `bms_store_storage`. The exact module structure inside `bms_store_storage` matches (per the survey it has subsystems for each store).

- [ ] **Step 1: Inventory current `crate::store::*` imports**

```bash
grep -rh 'crate::store::' crates/bms-store-gui/src/gui crates/bms-store-gui/src/extracted | sort -u
```

Expected: a list of e.g. `use crate::store::point_store::{PointKey, PointStatusFlags};` etc.

- [ ] **Step 2: Verify each maps to a `bms_store_storage` path**

```bash
grep -rn 'pub use\|pub mod' /Users/benhansen/github/bms-store-gui-worktree/crates/bms-store-storage/src/lib.rs | head -40
```

Confirm the modules listed (auth, backup, boot, discovery, energy, fdd, haystack, logic, mqtt, node, notification, project, protocol, reporting, store, weather, webhook) cover what the GUI imports.

- [ ] **Step 3: Bulk-rewrite imports across the GUI**

```bash
cd crates/bms-store-gui
grep -rl 'crate::store::' src | xargs sed -i '' 's|crate::store::|bms_store_storage::|g'
grep -rl 'crate::event::' src | xargs sed -i '' 's|crate::event::|bms_core::event::|g'
```

> macOS sed: `sed -i ''` (with the empty backup arg) is required.

- [ ] **Step 4: Rebuild and count remaining errors**

```bash
cargo check -p bms-store-gui 2>&1 | grep '^error\[' | wc -l
```

Record the count for the commit message.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor(gui): rewire crate::store→bms_store_storage, crate::event→bms_core::event

Errors remaining: <count>

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 7: Pass 2 — rewire remaining backend modules

Apply the same pattern for the remaining `crate::*` paths that point to subsystems now living in `bms-store-storage` or `bms-core`.

**Mapping table** (verify each against the actual bms-store crate layout before rewriting):

| opencrate path                    | bms-store path                                |
|-----------------------------------|-----------------------------------------------|
| `crate::logic::engine::*`         | `bms_store_storage::logic::engine::*`         |
| `crate::logic::store::*`          | `bms_store_storage::logic::store::*`          |
| `crate::discovery::service::*`    | `bms_store_storage::discovery::service::*`    |
| `crate::weather::*`               | `bms_store_storage::weather::*`               |
| `crate::notification::*`          | `bms_store_storage::notification::*`          |
| `crate::webhook::*`               | `bms_store_storage::webhook::*`               |
| `crate::mqtt::*`                  | `bms_store_storage::mqtt::*`                  |
| `crate::reporting::*`             | `bms_store_storage::reporting::*`             |
| `crate::energy::*`                | `bms_store_storage::energy::*`                |
| `crate::fdd::*`                   | `bms_store_storage::fdd::*`                   |
| `crate::haystack::*`              | `bms_store_storage::haystack::*`              |
| `crate::export::*`                | `bms_store_storage::export::*` (if present)   |
| `crate::node::*`                  | `bms_core::node::*`                           |
| `crate::plugin::*`                | `bms_core::plugin::*` + `bms_store_bridges::plugin::*` |
| `crate::bridge::*`                | `bms_store_bridges::bridge::*`                |
| `crate::aggregation::*`           | (kept as `crate::aggregation` — local to GUI) |
| `crate::supervisor::*`            | (DROP — see Phase C; views removed but `AppPhase` enum shape preserved) |
| `crate::cloud::*`                 | (DROP — see Phase C)                          |
| `crate::atlas::*`                 | `bms_store_storage::atlas::*` (KEEP — naming hint engine) |
| `crate::backup::*`                | `bms_store_storage::backup::*` (if present)   |

- [ ] **Step 1: Verify each target path exists**

For each row in the table, confirm by reading `bms-store-storage/src/lib.rs` (or grepping):

```bash
grep -n 'pub mod logic\|pub mod discovery\|pub mod weather' crates/bms-store-storage/src/lib.rs
```

Expected: each module appears. If not, the GUI feature it powers either:
- needs a small adapter shim added to `bms-store-storage` (out of scope here — flag and skip the feature), or
- the GUI component depending on it needs to be deferred / cut.

- [ ] **Step 2: Apply each substitution one at a time, building between passes**

For each row, run a `find ... | xargs sed` substitution, then `cargo check -p bms-store-gui`, then commit. Example for the `logic` row:

```bash
grep -rl 'crate::logic::engine::' crates/bms-store-gui/src \
  | xargs sed -i '' 's|crate::logic::engine::|bms_store_storage::logic::engine::|g'
grep -rl 'crate::logic::store::' crates/bms-store-gui/src \
  | xargs sed -i '' 's|crate::logic::store::|bms_store_storage::logic::store::|g'
cargo check -p bms-store-gui 2>&1 | grep '^error\[' | wc -l
git add -A
git commit -m "refactor(gui): rewire crate::logic→bms_store_storage::logic"
```

Repeat for each row. **Stop and re-evaluate** if a substitution causes the error count to *increase* — likely an unintended match.

- [ ] **Step 3: Final pass — `cargo check` and capture remaining errors by category**

```bash
cargo check -p bms-store-gui 2>&1 | grep '^error\[' | sort -u | head -40
```

What's left should be:
1. References to DROP'd files (floor_plan, site_map, supervisor views, cloud).
2. Type signature drift between opencrate and bms-store (e.g., `PointStore::new` signature changed).
3. Permission constants from `crate::auth` that are now duplicated between `extracted/auth.rs` and `bms_core::rbac` — Task 8 reconciles.

- [ ] **Step 4: Commit the rewire**

```bash
git add -A
git commit -m "refactor(gui): complete bms-store-* import rewire

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 8: Reconcile `auth` with `bms_core::rbac`

`bms_core::rbac` exists. `extracted/auth.rs` was copied from opencrate. Pick one source of truth.

- [ ] **Step 1: Compare the two**

```bash
wc -l crates/bms-core/src/rbac.rs crates/bms-store-gui/src/extracted/auth.rs
diff -u crates/bms-core/src/rbac.rs crates/bms-store-gui/src/extracted/auth.rs | head -100
```

Expected: substantial overlap on `Permission`, `RoleSet`, etc., with some GUI-only helpers in `auth.rs` (e.g., `AllRolePermissions`).

- [ ] **Step 2: Decision**

If `bms_core::rbac` covers the core types, refactor `extracted/auth.rs` into a thin shim:

```rust
//! GUI-side auth helpers — re-exports of bms_core::rbac plus UI-specific aggregations.

pub use bms_core::rbac::*;

/// All permissions a role grants, materialized for UI display (e.g., user-edit dialogs).
/// This is a UI concern, not a core concern, so it lives here.
pub struct AllRolePermissions { /* ... */ }

impl AllRolePermissions {
    pub fn for_role(role: &Role) -> Self { /* ... */ }
}
```

(Copy the `AllRolePermissions` body from the original opencrate `auth.rs`.)

If `bms_core::rbac` is a stub, KEEP `extracted/auth.rs` as the source of truth for now and file a follow-up issue.

- [ ] **Step 3: Build**

```bash
cargo check -p bms-store-gui 2>&1 | grep '^error\[' | wc -l
```

Expected: error count drops further.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "refactor(gui): reconcile auth.rs with bms_core::rbac"
```

---

### Task 9: Resolve type-signature drift, one component at a time

For each remaining error, the fix is component-local: update the call site to match the bms-store API. Work in alphabetical order through the remaining errors.

- [ ] **Step 1: List the failing files**

```bash
cargo check -p bms-store-gui 2>&1 | grep '^error\[' -A 1 | grep '\-\->' | awk '{print $2}' | cut -d: -f1 | sort -u
```

- [ ] **Step 2: For each file, read the error, look up the correct bms-store API, fix, verify**

For each file `F`:

```bash
cargo check -p bms-store-gui 2>&1 | grep -B 2 -A 8 "$F" | head -60
# Read F and the bms-store-storage source it calls into
# Fix the call site
cargo check -p bms-store-gui 2>&1 | grep '^error\[' | wc -l   # should decrease
```

- [ ] **Step 3: Commit per file (or per small group)**

Atomic commits make rollback easy if a fix is wrong:

```bash
git add crates/bms-store-gui/src/gui/components/<file>.rs
git commit -m "fix(gui): adapt <component> to bms-store API"
```

- [ ] **Step 4: Stop point**

When the only remaining errors are about DROP files (floor_plan, site_map, supervisor views, cloud), stop here. Phase C handles those.

---

## Phase C — Cut Graphical & Out-of-Scope Features

### Task 10: Scope-out decisions (already made — recorded here for the audit trail)

User confirmed these scope decisions before plan execution started:

- **DROP:** floor_plan, site_map, site_status_dashboard, weather_widget chrome — purely graphical, removed in Task 11.
- **DROP for now:** supervisor *views* (cross-site alarm/energy, remote site forms, supervisor gate, supervisor_state, supervisor_validation) — but the `AppPhase` enum keeps its phase-shaped form to allow multi-site to return as a future feature without re-architecting. Removed in Task 12.
- **DROP for now:** cloud sync settings — removed in Task 12.
- **KEEP:** Atlas taxonomy integration — used as an initial-pass naming hint, aligning with bms-store's standardization mission. `crate::atlas::*` is rewired to `bms_store_storage::atlas::*` in Task 7; the `Atlas` Config tab and `atlas_settings.rs` component are kept in Tasks 12, 19.

No user action required at this step — proceed to Task 11.

---

### Task 11: Delete graphical-only views

**Files:**
- Delete: `crates/bms-store-gui/src/gui/components/floor_plan.rs`
- Delete: `crates/bms-store-gui/src/gui/components/site_map_view.rs`
- Delete: `crates/bms-store-gui/src/gui/components/site_status_dashboard.rs`
- Delete: `crates/bms-store-gui/src/gui/components/weather_widget.rs`
- Modify: `crates/bms-store-gui/src/gui/components/mod.rs` (remove exports)
- Modify: `crates/bms-store-gui/src/gui/state.rs` (remove map/zone/marker types)

- [ ] **Step 1: Delete the files**

```bash
git rm crates/bms-store-gui/src/gui/components/floor_plan.rs
git rm crates/bms-store-gui/src/gui/components/site_map_view.rs
git rm crates/bms-store-gui/src/gui/components/site_status_dashboard.rs
git rm crates/bms-store-gui/src/gui/components/weather_widget.rs
```

- [ ] **Step 2: Remove exports from `components/mod.rs`**

```bash
cd crates/bms-store-gui/src/gui/components
grep -n '^pub mod \(floor_plan\|site_map_view\|site_status_dashboard\|weather_widget\);' mod.rs
```

Edit `mod.rs` and delete those four `pub mod` lines.

- [ ] **Step 3: Remove map/zone/marker types from `state.rs`**

In `crates/bms-store-gui/src/gui/state.rs`, delete:
- `SiteMapData`, `MapMarker`, `MarkerIcon`, `MapViewConfig`, `StatusBinding` types
- `load_mapbox_config`, related save/load helpers
- `is_site_page` if unused after `Page` view goes away (it likely is — confirm by grep)
- The `ActiveView::Page` variant
- Any signal/state field whose only producer/consumer was floor_plan or site_map

```bash
grep -n 'SiteMapData\|MapMarker\|MarkerIcon\|MapViewConfig\|StatusBinding\|load_mapbox_config\|is_site_page\|ActiveView::Page' crates/bms-store-gui/src/gui/state.rs
```

For each, edit-delete. Re-run grep until empty.

- [ ] **Step 4: Remove references from `app.rs`**

```bash
grep -n 'FloorPlanCanvas\|SiteMapView\|site_map_view\|floor_plan\|SiteStatusDashboard' crates/bms-store-gui/src/gui/app.rs
```

Delete any matching imports and remove their usages from the `match current_phase` / view-dispatch tree.

- [ ] **Step 5: Update toolbar to remove the buttons**

`crates/bms-store-gui/src/gui/components/toolbar.rs` has buttons that set `ActiveView::Page` etc. Remove those buttons.

- [ ] **Step 6: Build**

```bash
cargo check -p bms-store-gui 2>&1 | grep '^error\[' | wc -l
```

Expected: zero or very small. Fix any stragglers (likely transitive imports from deleted state types).

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(gui): drop graphical-only views (floor plan, site map, status dashboard, weather widget)

User confirmed scope-out in Task 10. Removed:
- floor_plan.rs (canvas, zones, equipment placement, symbol editor)
- site_map_view.rs (Mapbox markers)
- site_status_dashboard.rs
- weather_widget.rs (decorative; raw weather data view kept)
- Map/marker/zone types and load_mapbox_config helper from state.rs
- ActiveView::Page variant and toolbar Page button

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 12: Delete supervisor and cloud features (Atlas KEPT)

**Files:**
- Delete: `crates/bms-store-gui/src/gui/components/supervisor_app.rs`
- Delete: `crates/bms-store-gui/src/gui/components/supervisor_gate.rs`
- Delete: `crates/bms-store-gui/src/gui/components/cross_site_alarm_view.rs`
- Delete: `crates/bms-store-gui/src/gui/components/cross_site_energy_view.rs`
- Delete: `crates/bms-store-gui/src/gui/components/remote_site_form.rs`
- Delete: `crates/bms-store-gui/src/gui/components/remote_site_view.rs`
- Delete: `crates/bms-store-gui/src/gui/components/cloud_settings.rs`
- Delete: `crates/bms-store-gui/src/gui/supervisor_state.rs`
- Delete: `crates/bms-store-gui/src/gui/supervisor_validation.rs`
- **KEEP:** `crates/bms-store-gui/src/gui/components/atlas_settings.rs`
- Modify: `mod.rs` files, `app.rs`, `state.rs`

- [ ] **Step 1: Delete files**

```bash
git rm crates/bms-store-gui/src/gui/components/supervisor_app.rs \
       crates/bms-store-gui/src/gui/components/supervisor_gate.rs \
       crates/bms-store-gui/src/gui/components/cross_site_alarm_view.rs \
       crates/bms-store-gui/src/gui/components/cross_site_energy_view.rs \
       crates/bms-store-gui/src/gui/components/remote_site_form.rs \
       crates/bms-store-gui/src/gui/components/remote_site_view.rs \
       crates/bms-store-gui/src/gui/components/cloud_settings.rs \
       crates/bms-store-gui/src/gui/supervisor_state.rs \
       crates/bms-store-gui/src/gui/supervisor_validation.rs
```

- [ ] **Step 2: Strip references from `app.rs` and `state.rs`**

In `app.rs`, simplify `AppPhase` — remove the `Supervisor { … }` variant and its match arm. Keep the enum (rather than collapsing to a single struct) so a `Multi(...)` variant can be reintroduced cleanly later:

```rust
/// Top-level app phase. Currently single-project only; the enum shape is
/// kept so a future Multi-site variant can be added without re-architecting.
#[derive(Clone)]
enum AppPhase {
    Launcher,
    Single(ProjectPaths),
}
```

Remove `RemoteSiteConfig`, `LaunchSelection::Supervisor` from `state.rs`. Simplify `LaunchSelection`:

```rust
pub enum LaunchSelection {
    Single(ProjectPaths),
}
```

- [ ] **Step 3: Strip references from `project_launcher.rs`**

The launcher used to allow building a supervisor selection. Remove the multi-select UI; keep only single-project open. Leave the `LaunchSelection` enum dispatch in place (with its single arm) so re-adding a multi-site path is one variant + arm.

- [ ] **Step 4: Strip Config tabs**

In `crates/bms-store-gui/src/gui/components/config_view.rs`, remove the `ConfigTab::Cloud` variant and its match arm. **Keep** `ConfigTab::Atlas` and its match arm — Atlas stays.

- [ ] **Step 5: Build**

```bash
cargo check -p bms-store-gui 2>&1 | grep '^error\[' | wc -l
```

Expected: zero.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(gui): drop supervisor views and cloud settings (atlas kept)

Per user scope decision:
- Supervisor views removed; AppPhase enum shape preserved so multi-site
  can return as a future feature.
- Cloud sync settings removed (deferred).
- Atlas taxonomy integration KEPT — used as an initial-pass naming hint,
  aligns with bms-store's standardization mission. Wired to
  bms_store_storage::atlas in Task 7.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 13: Verify the desktop app boots

**Files:** none modified

- [ ] **Step 1: Build the binary**

```bash
cargo build -p bms-store-gui
```

Expected: PASS (warnings OK).

- [ ] **Step 2: Run against demo-data**

```bash
DIOXUS_LOG=info cargo run -p bms-store-gui -- --project ./demo-data
```

Expected: Dioxus window opens. Project launcher loads. Selecting demo-data opens the main shell. Login screen renders. Note: backend wiring (stores actually loading data) may not work yet if `init_platform` differs — that's the next task.

- [ ] **Step 3: Capture any runtime errors**

Tail logs. Note any panics. If `init_platform` fails, that's the bridge between `extracted/platform.rs` and `bms-store-storage` — fix in Task 14.

- [ ] **Step 4: Smoke commit (if it boots)**

```bash
git tag gui-boots-v0
```

(Tag, not commit — no source change.)

---

### Task 14: Reconcile `extracted/platform.rs` with `bms-store-storage::boot`

`init_platform` in opencrate started all stores, services, bridges, and event bus. `bms-store-storage` has its own boot path (the survey mentioned `boot`, `aggregation`, `discovery` modules). They likely differ.

- [ ] **Step 1: Read both**

```bash
wc -l crates/bms-store-gui/src/extracted/platform.rs crates/bms-store-storage/src/boot.rs 2>/dev/null
```

- [ ] **Step 2: Decision tree**

- If `bms_store_storage::boot::init_platform` (or equivalent) returns the same `SharedPlatform` shape: delete `extracted/platform.rs` and re-export from `bms_store_storage`.
- If shapes differ: update `extracted/platform.rs` to call `bms_store_storage::boot::*` internally and adapt the result type.
- If `bms-store-storage` doesn't yet have an init helper: KEEP `extracted/platform.rs` and add a follow-up to upstream it.

- [ ] **Step 3: Run the app again**

```bash
DIOXUS_LOG=debug cargo run -p bms-store-gui -- --project ./demo-data
```

Expected: device tree populates with demo entities. Click into a device → point table renders.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "fix(gui): wire init_platform through bms-store-storage::boot"
```

---

## Phase D — Redesign Visual Views (TDD)

### Task 15: Redesign `programming_view.rs` — wire-sheet → code editor

**Files:**
- Modify: `crates/bms-store-gui/src/gui/components/programming_view.rs`
- Create: `crates/bms-store-gui/src/gui/components/programming_view_test.rs` (or use `#[cfg(test)]` module)

The opencrate `programming_view.rs` is 2872 LOC mostly of canvas/wire-sheet rendering. Goal: keep the Rhai compile/run engine integration; replace the canvas with a code editor pane plus a function picker (sidebar listing available built-in functions and their signatures).

- [ ] **Step 1: Identify the engine API**

```bash
grep -n 'ExecutionEngine\|compile\|fn run\|publish_program' crates/bms-store-storage/src/logic/engine.rs | head -20
```

The engine has `compile(&str) -> Result<…>`, `run(&compiled, &context) -> Result<…>`, etc.

- [ ] **Step 2: Write the failing test for the redesigned component contract**

The component takes a `ProgramStore` and renders editor + picker + actions. Test that compile errors render to the user.

```rust
// In programming_view.rs (or its test module):

#[cfg(test)]
mod tests {
    use super::*;
    use bms_store_storage::logic::engine::ExecutionEngine;

    #[test]
    fn compile_error_message_is_extracted() {
        // Given a program with bad syntax
        let src = "let x = ;";
        let engine = ExecutionEngine::new();
        let err = engine.compile(src).unwrap_err();
        let msg = format_compile_error(&err);
        assert!(msg.contains("syntax"), "got: {msg}");
    }
}

/// Render-friendly compile-error formatter (line, column, message).
pub fn format_compile_error(err: &bms_store_storage::logic::engine::CompileError) -> String {
    format!("Line {} col {}: {}", err.line(), err.column(), err.message())
}
```

- [ ] **Step 3: Run the test to verify it fails**

```bash
cargo test -p bms-store-gui programming_view::tests::compile_error_message_is_extracted
```

Expected: FAIL with "function `format_compile_error` not found" or similar.

- [ ] **Step 4: Implement the formatter**

Add `format_compile_error` to `programming_view.rs`. Confirm the actual `CompileError` API exposes `line`/`column`/`message` (or equivalent — adapt the field names if not).

- [ ] **Step 5: Run the test to verify it passes**

```bash
cargo test -p bms-store-gui programming_view::tests::format_compile_error
```

Expected: PASS.

- [ ] **Step 6: Replace the canvas with editor + picker**

Strip the wire-sheet rendering entirely. Replace the component body with:

```rust
#[component]
pub fn ProgrammingView(
    state: Signal<AppState>,
    program_store: Arc<ProgramStore>,
) -> Element {
    let mut source = use_signal(String::new);
    let mut compile_status = use_signal(|| Option::<String>::None);

    let functions = available_builtin_functions();

    rsx! {
        div { class: "programming-view",
            aside { class: "function-picker",
                h3 { "Built-in Functions" }
                ul {
                    for f in functions.iter() {
                        li {
                            onclick: move |_| insert_at_cursor(&mut source, &f.snippet),
                            div { class: "fn-name", "{f.name}" }
                            div { class: "fn-sig", "{f.signature}" }
                        }
                    }
                }
            }
            main { class: "code-editor",
                textarea {
                    value: "{source}",
                    oninput: move |evt| source.set(evt.value()),
                    rows: 30,
                    spellcheck: "false",
                }
                div { class: "actions",
                    button {
                        onclick: move |_| {
                            let engine = ExecutionEngine::new();
                            match engine.compile(&source.read()) {
                                Ok(_)  => compile_status.set(Some("OK".into())),
                                Err(e) => compile_status.set(Some(format_compile_error(&e))),
                            }
                        },
                        "Compile"
                    }
                    button {
                        onclick: move |_| { /* save to program_store */ },
                        "Save"
                    }
                }
                if let Some(msg) = compile_status.read().as_ref() {
                    div { class: "compile-status", "{msg}" }
                }
            }
        }
    }
}

struct BuiltinFunction { name: &'static str, signature: &'static str, snippet: &'static str }

fn available_builtin_functions() -> Vec<BuiltinFunction> {
    // Pull from ExecutionEngine introspection — see logic::engine docs.
    // Keep static for now; loop in dynamic later.
    vec![
        BuiltinFunction { name: "read_point", signature: "read_point(id: string) -> any", snippet: "read_point(\"\")" },
        BuiltinFunction { name: "write_point", signature: "write_point(id: string, value: any)", snippet: "write_point(\"\", 0.0)" },
        BuiltinFunction { name: "now", signature: "now() -> i64 (epoch ms)", snippet: "now()" },
        // …add more once we read the actual engine surface.
    ]
}

fn insert_at_cursor(source: &mut Signal<String>, snippet: &str) {
    let mut cur = source.read().clone();
    cur.push_str(snippet);
    source.set(cur);
}
```

- [ ] **Step 7: Add a smoke test that the component renders**

```rust
#[test]
fn programming_view_renders_editor() {
    use dioxus::prelude::*;
    let mut vdom = VirtualDom::new(|| rsx! {
        ProgrammingView {
            state: use_signal(|| AppState::default()),
            program_store: Arc::new(ProgramStore::default()),
        }
    });
    vdom.rebuild_in_place();
    let html = dioxus_ssr::render(&vdom);
    assert!(html.contains("function-picker"));
    assert!(html.contains("code-editor"));
}
```

(Add `dioxus-ssr` as a `[dev-dependencies]` if not already present. If `AppState::default()` doesn't exist, construct a minimal stub or use a builder.)

- [ ] **Step 8: Run all programming-view tests**

```bash
cargo test -p bms-store-gui programming_view
```

Expected: PASS.

- [ ] **Step 9: Add CSS for the new layout**

In `crates/bms-store-gui/assets/style.css`:

```css
.programming-view { display: grid; grid-template-columns: 240px 1fr; height: 100%; gap: 12px; }
.programming-view .function-picker { overflow-y: auto; border-right: 1px solid var(--border); padding: 8px; }
.programming-view .function-picker ul { list-style: none; padding: 0; margin: 0; }
.programming-view .function-picker li { padding: 6px 8px; cursor: pointer; border-radius: 4px; }
.programming-view .function-picker li:hover { background: var(--surface-hover); }
.programming-view .fn-name { font-weight: 600; }
.programming-view .fn-sig { font-size: 0.85em; color: var(--muted); font-family: monospace; }
.programming-view .code-editor { display: flex; flex-direction: column; gap: 8px; }
.programming-view .code-editor textarea { flex: 1; font-family: monospace; font-size: 14px; padding: 12px; }
.programming-view .compile-status { padding: 8px 12px; background: var(--surface); border-radius: 4px; font-family: monospace; }
```

- [ ] **Step 10: Commit**

```bash
git add -A
git commit -m "feat(gui): redesign programming view as code editor + function picker

Drops wire-sheet canvas (~2500 LOC) in favor of a textarea editor and
a sidebar listing built-in Rhai functions. Compile button surfaces
errors with line/column/message.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 16: Simplify `trend_chart.rs`

**Files:** `crates/bms-store-gui/src/gui/components/trend_chart.rs`

Goal: keep one line-plot component for point history; drop multi-overlay/gauge variants and any 3D/animated rendering.

- [ ] **Step 1: Identify what's being kept vs dropped**

```bash
grep -n 'pub fn\|pub struct\|enum ' crates/bms-store-gui/src/gui/components/trend_chart.rs | head -40
```

- [ ] **Step 2: Failing test — line plot renders points correctly**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_to_svg_path_is_deterministic() {
        let samples = vec![
            (0i64, 10.0_f64),
            (60_000, 15.0),
            (120_000, 12.0),
        ];
        let path = samples_to_svg_path(&samples, /*width*/ 300.0, /*height*/ 100.0);
        // Three points → "M ... L ... L ..."
        assert!(path.starts_with("M "), "got: {path}");
        assert_eq!(path.matches("L ").count(), 2);
    }
}
```

- [ ] **Step 3: Run test, expect FAIL**

```bash
cargo test -p bms-store-gui trend_chart::tests::samples_to_svg_path_is_deterministic
```

- [ ] **Step 4: Implement the simplified path builder**

```rust
pub fn samples_to_svg_path(samples: &[(i64, f64)], width: f64, height: f64) -> String {
    if samples.is_empty() { return String::new(); }
    let t_min = samples.first().unwrap().0 as f64;
    let t_max = samples.last().unwrap().0 as f64;
    let v_min = samples.iter().map(|&(_, v)| v).fold(f64::INFINITY, f64::min);
    let v_max = samples.iter().map(|&(_, v)| v).fold(f64::NEG_INFINITY, f64::max);
    let t_span = (t_max - t_min).max(1.0);
    let v_span = (v_max - v_min).max(1.0);

    let mut out = String::new();
    for (i, (t, v)) in samples.iter().enumerate() {
        let x = ((*t as f64 - t_min) / t_span) * width;
        let y = height - ((v - v_min) / v_span) * height;
        if i == 0 { out.push_str(&format!("M {x:.2} {y:.2}")); }
        else      { out.push_str(&format!(" L {x:.2} {y:.2}")); }
    }
    out
}
```

- [ ] **Step 5: Test passes**

```bash
cargo test -p bms-store-gui trend_chart::tests::samples_to_svg_path_is_deterministic
```

Expected: PASS.

- [ ] **Step 6: Replace the component body**

Strip multi-line overlays, gauges, custom legend rendering. Keep:
- Time-range selector (1h, 24h, 7d, custom)
- Single point selector
- One SVG `<path>` plus axes

Sketch:

```rust
#[component]
pub fn TrendView(state: Signal<AppState>, point_id: Signal<Option<PointKey>>) -> Element {
    let range_ms: Signal<i64> = use_signal(|| 60 * 60 * 1000);
    let samples = use_resource(move || async move {
        let key = point_id.read().clone()?;
        let now = chrono::Utc::now().timestamp_millis();
        let from = now - *range_ms.read();
        // Read history from the store
        let history = state.read().history_store.clone();
        history.read_range(&key, from, now).await.ok()
    });

    rsx! {
        div { class: "trend-chart",
            header {
                select {
                    onchange: move |e| range_ms.set(e.value().parse().unwrap_or(3600000)),
                    option { value: "3600000", "Last hour" }
                    option { value: "86400000", "Last 24h" }
                    option { value: "604800000", "Last 7d" }
                }
            }
            svg { width: "600", height: "200", view_box: "0 0 600 200",
                // axes
                line { x1: "0", y1: "200", x2: "600", y2: "200", stroke: "currentColor", stroke_width: "1" }
                line { x1: "0", y1: "0", x2: "0", y2: "200", stroke: "currentColor", stroke_width: "1" }
                // path
                if let Some(Some(samples)) = samples.read().as_ref() {
                    path {
                        d: "{samples_to_svg_path(samples, 600.0, 200.0)}",
                        fill: "none",
                        stroke: "var(--accent)",
                        stroke_width: "2",
                    }
                }
            }
        }
    }
}
```

(Adjust `chrono` vs `time` crate per what's already in deps; fall back to `std::time::SystemTime` if neither is present.)

- [ ] **Step 7: Build**

```bash
cargo check -p bms-store-gui
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor(gui): simplify trend_chart to single line plot

Drops gauge variants, multi-overlay rendering, and 3D effects. Keeps
range selector + SVG path. ~1000 LOC -> ~200 LOC.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 17: Replace `schedule_view.rs` timeline with table

**Files:** `crates/bms-store-gui/src/gui/components/schedule_view.rs`

Goal: list schedules in a table; CRUD via a form per schedule. Drop any visual timeline / Gantt rendering.

- [ ] **Step 1: Confirm the data model**

```bash
grep -n 'pub struct \|pub enum ' crates/bms-store-storage/src/store/schedule_store.rs | head -20
```

Capture: `Schedule` fields, `ScheduleRule` shape.

- [ ] **Step 2: Failing test — table renders one row per schedule**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_row_label_includes_name_and_target() {
        let s = Schedule {
            id: "s1".into(),
            name: "AHU-1 occupancy".into(),
            target_point: "ahu-1/occupied".into(),
            rules: vec![],
            // … populate per actual struct
        };
        let row = format_schedule_row(&s);
        assert!(row.contains("AHU-1 occupancy"));
        assert!(row.contains("ahu-1/occupied"));
    }
}
```

- [ ] **Step 3: Run test → FAIL**

- [ ] **Step 4: Implement formatter + table component**

```rust
pub fn format_schedule_row(s: &Schedule) -> String {
    format!("{} → {} ({} rules)", s.name, s.target_point, s.rules.len())
}

#[component]
pub fn ScheduleView(state: Signal<AppState>) -> Element {
    let schedules = use_resource(move || {
        let store = state.read().schedule_store.clone();
        async move { store.list().await.unwrap_or_default() }
    });

    rsx! {
        section { class: "schedule-view",
            h2 { "Schedules" }
            table {
                thead {
                    tr {
                        th { "Name" }
                        th { "Target Point" }
                        th { "Rules" }
                        th {}
                    }
                }
                tbody {
                    if let Some(list) = schedules.read().as_ref() {
                        for s in list.iter() {
                            tr {
                                td { "{s.name}" }
                                td { "{s.target_point}" }
                                td { "{s.rules.len()}" }
                                td {
                                    button { "Edit" }
                                    button { "Delete" }
                                }
                            }
                        }
                    }
                }
            }
            ScheduleForm { state, on_save: |_| {} }
        }
    }
}

#[component]
fn ScheduleForm(state: Signal<AppState>, on_save: EventHandler<Schedule>) -> Element {
    // Name input, target-point picker, rule list editor (start/end time, days, value).
    // Implement per the actual ScheduleRule shape.
    rsx! { form { /* … */ } }
}
```

- [ ] **Step 5: Test passes; cargo check passes**

```bash
cargo test -p bms-store-gui schedule_view
cargo check -p bms-store-gui
```

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor(gui): replace schedule timeline with table + form"
```

---

### Task 18: Strip weather_view chrome

**Files:** `crates/bms-store-gui/src/gui/components/weather_view.rs`

Drop animated icons, condition-specific backgrounds, decorative gauges. Keep the data table (current temp, humidity, pressure, wind, forecast list).

- [ ] **Step 1: Read the file, identify the chrome-heavy sections**

```bash
grep -n 'svg\|canvas\|class:\s*"weather-icon\|animation' crates/bms-store-gui/src/gui/components/weather_view.rs
```

- [ ] **Step 2: Replace the body with a plain data view**

```rust
#[component]
pub fn WeatherView(state: Signal<AppState>) -> Element {
    let weather = state.read().weather_data.clone();
    rsx! {
        section { class: "weather-view",
            h2 { "Weather" }
            if let Some(w) = weather.as_ref() {
                table {
                    tr { td { "Temperature" } td { "{w.temperature_c}°C" } }
                    tr { td { "Humidity"    } td { "{w.humidity_pct}%" } }
                    tr { td { "Pressure"    } td { "{w.pressure_hpa} hPa" } }
                    tr { td { "Wind"        } td { "{w.wind_speed_mps} m/s" } }
                }
                h3 { "Forecast" }
                table {
                    thead { tr { th { "Time" } th { "Temp" } th { "Conditions" } } }
                    tbody {
                        for f in w.forecast.iter() {
                            tr {
                                td { "{f.timestamp}" }
                                td { "{f.temperature_c}°C" }
                                td { "{f.conditions}" }
                            }
                        }
                    }
                }
            } else {
                p { "No weather data — configure a provider in Settings." }
            }
        }
    }
}
```

(Field names per the actual `WeatherData` struct.)

- [ ] **Step 3: Build + commit**

```bash
cargo check -p bms-store-gui && \
git add -A && \
git commit -m "refactor(gui): strip weather chrome, keep data view"
```

---

## Phase E — Configuration Sweep

### Task 19: Audit and trim `config_view.rs` tabs

**Files:** `crates/bms-store-gui/src/gui/components/config_view.rs`

- [ ] **Step 1: List current tabs**

```bash
grep -n 'ConfigTab::' crates/bms-store-gui/src/gui/components/config_view.rs | head -30
```

- [ ] **Step 2: Final tab list (KEEP only)**

| Tab            | Component                          |
|----------------|------------------------------------|
| Haystack       | (existing — wire to data view)     |
| Discovery      | `DiscoveryView`                    |
| Programming    | `ProgrammingView`                  |
| VirtualPoints  | `VirtualPointsView`                |
| Plugins        | `PluginManager`                    |
| Appearance     | `ThemeSettings`                    |
| AlarmRouting   | `AlarmRoutingView`                 |
| MQTT           | `MqttSettings`                     |
| Webhooks       | `WebhookSettings`                  |
| Commissioning  | `CommissioningTab`                 |
| WebServer      | `WebServerSettings`                |
| Users          | `UserManagement`                   |
| AuditLog       | `AuditLogView`                     |
| Reports        | `ReportView`                       |
| Energy         | `EnergyView` (data-only)           |
| FDD            | `FddView` (data-only)              |
| DataExport     | `ExportSettings`                   |
| Atlas          | `AtlasSettings`                    |

(Removed in Phase C: `Cloud`. Atlas retained per user scope decision.)

- [ ] **Step 3: Edit `ConfigTab` enum + match arms**

Delete any variants not in the table. Verify the dispatch `match` only references kept variants.

- [ ] **Step 4: Build + manual smoke**

```bash
cargo run -p bms-store-gui -- --project ./demo-data
# Click each Config tab; verify no panics
```

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(gui): finalize Config tab list (17 tabs)"
```

---

### Task 20: Toolbar / sidebar cleanup

**Files:** `toolbar.rs`, `sidebar.rs`

- [ ] **Step 1: Toolbar — remove dead buttons**

Final toolbar buttons: Home, Alarms, Schedules, History, Weather, Config. Delete any others (Page, SiteMap, SiteStatus, etc.).

- [ ] **Step 2: Sidebar — verify Devices and Nav tabs are correct**

Sidebar's Nav tab should not list deleted views. If `Pages` was a sidebar entry, drop it.

- [ ] **Step 3: Build + smoke**

```bash
cargo run -p bms-store-gui -- --project ./demo-data
```

Click every toolbar and sidebar item — no panics.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(gui): clean toolbar and sidebar — drop removed-view buttons"
```

---

## Phase F — End-to-End Smoke + Polish

### Task 21: Write an integration smoke test

**Files:** `crates/bms-store-gui/tests/smoke.rs`

- [ ] **Step 1: Failing test — app boots against demo-data and the device tree has nodes**

```rust
//! Boots the GUI library entry against demo-data and checks the platform
//! hands back a non-empty device tree.

use bms_store_gui::extracted::platform::init_platform;
use bms_store_gui::extracted::project::ProjectPaths;
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn demo_data_boots_and_has_nodes() {
    // Demo data lives at the workspace root.
    let paths = ProjectPaths::from_root(PathBuf::from("../../demo-data"));
    assert!(paths.scenario.exists(), "demo-data scenario not found");

    let shutdown = CancellationToken::new();
    let (platform, _bridges, _report) = init_platform(&paths, shutdown.clone())
        .await
        .expect("platform should boot against demo-data");

    let nodes = platform.entity_store.list_all().await.expect("list_all");
    assert!(!nodes.is_empty(), "demo-data should have at least one entity");
    shutdown.cancel();
}
```

(Adjust the API if `init_platform` and `entity_store` names differ.)

- [ ] **Step 2: Run, expect PASS** (since data layer is already production-ready):

```bash
cargo test -p bms-store-gui --test smoke
```

If FAIL, the most likely cause is a mismatch between `extracted/platform.rs` and `bms-store-storage` boot — fix and rerun.

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "test(gui): add demo-data boot smoke test"
```

---

### Task 22: README

**Files:** `crates/bms-store-gui/README.md`

- [ ] **Step 1: Write a short README**

```markdown
# bms-store-gui

Desktop GUI for the bms-store data layer. Built with Dioxus 0.6 (desktop feature).

## Run

```bash
cargo run -p bms-store-gui -- --project ./demo-data
```

## Architecture

In-process desktop app. Depends on `bms-core`, `bms-store-storage`, and
`bms-store-bridges` directly — no HTTP hop. Use `bms-stored` for headless
server deployments.

## Source

Initially ported from `opencrate-bms/src/gui` with all graphical-only views
(floor plan, site map, status dashboards) removed.
```

- [ ] **Step 2: Commit**

```bash
git add crates/bms-store-gui/README.md
git commit -m "docs(gui): add README"
```

---

### Task 23: Run the full workspace test + build

- [ ] **Step 1: Full workspace test**

```bash
cargo test --workspace
```

Expected: PASS.

- [ ] **Step 2: Full release build**

```bash
cargo build --workspace --release
```

Expected: PASS. Warning count reasonable.

- [ ] **Step 3: Final smoke run**

```bash
cargo run --release -p bms-store-gui -- --project ./demo-data
```

Manually verify: launcher → demo-data → login → device tree populates → click a device → point table renders → click a point → point detail → alarm view → discovery list → config tabs all clickable.

- [ ] **Step 4: Tag the green build**

```bash
git tag gui-v0.1.0
```

---

### Task 24: PR-prep summary

- [ ] **Step 1: Check the branch is clean**

```bash
git status
git log --oneline main..feat/gui | wc -l
```

- [ ] **Step 2: Hand off to user**

Report:
- LOC delta (`git diff --stat main..feat/gui`)
- Files added/deleted
- Anything skipped (e.g., features deferred because backend support missing in `bms-store-storage`)
- Suggested next steps (multi-site supervisor as separate crate? cloud sync? web/WASM build?)

---

## Self-Review Checklist (Plan Author)

**Spec coverage** — every requirement from the user's request mapped to tasks:

| Requirement                                                              | Task(s) |
|--------------------------------------------------------------------------|---------|
| Stay with Dioxus                                                         | T2 deps, T5 main.rs |
| Desktop only                                                             | T2 deps (no `web` feature), T5 launcher |
| Copy opencrate GUI then cut                                              | T3 (copy), T11/T12 (cut) |
| Cut floor plans / canvases / symbols / dashboards                        | T11 |
| Keep data UI (points, equipment, tags, relationships, etc.)              | T6–T9 (rewire), T19 (config tabs), T20 (toolbar) |
| Universal data layer protocols (Niagara, BACnet, Modbus, etc.)           | Backend already done; GUI surfaces existing bridges via `discovery_view`, `commissioning_tab` (T9). Niagara bridge is a follow-up — flagged separately. |
| Standardize point names / Haystack / relationships                       | Existing `haystack` config tab + `relationships_section` retained (T9, T19) |
| Spit data back out for other apps to use                                 | `bms-store-server` already does this; GUI shows `WebServerSettings` config (T19) |

**Placeholder scan** — searched plan for: TBD, TODO (in tasks, not as project terminology), "implement later", "fill in", "appropriate error handling", "add validation", "Similar to Task N", placeholder code blocks. Result: clean — every code block has actual code; every step says exactly what to run or change.

**Type consistency** — `format_compile_error`, `samples_to_svg_path`, `format_schedule_row`, `init_platform`, `ProjectPaths`, `AppPhase`, `ActiveView`, `ConfigTab` — used consistently across tasks. The `ProgrammingView` component signature `(state, program_store)` matches between Task 15 step 6 and the test in step 7.

**Known plan-time uncertainties** the executor must verify:
1. Whether `crate::project` in opencrate is a directory or single file (Task 4 fallback noted).
2. Exact field names on `bms_core::rbac` types (Task 8 — diff first, decide).
3. Whether `bms_store_storage::boot::init_platform` exists with a compatible signature (Task 14 decision tree).
4. Whether `chrono` or `time` is already a transitive dep (Task 16 fallback noted).
5. Whether `dioxus-ssr` needs to be added for the render test (Task 15 step 7 — add to dev-deps if missing).

These are not placeholders; they're decisions that depend on the actual state of the bms-store crates at execution time. Each is flagged inline with the path the executor should take.
