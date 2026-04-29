use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use super::{CloudConnector, CloudError, CloudMessage, GooglePubSubConfig};

// ----------------------------------------------------------------
// Service account key (from GCP JSON key file)
// ----------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
struct ServiceAccountKey {
    client_email: String,
    private_key: String,
    token_uri: String,
}

// ----------------------------------------------------------------
// Pub/Sub API request/response types
// ----------------------------------------------------------------

#[derive(Serialize)]
struct PublishRequest {
    messages: Vec<PubSubMessage>,
}

#[derive(Serialize)]
struct PubSubMessage {
    data: String,
    attributes: std::collections::HashMap<String, String>,
    #[serde(rename = "orderingKey", skip_serializing_if = "Option::is_none")]
    ordering_key: Option<String>,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
}

#[derive(Deserialize)]
struct PublishResponse {
    #[serde(default)]
    message_ids: Vec<String>,
}

#[derive(Deserialize)]
struct PubSubErrorResponse {
    error: Option<PubSubErrorDetail>,
}

#[derive(Deserialize)]
struct PubSubErrorDetail {
    message: String,
}

// ----------------------------------------------------------------
// JWT claims for service account auth
// ----------------------------------------------------------------

#[derive(Serialize)]
struct JwtClaims {
    iss: String,
    scope: String,
    aud: String,
    iat: u64,
    exp: u64,
}

// ----------------------------------------------------------------
// GooglePubSubConnector
// ----------------------------------------------------------------

/// Google Cloud Pub/Sub connector using the REST API with service account JWT auth.
pub struct GooglePubSubConnector {
    config: GooglePubSubConfig,
    client: Option<reqwest::Client>,
    /// Cached access token and its absolute expiry time (unix seconds).
    token: Arc<Mutex<(String, i64)>>,
    /// Loaded service account key (populated on connect).
    service_account: Option<ServiceAccountKey>,
}

impl GooglePubSubConnector {
    pub fn new(config: GooglePubSubConfig) -> Self {
        Self {
            config,
            client: None,
            token: Arc::new(Mutex::new((String::new(), 0))),
            service_account: None,
        }
    }

    /// Load and parse the service account JSON key file.
    fn load_service_account(path: &str) -> Result<ServiceAccountKey, CloudError> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            CloudError::Config(format!("failed to read credentials file '{path}': {e}"))
        })?;
        serde_json::from_str::<ServiceAccountKey>(&content)
            .map_err(|e| CloudError::Config(format!("invalid service account key JSON: {e}")))
    }

    /// Create a signed JWT and exchange it for an access token.
    async fn obtain_access_token(
        client: &reqwest::Client,
        sa: &ServiceAccountKey,
    ) -> Result<(String, i64), CloudError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let claims = JwtClaims {
            iss: sa.client_email.clone(),
            scope: "https://www.googleapis.com/auth/pubsub".to_string(),
            aud: sa.token_uri.clone(),
            iat: now,
            exp: now + 3600,
        };

        let header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
        let encoding_key = jsonwebtoken::EncodingKey::from_rsa_pem(sa.private_key.as_bytes())
            .map_err(|e| CloudError::Auth(format!("invalid RSA private key: {e}")))?;

        let jwt = jsonwebtoken::encode(&header, &claims, &encoding_key)
            .map_err(|e| CloudError::Auth(format!("JWT signing failed: {e}")))?;

        let body = format!(
            "grant_type={}&assertion={}",
            urlencoding::encode("urn:ietf:params:oauth:grant-type:jwt-bearer"),
            urlencoding::encode(&jwt),
        );

        let resp = client
            .post(&sa.token_uri)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .map_err(|e| CloudError::Auth(format!("token request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(CloudError::Auth(format!(
                "token endpoint returned {status}: {text}"
            )));
        }

        let token_resp: TokenResponse = resp
            .json()
            .await
            .map_err(|e| CloudError::Auth(format!("failed to parse token response: {e}")))?;

        let expiry = now as i64 + token_resp.expires_in as i64;

        tracing::debug!(
            "obtained GCP access token (expires in {}s)",
            token_resp.expires_in
        );

        Ok((token_resp.access_token, expiry))
    }

    /// Return a valid access token, refreshing if expired or about to expire.
    async fn get_token(&self, client: &reqwest::Client) -> Result<String, CloudError> {
        let sa = self
            .service_account
            .as_ref()
            .ok_or_else(|| CloudError::Auth("service account not loaded".into()))?;

        let mut guard = self.token.lock().await;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Refresh if token is empty or within 5 minutes of expiry.
        if guard.0.is_empty() || now >= guard.1 - 300 {
            let (token, expiry) = Self::obtain_access_token(client, sa).await?;
            *guard = (token, expiry);
        }

        Ok(guard.0.clone())
    }

    /// Build the Pub/Sub publish URL for the configured project and topic.
    fn publish_url(&self) -> String {
        format!(
            "https://pubsub.googleapis.com/v1/projects/{}/topics/{}:publish",
            self.config.project_id, self.config.topic_id,
        )
    }
}

