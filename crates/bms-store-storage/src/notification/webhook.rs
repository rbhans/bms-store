use super::channel::{ChannelError, NotificationChannel, NotificationPayload};
use crate::store::notification_store::ChannelType;

pub struct WebhookChannel {
    client: reqwest::Client,
}

impl Default for WebhookChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl WebhookChannel {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_default();
        Self { client }
    }
}

#[async_trait::async_trait]
impl NotificationChannel for WebhookChannel {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Webhook
    }

    async fn send(
        &self,
        address: &str,
        config: &str,
        payload: &NotificationPayload,
    ) -> Result<(), ChannelError> {
        let mut builder = self.client.post(address).json(payload);

        // Parse custom headers from config JSON
        if let Ok(cfg) = serde_json::from_str::<serde_json::Value>(config) {
            if let Some(headers) = cfg.get("headers").and_then(|h| h.as_object()) {
                for (key, value) in headers {
                    if let Some(v) = value.as_str() {
                        if let (Ok(name), Ok(val)) = (
                            reqwest::header::HeaderName::from_bytes(key.as_bytes()),
                            reqwest::header::HeaderValue::from_str(v),
                        ) {
                            builder = builder.header(name, val);
                        }
                    }
                }
            }
        }

        let resp = builder
            .send()
            .await
            .map_err(|e| ChannelError::Transport(e.to_string()))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(ChannelError::Transport(format!("HTTP {}", resp.status())))
        }
    }
}
