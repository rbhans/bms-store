# bms-store CHANGELOG

## Unreleased — ease-of-use polish

Backend foundations + first GUI wiring for the operator-facing surface.

### Added

- **`Event::Toast { level, message, detail, source }`** in `bms-core`. Use
  `Event::toast(level, source, message)` to emit operator-visible alerts
  through the existing event bus. BACnet scan failures now publish one.
- **GUI toast banner** — `ToastBanner` component subscribes to
  `Event::Toast`, stacks notifications top-right, auto-dismisses Info/Warn
  after 6s, sticks Error until dismissed. Mounted globally in `app.rs`.
- **Bridge config CRUD store** — new `BridgeStore`
  (`bms-store-storage::store::bridge_store`) holds BACnet network +
  Modbus bus configs in SQLite. Lets the GUI register/edit/delete
  bridges instead of hand-editing `scenario.json` and restarting.
  Mutations emit a `Toast(Warn, "restart bms-store to activate")`.
- **REST endpoints for bridge config:**
  - `GET/POST   /api/bridges/bacnet`
  - `GET/PUT/DELETE /api/bridges/bacnet/{id}`
  - `GET/POST   /api/bridges/modbus`
  - `GET/PUT/DELETE /api/bridges/modbus/{id}`
- **Audit actions** — `CreateBacnetNetwork`, `UpdateBacnetNetwork`,
  `DeleteBacnetNetwork`, `CreateModbusBus`, `UpdateModbusBus`,
  `DeleteModbusBus`.
- **Bulk entity ops (one SQLite transaction each):**
  - `EntityStore::set_tags_batch(ids, tags)` — apply N tags to M entities
  - `EntityStore::remove_tags_batch(ids, tag_names)`
  - `EntityStore::set_ref_batch(ids, ref_tag, target)` — assign N entities
    to one parent (e.g. 50 points to one AHU)
- **REST endpoints for bulk ops:**
  - `POST /api/entities/tags-batch`
  - `POST /api/entities/tags-batch/remove`
  - `POST /api/entities/refs-batch`
- **Auto-tag dry-run:**
  - `DiscoveryService::preview_device_tags(id)` returns the tags
    `accept_device` would apply, with `source` (atlas / heuristic) +
    `confidence` per row, **without writing to storage**.
  - `DiscoveryService::accept_device_with_options(id, AcceptOptions)` —
    new opt-in entry point; current default matches old `accept_device`.
  - `GET /api/discovery/devices/{id}/preview-tags` exposes it over REST.
- **Top-level `README.md`** with quick-start + crate map.
- **Top-level `CHANGELOG.md`** (formerly `crates/bms-store-gui/PR_SUMMARY.md`).

### Removed (data-layer trim, see "Ease-of-use audit" notes)

- bms-haystack stripped to ontology + auto-tag only — codecs, HTTP facade,
  filter parser, runtime xeto loader, schema validator deleted. Lives in
  git history at commit `befdb12` and earlier on `main` if ever needed.

---

# bms-store GUI — PR Summary (v0.2.0)

Trims bms-store from a full BMS app to a pure universal data layer per
the project mission. After v0.1.0 (initial port from opencrate-bms with
graphical-only views removed), this revision drops consumer-app features
at all layers: GUI views, API routes, backend stores, Event variants.

## What's in (data-layer scope)

- **Ingestion:** BACnet + Modbus + MQTT bridges, discovery, commissioning.
- **Standardization:** Haystack tagging, Atlas naming hints, virtual points.
- **Relationships:** Site → Building → Floor → Space → Equipment → Point hierarchy.
- **Egress:** REST API (kept routes only), webhooks, MQTT publish, data exports.
- **Admin:** Users, roles, audit log, theme, programming (logic engine for
  derived data), web server config, plugin manager.

## What's out (moved to "consumer apps build their own")

- **Alarm engine + routing** — gone (alarm_store, alarm routes, alarm UI,
  notification module, AlarmRouter).
- **Schedule engine** — gone (schedule_store, schedule routes, schedule UI,
  ScheduleWritten Event variant).
- **Energy analytics** — gone (energy/, energy_store, energy routes/UI).
- **FDD (Fault Detection & Diagnostics)** — gone (fdd/, fdd_store, fdd routes/UI).
- **Reports** — gone (reporting/, report_store, report routes/UI).
- **Weather adapters** — gone (weather/, weather_view).
- **Cloud sync** — gone (cloud/, cloud_store, cloud routes).
- **Trend chart visualization** — gone (history backend kept; consumer apps
  do their own charting).

## Stats vs v0.1.0

- Files changed across branch: 140 (vs main baseline)
- LOC added: 36713 / LOC removed: 25764 (net +10949 vs main)
- Backend modules removed: reporting, energy, fdd, notification, weather, cloud
- Stores removed: alarm, schedule, report, energy, fdd, notification, cloud
- API routes removed: 6 (alarms, schedules, reports, energy, fdd, cloud)
- GUI components removed: 10 (alarm_view, alarm_routing_view, schedule_view,
  report_view, energy_view, fdd_view, weather_view, trend_chart, +2 aggregators)
