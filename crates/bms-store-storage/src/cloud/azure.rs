use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rumqttc::{AsyncClient, EventLoop, MqttOptions, QoS, Transport};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use super::{AzureAuthMethod, AzureIotHubConfig, CloudConnector, CloudError, CloudMessage};

/// Default SAS token lifetime in seconds (1 hour).
const SAS_TOKEN_LIFETIME_SECS: u64 = 3600;

/// Refresh the SAS token when this many seconds remain before expiry.
const SAS_TOKEN_REFRESH_BEFORE_SECS: u64 = 900; // 15 minutes

/// Azure IoT Hub API version used in the MQTT username.
const API_VERSION: &str = "2021-04-12";

/// Azure IoT Hub MQTT port (TLS).
const AZURE_MQTT_PORT: u16 = 8883;

/// Internal state shared behind `Arc<Mutex<>>` for async access.
struct ConnectorState {
    client: Option<AsyncClient>,
    event_loop_handle: Option<JoinHandle<()>>,
    /// Epoch seconds when the current SAS token expires (only relevant for SAS auth).
    sas_token_expiry: u64,
}

/// Azure IoT Hub cloud connector.
///
/// Connects to Azure IoT Hub over MQTT 3.1.1 with TLS on port 8883.
/// Supports both SAS token and X.509 certificate authentication.
pub struct AzureIotHubConnector {
    config: AzureIotHubConfig,
    state: Arc<Mutex<ConnectorState>>,
}

impl AzureIotHubConnector {
    pub fn new(config: AzureIotHubConfig) -> Self {
        Self {
            config,
            state: Arc::new(Mutex::new(ConnectorState {
                client: None,
                event_loop_handle: None,
                sas_token_expiry: 0,
            })),
        }
    }

    /// Build MQTT options and connect to Azure IoT Hub.
    async fn establish_connection(&self) -> Result<(AsyncClient, EventLoop, u64), CloudError> {
        let client_id = &self.config.device_id;
        let hostname = &self.config.hostname;

        let mut options = MqttOptions::new(client_id, hostname, AZURE_MQTT_PORT);
        options.set_keep_alive(Duration::from_secs(60));
        options.set_clean_session(true);

        let sas_expiry = match &self.config.auth_method {
            AzureAuthMethod::Sas { key } => {
                let expiry = current_epoch_secs() + SAS_TOKEN_LIFETIME_SECS;
                let username = format!("{}/{}/?api-version={}", hostname, client_id, API_VERSION,);
                let token = generate_sas_token(hostname, client_id, key, expiry)?;
                options.set_credentials(username, token);
                options.set_transport(Transport::tls_with_default_config());
                expiry
            }
            AzureAuthMethod::X509 {
                cert_path,
                key_path,
            } => {
                let username = format!("{}/{}/?api-version={}", hostname, client_id, API_VERSION,);
                // Azure still requires the username even for X.509 auth.
                options.set_credentials::<String, String>(username, String::new());

                let transport = build_x509_transport(cert_path, key_path)?;
                options.set_transport(transport);
                0 // no SAS expiry for X.509
            }
        };

        let (client, eventloop) = AsyncClient::new(options, 256);

        Ok((client, eventloop, sas_expiry))
    }
}

