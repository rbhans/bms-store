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
