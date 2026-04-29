use crate::store::notification_store::ChannelType;
use serde::{Deserialize, Serialize};

/// Payload delivered to notification channels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationPayload {
    pub alarm_id: i64,
    pub alarm_config_id: i64,
    pub device_id: String,
    pub point_id: String,
    pub alarm_type: String,
    pub severity: String,
    pub trigger_value: f64,
    pub trigger_time_ms: i64,
    pub context_snapshot: String,
    pub event_type: NotificationEventType,
    pub recipient_name: String,
    pub project_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationEventType {
    Raised,
    Cleared,
    Escalated,
}

impl NotificationEventType {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Raised => "ALARM RAISED",
            Self::Cleared => "ALARM CLEARED",
            Self::Escalated => "ALARM ESCALATED",
        }
    }
}

/// Error from a notification channel.
#[derive(Debug, thiserror::Error)]
pub enum ChannelError {
    #[error("transport error: {0}")]
    Transport(String),
    #[error("configuration error: {0}")]
    Config(String),
    #[error("rate limited")]
    RateLimited,
}

/// Trait for notification delivery channels.
#[async_trait::async_trait]
pub trait NotificationChannel: Send + Sync {
    fn channel_type(&self) -> ChannelType;
    async fn send(
        &self,
        address: &str,
        config: &str,
        payload: &NotificationPayload,
    ) -> Result<(), ChannelError>;
}