#[async_trait::async_trait]
impl CloudConnector for AzureIotHubConnector {
    async fn connect(&mut self) -> Result<(), CloudError> {
        let (client, mut eventloop, sas_expiry) = self.establish_connection().await?;

        // Spawn the event loop poller to keep the connection alive.
        let device_id = self.config.device_id.clone();
        let handle = tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(_notification) => {}
                    Err(e) => {
                        tracing::warn!(
                            device_id = %device_id,
                            "Azure IoT Hub event loop error: {e}"
                        );
                        // rumqttc will auto-reconnect on next poll.
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }
        });

        let mut state = self.state.lock().await;
        state.client = Some(client);
        state.event_loop_handle = Some(handle);
        state.sas_token_expiry = sas_expiry;

        tracing::info!(
            hostname = %self.config.hostname,
            device_id = %self.config.device_id,
            "Connected to Azure IoT Hub"
        );

        Ok(())
    }

    async fn test_connection(&self) -> Result<(), CloudError> {
        let state = self.state.lock().await;
        if state.client.is_none() {
            return Err(CloudError::Connection(
                "Azure IoT Hub client not connected".into(),
            ));
        }
        // The event loop task being alive indicates the connection is active.
        if let Some(ref handle) = state.event_loop_handle {
            if handle.is_finished() {
                return Err(CloudError::Connection(
                    "Azure IoT Hub event loop has stopped".into(),
                ));
            }
        }
        Ok(())
    }

    async fn publish_batch(&self, messages: &[CloudMessage]) -> Result<usize, CloudError> {
        let state = self.state.lock().await;
        let client = state
            .client
            .as_ref()
            .ok_or_else(|| CloudError::Connection("Azure IoT Hub client not connected".into()))?;

        let device_id = &self.config.device_id;
        let mut published = 0;

        for msg in messages {
            let event_type_str = match msg.event_type {
                super::CloudEventType::Telemetry => "telemetry",
                super::CloudEventType::Alarm => "alarm",
                super::CloudEventType::DeviceStatus => "device_status",
                super::CloudEventType::FddFault => "fdd_fault",
            };

            // Azure IoT Hub D2C message topic with system properties.
            let topic = format!(
                "devices/{}/messages/events/$.ct=application%2Fjson&$.ce=utf-8&event_type={}",
                device_id, event_type_str,
            );

            if let Err(e) = client
                .publish(&topic, QoS::AtLeastOnce, false, msg.payload.as_bytes())
                .await
            {
                tracing::warn!(
                    topic = %topic,
                    device_id = %device_id,
                    "Azure IoT Hub publish failed: {e}"
                );
                return Err(CloudError::Publish(format!(
                    "failed to publish to Azure IoT Hub: {e}"
                )));
            }
            published += 1;
        }

        // Optionally report device twin properties.
        if self.config.report_twin && !messages.is_empty() {
            let rid = current_epoch_secs();
            let twin_topic = format!("$iothub/twin/PATCH/properties/reported/?$rid={}", rid,);
            let twin_payload = serde_json::json!({
                "lastPublishMs": messages.last().map(|m| m.timestamp_ms).unwrap_or(0),
                "messagesPublished": published,
            });
            if let Err(e) = client
                .publish(
                    &twin_topic,
                    QoS::AtLeastOnce,
                    false,
                    twin_payload.to_string().as_bytes(),
                )
                .await
            {
                tracing::warn!(
                    device_id = %device_id,
                    "Azure IoT Hub twin report failed: {e}"
                );
                // Twin report failure is non-fatal; the batch was already published.
            }
        }

        Ok(published)
    }

    async fn health_check(&mut self) -> Result<(), CloudError> {
        // For X.509 auth, no token refresh is needed.
        let needs_refresh = match &self.config.auth_method {
            AzureAuthMethod::Sas { .. } => {
                let state = self.state.lock().await;
                let now = current_epoch_secs();
                let refresh_at = state
                    .sas_token_expiry
                    .saturating_sub(SAS_TOKEN_REFRESH_BEFORE_SECS);
                now >= refresh_at && state.sas_token_expiry > 0
            }
            AzureAuthMethod::X509 { .. } => false,
        };

        if needs_refresh {
            tracing::info!(
                device_id = %self.config.device_id,
                "SAS token approaching expiry, reconnecting to Azure IoT Hub"
            );

            // Disconnect the current session.
            self.close().await;

            // Reconnect with a fresh SAS token.
            self.connect().await?;
        } else {
            // Verify the event loop is still alive.
            self.test_connection().await?;
        }

        Ok(())
    }

    async fn close(&self) {
        let mut state = self.state.lock().await;

        if let Some(client) = state.client.take() {
            if let Err(e) = client.disconnect().await {
                tracing::debug!(
                    device_id = %self.config.device_id,
                    "Azure IoT Hub disconnect error (ignored): {e}"
                );
            }
        }

        if let Some(handle) = state.event_loop_handle.take() {
            handle.abort();
        }

        state.sas_token_expiry = 0;

        tracing::info!(
            device_id = %self.config.device_id,
            "Disconnected from Azure IoT Hub"
        );
    }
}

// ----------------------------------------------------------------
// SAS token generation
// ----------------------------------------------------------------

/// Generate an Azure IoT Hub SAS token.
///
/// Format: `SharedAccessSignature sr={resource_uri}&sig={sig}&se={expiry}`
///
/// - `resource_uri` = URL-encoded `{hostname}/devices/{device_id}`
/// - `sig` = Base64(HMAC-SHA256(Base64Decode(key), "{resource_uri}\n{expiry}"))
fn generate_sas_token(
    hostname: &str,
    device_id: &str,
    key: &str,
    expiry: u64,
) -> Result<String, CloudError> {
    use base64::engine::general_purpose::STANDARD as BASE64;
    use base64::Engine;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let raw_uri = format!("{}/devices/{}", hostname, device_id);
    let resource_uri = urlencoding::encode(&raw_uri);

    let string_to_sign = format!("{}\n{}", resource_uri, expiry);

    let decoded_key = BASE64
        .decode(key)
        .map_err(|e| CloudError::Auth(format!("invalid SAS key (base64 decode): {e}")))?;

    let mut mac = Hmac::<Sha256>::new_from_slice(&decoded_key)
        .map_err(|e| CloudError::Auth(format!("HMAC key error: {e}")))?;
    mac.update(string_to_sign.as_bytes());
    let raw_sig = BASE64.encode(mac.finalize().into_bytes());
    let encoded_sig = urlencoding::encode(&raw_sig);

    Ok(format!(
        "SharedAccessSignature sr={}&sig={}&se={}",
        resource_uri, encoded_sig, expiry,
    ))
}

