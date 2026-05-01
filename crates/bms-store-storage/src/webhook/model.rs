use serde::{Deserialize, Serialize};

// ----------------------------------------------------------------
// Provider
// ----------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Slack,
    Teams,
    PagerDuty,
    Ntfy,
    Generic,
}

impl Provider {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Slack => "slack",
            Self::Teams => "teams",
            Self::PagerDuty => "pagerduty",
            Self::Ntfy => "ntfy",
            Self::Generic => "generic",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "slack" => Some(Self::Slack),
            "teams" => Some(Self::Teams),
            "pagerduty" => Some(Self::PagerDuty),
            "ntfy" => Some(Self::Ntfy),
            "generic" => Some(Self::Generic),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Slack => "Slack",
            Self::Teams => "Teams",
            Self::PagerDuty => "PagerDuty",
            Self::Ntfy => "ntfy",
            Self::Generic => "Generic",
        }
    }

    pub fn all() -> &'static [Provider] {
        &[
            Self::Slack,
            Self::Teams,
            Self::PagerDuty,
            Self::Ntfy,
            Self::Generic,
        ]
    }
}

// ----------------------------------------------------------------
// Delivery status
// ----------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryStatus {
    Delivered,
    Failed,
    Retrying,
}

impl DeliveryStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Delivered => "delivered",
            Self::Failed => "failed",
            Self::Retrying => "retrying",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "delivered" => Some(Self::Delivered),
            "failed" => Some(Self::Failed),
            "retrying" => Some(Self::Retrying),
            _ => None,
        }
    }
}

// ----------------------------------------------------------------
// Webhook event types
// ----------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebhookEventType {
    AlarmRaised,
    AlarmCleared,
    AlarmAcknowledged,
    DeviceDown,
    DeviceRecovered,
    FddFaultRaised,
    FddFaultCleared,
}

impl WebhookEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AlarmRaised => "alarm_raised",
            Self::AlarmCleared => "alarm_cleared",
            Self::AlarmAcknowledged => "alarm_acknowledged",
            Self::DeviceDown => "device_down",
            Self::DeviceRecovered => "device_recovered",
            Self::FddFaultRaised => "fdd_fault_raised",
            Self::FddFaultCleared => "fdd_fault_cleared",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "alarm_raised" => Some(Self::AlarmRaised),
            "alarm_cleared" => Some(Self::AlarmCleared),
            "alarm_acknowledged" => Some(Self::AlarmAcknowledged),
            "device_down" => Some(Self::DeviceDown),
            "device_recovered" => Some(Self::DeviceRecovered),
            "fdd_fault_raised" => Some(Self::FddFaultRaised),
            "fdd_fault_cleared" => Some(Self::FddFaultCleared),
            _ => None,
        }
    }
}

// ----------------------------------------------------------------
// Tag matcher (for tag-based filtering)
// ----------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagMatcher {
    pub tag: String,
    pub op: String,
    pub value: String,
}

// ----------------------------------------------------------------
// Webhook endpoint (persisted config)
// ----------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookEndpoint {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub url: String,
    pub headers: Option<String>,
    pub secret: Option<String>,
    pub enabled: bool,
    pub on_alarm_raised: bool,
    pub on_alarm_cleared: bool,
    pub on_alarm_acknowledged: bool,
    pub on_device_down: bool,
    pub on_device_recovered: bool,
    pub on_fdd_fault_raised: bool,
    pub on_fdd_fault_cleared: bool,
    pub min_severity: String,
    pub tag_filters: Option<String>,
    pub created_ms: i64,
    pub updated_ms: i64,
}

impl WebhookEndpoint {
    /// Check if this endpoint's toggle is enabled for the given event type.
    pub fn accepts_event(&self, event_type: WebhookEventType) -> bool {
        match event_type {
            WebhookEventType::AlarmRaised => self.on_alarm_raised,
            WebhookEventType::AlarmCleared => self.on_alarm_cleared,
            WebhookEventType::AlarmAcknowledged => self.on_alarm_acknowledged,
            WebhookEventType::DeviceDown => self.on_device_down,
            WebhookEventType::DeviceRecovered => self.on_device_recovered,
            WebhookEventType::FddFaultRaised => self.on_fdd_fault_raised,
            WebhookEventType::FddFaultCleared => self.on_fdd_fault_cleared,
        }
    }

    /// Parse the tag_filters JSON into TagMatcher list.
    pub fn parsed_tag_filters(&self) -> Vec<TagMatcher> {
        self.tag_filters
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }

    /// Parse the provider string into a Provider enum.
    pub fn parsed_provider(&self) -> Option<Provider> {
        Provider::from_str(&self.provider)
    }
}

// ----------------------------------------------------------------
// Webhook delivery (log entry)
// ----------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookDelivery {
    pub id: i64,
    pub endpoint_id: String,
    pub event_type: String,
    pub timestamp_ms: i64,
    pub status: String,
    pub http_status: Option<u16>,
    pub error: Option<String>,
    pub payload_preview: Option<String>,
}

// ----------------------------------------------------------------
// Webhook payload (enriched event data for formatters)
// ----------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct WebhookPayload {
    pub event_type: WebhookEventType,
    pub alarm_id: Option<i64>,
    pub node_id: Option<String>,
    pub device_id: Option<String>,
    pub point_id: Option<String>,
    pub alarm_type: Option<String>,
    pub severity: Option<String>,
    pub trigger_value: Option<f64>,
    pub message: Option<String>,
    pub timestamp_ms: i64,
    pub project_name: String,
}

// ----------------------------------------------------------------
// Formatted payload (ready to send)
// ----------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct FormattedPayload {
    pub body: String,
    pub content_type: String,
    pub extra_headers: Vec<(String, String)>,
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_roundtrip() {
        for p in Provider::all() {
            let s = p.as_str();
            assert_eq!(Provider::from_str(s), Some(*p));
        }
        assert_eq!(Provider::from_str("unknown"), None);
    }

    #[test]
    fn delivery_status_roundtrip() {
        for (s, expected) in [
            ("delivered", DeliveryStatus::Delivered),
            ("failed", DeliveryStatus::Failed),
            ("retrying", DeliveryStatus::Retrying),
        ] {
            assert_eq!(DeliveryStatus::from_str(s), Some(expected));
            assert_eq!(expected.as_str(), s);
        }
    }

    #[test]
    fn webhook_event_type_roundtrip() {
        for (s, expected) in [
            ("device_down", WebhookEventType::DeviceDown),
            ("device_recovered", WebhookEventType::DeviceRecovered),
            ("fdd_fault_raised", WebhookEventType::FddFaultRaised),
            ("fdd_fault_cleared", WebhookEventType::FddFaultCleared),
        ] {
            assert_eq!(WebhookEventType::from_str(s), Some(expected));
            assert_eq!(expected.as_str(), s);
        }
        assert_eq!(WebhookEventType::from_str("unknown"), None);
    }

    #[test]
    fn endpoint_accepts_event() {
        let ep = WebhookEndpoint {
            id: "test".into(),
            name: "Test".into(),
            provider: "generic".into(),
            url: "https://example.com".into(),
            headers: None,
            secret: None,
            enabled: true,
            on_alarm_raised: true,
            on_alarm_cleared: false,
            on_alarm_acknowledged: true,
            on_device_down: true,
            on_device_recovered: false,
            on_fdd_fault_raised: true,
            on_fdd_fault_cleared: false,
            min_severity: "info".into(),
            tag_filters: None,
            created_ms: 0,
            updated_ms: 0,
        };
        assert!(ep.accepts_event(WebhookEventType::DeviceDown));
        assert!(!ep.accepts_event(WebhookEventType::DeviceRecovered));
        assert!(ep.accepts_event(WebhookEventType::FddFaultRaised));
        assert!(!ep.accepts_event(WebhookEventType::FddFaultCleared));
    }

    #[test]
    fn tag_filter_parsing() {
        let ep = WebhookEndpoint {
            id: "test".into(),
            name: "Test".into(),
            provider: "generic".into(),
            url: "https://example.com".into(),
            headers: None,
            secret: None,
            enabled: true,
            on_alarm_raised: true,
            on_alarm_cleared: true,
            on_alarm_acknowledged: true,
            on_device_down: true,
            on_device_recovered: true,
            on_fdd_fault_raised: true,
            on_fdd_fault_cleared: true,
            min_severity: "info".into(),
            tag_filters: Some(r#"[{"tag":"equip","op":"=","value":"ahu"}]"#.into()),
            created_ms: 0,
            updated_ms: 0,
        };
        let filters = ep.parsed_tag_filters();
        assert_eq!(filters.len(), 1);
        assert_eq!(filters[0].tag, "equip");
        assert_eq!(filters[0].op, "=");
        assert_eq!(filters[0].value, "ahu");

        // Empty / null tag_filters returns empty vec
        let ep2 = WebhookEndpoint {
            tag_filters: None,
            ..ep
        };
        assert!(ep2.parsed_tag_filters().is_empty());
    }
}