// ----------------------------------------------------------------
// CloudConnector implementation
// ----------------------------------------------------------------

#[async_trait::async_trait]
impl CloudConnector for GooglePubSubConnector {
    async fn connect(&mut self) -> Result<(), CloudError> {
        let sa = Self::load_service_account(&self.config.credentials_json_path)?;
        self.service_account = Some(sa);

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| CloudError::Connection(format!("failed to create HTTP client: {e}")))?;

        // Obtain initial access token.
        let (token, expiry) =
            Self::obtain_access_token(&client, self.service_account.as_ref().unwrap()).await?;

        *self.token.lock().await = (token, expiry);
        self.client = Some(client);

        tracing::info!(
            project = %self.config.project_id,
            topic = %self.config.topic_id,
            "connected to Google Cloud Pub/Sub"
        );

        Ok(())
    }

    async fn test_connection(&self) -> Result<(), CloudError> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| CloudError::Connection("not connected".into()))?;

        let token = self.get_token(client).await?;

        // Publish an empty test message with a `test=true` attribute.
        let mut attrs = std::collections::HashMap::new();
        attrs.insert("test".to_string(), "true".to_string());

        let msg = PubSubMessage {
            data: BASE64.encode(b""),
            attributes: attrs,
            ordering_key: None,
        };

        let body = PublishRequest {
            messages: vec![msg],
        };

        let resp = client
            .post(&self.publish_url())
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .map_err(|e| CloudError::Connection(format!("test publish failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            // Try to extract a structured error message.
            if let Ok(err_resp) = serde_json::from_str::<PubSubErrorResponse>(&text) {
                if let Some(detail) = err_resp.error {
                    return Err(CloudError::Connection(format!(
                        "test publish returned {status}: {}",
                        detail.message
                    )));
                }
            }
            return Err(CloudError::Connection(format!(
                "test publish returned {status}: {text}"
            )));
        }

        tracing::debug!("GCP Pub/Sub test connection succeeded");
        Ok(())
    }

    async fn publish_batch(&self, messages: &[CloudMessage]) -> Result<usize, CloudError> {
        if messages.is_empty() {
            return Ok(0);
        }

        let client = self
            .client
            .as_ref()
            .ok_or_else(|| CloudError::Connection("not connected".into()))?;

        let token = self.get_token(client).await?;

        let ordering_prefix = self.config.ordering_key_prefix.as_deref();

        // Google Pub/Sub supports up to 1000 messages per publish request.
        const MAX_BATCH: usize = 1000;
        let mut total_published = 0usize;

        for chunk in messages.chunks(MAX_BATCH) {
            let pubsub_messages: Vec<PubSubMessage> = chunk
                .iter()
                .map(|m| {
                    let mut attrs = std::collections::HashMap::new();
                    attrs.insert("event_type".to_string(), event_type_str(m.event_type));
                    attrs.insert("topic_suffix".to_string(), m.topic_suffix.clone());
                    attrs.insert("timestamp".to_string(), m.timestamp_ms.to_string());

                    let ordering_key =
                        ordering_prefix.map(|prefix| format!("{prefix}/{}", m.topic_suffix));

                    PubSubMessage {
                        data: BASE64.encode(m.payload.as_bytes()),
                        attributes: attrs,
                        ordering_key,
                    }
                })
                .collect();

            let body = PublishRequest {
                messages: pubsub_messages,
            };

            let resp = client
                .post(&self.publish_url())
                .bearer_auth(&token)
                .json(&body)
                .send()
                .await
                .map_err(|e| CloudError::Publish(format!("publish request failed: {e}")))?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                if let Ok(err_resp) = serde_json::from_str::<PubSubErrorResponse>(&text) {
                    if let Some(detail) = err_resp.error {
                        return Err(CloudError::Publish(format!(
                            "publish returned {status}: {}",
                            detail.message
                        )));
                    }
                }
                return Err(CloudError::Publish(format!(
                    "publish returned {status}: {text}"
                )));
            }

            // Parse response to count confirmed message IDs.
            let publish_resp: PublishResponse = resp.json().await.map_err(|e| {
                CloudError::Publish(format!("failed to parse publish response: {e}"))
            })?;

            total_published += publish_resp.message_ids.len();
        }

        tracing::debug!(count = total_published, "published messages to GCP Pub/Sub");
        Ok(total_published)
    }

    async fn health_check(&mut self) -> Result<(), CloudError> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| CloudError::Connection("not connected".into()))?
            .clone();

        // get_token will refresh automatically if within 5 minutes of expiry.
        self.get_token(&client).await?;
        Ok(())
    }

    async fn close(&self) {
        // HTTP client has no persistent connection to tear down.
        tracing::debug!("GCP Pub/Sub connector closed");
    }
}

