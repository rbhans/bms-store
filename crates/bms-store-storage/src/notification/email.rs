use super::channel::{ChannelError, NotificationChannel, NotificationPayload};
use crate::store::notification_store::ChannelType;

pub struct EmailChannel {
    http_client: reqwest::Client,
}

impl Default for EmailChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl EmailChannel {
    pub fn new() -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .unwrap_or_default();
        Self { http_client }
    }

    fn format_subject(payload: &NotificationPayload) -> String {
        format!(
            "[{}] {} — {}/{}",
            payload.severity.to_uppercase(),
            payload.event_type.label(),
            payload.device_id,
            payload.point_id,
        )
    }

    fn format_body(payload: &NotificationPayload) -> String {
        format!(
            "{event}\n\nDevice: {device}\nPoint: {point}\nType: {atype}\nSeverity: {severity}\nValue: {value}\nTime: {time}\nProject: {project}\n",
            event = payload.event_type.label(),
            device = payload.device_id,
            point = payload.point_id,
            atype = payload.alarm_type,
            severity = payload.severity,
            value = payload.trigger_value,
            time = payload.trigger_time_ms,
            project = payload.project_name,
        )
    }
}

#[async_trait::async_trait]
impl NotificationChannel for EmailChannel {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Email
    }

    async fn send(
        &self,
        address: &str,
        config: &str,
        payload: &NotificationPayload,
    ) -> Result<(), ChannelError> {
        let cfg: serde_json::Value =
            serde_json::from_str(config).map_err(|e| ChannelError::Config(e.to_string()))?;

        // Check for HTTP API mode first (SendGrid/Mailgun/generic)
        if let Some(api_url) = cfg.get("api_url").and_then(|v| v.as_str()) {
            let api_key = cfg.get("api_key").and_then(|v| v.as_str()).unwrap_or("");
            let from = cfg
                .get("from_address")
                .and_then(|v| v.as_str())
                .unwrap_or("opencrate@localhost");

            let body = serde_json::json!({
                "to": address,
                "from": from,
                "subject": Self::format_subject(payload),
                "text": Self::format_body(payload),
            });

            let mut req = self.http_client.post(api_url).json(&body);
            if !api_key.is_empty() {
                req = req.bearer_auth(api_key);
            }
            let resp = req
                .send()
                .await
                .map_err(|e| ChannelError::Transport(e.to_string()))?;
            if resp.status().is_success() {
                return Ok(());
            }
            return Err(ChannelError::Transport(format!("HTTP {}", resp.status())));
        }

        // SMTP mode via lettre
        let host = cfg
            .get("smtp_host")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ChannelError::Config("missing smtp_host".into()))?;
        let port = cfg.get("smtp_port").and_then(|v| v.as_u64()).unwrap_or(587) as u16;
        let username = cfg.get("smtp_user").and_then(|v| v.as_str()).unwrap_or("");
        let password = cfg
            .get("smtp_password")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let from_addr = cfg
            .get("from_address")
            .and_then(|v| v.as_str())
            .unwrap_or("opencrate@localhost");
        let use_tls = cfg.get("use_tls").and_then(|v| v.as_bool()).unwrap_or(true);

        let email = lettre::Message::builder()
            .from(
                from_addr
                    .parse()
                    .map_err(|e: lettre::address::AddressError| {
                        ChannelError::Config(e.to_string())
                    })?,
            )
            .to(address
                .parse()
                .map_err(|e: lettre::address::AddressError| ChannelError::Config(e.to_string()))?)
            .subject(Self::format_subject(payload))
            .body(Self::format_body(payload))
            .map_err(|e| ChannelError::Config(e.to_string()))?;

        // Build transport based on TLS setting
        use lettre::transport::smtp::authentication::Credentials;
        use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};

        let creds = Credentials::new(username.to_string(), password.to_string());

        let transport = if use_tls {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)
                .map_err(|e| ChannelError::Transport(e.to_string()))?
                .port(port)
                .credentials(creds)
                .build()
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host)
                .port(port)
                .credentials(creds)
                .build()
        };

        transport
            .send(email)
            .await
            .map_err(|e| ChannelError::Transport(e.to_string()))?;
        Ok(())
    }
}
