pub mod aws;
pub mod azure;
pub mod gcp;
pub mod publisher;

use serde::{Deserialize, Serialize};

// ----------------------------------------------------------------
// Cloud provider
// ----------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudProvider {
    AwsIotCore,
    AzureIotHub,
    GooglePubSub,
}

impl CloudProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AwsIotCore => "aws_iot_core",
            Self::AzureIotHub => "azure_iot_hub",
            Self::GooglePubSub => "google_pubsub",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "aws_iot_core" => Some(Self::AwsIotCore),
            "azure_iot_hub" => Some(Self::AzureIotHub),
            "google_pubsub" => Some(Self::GooglePubSub),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::AwsIotCore => "AWS IoT Core",
            Self::AzureIotHub => "Azure IoT Hub",
            Self::GooglePubSub => "Google Cloud Pub/Sub",
        }
    }

    pub fn all() -> &'static [CloudProvider] {
        &[Self::AwsIotCore, Self::AzureIotHub, Self::GooglePubSub]
    }
}

// ----------------------------------------------------------------
// Provider configs (JSON-serialized in store)
// ----------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AwsIotCoreConfig {
    pub endpoint: String,
    #[serde(default = "default_aws_client_id")]
    pub client_id: String,
    pub thing_name: String,
    pub cert_pem_path: String,
    pub key_pem_path: String,
    #[serde(default)]
    pub root_ca_path: Option<String>,
    #[serde(default = "default_topic_prefix")]
    pub topic_prefix: String,
    #[serde(default)]
    pub use_shadow: bool,
}

fn default_aws_client_id() -> String {
    "opencrate-bms".to_string()
}