- Config tabs: 17 → 14
- Toolbar buttons: 6 → 2 (Home, Config)
- Event enum variants: dropped AlarmRaised, AlarmCleared,
  AlarmAcknowledged, ScheduleWritten
- Cargo deps pruned: lettre, rustls-native-certs, jsonwebtoken (storage),
  aes-gcm, flate2, tar, ed25519-dalek, blake3 (gui), `cloud` feature

## What stayed unchanged

- All discovery + commissioning workflows.
- All Haystack/Atlas tagging.
- All virtual points + logic engine.
- All BACnet/Modbus device-level inspection (bacnet_device_*, modbus_device_*).
- Project launcher, login, admin setup, user management, audit log.
- Plugin manager.
- History backend (`history_store`, `/api/history`) — consumer apps can
  chart on their own.

## Verification

- `cargo build --workspace --release` ✅
- `cargo test --workspace` ✅ (349 tests pass)
- `cargo run -p bms-store-gui -- --project ./demo-data` ✅ (boots demo-data,
  loads stores + bridges, renders main view — "storage runtime booted" +
  "bridge runtime booted" log lines confirmed, no panic)

## Deferred (future plans)

- Niagara connector — substantial subsystem, separate plan.
- API contract docs / OpenAPI spec for downstream consumer apps.
- Webhook event schema versioning.

---

# bms-store GUI — PR Summary (v0.3.0) — Standardization UX

This release (batches 1–3 + wrap-up) adds the full standardization and
UX layer on top of the v0.2.0 data-layer foundation. Every change is
scoped to the data layer's own UX — no consumer-app concerns were added.

## Backend foundations

- **Tag validation engine** (`crates/bms-store-bridges/src/haystack/validation.rs`)
  — rule-based validation with `ValidationWarning` results per tag.
- **Unit normalization** (`discovery_utils`) — normalizes common BMS unit
  strings (°F, degF, Fahrenheit, etc.) to canonical forms at ingest time.
- **Naming rule store** (`crates/bms-store-storage`) — CRUD for persisted
  find/replace naming rule sets; surfaces via `NamingRuleStore`.
- **Duplicate detection** (`discovery_view`) — cross-protocol duplicate
  grouping with configurable match threshold.
- **Shared `PreviewModal`** (`crates/bms-store-gui/src/gui/components/preview_modal.rs`)
  — reusable before/after diff preview for any bulk operation.
- **Entity-store validation warnings** — `entity_store` emits
  `ValidationWarning` on writes so callers see issues at commit time.

## Haystack tab features

- **Apply-prototype UI** — batch tag editor + point detail both expose a
  one-click "apply prototype" button to stamp a Haystack prototype onto
  selected points.
- **Assign-to-equip/space widget** — inline assignment of a Haystack
  entity to an equipment or space reference directly from the tag editor.
- **Inline tag validation warnings** — warnings from the validation engine
  appear inline in the tag editor; blocking tags are highlighted in amber.
- **Batch-tag dry-run preview** — the batch apply path wraps in
  `PreviewModal`, showing the full diff before writing.

## Discovery + point-table features

- **Related-equipment surfacing** — discovery view groups related equipment
  (shared namespace prefix, shared parent node) and surfaces them as a
  collapsible "Related Equipment" panel.
- **Cross-protocol duplicate review** — duplicate candidates are shown in a
  dedicated pane with Accept/Dismiss per pair; accepted pairs write a
  `duplicateOf` relationship to the node store.
- **Bulk rename with regex + preview** — point table bulk-rename modal
  supports literal or full regex find/replace (via `regex` crate), plus
  prefix/suffix. Invalid regex patterns surface an inline error and
  disable the Preview button. Live preview runs through the same path.
- **Saved naming rules** — naming rules persist to `NamingRuleStore` and
  can be reloaded in later sessions.
- **Server-side validation warnings** — point table rows surface per-point
  `ValidationWarning` badges; warnings come from entity-store write hooks.

## Stats (vs gui-v0.2.0)

- **Commits:** 15
- **Files changed:** 21
- **LOC:** +3 901 / −194 (net +3 707)
- **Tests:** 209 passing (up from 207 at v0.2.0 baseline)

## Known follow-ups

- **Auto-tag redesign** — the current keyword+unit heuristic engine is
  a known limitation. See the `//!` redesign note in
  `crates/bms-store-bridges/src/haystack/auto_tag.rs` for the full
  five-point plan (Atlas primary → LLM fallback → heuristics tertiary,
  confidence scores, data-driven rule set). Out of scope for v0.3.0;
  should be a separate plan driven by downstream consumer feedback.
- **Niagara connector** — still deferred; substantial subsystem.

## Verification

- `cargo test --workspace` ✅ (209 tests pass)
- `cargo build --workspace --release` ✅
- Boot smoke (10 s run against `./demo-data`) — no panic log, clean exit.

---

# v0.4.0 — Commissioning subsystem removed

Follow-up cleanup. The earlier audit miscategorized commissioning as
"device onboarding" and kept it as data-layer essential. On re-read it
is clearly a consumer-app workflow: per-device installer sign-off,
per-point verification status (NotStarted/InProgress/Verified/Failed/
Deferred), `sign_off_session(device_id, username)`, CSV export of
installer progress. That's installation project management, not data
layer.

