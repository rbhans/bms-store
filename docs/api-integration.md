# bms-store API Integration Guide

This is the canonical reference for any consumer app (UI, integrator,
3rd-party service) connecting to bms-store. It covers authentication,
the REST + WebSocket surface, the typed DTOs, error handling, and
recommended flow for a real-time operator UI.

> Versioning: bms-store follows semver. `0.x.y` → API may evolve;
> 0.x → breaking only on `x` bump. The OpenAPI spec at
> `GET /api/openapi.json` is the live source of truth — this document
> mirrors it in human-readable form. When the two disagree, the spec
> wins.

---

## 1. Connection model

bms-store exposes **five outbound channels**. A typical consumer app
uses two of them:

| Channel | Best for | Auth |
|---|---|---|
| **REST** (HTTP/HTTPS) | On-demand reads, writes, configuration | JWT (Bearer) or API key |
| **WebSocket** (`/ws`) | Live value/discovery/alarm streaming | JWT in `?token=` query param |
| MQTT publish | External systems on the building's MQTT bus | Per-broker creds (server side) |
| Webhooks (outbound) | Slack/Teams/PagerDuty/ntfy/generic JSON notifications | HMAC sig optional |
| Generic export | Data warehouse / lake bulk push | Per-connector |

A real-time UI uses **REST + WebSocket**. The other three channels are
infrastructure for non-UI consumers and don't require UI work.

Default base URL: `http://localhost:8080`. The server can also serve
HTTPS — port and cert path configured in `scenario.json`.

---

## 2. Authentication

### 2.1 First-time setup (admin only)

When the user database is empty, the first call must create an admin:

```http
POST /api/auth/setup
Content-Type: application/json

{ "username": "admin", "password": "..." }
```

### 2.2 Login → JWT

```http
POST /api/auth/login
Content-Type: application/json

{ "username": "admin", "password": "..." }
```

Response:

```json
{
  "token": "<jwt>",
  "user": {
    "id": "u-abc123",
    "username": "admin",
    "display_name": "Admin",
    "role": "admin"
  }
}
```

The JWT is a short-lived bearer token. Pass it on every REST request:

```http
GET /api/points
Authorization: Bearer <jwt>
```

### 2.3 Refresh

```http
POST /api/auth/refresh
Authorization: Bearer <jwt>
```

Returns a fresh JWT. Call before expiry; recommended cadence: rotate on
401 or every ~50 minutes.

### 2.4 Current user

```http
GET /api/auth/me
Authorization: Bearer <jwt>
```

Returns the same `UserInfo` shape as `LoginResponse.user`.

### 2.5 API keys (machine-to-machine)

For headless integrations (scripts, exporters), the user can mint
long-lived API keys at `POST /api/auth/api-keys`. The key is
returned **once** at creation; pass on subsequent requests via the
same `Authorization: Bearer <key>` header. CRUD:

- `GET /api/auth/api-keys`
- `POST /api/auth/api-keys`
- `PUT /api/auth/api-keys/{id}`
- `DELETE /api/auth/api-keys/{id}`

### 2.6 Permissions / RBAC

The JWT/API-key carries a `role`. Routes that mutate state require a
permission tied to that role. The server returns `403 Forbidden` if
the role lacks permission. Common permissions (UI should hide the
corresponding affordance when missing):

| Permission | Endpoints (gated) |
|---|---|
| `read_data` | All `GET` endpoints |
| `write_points` | `POST /api/points/.../write`, `/relinquish` |
| `manage_discovery` | `accept_device`, `ignore_device`, `bulk_*`, scan |
| `manage_users` | `users` CRUD, `system/backup*` |
| `manage_alarms` | (planned, alarm rule CRUD) |

`GET /api/auth/me` returns the role; UI uses it to hide buttons.

---

## 3. WebSocket — `/ws`

### 3.1 Connect

```
ws://host:8080/ws?token=<jwt>
```

The token is a query param because browsers don't send `Authorization`
on WebSocket upgrades. Server enforces:

- Max 5 concurrent connections per user
- 64 KB message + frame size cap
- 4-hour idle timeout
- 30-second server pings (respond with pong)

### 3.2 Subscribe / filter

After upgrade, send a JSON subscribe message:

