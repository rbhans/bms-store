use hmac::{Hmac, Mac};
use sha2::Sha256;

use super::model::{FormattedPayload, Provider, WebhookEventType, WebhookPayload};

type HmacSha256 = Hmac<Sha256>;

/// Severity to Slack attachment color.
fn severity_color(severity: &str) -> &'static str {
    match severity {
        "info" => "#3b82f6",
        "warning" => "#f59e0b",
        "critical" => "#dc2626",
        "life_safety" => "#7c2d12",
        _ => "#6b7280",
    }
}

/// Severity to ntfy priority (1-5).
fn severity_to_ntfy_priority(severity: &str) -> u8 {
    match severity {
        "info" => 2,
        "warning" => 3,
        "critical" => 4,
        "life_safety" => 5,
        _ => 2,
    }
}

/// Event type to human-readable header text.
fn event_header(event_type: WebhookEventType, severity: Option<&str>) -> String {
    let sev = severity.unwrap_or("Info");
    let sev_cap = capitalize(sev);
    match event_type {
        WebhookEventType::AlarmRaised => format!("{} Alarm Raised", sev_cap),
        WebhookEventType::AlarmCleared => format!("{} Alarm Cleared", sev_cap),
        WebhookEventType::AlarmAcknowledged => "Alarm Acknowledged".into(),
        WebhookEventType::DeviceDown => "Device Down".into(),
        WebhookEventType::DeviceRecovered => "Device Recovered".into(),
        WebhookEventType::FddFaultRaised => format!("{} FDD Fault Detected", sev_cap),
        WebhookEventType::FddFaultCleared => "FDD Fault Cleared".into(),
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => {
            let mut result = f.to_uppercase().collect::<String>();
            result.push_str(&c.as_str().replace('_', " "));
            result
        }
    }
}

