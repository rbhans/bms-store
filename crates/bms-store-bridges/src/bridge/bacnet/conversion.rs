use rustbac_client::ClientDataValue;
use rustbac_core::types::ObjectType;

use crate::config::profile::PointValue;
use crate::store::point_store::{PointKey, PointStatusFlags, PointStore};

use super::BacnetObject;

/// Map BACnet StatusFlags (BitString) to PointStatusFlags on the store.
///
/// BACnet StatusFlags bit ordering (ASHRAE 135):
///   bit 0 = IN_ALARM
///   bit 1 = FAULT
///   bit 2 = OVERRIDDEN
///   bit 3 = OUT_OF_SERVICE
pub(crate) fn apply_bacnet_status_flags(
    store: &PointStore,
    key: &PointKey,
    value: &ClientDataValue,
) {
    let (unused_bits, data) = match value {
        ClientDataValue::BitString { unused_bits, data } => (*unused_bits, data.as_slice()),
        _ => return,
    };

    // BACnet StatusFlags is a 4-bit BitString.
    // Bit ordering: MSB-first within each byte.
    // Bit 0 (MSB of byte 0) = IN_ALARM
    // Bit 1 = FAULT
    // Bit 2 = OVERRIDDEN
    // Bit 3 = OUT_OF_SERVICE
    let total_bits = data.len() * 8 - unused_bits as usize;

    let mappings: &[(usize, u8)] = &[
        (0, PointStatusFlags::ALARM),      // IN_ALARM
        (1, PointStatusFlags::FAULT),      // FAULT
        (2, PointStatusFlags::OVERRIDDEN), // OVERRIDDEN
        (3, PointStatusFlags::DISABLED),   // OUT_OF_SERVICE
    ];

    for &(bit_index, flag) in mappings {
        if bit_index < total_bits {
            let byte_idx = bit_index / 8;
            let bit_pos = 7 - (bit_index % 8); // MSB-first
            let is_set = byte_idx < data.len() && (data[byte_idx] & (1 << bit_pos)) != 0;
            if is_set {
                store.set_status(key, flag);
            } else {
                store.clear_status(key, flag);
            }
        }
    }
}

/// Extract (timestamp_ms, f64) pairs from TrendLog ReadRange items.
/// TrendLog entries are typically Constructed values with date/time + value.
pub(crate) fn trend_log_items_to_samples(items: &[ClientDataValue]) -> Vec<(i64, f64)> {
    let mut samples = Vec::new();
    for item in items {
        if let ClientDataValue::Constructed { values, .. } = item {
            // BACnet LogRecord: { timestamp, logDatum }
            // Try to extract a numeric value from the last element
            let value = values.iter().rev().find_map(|p| match p {
                ClientDataValue::Real(f) => Some(*f as f64),
                ClientDataValue::Double(f) => Some(*f),
                ClientDataValue::Unsigned(u) => Some(*u as f64),
                ClientDataValue::Signed(i) => Some(*i as f64),
                ClientDataValue::Enumerated(e) => Some(*e as f64),
                ClientDataValue::Boolean(b) => Some(if *b { 1.0 } else { 0.0 }),
                _ => None,
            });
            // Try to extract a timestamp from a Date+Time pair at the start
            let ts_ms = extract_log_timestamp(values);
            if let (Some(ts), Some(val)) = (ts_ms, value) {
                samples.push((ts, val));
            }
        }
    }
    samples
}

/// Try to extract a Unix timestamp from BACnet Date+Time values at the start of a LogRecord.
fn extract_log_timestamp(parts: &[ClientDataValue]) -> Option<i64> {
    // Look for a Date followed by a Time in the constructed value
    let mut date_opt = None;
    let mut time_opt = None;
    for part in parts {
        if let ClientDataValue::Constructed { values: inner, .. } = part {
            // Nested date-time constructed value
            return extract_log_timestamp(inner);
        }
        // Date is typically encoded as OctetString(4 bytes) or as a tagged value
        if let ClientDataValue::OctetString(bytes) = part {
            if bytes.len() == 4 && date_opt.is_none() {
                // year_since_1900, month, day, weekday
                let year = 1900 + bytes[0] as i64;
                let month = bytes[1] as i64;
                let day = bytes[2] as i64;
                // Simple conversion — days since epoch
                let days = civil_to_days(year as i32, month as i32, day as i32);
                date_opt = Some(days * 86400 * 1000);
            } else if bytes.len() == 4 && date_opt.is_some() {
                // hour, minute, second, hundredths
                let ms = (bytes[0] as i64) * 3_600_000
                    + (bytes[1] as i64) * 60_000
                    + (bytes[2] as i64) * 1000
                    + (bytes[3] as i64) * 10;
                time_opt = Some(ms);
            }
        }
    }
    match (date_opt, time_opt) {
        (Some(d), Some(t)) => Some(d + t),
        (Some(d), None) => Some(d),
        _ => {
            // Fallback: use current time if we can't parse the timestamp
            use std::time::{SystemTime, UNIX_EPOCH};
            Some(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as i64,
            )
        }
    }
}

