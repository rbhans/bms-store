# bms-store

Standalone universal data layer for building management. Ingests BMS data
(BACnet, Modbus, MQTT), tags it with [Project Haystack 5](https://project-haystack.org)
semantics, and serves it to consumer apps via REST / WebSocket.

## Quick start

```bash
# Headless data daemon (HTTP + WebSocket on :8080)
cargo run --bin bms-stored -- --data-dir demo-data

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

bms-store is a **data layer**. The following intentionally live in consumer
apps that build on top:

- Alarm / FDD engines, schedule engine, energy analytics, reports
- Trend chart UI (history backend is here; charting is the consumer's)
- Cloud sync, weather adapters

See [CHANGELOG.md](CHANGELOG.md) for the v0.2.0 trim that established this
boundary.

## Documentation

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