```json
{
  "subscribe": {
    "event_types": ["values", "discovery", "status"],
    "node_ids": ["ahu-1/discharge-air-temp"],
    "since_seq": 12345
  }
}
```

All three fields are optional:

- `event_types`: empty/missing = receive all event types
- `node_ids`: empty/missing = no node filter (all nodes)
- `since_seq`: replay from the given event-journal sequence number
  before live streaming. Lets the UI catch up after reconnect without
  losing events. Only works when the server has the event journal
  enabled (`scenario.json → settings.event_journal.enabled = true`).

A legacy form is also accepted: `{"subscribe": ["values", "discovery"]}`
treats the array as `event_types`.

### 3.3 Event types

Every server message is a JSON object:

```json
{
  "type": "<event-type>",
  "seq": 12346,        // optional, only when replaying
  ...event-specific fields
}
```

| `type` | Event | Payload fields |
|---|---|---|
| `values` | A point's value changed | `node_id`, `value` (bool/int/float), `timestamp_ms` |
| `status` | A point's status flags changed | `node_id`, `flags` (u8 bitfield) |
| `discovery` | Device-lifecycle event | `event` ∈ {`discovered`,`device_down`,`accepted`,`scan_complete`}, plus `bridge_type` / `device_key` / `protocol` / `device_count` per variant |

Example `values` event:

```json
{
  "type": "values",
  "node_id": "ahu-1/discharge-air-temp",
  "value": 72.5,
  "timestamp_ms": 1735689600000
}
```

Example `discovery` event:

```json
{
  "type": "discovery",
  "event": "accepted",
  "device_key": "ahu-1",
  "protocol": "bacnet",
  "point_count": 12
}
```

### 3.4 Status flag bitfield

`flags` in `status` events is a u8 with bits:

| Bit | Name |
|---|---|
| 0x01 | `alarm` |
| 0x02 | `stale` |
| 0x04 | `fault` |
| 0x08 | `overridden` |
| 0x10 | `down` |
| 0x20 | `disabled` |

Multiple bits can be set. `0` = normal.

### 3.5 Reconnect strategy

UI should:

1. Track the latest `seq` it has seen.
2. On disconnect, exponential-backoff reconnect.
3. On reconnect, send `{"subscribe": {"since_seq": <last_seq>}}` to
   replay missed events before the live stream resumes.
4. If the journal is disabled, accept that bridging gaps requires a
   REST refetch of `/api/points`.

---

## 4. REST surface

All paths are prefixed with `/api`. All responses are JSON. All
non-2xx responses follow the error schema in § 6.

### 4.1 Points

| Method | Path | Returns | Body |
|---|---|---|---|
| GET | `/points` | `PaginatedResponse<PointResponse>` | – |
| GET | `/points/{device_id}` | `PointResponse[]` | – |
| GET | `/points/{device_id}/{point_id}` | `PointResponse` | – |
| POST | `/points/{device_id}/{point_id}/write` | `WriteResponse` | `WriteRequest` |
| POST | `/points/{device_id}/{point_id}/relinquish` | `WriteResponse` | – |

Query params (read endpoints):

- `?raw=true` — return the underlying numeric/bool value instead of
  the canonical mapped string. Default `false` (canonical wins when
  the point has an `enum` ValueMap tag).

`PointResponse` (from `bms-store-domain::points`):

```json
{
  "device_id": "ahu-1",
  "point_id": "discharge-air-temp",
  "value": 72.5,
  "raw_value": 72.5,
  "value_mapped": false,
  "status": ["stale"],
  "ingest_ts_ms": 1735689600000,
  "source_ts_ms": 1735689599980
}
```

`WriteRequest`:

```json
{
  "value": 72.5,
  "priority": 8,
  "expires_ms": 1735776000000
}
```

- `priority` 1–16 for BACnet (lower = higher priority); ignored by
  Modbus/MQTT
- `expires_ms` Unix-ms wall-clock auto-release time; omit for
  indefinite

### 4.2 Entities

Site → Building → Floor → Space → Equip → Point + tag/ref graph.

