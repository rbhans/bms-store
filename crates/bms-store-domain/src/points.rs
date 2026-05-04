//! Point read/write DTOs.
//!
//! Used by the `/api/points/*` REST routes and (in summary form) by the
//! `/ws` WebSocket value-change events.

use serde::{Deserialize, Serialize};

/// Latest known value for a single point, returned by `GET /api/points/...`.
///
/// `value` is the canonical/display form — a mapped string when an `enum`
/// tag is present on the point entity, otherwise the raw numeric/bool. To
/// always receive the raw form, pass `?raw=true` on the request.
///
/// `raw_value` is always the unmodified protocol value. Use this for
/// alarm thresholds, control logic, and anywhere a numeric is required.
///
/// `value_mapped` is `true` when `value` came from a per-point ValueMap.
///
/// `status` lists active status flags (e.g. `["stale"]`, `["overridden"]`).
/// Empty when normal. The flag list is bms-core's `PointStatusFlags`
/// active flag names; consult that module for the canonical strings.
///
/// `ingest_ts_ms` is the Unix-ms wall-clock at which bms-store accepted
/// the value. Always populated.
///
/// `source_ts_ms` is the Unix-ms timestamp the device measured the value,
/// when the protocol provides one (BACnet COV TimeStamp, BACnet TrendLog,
/// MQTT message timestamps). `None` otherwise — consumers should fall
/// back to `ingest_ts_ms`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PointResponse {
    pub device_id: String,
    pub point_id: String,
    pub value: serde_json::Value,
    pub raw_value: serde_json::Value,
    pub value_mapped: bool,
    pub status: Vec<String>,
    pub ingest_ts_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ts_ms: Option<i64>,
}

/// Body of `POST /api/points/:device_id/:point_id/write`.
///
/// `value` accepts JSON bool, integer, or float — booleans go to binary
/// outputs, integers to multistate or analog, floats to analog.
///
/// `priority` is the BACnet write priority (1–16, lower = higher priority).
/// Non-BACnet protocols ignore this. Defaults to 16 when omitted (lowest
/// priority — equivalent to a normal user write).
///
/// `expires_ms` is the Unix-ms wall-clock at which the override should
/// auto-release. Omit for indefinite override; the override engine
/// records the entry and the operator can release manually.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WriteRequest {
    pub value: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_ms: Option<i64>,
}

/// Response from `POST /api/points/:device_id/:point_id/write`.
///
/// `ok` is `true` on success. The HTTP status carries failure semantics —
/// a 200 with `ok: false` does not occur. Future fields (e.g. echoed
/// override id) may be added; consumers should ignore unknown fields.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WriteResponse {
    pub ok: bool,
}