fn timestamp_iso8601(ms: i64) -> String {
    let secs = ms / 1000;
    let nanos = ((ms % 1000) * 1_000_000) as u32;
    let dt = std::time::UNIX_EPOCH + std::time::Duration::new(secs as u64, nanos);
    // Format as ISO 8601 manually (no chrono dependency)
    let dur = dt.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let total_secs = dur.as_secs();
    let days = total_secs / 86400;
    let time_secs = total_secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    // Approximate date from days since epoch (good enough for display)
    let (year, month, day) = days_to_date(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

fn days_to_date(days: u64) -> (u64, u64, u64) {
    // Algorithm from https://howardhinnant.github.io/date_algorithms.html
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Compute HMAC-SHA256 signature as hex string.
pub fn hmac_sha256_hex(key: &str, body: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(key.as_bytes()).expect("HMAC key");
    mac.update(body.as_bytes());
    let result = mac.finalize();
    hex_encode(result.into_bytes().as_slice())
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

// ----------------------------------------------------------------
// Format: Generic
// ----------------------------------------------------------------

pub fn format_generic(payload: &WebhookPayload, secret: Option<&str>) -> FormattedPayload {
    let body = build_generic_json(payload);
    let body_str = serde_json::to_string(&body).unwrap_or_default();

    let mut extra_headers = Vec::new();
    if let Some(key) = secret {
        if !key.is_empty() {
            let sig = hmac_sha256_hex(key, &body_str);
            extra_headers.push(("X-OpenCrate-Signature".into(), format!("sha256={}", sig)));
        }
    }

    FormattedPayload {
        body: body_str,
        content_type: "application/json".into(),
        extra_headers,
    }
}

fn build_generic_json(p: &WebhookPayload) -> serde_json::Value {
    let timestamp = timestamp_iso8601(p.timestamp_ms);

    match p.event_type {
        WebhookEventType::AlarmRaised | WebhookEventType::AlarmCleared => {
            serde_json::json!({
                "event": p.event_type.as_str(),
                "timestamp": timestamp,
                "alarm": {
                    "id": p.alarm_id,
                    "device_id": p.device_id,
                    "point_id": p.point_id,
                    "type": p.alarm_type,
                    "severity": p.severity,
                    "value": p.trigger_value,
                    "message": p.message,
                },
                "source": {
                    "project": p.project_name,
                    "system": "OpenCrate BMS",
                }
            })
        }
        WebhookEventType::AlarmAcknowledged => {
            serde_json::json!({
                "event": p.event_type.as_str(),
                "timestamp": timestamp,
                "alarm": {
                    "id": p.alarm_id,
                },
                "source": {
                    "project": p.project_name,
                    "system": "OpenCrate BMS",
                }
            })
        }
        WebhookEventType::DeviceDown | WebhookEventType::DeviceRecovered => {
            serde_json::json!({
                "event": p.event_type.as_str(),
                "timestamp": timestamp,
                "device": {
                    "protocol": p.device_id,
                    "device_id": p.node_id,
                },
                "source": {
                    "project": p.project_name,
                    "system": "OpenCrate BMS",
                }
            })
        }
        WebhookEventType::FddFaultRaised | WebhookEventType::FddFaultCleared => {
            serde_json::json!({
                "event": p.event_type.as_str(),
                "timestamp": timestamp,
                "fault": {
                    "id": p.alarm_id,
                    "equipment": p.node_id,
                    "severity": p.severity,
                    "message": p.message,
                },
                "source": {
                    "project": p.project_name,
                    "system": "OpenCrate BMS",
                }
            })
        }
    }
}

// ----------------------------------------------------------------
// Format: Slack
// ----------------------------------------------------------------

pub fn format_slack(payload: &WebhookPayload) -> FormattedPayload {
    let header = event_header(payload.event_type, payload.severity.as_deref());
    let color = severity_color(payload.severity.as_deref().unwrap_or("info"));

    let emoji = match payload.severity.as_deref() {
        Some("critical") | Some("life_safety") => "\u{1f534}",
        Some("warning") => "\u{1f7e1}",
        _ => "\u{1f535}",
    };

    let is_alarm = matches!(
        payload.event_type,
        WebhookEventType::AlarmRaised | WebhookEventType::AlarmCleared
    );

    let mut fields = Vec::new();
    if is_alarm {
        if let Some(ref dev) = payload.device_id {
            fields
                .push(serde_json::json!({"type": "mrkdwn", "text": format!("*Device:*\n{}", dev)}));
        }
        if let Some(ref pt) = payload.point_id {
            fields.push(serde_json::json!({"type": "mrkdwn", "text": format!("*Point:*\n{}", pt)}));
        }
        if let Some(val) = payload.trigger_value {
            fields
                .push(serde_json::json!({"type": "mrkdwn", "text": format!("*Value:*\n{}", val)}));
        }
        if let Some(ref at) = payload.alarm_type {
            fields.push(serde_json::json!({"type": "mrkdwn", "text": format!("*Type:*\n{}", capitalize(at))}));
        }
    } else {
        if let Some(ref node) = payload.node_id {
            fields.push(
                serde_json::json!({"type": "mrkdwn", "text": format!("*Device:*\n{}", node)}),
            );
        }
        if let Some(ref dev) = payload.device_id {
            fields.push(
                serde_json::json!({"type": "mrkdwn", "text": format!("*Protocol:*\n{}", dev)}),
            );
        }
    }

    let fallback = payload
        .message
        .clone()
        .unwrap_or_else(|| format!("{}: {}", header, payload.node_id.as_deref().unwrap_or("")));

    let body = serde_json::json!({
        "blocks": [
            {
                "type": "header",
                "text": {"type": "plain_text", "text": format!("{} {}", emoji, header)}
            },
            {
                "type": "section",
                "fields": fields
            }
        ],
        "attachments": [
            {
                "color": color,
                "fallback": fallback
            }
        ]
    });

    FormattedPayload {
        body: serde_json::to_string(&body).unwrap_or_default(),
        content_type: "application/json".into(),
        extra_headers: Vec::new(),
    }
}

// ----------------------------------------------------------------
// Format: Teams (Adaptive Card)
// ----------------------------------------------------------------

pub fn format_teams(payload: &WebhookPayload) -> FormattedPayload {
    let header = event_header(payload.event_type, payload.severity.as_deref());

    let is_alarm = matches!(
        payload.event_type,
        WebhookEventType::AlarmRaised | WebhookEventType::AlarmCleared
    );

    let color = match payload.severity.as_deref() {
        Some("critical") | Some("life_safety") => "Attention",
        Some("warning") => "Warning",
        _ => "Default",
    };

    let mut facts = Vec::new();
    if is_alarm {
        if let Some(ref dev) = payload.device_id {
            facts.push(serde_json::json!({"title": "Device", "value": dev}));
        }
        if let Some(ref pt) = payload.point_id {
            facts.push(serde_json::json!({"title": "Point", "value": pt}));
        }
        if let Some(val) = payload.trigger_value {
            facts.push(serde_json::json!({"title": "Value", "value": val.to_string()}));
        }
        if let Some(ref at) = payload.alarm_type {
            facts.push(serde_json::json!({"title": "Type", "value": capitalize(at)}));
        }
    } else {
        if let Some(ref node) = payload.node_id {
            facts.push(serde_json::json!({"title": "Device", "value": node}));
        }
        if let Some(ref dev) = payload.device_id {
            facts.push(serde_json::json!({"title": "Protocol", "value": dev}));
        }
    }

    let body = serde_json::json!({
        "type": "message",
        "attachments": [{
            "contentType": "application/vnd.microsoft.card.adaptive",
            "content": {
                "type": "AdaptiveCard",
                "$schema": "http://adaptivecards.io/schemas/adaptive-card.json",
                "version": "1.4",
                "body": [
                    {
                        "type": "TextBlock",
                        "text": header,
                        "weight": "Bolder",
                        "size": "Medium",
                        "color": color
                    },
                    {
                        "type": "FactSet",
                        "facts": facts
                    }
                ]
            }
        }]
    });

    FormattedPayload {
        body: serde_json::to_string(&body).unwrap_or_default(),
        content_type: "application/json".into(),
        extra_headers: Vec::new(),
    }
}

// ----------------------------------------------------------------
// Format: PagerDuty (Events API v2)
// ----------------------------------------------------------------

pub fn format_pagerduty(payload: &WebhookPayload, routing_key: &str) -> FormattedPayload {
    let event_action = match payload.event_type {
        WebhookEventType::AlarmCleared
        | WebhookEventType::FddFaultCleared
        | WebhookEventType::DeviceRecovered => "resolve",
        _ => "trigger",
    };

    let severity = match payload.severity.as_deref() {
        Some("life_safety") => "critical",
        Some(s) => s,
        None => "info",
    };

    let dedup_key = payload
        .alarm_id
        .map(|id| format!("opencrate-alarm-{}", id))
        .unwrap_or_default();

    let summary = payload.message.clone().unwrap_or_else(|| {
        format!(
            "{}: {} on {}",
            capitalize(severity),
            payload.point_id.as_deref().unwrap_or("unknown"),
            payload.device_id.as_deref().unwrap_or("unknown"),
        )
    });

    let component = format!(
        "{}/{}",
        payload.device_id.as_deref().unwrap_or(""),
        payload.point_id.as_deref().unwrap_or("")
    );

    let body = serde_json::json!({
        "routing_key": routing_key,
        "event_action": event_action,
        "dedup_key": dedup_key,
        "payload": {
            "summary": summary,
            "severity": severity,
            "source": "OpenCrate BMS",
            "component": component,
            "custom_details": {
                "alarm_id": payload.alarm_id,
                "trigger_value": payload.trigger_value,
                "alarm_type": payload.alarm_type,
            }
        }
    });

    FormattedPayload {
        body: serde_json::to_string(&body).unwrap_or_default(),
        content_type: "application/json".into(),
        extra_headers: Vec::new(),
    }
}

// ----------------------------------------------------------------
// Format: ntfy
// ----------------------------------------------------------------

pub fn format_ntfy(payload: &WebhookPayload) -> FormattedPayload {
    let header = event_header(payload.event_type, payload.severity.as_deref());
    let priority = severity_to_ntfy_priority(payload.severity.as_deref().unwrap_or("info"));

    let tag = match payload.event_type {
        WebhookEventType::AlarmRaised => "rotating_light",
        WebhookEventType::AlarmCleared => "white_check_mark",
        WebhookEventType::AlarmAcknowledged => "eyes",
        WebhookEventType::DeviceDown => "x",
        WebhookEventType::DeviceRecovered => "arrow_up",
        WebhookEventType::FddFaultRaised => "warning",
        WebhookEventType::FddFaultCleared => "white_check_mark",
    };

    let body_text = payload
        .message
        .clone()
        .unwrap_or_else(|| match payload.event_type {
            WebhookEventType::AlarmRaised | WebhookEventType::AlarmCleared => {
                format!(
                    "{}. Value: {}. Device: {}.",
                    payload.alarm_type.as_deref().unwrap_or("Alarm"),
                    payload
                        .trigger_value
                        .map(|v| v.to_string())
                        .unwrap_or_default(),
                    payload.device_id.as_deref().unwrap_or("unknown"),
                )
            }
            _ => {
                format!(
                    "Device {} is {}.",
                    payload.node_id.as_deref().unwrap_or("unknown"),
                    if payload.event_type == WebhookEventType::DeviceDown {
                        "offline"
                    } else {
                        "back online"
                    },
                )
            }
        });

    let extra_headers = vec![
        ("Title".into(), header),
        ("Priority".into(), priority.to_string()),
        ("Tags".into(), tag.into()),
    ];

    FormattedPayload {
        body: body_text,
        content_type: "text/plain".into(),
        extra_headers,
    }
}

/// Format a payload for the given provider.
pub fn format_for_provider(
    provider: Provider,
    payload: &WebhookPayload,
    secret: Option<&str>,
    routing_key: &str,
) -> FormattedPayload {
    match provider {
        Provider::Generic => format_generic(payload, secret),
        Provider::Slack => format_slack(payload),
        Provider::Teams => format_teams(payload),
        Provider::PagerDuty => format_pagerduty(payload, routing_key),
        Provider::Ntfy => format_ntfy(payload),
    }
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_alarm_payload() -> WebhookPayload {
        WebhookPayload {
            event_type: WebhookEventType::AlarmRaised,
            alarm_id: Some(42),
            node_id: Some("bacnet-1000/zone-temp".into()),
            device_id: Some("bacnet-1000".into()),
            point_id: Some("zone-temp".into()),
            alarm_type: Some("high_limit".into()),
            severity: Some("critical".into()),
            trigger_value: Some(85.2),
            message: Some("Zone Temp exceeded high limit (80.0)".into()),
            timestamp_ms: 1711200600000,
            project_name: "Main Campus".into(),
        }
    }

    fn sample_fdd_payload() -> WebhookPayload {
        WebhookPayload {
            event_type: WebhookEventType::FddFaultRaised,
            alarm_id: Some(42),
            node_id: Some("ahu-01".into()),
            device_id: None,
            point_id: None,
            alarm_type: Some("fdd".into()),
            severity: Some("critical".into()),
            trigger_value: None,
            message: Some("FDD fault on equipment ahu-01 (rule #7)".into()),
            timestamp_ms: 1711200600000,
            project_name: "Main Campus".into(),
        }
    }

    fn sample_device_down_payload() -> WebhookPayload {
        WebhookPayload {
            event_type: WebhookEventType::DeviceDown,
            alarm_id: None,
            node_id: Some("bacnet-1000".into()),
            device_id: Some("bacnet".into()),
            point_id: None,
            alarm_type: None,
            severity: None,
            trigger_value: None,
            message: None,
            timestamp_ms: 1711200600000,
            project_name: "Main Campus".into(),
        }
    }

    #[test]
    fn generic_format_contains_event_and_source() {
        let payload = sample_alarm_payload();
        let result = format_generic(&payload, None);
        let json: serde_json::Value = serde_json::from_str(&result.body).unwrap();
        assert_eq!(json["event"], "alarm_raised");
        assert_eq!(json["source"]["system"], "OpenCrate BMS");
        assert_eq!(json["source"]["project"], "Main Campus");
        assert_eq!(json["alarm"]["id"], 42);
        assert_eq!(json["alarm"]["severity"], "critical");
        assert!(result.extra_headers.is_empty());
    }

    #[test]
    fn generic_format_with_hmac() {
        let payload = sample_alarm_payload();
        let result = format_generic(&payload, Some("my-secret"));
        assert_eq!(result.extra_headers.len(), 1);
        assert_eq!(result.extra_headers[0].0, "X-OpenCrate-Signature");
        assert!(result.extra_headers[0].1.starts_with("sha256="));

        // Verify HMAC is deterministic
        let sig = hmac_sha256_hex("my-secret", &result.body);
        assert_eq!(result.extra_headers[0].1, format!("sha256={}", sig));
    }

    #[test]
    fn slack_format_has_blocks() {
        let payload = sample_alarm_payload();
        let result = format_slack(&payload);
        let json: serde_json::Value = serde_json::from_str(&result.body).unwrap();
        assert!(json["blocks"].is_array());
        assert_eq!(json["blocks"][0]["type"], "header");
        assert!(json["attachments"][0]["color"]
            .as_str()
            .unwrap()
            .starts_with('#'));
    }

    #[test]
    fn teams_format_has_adaptive_card() {
        let payload = sample_alarm_payload();
        let result = format_teams(&payload);
        let json: serde_json::Value = serde_json::from_str(&result.body).unwrap();
        assert_eq!(json["type"], "message");
        let card = &json["attachments"][0]["content"];
        assert_eq!(card["type"], "AdaptiveCard");
        assert_eq!(card["version"], "1.4");
    }

    #[test]
    fn pagerduty_trigger_and_resolve() {
        let raised = sample_alarm_payload();
        let result = format_pagerduty(&raised, "test-routing-key");
        let json: serde_json::Value = serde_json::from_str(&result.body).unwrap();
        assert_eq!(json["event_action"], "trigger");
        assert_eq!(json["routing_key"], "test-routing-key");
        assert_eq!(json["dedup_key"], "opencrate-alarm-42");

        let mut cleared = sample_alarm_payload();
        cleared.event_type = WebhookEventType::AlarmCleared;
        let result = format_pagerduty(&cleared, "test-routing-key");
        let json: serde_json::Value = serde_json::from_str(&result.body).unwrap();
        assert_eq!(json["event_action"], "resolve");
        assert_eq!(json["dedup_key"], "opencrate-alarm-42");
    }

    #[test]
    fn ntfy_format_has_headers() {
        let payload = sample_alarm_payload();
        let result = format_ntfy(&payload);
        assert_eq!(result.content_type, "text/plain");
        let headers: std::collections::HashMap<&str, &str> = result
            .extra_headers
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        assert!(headers.contains_key("Title"));
        assert_eq!(headers["Priority"], "4"); // critical = 4
        assert_eq!(headers["Tags"], "rotating_light");
    }

    #[test]
    fn device_event_formats() {
        let payload = sample_device_down_payload();

        // Generic
        let result = format_generic(&payload, None);
        let json: serde_json::Value = serde_json::from_str(&result.body).unwrap();
        assert_eq!(json["event"], "device_down");

        // Slack
        let result = format_slack(&payload);
        let json: serde_json::Value = serde_json::from_str(&result.body).unwrap();
        assert!(json["blocks"][0]["text"]["text"]
            .as_str()
            .unwrap()
            .contains("Device Down"));

        // ntfy
        let result = format_ntfy(&payload);
        assert!(result.body.contains("offline"));
    }

    #[test]
    fn hmac_sha256_known_value() {
        // Verify against a known HMAC-SHA256 value
        let sig = hmac_sha256_hex("key", "The quick brown fox jumps over the lazy dog");
        assert_eq!(
            sig,
            "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
        );
    }
}
