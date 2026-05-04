# bms-store

Standalone universal data layer for building management. Discovers BMS
points (BACnet, Modbus; MQTT outbound), standardizes them with semantic
tags (Project Haystack vocabulary by default — consumers do not need to
adopt Haystack), models the Site → Building → Floor → Space → Equipment
→ Point hierarchy, and serves the result to consumer apps via REST /
WebSocket / MQTT. Supports write-back (setpoints, BACnet priority
overrides, relinquish) with audit + RBAC. Threshold-based alarming
ships as a sibling crate (`bms-store-alarms`) that consumes the same
public API.

See [docs/v1-criteria.md](docs/v1-criteria.md) for the locked v1.0
scope and done criteria.

## Quick start

```bash
# Headless data daemon (HTTP + WebSocket on :8080)
cargo run --bin bms-stored -- --project ./demo-data

# Desktop GUI (Dioxus)
cargo run -p bms-store-gui -- --project ./demo-data
```

Demo data includes 58 discoverable devices (mocked BACnet + Modbus). Default
login: see `demo-data/users.json`.

## Workspace

| Crate | Purpose |
|-------|---------|
| [`bms-core`](crates/bms-core/) | Shared types, event bus, RBAC, plugin/protocol traits |
| [`bms-store-domain`](crates/bms-store-domain/) | Wire DTOs for HTTP/WebSocket APIs |
| [`bms-haystack`](crates/bms-haystack/) | Project Haystack 5 ontology + auto-tagging |
| [`bms-store-storage`](crates/bms-store-storage/) | SQLite stores + background services |
| [`bms-store-bridges`](crates/bms-store-bridges/) | BACnet / Modbus / MQTT bridges + discovery |
| [`bms-store-server`](crates/bms-store-server/) | HTTP + WebSocket API |
| [`bms-store-client`](crates/bms-store-client/) | Rust client SDK |
| [`bms-store-gui`](crates/bms-store-gui/) | Dioxus desktop UI |

## Architecture (one paragraph)

Bridges discover devices and stream values into `PointStore` (in-memory) and
`HistoryStore` (SQLite). `EntityStore` holds the typed Site → Building → Floor
→ Space → Equipment → Point hierarchy and the tag/ref graph. The HTTP API
exposes filtered reads (`/api/entities?filter=ahu`), point writes,
overrides, history queries, discovery actions, and a WebSocket event stream
at `/ws`. The GUI is an in-process Dioxus desktop app that talks directly to
the storage stores (no HTTP hop) — `bms-stored` is the headless variant for
remote consumers.

## What's not here

bms-store is a **data layer + write-back surface + reference alarm
engine sibling**. The following intentionally live in separate consumer
apps that build on top:

- Advanced FDD (fault detection / diagnostics) beyond simple thresholds
- Schedule engine, energy analytics, reports
- Trend chart UI (history backend is here; charting is the consumer's)
- Cloud sync, weather adapters

The threshold-based alarm engine ships as a sibling crate
(`bms-store-alarms`) that uses only the public REST/WS API — it is the
reference for how a consumer app integrates with bms-store.

See [CHANGELOG.md](CHANGELOG.md) for the v0.2.0 boundary trim and
[docs/v1-criteria.md](docs/v1-criteria.md) for v1.0 scope.

## Documentation

- [docs/api-integration.md](docs/api-integration.md) — **consumer
  integration guide** (REST + WebSocket reference, auth, DTOs, error
  model, recommended UI flow). Start here when building an app that
  talks to bms-store.
- [docs/v1-criteria.md](docs/v1-criteria.md) — locked v1.0 scope and
  done criteria
- [CHANGELOG.md](CHANGELOG.md) — release notes (v0.2.0 → current)
- [crates/bms-store-gui/README.md](crates/bms-store-gui/README.md) — GUI run/architecture
- [crates/bms-haystack/README.md](crates/bms-haystack/README.md) — tagging
- [docs/superpowers/](docs/superpowers/) — design plans

## Tests

```bash
cargo test --workspace
```

## License

MIT OR Apache-2.0 (workspace). Vendored Project Haystack xeto bundle at
`assets/xeto-master/` is AFL-3.0.
