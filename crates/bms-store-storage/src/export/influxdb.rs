use crate::export::{ExportAlarm, ExportConnector, ExportError, ExportSample, InfluxDbConfig};

/// InfluxDB v2 connector using Line Protocol over HTTP (reqwest).
pub struct InfluxDbConnector {
    config: InfluxDbConfig,
    client: reqwest::Client,
}

impl InfluxDbConnector {
    pub fn new(config: InfluxDbConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default();
        Self { config, client }
    }

    fn write_url(&self) -> String {
        format!(
            "{}/api/v2/write?org={}&bucket={}&precision=ms",
            self.config.url.trim_end_matches('/'),
            urlencoding::encode(&self.config.org),
            urlencoding::encode(&self.config.bucket),
        )
    }

    fn health_url(&self) -> String {
        format!("{}/health", self.config.url.trim_end_matches('/'))
    }
}

#[async_trait::async_trait]
impl ExportConnector for InfluxDbConnector {
    async fn test_connection(&self) -> Result<(), ExportError> {
        let resp = self
            .client
            .get(self.health_url())
            .send()
            .await
            .map_err(|e| ExportError::Connection(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ExportError::Connection(format!(
                "InfluxDB health check failed: {} {}",
                status, body
            )));
        }
        Ok(())
    }

    async fn write_history_batch(&self, samples: &[ExportSample]) -> Result<usize, ExportError> {
        if samples.is_empty() {
            return Ok(0);
        }

        let body = encode_line_protocol_samples(&self.config.measurement, samples);
        let resp = self
            .client
            .post(self.write_url())
            .header("Authorization", format!("Token {}", self.config.token))
            .header("Content-Type", "text/plain; charset=utf-8")
            .body(body)
            .send()
            .await
            .map_err(|e| ExportError::Write(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ExportError::Write(format!(
                "InfluxDB write failed: {} {}",
                status, body
            )));
        }

        Ok(samples.len())
    }

    async fn write_alarm_batch(&self, alarms: &[ExportAlarm]) -> Result<usize, ExportError> {
        if alarms.is_empty() {
            return Ok(0);
        }

        let body = encode_line_protocol_alarms(alarms);
        let resp = self
            .client
            .post(self.write_url())
            .header("Authorization", format!("Token {}", self.config.token))
            .header("Content-Type", "text/plain; charset=utf-8")
            .body(body)
            .send()
            .await
            .map_err(|e| ExportError::Write(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ExportError::Write(format!(
                "InfluxDB alarm write failed: {} {}",
                status, body
            )));
        }

        Ok(alarms.len())
    }

    async fn close(&self) {
        // HTTP client needs no explicit close
    }
}

// ----------------------------------------------------------------
// Line Protocol encoding
// ----------------------------------------------------------------

/// Encode samples as InfluxDB Line Protocol.
///
/// Format: `<measurement>,device=<dev>,point=<pt> value=<v> <timestamp_ms>`
pub fn encode_line_protocol_samples(measurement: &str, samples: &[ExportSample]) -> String {
    let mut buf = String::with_capacity(samples.len() * 80);
    for s in samples {
        // Escape tag values: commas, spaces, equals
        let device = escape_tag_value(&s.device_id);
        let point = escape_tag_value(&s.point_id);

        buf.push_str(measurement);
        buf.push_str(",device=");
        buf.push_str(&device);
        buf.push_str(",point=");
        buf.push_str(&point);
        buf.push_str(" value=");
        buf.push_str(&format_float(s.value));
        buf.push(' ');
        buf.push_str(&s.timestamp_ms.to_string());
        buf.push('\n');
    }
    buf
}

/// Encode alarm events as InfluxDB Line Protocol.
///
/// Format: `alarm,node=<id>,severity=<sev> state="<state>",alarm_id=<id>i <timestamp_ms>`
pub fn encode_line_protocol_alarms(alarms: &[ExportAlarm]) -> String {
    let mut buf = String::with_capacity(alarms.len() * 120);
    for a in alarms {
        let node = escape_tag_value(&a.node_id);
        let severity = escape_tag_value(&a.severity);

        buf.push_str("alarm,node=");
        buf.push_str(&node);
        buf.push_str(",severity=");
        buf.push_str(&severity);
        buf.push_str(" state=\"");
        buf.push_str(&escape_field_string(&a.state));
        buf.push_str("\",alarm_id=");
        buf.push_str(&a.alarm_id.to_string());
        buf.push('i');

        if let Some(v) = a.value {
            buf.push_str(",value=");
            buf.push_str(&format_float(v));
        }

        buf.push(' ');
        buf.push_str(&a.timestamp_ms.to_string());
        buf.push('\n');
    }
    buf
}