// ----------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------

fn event_type_str(et: super::CloudEventType) -> String {
    match et {
        super::CloudEventType::Telemetry => "telemetry".to_string(),
        super::CloudEventType::Alarm => "alarm".to_string(),
        super::CloudEventType::DeviceStatus => "device_status".to_string(),
        super::CloudEventType::FddFault => "fdd_fault".to_string(),
    }
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connector_creation() {
        let config = GooglePubSubConfig {
            project_id: "test-project".to_string(),
            topic_id: "bms-events".to_string(),
            credentials_json_path: "/tmp/sa.json".to_string(),
            ordering_key_prefix: None,
        };
        let conn = GooglePubSubConnector::new(config);
        assert!(conn.client.is_none());
        assert!(conn.service_account.is_none());
        assert_eq!(
            conn.publish_url(),
            "https://pubsub.googleapis.com/v1/projects/test-project/topics/bms-events:publish"
        );
    }

    #[test]
    fn publish_url_format() {
        let config = GooglePubSubConfig {
            project_id: "my-gcp-project".to_string(),
            topic_id: "opencrate-telemetry".to_string(),
            credentials_json_path: "/keys/sa.json".to_string(),
            ordering_key_prefix: Some("site-1".to_string()),
        };
        let conn = GooglePubSubConnector::new(config);
        assert_eq!(
            conn.publish_url(),
            "https://pubsub.googleapis.com/v1/projects/my-gcp-project/topics/opencrate-telemetry:publish"
        );
    }

    #[test]
    fn event_type_string_mapping() {
        assert_eq!(
            event_type_str(super::super::CloudEventType::Telemetry),
            "telemetry"
        );
        assert_eq!(event_type_str(super::super::CloudEventType::Alarm), "alarm");
        assert_eq!(
            event_type_str(super::super::CloudEventType::DeviceStatus),
            "device_status"
        );
        assert_eq!(
            event_type_str(super::super::CloudEventType::FddFault),
            "fdd_fault"
        );
    }

    #[test]
    fn service_account_deserialize() {
        let json = r#"{
            "type": "service_account",
            "project_id": "test",
            "private_key_id": "abc123",
            "private_key": "-----BEGIN RSA PRIVATE KEY-----\nfake\n-----END RSA PRIVATE KEY-----\n",
            "client_email": "sa@test.iam.gserviceaccount.com",
            "client_id": "123",
            "auth_uri": "https://accounts.google.com/o/oauth2/auth",
            "token_uri": "https://oauth2.googleapis.com/token",
            "auth_provider_x509_cert_url": "https://www.googleapis.com/oauth2/v1/certs",
            "client_x509_cert_url": "https://www.googleapis.com/robot/v1/metadata/x509/sa%40test.iam.gserviceaccount.com"
        }"#;
        let sa: ServiceAccountKey = serde_json::from_str(json).unwrap();
        assert_eq!(sa.client_email, "sa@test.iam.gserviceaccount.com");
        assert_eq!(sa.token_uri, "https://oauth2.googleapis.com/token");
        assert!(sa.private_key.contains("RSA PRIVATE KEY"));
    }

    #[test]
    fn load_missing_credentials_file() {
        let result = GooglePubSubConnector::load_service_account("/nonexistent/path.json");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, CloudError::Config(_)));
    }
}