| Method | Path | Returns | Body |
|---|---|---|---|
| GET | `/entities` | `EntityResponse[]` | – |
| GET | `/entities/{id}` | `EntityResponse` | – |
| GET | `/entities/{id}/referrers?tag=equipRef` | `EntityResponse[]` | – |
| GET | `/entities/{id}/supply-chain` | `EntityResponse[]` | – |
| GET | `/entities/{id}/return-chain` | `EntityResponse[]` | – |
| GET | `/relationships/issues` | issue list | – |
| POST | `/entities/tags-batch` | summary | `{entity_ids, tags}` |
| POST | `/entities/tags-batch/remove` | summary | `{entity_ids, tag_names}` |
| POST | `/entities/refs-batch` | summary | `{source_ids, ref_tag, target_id}` |

Query params on `GET /entities`:

- `?filter=equip and ahu` — Project Haystack v4 filter expression.
  Empty/missing = all entities.
- `?limit=N` — default 1000.
- `?entity_type=equip` — filter by type.

`EntityResponse`:

```json
{
  "id": "ahu-1",
  "entity_type": "equip",
  "dis": "AHU-1",
  "parent_id": "floor-2",
  "tags": {
    "equip": null,
    "ahu": null,
    "discharge": null
  },
  "refs": {
    "siteRef": "site-main",
    "buildingRef": "bldg-1",
    "floorRef": "floor-2"
  },
  "created_ms": 1735689600000,
  "updated_ms": 1735689600000
}
```

A `null` tag value = marker tag (e.g. `equip`). String values =
key-value tags (e.g. `unit: "degF"`).

### 4.3 Nodes

The spatial / equip tree. Richer than entities — carries protocol
binding + capability flags.

| Method | Path | Returns | Body |
|---|---|---|---|
| GET | `/nodes` | `PaginatedResponse<NodeResponse>` | – |
| GET | `/nodes/{id}` | `NodeResponse` | – |
| POST | `/nodes` | `{ok, id}` | `CreateNodeRequest` |
| PUT | `/nodes/{id}` | `{ok}` | `UpdateNodeRequest` |
| DELETE | `/nodes/{id}/delete` | `{ok}` | – |
| PUT | `/nodes/{id}/tags` | `{ok}` | `SetTagsRequest` |
| GET | `/nodes/{id}/hierarchy` | `NodeResponse[]` | – |
| GET | `/nodes/{id}/ancestors` | `NodeResponse[]` | – |

`NodeResponse`:

```json
{
  "id": "ahu-1",
  "node_type": "equip",
  "dis": "AHU-1",
  "parent_id": "group-x",
  "tags": { ... },
  "refs": { ... },
  "properties": { "atlasPointId": "..." },
  "capabilities": {
    "readable": true,
    "writable": true,
    "historizable": true,
    "alarmable": true,
    "schedulable": false
  },
  "binding": { "protocol": "bacnet", "config": { ... } },
  "created_ms": 1735689600000,
  "updated_ms": 1735689600000
}
```

UI uses `capabilities.writable` to gate the "Write" button,
`capabilities.alarmable` for the "Add alarm rule" affordance, etc.

### 4.4 History

| Method | Path | Returns |
|---|---|---|
| GET | `/history/{device_id}/{point_id}` | `HistoryResponse` |
| GET | `/history/{device_id}/{point_id}/range` | `TimeRangeResponse` |
| GET | `/history/{device_id}/{point_id}/export` | CSV stream |

Query params on `/history/...`:

- `?from=<ms>` (or `start_ms`, `cursor` aliases)
- `?to=<ms>` (or `end_ms`)
- `?limit=N` (or `max_results`)

Default range: last 24 hours. Pagination via `next_cursor` (echoed in
response, pass back as `?cursor=…`).

`HistoryResponse`:

```json
{
  "device_id": "ahu-1",
  "point_id": "discharge-air-temp",
  "samples": [
    { "timestamp_ms": 1735689600000, "value": 72.5 },
    { "timestamp_ms": 1735689601000, "value": 72.6 }
  ],
  "next_cursor": 1735689602001
}
```

`/range` returns `{device_id, point_id, start_ms, end_ms}` — useful
for setting a chart's default zoom window.

### 4.5 Discovery