fn default_topic_prefix() -> String {
    "opencrate".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AzureIotHubConfig {
    pub hostname: String,
    pub device_id: String,
    pub auth_method: AzureAuthMethod,
    #[serde(default = "default_topic_prefix")]
    pub topic_prefix: String,
    #[serde(default)]
    pub report_twin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AzureAuthMethod {
    Sas { key: String },
    X509 { cert_path: String, key_path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GooglePubSubConfig {
    pub project_id: String,
    pub topic_id: String,
    pub credentials_json_path: String,
    #[serde(default)]
    pub ordering_key_prefix: Option<String>,
}

// ----------------------------------------------------------------
// Cloud message (internal event representation)
// ----------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CloudMessage {
    /// Topic suffix — provider connector resolves the full topic path.
    pub topic_suffix: String,
    /// JSON payload.
    pub payload: String,
    /// Event category.
    pub event_type: CloudEventType,
    pub timestamp_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudEventType {
    Telemetry,
    Alarm,
    DeviceStatus,
    FddFault,
}

// ----------------------------------------------------------------
// Bridge config (persisted row)
// ----------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudBridgeConfig {
    pub id: String,
    pub name: String,
    pub provider: String,
    /// JSON-serialized provider config.
    pub config: String,
    pub enabled: bool,
    pub on_values: bool,
    pub on_alarms: bool,
    pub on_fdd: bool,
    pub on_device_status: bool,
    pub created_ms: i64,
    pub updated_ms: i64,
}

// ----------------------------------------------------------------
// Bridge status
// ----------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudBridgeStatus {
    pub bridge_id: String,
    pub last_publish_ms: i64,
    pub messages_published: i64,
    pub last_error: Option<String>,
    /// idle, publishing, error, disconnected
    pub state: String,
}

// ----------------------------------------------------------------
// Error
// ----------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum CloudError {
    #[error("connection error: {0}")]
    Connection(String),
    #[error("publish error: {0}")]
    Publish(String),
    #[error("auth error: {0}")]
    Auth(String),
    #[error("config error: {0}")]
    Config(String),
    #[error("timeout")]
    Timeout,
}

// ----------------------------------------------------------------
// Connector trait
// ----------------------------------------------------------------

#[async_trait::async_trait]
pub trait CloudConnector: Send + Sync {
    /// Establish connection to the cloud platform.
    async fn connect(&mut self) -> Result<(), CloudError>;

    /// Test the connection. Returns Ok(()) or a descriptive error.
    async fn test_connection(&self) -> Result<(), CloudError>;

    /// Publish a batch of messages.
    async fn publish_batch(&self, messages: &[CloudMessage]) -> Result<usize, CloudError>;

    /// Periodic health check — token refresh, reconnect, etc.
    async fn health_check(&mut self) -> Result<(), CloudError>;

    /// Close the connection gracefully.
    async fn close(&self);
}

// ----------------------------------------------------------------
// Factory
// ----------------------------------------------------------------

/// Build a cloud connector from persisted config.
pub fn build_connector(
    provider: &str,
    config_json: &str,
) -> Result<Box<dyn CloudConnector>, CloudError> {
    match provider {
        "aws_iot_core" => {
            let cfg: AwsIotCoreConfig = serde_json::from_str(config_json)
                .map_err(|e| CloudError::Config(format!("invalid AWS IoT config: {e}")))?;
            Ok(Box::new(aws::AwsIotConnector::new(cfg)))
        }
        "azure_iot_hub" => {
            let cfg: AzureIotHubConfig = serde_json::from_str(config_json)
                .map_err(|e| CloudError::Config(format!("invalid Azure IoT config: {e}")))?;
            Ok(Box::new(azure::AzureIotHubConnector::new(cfg)))
        }
        "google_pubsub" => {
            let cfg: GooglePubSubConfig = serde_json::from_str(config_json)
                .map_err(|e| CloudError::Config(format!("invalid Google Pub/Sub config: {e}")))?;
            Ok(Box::new(gcp::GooglePubSubConnector::new(cfg)))
        }
        _ => Err(CloudError::Config(format!(
            "unknown cloud provider: {provider}"
        ))),
    }
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_roundtrip() {
        assert_eq!(
            CloudProvider::from_str("aws_iot_core"),
            Some(CloudProvider::AwsIotCore)
        );
        assert_eq!(CloudProvider::AwsIotCore.as_str(), "aws_iot_core");
        assert_eq!(CloudProvider::AwsIotCore.label(), "AWS IoT Core");

        assert_eq!(
            CloudProvider::from_str("azure_iot_hub"),
            Some(CloudProvider::AzureIotHub)
        );
        assert_eq!(
            CloudProvider::from_str("google_pubsub"),
            Some(CloudProvider::GooglePubSub)
        );
        assert_eq!(CloudProvider::from_str("unknown"), None);
    }

    #[test]
    fn aws_config_defaults() {
        let json = r#"{"endpoint":"a1b2.iot.us-east-1.amazonaws.com","thing_name":"gw","cert_pem_path":"/c","key_pem_path":"/k"}"#;
        let cfg: AwsIotCoreConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.client_id, "opencrate-bms");
        assert_eq!(cfg.topic_prefix, "opencrate");
        assert!(!cfg.use_shadow);
        assert!(cfg.root_ca_path.is_none());
    }

    #[test]
    fn azure_sas_config() {
        let json = r#"{"hostname":"hub.azure-devices.net","device_id":"gw-1","auth_method":{"type":"sas","key":"base64key=="}}"#;
        let cfg: AzureIotHubConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.hostname, "hub.azure-devices.net");
        assert!(matches!(cfg.auth_method, AzureAuthMethod::Sas { .. }));
    }

    #[test]
    fn azure_x509_config() {
        let json = r#"{"hostname":"hub.azure-devices.net","device_id":"gw-1","auth_method":{"type":"x509","cert_path":"/c","key_path":"/k"}}"#;
        let cfg: AzureIotHubConfig = serde_json::from_str(json).unwrap();
        assert!(matches!(cfg.auth_method, AzureAuthMethod::X509 { .. }));
    }

    #[test]
    fn gcp_config() {
        let json = r#"{"project_id":"my-proj","topic_id":"bms-events","credentials_json_path":"/sa.json"}"#;
        let cfg: GooglePubSubConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.project_id, "my-proj");
        assert!(cfg.ordering_key_prefix.is_none());
    }
}
