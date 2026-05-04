# bms-store v1.0 Done Criteria

This document fixes the scope of v1.0 so reviews score against a stable
target instead of an ever-growing wishlist. Items outside this list are
v1.1+.

## Scope (what v1.0 IS)

bms-store v1.0 is a **building data layer** that:

1. **Discovers** points from real BMS devices (BACnet/IP, BACnet/MSTP,
   Modbus TCP, Modbus RTU). MQTT inbound is v1.1+.
2. **Standardizes** points by tagging (semantic taxonomy — Project Haystack
   used as the default vocabulary, but consumers receive opaque tag pairs
   and are not required to adopt Haystack) and by normalizing values
   (raw + canonical + units + quality + timestamps end-to-end).
3. **Models relationships** — Site → Building → Floor → Space → Equipment
   → Point hierarchy via `siteRef` / `buildingRef` / `floorRef` /
   `spaceRef` / `equipRef`, manually editable through the API and GUI.
4. **Exposes** data via REST + WebSocket + MQTT publish, against a
   stable wire schema (`bms-store-domain`) documented by an OpenAPI /
   AsyncAPI spec.
5. **Writes back** — setpoint writes, BACnet priority overrides, and
   relinquish/release with full audit trail and RBAC gating.
6. **Alarms** — threshold and quality-based alarm detection lives in a
   separate sibling crate (`bms-store-alarms`) that consumes the
   bms-store event stream and writes alarm entities back through the
   same public API. Alarm lifecycle: trip → notify (webhook) → ack →
   clear.

## Out of scope for v1.0 (later)

- MQTT inbound (devices publishing to bms-store)
- BACnet Secure Connect production hardening (basic support OK)
- Project Haystack 5 conformance certification (we use the vocabulary;
  we do not pursue formal listing)
- Cloud sync, weather adapters, energy analytics, FDD beyond simple
  thresholds, schedule engine, reports
- BTL (BACnet Testing Labs) listing
- Historian compression / cold tiering
- Multi-tenant data partitioning
- Pluggable taxonomies beyond Haystack (Brick Schema, custom)

## Done = ALL of:

### A. End-to-end against real hardware  **[EXTERNAL]**

1. **[EXTERNAL: requires real BACnet+Modbus hardware or external sim
   process]** Discover → accept → tag → normalize value → set equipRef
   → query history → consume via WS: full path passes against **1 real
   BACnet device** (or `bacnet-stack` server emulating one) and **1
   real Modbus device** (or `pymodbus` server). Smoke test scripted.
   *Codeable substitute shipped: in-process self-test binary
   (`bms-stored --selftest`) exercises the full path against
   in-process simulators — see § F.*
2. **[EXTERNAL: requires same hardware]** Write path: setpoint +
   override + relinquish round-trip against the same devices. Each
   write produces an audit row. Unauthorized writes denied at RBAC
   gate. Tested in CI against `pymodbus` + `bacnet-stack` in-process.
   *Codeable substitute: write path covered by the self-test binary
   against in-process sim.*
3. **[EXTERNAL: requires hardware + webhook receiver]** Alarm path: a
   threshold rule trips on a real value change → webhook fires within
   5 s → ack via API → cleared on return-to-normal.
   *Codeable substitute: same flow against in-process simulator + a
   localhost webhook collector — also part of the self-test.*

### B. Consumer integration

4. A 3rd-party reference consumer app (small Rust or TypeScript program,
   shipped in `examples/`) connects using only:
   - `bms-store-domain` DTOs (or generated TS types from OpenAPI)
   - The OpenAPI / AsyncAPI spec
   - An API key
   It must: subscribe WS, query REST history, write a setpoint,
   register and receive an alarm webhook. No reference to internal
   types.
   *Note: the consumer is the separate UI repo; this criterion is met
   when the UI repo successfully integrates against bms-store using
   only the published wire types.*

### C. Performance baseline

5. Bench harness publishes numbers for: 10 000 mock points at 1 Hz, 24 h
   history retention, 100 simultaneous WS subscribers.
   Targets:
   - Steady-state CPU < 25 % on a 4-core x86_64 desktop class machine
   - Memory < 1 GB
   - REST history range query (1 point, 24 h) p99 < 200 ms
   - WS broadcast latency p99 < 250 ms

### D. Onboarding  **[EXTERNAL]**

6. **[EXTERNAL: requires 2 human testers]** README "first 30 minutes"
   walkthrough is executed by **2 people who are not the author** —
   one Rust dev with no BAS background, one BAS integrator with no
   Rust background. Both reach a tagged, queryable point in their
   browser. Friction notes filed as v1.1 issues.

### E. Boundary hygiene

7. The README scope section accurately reflects what ships: data layer
   + write-back + alarm engine sibling. No more "alarms live elsewhere"
   contradiction.
8. `bms-store-domain` is non-empty: every public REST/WS payload has a
   typed DTO, and the OpenAPI spec is generated from the axum routes
   (e.g. via `utoipa`).

### F. Self-test (codeable substitute for hardware criteria)

9. `bms-stored --selftest` boots a temp project with in-process BACnet
   and Modbus simulators, runs the full v1 pipeline (discover → accept
   → auto-tag → standardize value → equipRef → REST history query →
   WS subscribe → write setpoint → trigger threshold alarm → fire
   webhook → ack), and exits 0 on success or non-zero with diagnostic
   on the first failed stage. CI runs it on every push.

## Codeable vs external split

| # | Item | Status |
|---|---|---|
| A.1 | Real-device discover→consume | **[EXTERNAL]** — substitute via F.9 |
| A.2 | Real-device write path | **[EXTERNAL]** — substitute via F.9 |
| A.3 | Real-device alarm path | **[EXTERNAL]** — substitute via F.9 |
| B.4 | UI repo integration | **[EXTERNAL]** — UI repo is separate |
| C.5 | Bench harness | codeable here |
| D.6 | 2-human UX test | **[EXTERNAL]** — needs humans |
| E.7 | README scope alignment | ✓ done (commit 27c2c29) |
| E.8 | Domain DTOs + OpenAPI | codeable here |
| F.9 | Self-test binary | codeable here |

**Codeable items remaining for v1.0 in this repo:** C.5, E.8, F.9, plus
the underlying value normalization + tag provenance work that those
depend on. External items get a tracking issue and ship label `v1.0`
when the external action confirms pass.

## Non-goals for the v1.0 review pass

After v1.0 ships, future review rounds frame findings as **v1.1
wishlist** rather than **v1.0 blockers** unless they are correctness or
data-loss bugs. This is the contract — without it, every review extends
the bar indefinitely.

## Tracking

Open one GitHub issue per criterion above with the label `v1.0`.
Mergeable to v1.0 only when all 8 are checked. New ideas surfaced in
the meantime get labelled `v1.1` instead and stay out of the v1.0
milestone.
