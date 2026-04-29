use super::channel::{ChannelError, NotificationChannel, NotificationPayload};
use crate::store::notification_store::ChannelType;

pub struct SmsChannel {
    client: reqwest::Client,
}

impl Default for SmsChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl SmsChannel {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_default();
        Self { client }
    }

    fn format_message(payload: &NotificationPayload) -> String {
        format!(
            "{} {}/{} [{}] val={:.1}",
            payload.event_type.label(),
            payload.device_id,
            payload.point_id,
            payload.severity.to_uppercase(),
            payload.trigger_value,
        )
    }
}

#[async_trait::async_trait]
impl NotificationChannel for SmsChannel {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Sms
    }

    async fn send(
        &self,
        address: &str,
        config: &str,
        payload: &NotificationPayload,
    ) -> Result<(), ChannelError> {
        let cfg: serde_json::Value =
            serde_json::from_str(config).map_err(|e| ChannelError::Config(e.to_string()))?;

        let api_url = cfg
            .get("api_url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ChannelError::Config("missing api_url in SMS config".into()))?;
        let provider = cfg
            .get("provider")
            .and_then(|v| v.as_str())
            .unwrap_or("generic");
        let message = Self::format_message(payload);

        let resp = match provider {
            "twilio" => {
                let account_sid = cfg
                    .get("account_sid")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let auth_token = cfg.get("auth_token").and_then(|v| v.as_str()).unwrap_or("");
                let from_number = cfg
                    .get("from_number")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                self.client
                    .post(api_url)
                    .basic_auth(account_sid, Some(auth_token))
                    .form(&[("To", address), ("From", from_number), ("Body", &message)])
                    .send()
                    .await
                    .map_err(|e| ChannelError::Transport(e.to_string()))?
            }
            _ => {
                // Generic JSON POST
                let body = serde_json::json!({
                    "to": address,
                    "message": message,
                });
                self.client
                    .post(api_url)
                    .json(&body)
                    .send()
                    .await
                    .map_err(|e| ChannelError::Transport(e.to_string()))?
            }
        };

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(ChannelError::Transport(format!("HTTP {}", resp.status())))
        }
    }
}