/// Escape characters that are special in InfluxDB tag values: comma, space, equals.
fn escape_tag_value(s: &str) -> String {
    s.replace(',', r"\,")
        .replace(' ', r"\ ")
        .replace('=', r"\=")
}

/// Escape characters in InfluxDB field string values: backslash, double quote.
fn escape_field_string(s: &str) -> String {
    s.replace('\\', r"\\").replace('"', r#"\""#)
}

/// Format a float for Line Protocol, avoiding scientific notation.
fn format_float(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{:.1}", v)
    } else {
        format!("{}", v)
    }
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_protocol_simple() {
        let samples = vec![ExportSample {
            point_key: "ahu-1/dat".into(),
            device_id: "ahu-1".into(),
            point_id: "dat".into(),
            value: 72.5,
            timestamp_ms: 1711756800000,
        }];
        let lp = encode_line_protocol_samples("point_value", &samples);
        assert_eq!(
            lp,
            "point_value,device=ahu-1,point=dat value=72.5 1711756800000\n"
        );
    }

    #[test]
    fn line_protocol_batch() {
        let samples = vec![
            ExportSample {
                point_key: "ahu-1/dat".into(),
                device_id: "ahu-1".into(),
                point_id: "dat".into(),
                value: 72.5,
                timestamp_ms: 1000,
            },
            ExportSample {
                point_key: "ahu-1/sat".into(),
                device_id: "ahu-1".into(),
                point_id: "sat".into(),
                value: 55.0,
                timestamp_ms: 1000,
            },
        ];
        let lp = encode_line_protocol_samples("point_value", &samples);
        let lines: Vec<&str> = lp.trim().split('\n').collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("point_value,device=ahu-1,point=dat"));
        assert!(lines[1].starts_with("point_value,device=ahu-1,point=sat"));
    }

    #[test]
    fn line_protocol_escape_tags() {
        let samples = vec![ExportSample {
            point_key: "test".into(),
            device_id: "device with, special=chars".into(),
            point_id: "point".into(),
            value: 1.0,
            timestamp_ms: 1000,
        }];
        let lp = encode_line_protocol_samples("m", &samples);
        assert!(lp.contains(r"device=device\ with\,\ special\=chars"));
    }

    #[test]
    fn line_protocol_integer_value() {
        let samples = vec![ExportSample {
            point_key: "test".into(),
            device_id: "d".into(),
            point_id: "p".into(),
            value: 100.0,
            timestamp_ms: 1000,
        }];
        let lp = encode_line_protocol_samples("m", &samples);
        assert!(lp.contains("value=100.0"));
    }

    #[test]
    fn alarm_line_protocol() {
        let alarms = vec![ExportAlarm {
            alarm_id: 42,
            node_id: "ahu-1/dat".into(),
            severity: "critical".into(),
            state: "raised".into(),
            timestamp_ms: 2000,
            value: Some(95.5),
            note: None,
        }];
        let lp = encode_line_protocol_alarms(&alarms);
        assert!(lp.contains("alarm,node=ahu-1/dat,severity=critical"));
        assert!(lp.contains("state=\"raised\""));
        assert!(lp.contains("alarm_id=42i"));
        assert!(lp.contains("value=95.5"));
        assert!(lp.contains("2000\n"));
    }

    #[test]
    fn alarm_line_protocol_no_value() {
        let alarms = vec![ExportAlarm {
            alarm_id: 1,
            node_id: "vav-1/zat".into(),
            severity: "warning".into(),
            state: "cleared".into(),
            timestamp_ms: 3000,
            value: None,
            note: None,
        }];
        let lp = encode_line_protocol_alarms(&alarms);
        assert!(!lp.contains("value="));
        assert!(lp.contains("alarm_id=1i 3000"));
    }

    #[test]
    fn escape_field_string_special_chars() {
        assert_eq!(
            escape_field_string(r#"hello "world""#),
            r#"hello \"world\""#
        );
        assert_eq!(escape_field_string(r"back\slash"), r"back\\slash");
    }

    #[test]
    fn write_url_construction() {
        let cfg = InfluxDbConfig {
            url: "http://localhost:8086".into(),
            token: "tok".into(),
            org: "my org".into(),
            bucket: "my bucket".into(),
            measurement: "point_value".into(),
        };
        let connector = InfluxDbConnector::new(cfg);
        let url = connector.write_url();
        assert!(url.contains("org=my%20org"));
        assert!(url.contains("bucket=my%20bucket"));
        assert!(url.contains("precision=ms"));
    }
}