| Method | Path | Returns | Body |
|---|---|---|---|
| GET | `/discovery/devices` | device list | – |
| GET | `/discovery/devices/{id}` | device | – |
| GET | `/discovery/devices/{id}/points` | discovered points | – |
| GET | `/discovery/devices/{id}/preview-tags` | `PreviewTagsResponse` | – |
| POST | `/discovery/devices/{id}/accept` | `{ok}` | `AcceptDeviceBody` |
| POST | `/discovery/devices/{id}/ignore` | `{ok}` | – |
| POST | `/discovery/devices/{id}/rename` | `{ok}` | `{display_name}` |
| POST | `/discovery/devices/bulk-accept` | summary | `{device_ids, target_space_id?}` |
| POST | `/discovery/devices/bulk-ignore` | summary | `{device_ids}` |
| POST | `/discovery/scan/bacnet` | `{ok}` | `{network_id?}` |
| POST | `/discovery/scan/modbus` | `{ok}` | `{host?}` |

`AcceptDeviceBody`:

```json
{
  "target_space_id": "room-101",
  "skip_auto_tag": false
}
```

- `target_space_id` — optional NodeStore id; when set, the new equip
  gets `siteRef`/`buildingRef`/`floorRef`/`spaceRef` populated from
  the ancestor walk
- `skip_auto_tag` — when `true`, skip Atlas + heuristic tagging
  entirely. Equip and point entities are created with empty tag sets
  (use the entity tag API to apply your own taxonomy)

### 4.6 Bridges (BACnet networks + Modbus buses)

CRUD for protocol bridge configs. Edits require a server restart to
take effect. The server emits a `Toast(Warn, "restart bms-store to
activate")` on every mutation.

```
GET    /bridges/bacnet
POST   /bridges/bacnet         { name, config, enabled }
GET    /bridges/bacnet/{id}
PUT    /bridges/bacnet/{id}
DELETE /bridges/bacnet/{id}
GET    /bridges/modbus
POST   /bridges/modbus         { name, config, enabled }
GET    /bridges/modbus/{id}
PUT    /bridges/modbus/{id}
DELETE /bridges/modbus/{id}
```

`config` is a free-form object matching the `BacnetNetworkConfig` /
`ModbusNetworkConfig` shape from `bms-store-storage::config::scenario`.

### 4.7 Overrides

| Method | Path | Returns | Body |
|---|---|---|---|
| GET | `/overrides` | override list | – |
| GET | `/overrides/active` | active overrides | – |
| PUT | `/overrides/{id}` | `{ok}` | `UpdateExpiryRequest` |

### 4.8 Webhooks

For consumer apps that want notifications without holding a WS open:

```
GET    /webhooks
POST   /webhooks                 { url, provider, events, secret? }
GET    /webhooks/config
PUT    /webhooks/config
GET    /webhooks/deliveries
GET    /webhooks/{id}
PUT    /webhooks/{id}
DELETE /webhooks/{id}
POST   /webhooks/{id}/test
```

`provider` ∈ `slack` / `teams` / `pagerduty` / `ntfy` / `generic`.
`events` — array of event-type globs to subscribe to.

### 4.9 Audit

```
GET /audit?start_ms=&end_ms=&actor=&resource_type=&action=&limit=
GET /audit/count?...same filters
```

Returns audit rows: actor, action, resource type/id, details, ok flag,
ts. Use for compliance reports + "who changed this?" diagnostics.

### 4.10 System

```
GET  /health                  no-auth — { status, components[] }
GET  /system/info             { version, point_count, device_count, scenario_name }
GET  /system/capabilities     { version, bridges, features }
POST /system/backup           manual backup trigger
GET  /system/backups          list backup files
GET  /system/backup-config    schedule + retention
PUT  /system/backup-config
```

`/health` is the only no-auth route — useful for load balancers and
status pages. Everything else requires `Authorization`.

### 4.11 OpenAPI spec

```
GET /api/openapi.json    no-auth — full spec
```

Use this as the source of truth + run `openapi-typescript` (or
similar) to codegen TS types in the UI repo.

---

## 5. DTO catalog

The `bms-store-domain` Cargo crate exports every wire shape as Rust
types. For TypeScript / JS, codegen from `/api/openapi.json`.