/// Convert a civil date to days since Unix epoch (inverse of days_to_ymd).
pub(crate) fn civil_to_days(year: i32, month: i32, day: i32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let m = if month <= 2 { month + 9 } else { month - 3 } as u32;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let doy = (153 * m + 2) / 5 + day as u32 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era as i64) * 146097 + (doe as i64) - 719468
}

/// Returns true for object types that represent monitorable points.
pub(crate) fn is_point_object(ot: ObjectType) -> bool {
    matches!(
        ot,
        ObjectType::AnalogInput
            | ObjectType::AnalogOutput
            | ObjectType::AnalogValue
            | ObjectType::BinaryInput
            | ObjectType::BinaryOutput
            | ObjectType::BinaryValue
            | ObjectType::MultiStateInput
            | ObjectType::MultiStateOutput
            | ObjectType::MultiStateValue
            | ObjectType::Accumulator
            | ObjectType::PulseConverter
    )
}

/// Build a stable point ID string from a BACnet object.
/// Prefers ObjectName if available, otherwise uses "type-instance" format.
pub(crate) fn object_point_id(obj: &BacnetObject) -> String {
    match &obj.object_name {
        Some(name) if !name.is_empty() => name.clone(),
        _ => format!(
            "{}-{}",
            obj.object_id.object_type(),
            obj.object_id.instance()
        ),
    }
}

/// Convert our PointValue to a BACnet ClientDataValue appropriate for the object type.
pub(crate) fn point_value_to_client(pv: &PointValue, ot: ObjectType) -> ClientDataValue {
    let classification = rustbac_client::point::classify_point(ot);
    match (pv, classification.kind) {
        (PointValue::Float(f), rustbac_client::PointKind::Analog) => {
            ClientDataValue::Real(*f as f32)
        }
        (PointValue::Integer(i), rustbac_client::PointKind::Analog) => {
            ClientDataValue::Real(*i as f32)
        }
        (PointValue::Bool(b), rustbac_client::PointKind::Binary) => {
            ClientDataValue::Enumerated(if *b { 1 } else { 0 })
        }
        (PointValue::Integer(i), rustbac_client::PointKind::MultiState) => {
            ClientDataValue::Unsigned(*i as u32)
        }
        // Fallbacks
        (PointValue::Float(f), _) => ClientDataValue::Real(*f as f32),
        (PointValue::Integer(i), _) => ClientDataValue::Unsigned(*i as u32),
        (PointValue::Bool(b), _) => ClientDataValue::Enumerated(if *b { 1 } else { 0 }),
    }
}

/// Convert a BACnet ClientDataValue to our PointValue, using the object type
/// to preserve semantic types (e.g. binary objects -> Bool, not Integer).
pub(crate) fn client_to_point_value(cv: &ClientDataValue, ot: ObjectType) -> PointValue {
    let classification = rustbac_client::point::classify_point(ot);
    match classification.kind {
        rustbac_client::PointKind::Binary => {
            // BACnet binary uses Enumerated(0=inactive, 1=active)
            let active = match cv {
                ClientDataValue::Enumerated(e) => *e != 0,
                ClientDataValue::Boolean(b) => *b,
                ClientDataValue::Unsigned(u) => *u != 0,
                ClientDataValue::Real(f) => *f != 0.0,
                _ => false,
            };
            PointValue::Bool(active)
        }
        rustbac_client::PointKind::MultiState => {
            let state = match cv {
                ClientDataValue::Unsigned(u) => *u as i64,
                ClientDataValue::Enumerated(e) => *e as i64,
                ClientDataValue::Signed(i) => *i as i64,
                ClientDataValue::Real(f) => *f as i64,
                _ => 0,
            };
            PointValue::Integer(state)
        }
        _ => {
            // Analog and everything else -> Float
            match cv {
                ClientDataValue::Real(f) => PointValue::Float(*f as f64),
                ClientDataValue::Double(f) => PointValue::Float(*f),
                ClientDataValue::Unsigned(u) => PointValue::Float(*u as f64),
                ClientDataValue::Signed(i) => PointValue::Float(*i as f64),
                ClientDataValue::Boolean(b) => PointValue::Float(if *b { 1.0 } else { 0.0 }),
                ClientDataValue::Enumerated(e) => PointValue::Float(*e as f64),
                _ => PointValue::Float(0.0),
            }
        }
    }
}