// ----------------------------------------------------------------
// X.509 TLS transport
// ----------------------------------------------------------------

/// Build a rumqttc `Transport` using client certificate mutual TLS.
///
/// Loads system root CAs for server verification and the supplied client
/// certificate + private key for mutual TLS authentication.
fn build_x509_transport(cert_path: &str, key_path: &str) -> Result<Transport, CloudError> {
    use rumqttc::tokio_rustls::rustls::{ClientConfig, RootCertStore};
    use std::io::{BufReader, Cursor};

    // --- Root CA store (system trust anchors) ---
    let mut root_store = RootCertStore::empty();
    let native = rustls_native_certs::load_native_certs();
    for cert in native.certs {
        // Ignore individual cert parse failures; some system stores contain
        // certificates in formats rustls doesn't support.
        let _ = root_store.add(cert);
    }
    if root_store.is_empty() {
        return Err(CloudError::Config(
            "no valid system root CA certificates found".into(),
        ));
    }

    // --- Client certificate ---
    let cert_data = std::fs::read(cert_path).map_err(|e| {
        CloudError::Config(format!("failed to read certificate file {cert_path}: {e}"))
    })?;
    let certs: Vec<_> = rustls_pemfile::certs(&mut BufReader::new(Cursor::new(&cert_data)))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| CloudError::Config(format!("failed to parse certificate PEM: {e}")))?;
    if certs.is_empty() {
        return Err(CloudError::Config(format!(
            "no certificates found in {cert_path}"
        )));
    }

    // --- Client private key ---
    let key_data = std::fs::read(key_path).map_err(|e| {
        CloudError::Config(format!("failed to read private key file {key_path}: {e}"))
    })?;
    let key = {
        let mut reader = BufReader::new(Cursor::new(&key_data));
        loop {
            match rustls_pemfile::read_one(&mut reader)
                .map_err(|e| CloudError::Config(format!("failed to parse key PEM: {e}")))?
            {
                Some(rustls_pemfile::Item::Pkcs1Key(k)) => break k.into(),
                Some(rustls_pemfile::Item::Pkcs8Key(k)) => break k.into(),
                Some(rustls_pemfile::Item::Sec1Key(k)) => break k.into(),
                Some(_) => continue,
                None => {
                    return Err(CloudError::Config(format!(
                        "no valid private key found in {key_path}"
                    )));
                }
            }
        }
    };

    let config = ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_client_auth_cert(certs, key)
        .map_err(|e| CloudError::Config(format!("TLS client auth config error: {e}")))?;

    Ok(Transport::tls_with_config(
        rumqttc::TlsConfiguration::Rustls(Arc::new(config)),
    ))
}

// ----------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------

fn current_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sas_token_generation() {
        // Known-good test vector: a deterministic key and expiry.
        let hostname = "myhub.azure-devices.net";
        let device_id = "device-01";
        // A sample base64-encoded key (32 random bytes, base64-encoded).
        let key = "c2VjcmV0a2V5MTIzNDU2Nzg5MGFiY2RlZg==";
        let expiry = 1_700_000_000u64;

        let token = generate_sas_token(hostname, device_id, key, expiry).unwrap();

        assert!(token.starts_with("SharedAccessSignature sr="));
        assert!(token.contains("&sig="));
        assert!(token.contains(&format!("&se={}", expiry)));

        // Verify the resource URI is URL-encoded.
        let expected_uri =
            urlencoding::encode(&format!("{}/devices/{}", hostname, device_id)).to_string();
        assert!(token.contains(&format!("sr={}", expected_uri)));
    }

    #[test]
    fn sas_token_invalid_key() {
        let result = generate_sas_token("hub.azure-devices.net", "dev1", "not-valid-base64!!!", 0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, CloudError::Auth(_)));
    }

    #[test]
    fn sas_token_deterministic() {
        let hostname = "test.azure-devices.net";
        let device_id = "gw-1";
        let key = "dGVzdGtleQ=="; // "testkey" in base64
        let expiry = 1_000_000u64;

        let t1 = generate_sas_token(hostname, device_id, key, expiry).unwrap();
        let t2 = generate_sas_token(hostname, device_id, key, expiry).unwrap();
        assert_eq!(t1, t2);
    }

    #[test]
    fn connector_new() {
        let config = AzureIotHubConfig {
            hostname: "hub.azure-devices.net".into(),
            device_id: "dev-1".into(),
            auth_method: AzureAuthMethod::Sas {
                key: "dGVzdA==".into(),
            },
            topic_prefix: "opencrate".into(),
            report_twin: false,
        };

        let connector = AzureIotHubConnector::new(config);
        assert_eq!(connector.config.hostname, "hub.azure-devices.net");
        assert_eq!(connector.config.device_id, "dev-1");
    }
}