| Module | Types |
|---|---|
| `points` | `PointResponse`, `WriteRequest`, `WriteResponse` |
| `entities` | `EntityResponse`, `ListEntitiesQuery` |
| `history` | `HistoryQueryParams`, `HistoryResponse`, `SampleResponse`, `TimeRangeResponse` |
| `nodes` | `NodeResponse`, `NodeCapabilitiesResponse`, `CreateNodeRequest`, `UpdateNodeRequest`, `SetTagsRequest` |
| `system` | `HealthResponse`, `ComponentHealth`, `SystemInfoResponse`, `CapabilitiesResponse` |
| `pagination` | `PaginationParams`, `PaginatedResponse<T>` |

All shapes are stable per semver. Additions are non-breaking; field
removals or renames bump the minor version.

---

## 6. Error model

Non-2xx responses use a uniform JSON shape:

```json
{
  "error": "<short message>",
  "details": "<optional longer text>"
}
```

Status codes follow standard HTTP semantics:

| Code | Meaning |
|---|---|
| 200 | OK |
| 400 | Bad request — invalid JSON, missing required field, invalid value |
| 401 | Unauthorized — missing/invalid/expired JWT |
| 403 | Forbidden — authenticated but role lacks permission |
| 404 | Not found |
| 409 | Conflict — duplicate name / state mismatch |
| 413 | Payload too large (1 MB body limit) |
| 429 | Too many requests — rate limit on `/login` (20/min/IP) and `/ws` (5/user) |
| 500 | Internal server error — bug, file an issue |

UI should:

1. On 401, attempt one refresh, then redirect to login on failure.
2. On 403, display "you don't have permission" without offering retry.
3. On 429, display a brief banner; back off network calls.

---

## 7. Recommended UI flow

```
1. Boot
   ├─ POST /api/auth/login → JWT
   ├─ GET  /api/auth/me → role (gate UI affordances)
   ├─ GET  /api/system/capabilities → which protocols are enabled
   └─ GET  /api/system/info → version, scenario, point count

2. Load initial state
   ├─ GET /api/entities?filter=equip → equipment list (left rail)
   ├─ GET /api/nodes?node_type=site → spatial root → walk tree
   └─ GET /api/points?limit=200 → first page of values

3. Open WebSocket
   ws://host/ws?token=<jwt>
   Send: { "subscribe": { "event_types": ["values","discovery","status"] } }
   On: values → patch the corresponding card/row in place
       status → update status badge
       discovery → toast + refresh device list

4. Detail screen (user clicks an equip)
   ├─ GET /api/entities/{id} → tags + refs
   ├─ GET /api/nodes/{id} → capabilities + binding
   ├─ For each child point:
   │   ├─ GET /api/points/{dev}/{pt} → current value
   │   └─ GET /api/history/{dev}/{pt}?from=<24h-ago> → trend chart
   └─ WS subscribe with node_ids filter limited to this equip's points

5. Write (user clicks "Set" on a writable point)
   POST /api/points/{dev}/{pt}/write
        { "value": 72.5, "priority": 8 }
   On 200: optimistic update is already correct (server emits a
   matching values event over WS within ~10ms)
   On 4xx/5xx: rollback optimistic UI, show error

6. Reconnect
   On WS close:
     – Exponential backoff (1s, 2s, 5s, 15s)
     – On reconnect, send subscribe with since_seq=<last_seq>
   On 401:
     – POST /api/auth/refresh
     – Reopen WS with new token

7. Cleanup
   On user logout / tab close:
     – Close WS
     – Discard JWT
```

---

## 8. Examples

### 8.1 Login + first pull (TypeScript)

```ts
const base = "http://localhost:8080";

const login = await fetch(`${base}/api/auth/login`, {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({ username: "admin", password: "..." })
});
const { token, user } = await login.json();

const points = await fetch(`${base}/api/points?limit=50`, {
  headers: { authorization: `Bearer ${token}` }
}).then(r => r.json());
```

### 8.2 WebSocket subscribe (TypeScript)

