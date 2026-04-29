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
(floor plan, site map, status dashboards, weather widget chrome) removed.
Multi-site supervisor views were also deferred; the `AppPhase` enum keeps a
shape that allows them to return as a future feature.

Atlas taxonomy integration is retained as the initial-pass naming-hint engine.