Same removal pattern as alarms/schedules/reports/etc.

## Removed

- Backend store: `bms-store-storage::store::commissioning_store`
- GUI components: `commissioning_tab.rs`, `commissioning_overview.rs`
- Config tab: `ConfigSection::Commissioning`
- Boot wiring: removed from `boot_project`, `StorageRuntime`,
  `SharedPlatform`, `AppState`, `SiteContext`
- Permission: `Permission::ManageCommissioning` from `bms-core::rbac`
- 5 commissioning-related `AuditAction` variants from `audit_store`
- `DeviceDetailTab::Commission` from discovery utils + detail pane
- API route: not present (no `commissioning.rs` in routes)

## Stats (vs gui-v0.3.0)

- **Commits:** 1
- **Files changed:** 15
- **LOC:** +1 / −2199 (net −2198)
- **Tests:** 389 passing
- **ConfigSection variants:** 14 → 13

## What stayed

- Discovery (find devices) — data-layer, kept
- Acceptance flow (decide which discovered devices to track) — kept
- Everything else from v0.3.0

---

# v0.5.0 — Tier-1 data-layer completion + Tier-2 GUI surfacing

The previous releases trimmed bms-store to a clean data-layer scope and built
out the standardization UX. v0.5.0 closes the foundational data-layer gaps
identified in the post-v0.4.0 evaluation, and surfaces the operational backend
features that previously had no GUI.

## Tier-1: foundational data-layer mechanics (was missing)

### Value / state normalization
Per-point `ValueMap` translates raw protocol values to canonical tokens
(`0/1` → `OFF/ON`, `0/1` → `OPEN/CLOSED`, etc.). Stored as JSON in the entity
`enum` tag. API returns canonical by default; `?raw=true` opts into raw.
- Editor: per-point + bulk preset application via `BatchTagEditor`.
- Pre-built presets: ON/OFF, OPEN/CLOSED, OCCUPIED/UNOCCUPIED, AUTO/MANUAL.

### Haystack-filter query API
Real recursive-descent parser for the Haystack-4 filter grammar subset
(Has, Cmp ==/!=/<≤>/≥, And, Or, Not, parens, all literal types).
- New endpoint: `GET /api/entities?filter=<expr>`.
- Filter input wired into the point table in the desktop GUI.

### Functional relationships
`supplyRef`, `returnRef`, `connectedTo` (Haystack-4 conventions). Helpers:
`find_referrers`, `walk_supply_chain`, `walk_return_chain`,
`validate_relationships`. Cycle-safe walks.
- Endpoints: `GET /api/entities/<id>/referrers?tag=`, `/supply-chain`,
  `/return-chain`, `GET /api/relationships/issues`.
- UI: editor on equip detail; supply-chain breadcrumb on point detail.

### Quality / freshness propagation
- `Event::QualityChanged { node_id, flags, reason }` and
  `Event::BridgeQualityChanged { bridge_type, network_id, reason, ... }`
  added to `bms-core`.
- `QualityReason`: Stale, BridgeDown, Recovered, ManualOverride, OutOfService.
- Stale detector background task: 60 s sweep, edge-triggered, per-point
  `updateRate` tag, graceful shutdown via CancellationToken.
- Bridge-down emits one bulk event per device transition (not per point).
- Live quality badges on point detail subscribe to the event stream.

## Tag coverage report
- New Config tab: `Coverage`. Project-wide score, per-equipment-type breakdown,
  validation issues breakdown.
- Coverage scoring: 0 / 50 / 75 / 100 % based on tag presence + validation pass.

## Tier-2: GUI surfacing of existing backend features
- **Health dashboard** — bridges/stores/background tasks status (auto-refresh 5s).
- **Override management** — list active overrides, per-row Release, Release All.
- **Backup / restore** — Create Backup, list snapshots, Restore (destructive
  confirmation modal).
- **Retention config** — informational view of the hot/warm/cold/archive tiers
  (currently fixed in source; runtime config is a follow-up).
- **API keys** — CRUD view, create-shows-secret-once modal.

## Stats (vs gui-v0.4.0)

- Commits: 16
- Files changed: 32
- LOC: +5349 / −33
- Tests: 458 passing (up from ~389)
- Config tabs: 13 → 19

## Known follow-ups

- `fetch_update_rate` in stale detector currently returns the default poll
  interval; should batch-load `updateRate` tags at sweep start
  (`bms-store-storage/src/quality/stale_detector.rs`).
- Retention tier windows are compile-time constants; runtime config UI is
  informational only. Persistable config is a follow-up.
- Auto-tag heuristic engine still flagged for full redesign (see
  `crates/bms-store-bridges/src/haystack/auto_tag.rs` doc block).
- Niagara connector still deferred.

## Verification

- `cargo test --workspace` ✅
- `cargo build --workspace --release` ✅
- Boot smoke against `./demo-data`: storage + bridges + stale-detector all
  start cleanly within 2s; no panic log written.
