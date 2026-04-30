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