```ts
const ws = new WebSocket(`ws://localhost:8080/ws?token=${token}`);
ws.onopen = () => {
  ws.send(JSON.stringify({
    subscribe: { event_types: ["values", "discovery"] }
  }));
};
ws.onmessage = (e) => {
  const msg = JSON.parse(e.data);
  if (msg.type === "values") {
    updatePointValue(msg.node_id, msg.value, msg.timestamp_ms);
  } else if (msg.type === "discovery") {
    refreshDeviceList();
  }
};
```

### 8.3 Codegen TS types

```bash
npx openapi-typescript http://localhost:8080/api/openapi.json -o src/types/bms.ts
```

Then import:

```ts
import type { components } from "./types/bms";
type PointResponse = components["schemas"]["PointResponse"];
```

### 8.4 Write setpoint (TypeScript)

```ts
await fetch(`${base}/api/points/ahu-1/sat-setpoint/write`, {
  method: "POST",
  headers: {
    authorization: `Bearer ${token}`,
    "content-type": "application/json",
  },
  body: JSON.stringify({
    value: 72.5,
    priority: 8,
    expires_ms: Date.now() + 8 * 60 * 60 * 1000  // 8h
  })
});
```

### 8.5 Subscribe to one equip's points only (TypeScript)

```ts
const equipId = "ahu-1";
const points = await fetch(`${base}/api/discovery/devices/${equipId}/points`, {
  headers: { authorization: `Bearer ${token}` }
}).then(r => r.json());

const nodeIds = points.map((p: any) => `${equipId}/${p.id}`);
ws.send(JSON.stringify({
  subscribe: { event_types: ["values", "status"], node_ids: nodeIds }
}));
```

---

## 9. Limits + rate limits

| Limit | Value | Where enforced |
|---|---|---|
| Request body | 1 MB | global axum middleware |
| WS message size | 64 KB | WS upgrade |
| WS frame size | 64 KB | WS upgrade |
| WS connections per user | 5 | WS upgrade |
| WS idle timeout | 4 hours | WS handler |
| Login attempts per IP | 20/min | `/login` rate limiter |
| Tags per `set_tags` request | 100 | `/nodes/{id}/tags` |

Going over a limit returns `429 Too Many Requests` (rate) or
`413 Payload Too Large` (size).

---

## 10. Versioning + change policy

- The API surface follows semver. While bms-store is `0.x`, breaking
  changes can land on minor bumps; bumps documented in
  [CHANGELOG.md](../CHANGELOG.md).
- Additive fields (new `Option<T>` on response, new endpoint, new
  event type) are non-breaking and don't bump the minor version.
- Rename / removal / type change = breaking. Bump minor + announce in
  the changelog.
- Stability tier when bms-store hits `1.0.0`: every shape in
  `bms-store-domain` becomes semver-stable; breaking changes bump
  major.

When in doubt, check the CHANGELOG and the OpenAPI spec — both ship
inside the bms-store repo + binary.

---

## 11. Where this maps to bms-store internals

This section is for implementers debugging from the consumer side; UI
devs can skip.

| Consumer surface | Internal source |
|---|---|
| `PointResponse.value` | `PointStore::get` → `TimestampedValue.canonical_value` if present, else `value` |
| `PointResponse.raw_value` | `TimestampedValue.value` |
| `PointResponse.value_mapped` | `canonical_value.is_some()` |
| `PointResponse.status` | `TimestampedValue.status.active_flags()` |
| `PointResponse.ingest_ts_ms` | `TimestampedValue.ingest_ts_ms` (always) |
| `PointResponse.source_ts_ms` | `TimestampedValue.source_ts_ms` (when protocol provides) |
| `EntityResponse.tags` | `entity_tag` rows (SQLite) |
| `EntityResponse.refs` | `entity_ref` rows |
| Tag provenance (not yet on responses, v1.1+) | `entity_tag_provenance` rows |
| `HistoryResponse.samples` | `history_hot` / `history_warm` / `history_cold` (tiered) |
| WS `values` event | `Event::ValueChanged` from `point_store.set(...)` |
| WS `discovery` event | `Event::Device*` from the discovery service |
| Webhook fan-out | `WebhookDispatcher` listens to the same EventBus |
| MQTT publish | `MqttPublisher` listens to the same EventBus |

All consumer channels read from one EventBus + one set of stores. No
data divergence is possible across channels — only format and timing
differ.

---

_Last updated: 2026-05-04. Source of truth = `GET /api/openapi.json`
on a running bms-store instance._
