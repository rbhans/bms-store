use std::sync::Arc;

use rumqttc::{AsyncClient, EventLoop, MqttOptions, QoS, Transport};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use super::{AwsIotCoreConfig, CloudConnector, CloudError, CloudEventType, CloudMessage};

/// AWS IoT Core MQTT port (mutual TLS on 8883).
const AWS_IOT_PORT: u16 = 8883;

/// MQTT event loop channel capacity.
const EVENTLOOP_CAP: usize = 256;

// ----------------------------------------------------------------
// AwsIotConnector
// ----------------------------------------------------------------

/// Cloud connector for AWS IoT Core using MQTT 3.1.1 with X.509 mutual TLS.
pub struct AwsIotConnector {
    config: AwsIotCoreConfig,
    client: Arc<Mutex<Option<AsyncClient>>>,
    event_loop_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl AwsIotConnector {
    pub fn new(config: AwsIotCoreConfig) -> Self {
        Self {
            config,
            client: Arc::new(Mutex::new(None)),
            event_loop_handle: Arc::new(Mutex::new(None)),
        }
    }

    /// Build rumqttc MqttOptions with mutual TLS transport for AWS IoT Core.
    fn build_mqtt_options(&self) -> Result<MqttOptions, CloudError> {
        let mut opts =
            MqttOptions::new(&self.config.client_id, &self.config.endpoint, AWS_IOT_PORT);
        opts.set_keep_alive(std::time::Duration::from_secs(30));
        opts.set_clean_session(true);

        let transport = build_tls_transport(
            &self.config.cert_pem_path,
            &self.config.key_pem_path,
            self.config.root_ca_path.as_deref(),
        )?;
        opts.set_transport(transport);

        Ok(opts)
    }

    /// Resolve the full MQTT topic for a given event type and suffix.
    ///
    /// `CloudPublisher` produces suffixes that already include the event
    /// category ("telemetry/...", "alarms/...", "status/...", "fdd/..."),
    /// because Azure and GCP consume the category as part of the suffix.
    /// AWS builds its topic as `{prefix}/{category}/{tail}`, so we strip the
    /// leading category from the suffix if it's there to avoid producing
    /// `opencrate/telemetry/telemetry/...` topics.
    fn resolve_topic(&self, event_type: &CloudEventType, suffix: &str) -> String {
        let category = match event_type {
            CloudEventType::Telemetry => "telemetry",
            CloudEventType::Alarm => "alarms",
            CloudEventType::DeviceStatus => "status",
            CloudEventType::FddFault => "fdd",
        };
        let tail = suffix
            .strip_prefix(&format!("{category}/"))
            .unwrap_or(suffix);
        format!("{}/{}/{}", self.config.topic_prefix, category, tail)
    }

    /// Build the AWS IoT Device Shadow update payload from a batch of messages.
    fn build_shadow_payload(&self, messages: &[CloudMessage]) -> String {
        let mut reported = serde_json::Map::new();
        for msg in messages {
            let key = match msg.event_type {
                CloudEventType::Telemetry => format!("telemetry_{}", msg.topic_suffix),
                CloudEventType::Alarm => format!("alarm_{}", msg.topic_suffix),
                CloudEventType::DeviceStatus => format!("status_{}", msg.topic_suffix),
                CloudEventType::FddFault => format!("fdd_{}", msg.topic_suffix),
            };
            // Sanitize key for JSON (replace / with _).
            let key = key.replace('/', "_");
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&msg.payload) {
                reported.insert(key, val);
            } else {
                reported.insert(key, serde_json::Value::String(msg.payload.clone()));
            }
        }
        reported.insert(
            "last_update_ms".to_string(),
            serde_json::json!(messages.last().map(|m| m.timestamp_ms).unwrap_or(0)),
        );

        serde_json::json!({
            "state": {
                "reported": reported
            }
        })
        .to_string()
    }
}

#[async_trait::async_trait]
impl CloudConnector for AwsIotConnector {
    async fn connect(&mut self) -> Result<(), CloudError> {
        // Tear down any existing connection first.
        self.close().await;

        let opts = self.build_mqtt_options()?;
        let (client, eventloop) = AsyncClient::new(opts, EVENTLOOP_CAP);

        // Store the client before spawning so publish calls can find it
        // immediately after the event loop starts processing.
        {
            let mut guard = self.client.lock().await;
            *guard = Some(client);
        }

        // Spawn the event loop poller — keeps the connection alive and handles
        // reconnection automatically (rumqttc reconnects on next poll after error).
        let handle = spawn_event_loop(eventloop, self.config.endpoint.clone());
        {
            let mut guard = self.event_loop_handle.lock().await;
            *guard = Some(handle);
        }

        tracing::info!(
            endpoint = %self.config.endpoint,
            client_id = %self.config.client_id,
            thing = %self.config.thing_name,
            "AWS IoT Core connector connected"
        );

        Ok(())
    }

    async fn test_connection(&self) -> Result<(), CloudError> {
        let guard = self.client.lock().await;
        let client = guard
            .as_ref()
            .ok_or_else(|| CloudError::Connection("not connected".into()))?;

        let test_topic = format!("{}/test", self.config.topic_prefix);
        let payload = serde_json::json!({
            "test": true,
            "timestamp_ms": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64,
        })
        .to_string();

        client
            .publish(&test_topic, QoS::AtLeastOnce, false, payload.as_bytes())
            .await
            .map_err(|e| CloudError::Publish(format!("test publish failed: {e}")))?;

        tracing::debug!(topic = %test_topic, "AWS IoT Core test publish succeeded");
        Ok(())
    }

    async fn publish_batch(&self, messages: &[CloudMessage]) -> Result<usize, CloudError> {
        if messages.is_empty() {
            return Ok(0);
        }

        let guard = self.client.lock().await;
        let client = guard
            .as_ref()
            .ok_or_else(|| CloudError::Connection("not connected".into()))?;

        let mut published = 0usize;

        for msg in messages {
            let topic = self.resolve_topic(&msg.event_type, &msg.topic_suffix);
            if let Err(e) = client
                .publish(&topic, QoS::AtLeastOnce, false, msg.payload.as_bytes())
                .await
            {
                tracing::warn!(topic = %topic, "AWS IoT publish failed: {e}");
                // Continue with remaining messages — partial success is acceptable.
                continue;
            }
            published += 1;
        }

        // If shadow reporting is enabled, publish an aggregated state update.
        if self.config.use_shadow && published > 0 {
            let shadow_topic = format!("$aws/things/{}/shadow/update", self.config.thing_name);
            let shadow_payload = self.build_shadow_payload(messages);
            if let Err(e) = client
                .publish(
                    &shadow_topic,
                    QoS::AtLeastOnce,
                    false,
                    shadow_payload.as_bytes(),
                )
                .await
            {
                tracing::warn!(
                    thing = %self.config.thing_name,
                    "AWS IoT shadow update failed: {e}"
                );
            }
        }

        Ok(published)
    }

    async fn health_check(&mut self) -> Result<(), CloudError> {
        // X.509 certificates are long-lived — no token refresh needed.
        // The rumqttc event loop handles reconnection automatically.
        Ok(())
    }

    async fn close(&self) {
        // Abort the event loop task first so it stops polling.
        {
            let mut guard = self.event_loop_handle.lock().await;
            if let Some(handle) = guard.take() {
                handle.abort();
            }
        }

        // Disconnect the client.
        {
            let mut guard = self.client.lock().await;
            if let Some(client) = guard.take() {
                if let Err(e) = client.disconnect().await {
                    tracing::debug!("AWS IoT disconnect error (non-fatal): {e}");
                }
            }
        }

        tracing::info!(
            endpoint = %self.config.endpoint,
            "AWS IoT Core connector closed"
        );
    }
}

// ----------------------------------------------------------------
// TLS setup
// ----------------------------------------------------------------

/// Build a `Transport::Tls` configured for AWS IoT Core mutual TLS authentication.
///
/// Reads the X.509 client certificate and private key from PEM files, and
/// optionally a custom root CA (e.g. Amazon Root CA 1). If no root CA is
/// provided, the operating system's native certificate store is used.
fn build_tls_transport(
    cert_path: &str,
    key_path: &str,
    root_ca_path: Option<&str>,
) -> Result<Transport, CloudError> {
    use rumqttc::tokio_rustls::rustls::ClientConfig;

    // --- Root certificate store ---
    let mut root_store = rumqttc::tokio_rustls::rustls::RootCertStore::empty();

    if let Some(ca_path) = root_ca_path {
        let ca_pem = std::fs::read(ca_path)
            .map_err(|e| CloudError::Config(format!("failed to read root CA {ca_path}: {e}")))?;
        let mut cursor = std::io::Cursor::new(&ca_pem);
        let certs = rustls_pemfile::certs(&mut cursor)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| CloudError::Config(format!("failed to parse root CA PEM: {e}")))?;
        if certs.is_empty() {
            return Err(CloudError::Config(format!(
                "no certificates found in root CA file: {ca_path}"
            )));
        }
        for cert in certs {
            root_store
                .add(cert)
                .map_err(|e| CloudError::Config(format!("failed to add root CA cert: {e}")))?;
        }
    } else {
        // Fall back to OS-native root certificates.
        let native = rustls_native_certs::load_native_certs();
        if native.certs.is_empty() {
            return Err(CloudError::Config(
                "no native root certificates found on this system".into(),
            ));
        }
        for cert in native.certs {
            root_store
                .add(cert)
                .map_err(|e| CloudError::Config(format!("failed to add native cert: {e}")))?;
        }
    }

    // --- Client certificate ---
    let cert_pem = std::fs::read(cert_path)
        .map_err(|e| CloudError::Config(format!("failed to read client cert {cert_path}: {e}")))?;
    let mut cert_cursor = std::io::Cursor::new(&cert_pem);
    let client_certs = rustls_pemfile::certs(&mut cert_cursor)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| CloudError::Config(format!("failed to parse client cert PEM: {e}")))?;
    if client_certs.is_empty() {
        return Err(CloudError::Config(format!(
            "no certificates found in client cert file: {cert_path}"
        )));
    }

    // --- Private key ---
    let key_pem = std::fs::read(key_path)
        .map_err(|e| CloudError::Config(format!("failed to read private key {key_path}: {e}")))?;
    let mut key_cursor = std::io::Cursor::new(&key_pem);
    let private_key = rustls_pemfile::private_key(&mut key_cursor)
        .map_err(|e| CloudError::Config(format!("failed to parse private key PEM: {e}")))?
        .ok_or_else(|| {
            CloudError::Config(format!("no private key found in key file: {key_path}"))
        })?;

    // --- Build rustls ClientConfig with mutual TLS ---
    let tls_config = ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_client_auth_cert(client_certs, private_key)
        .map_err(|e| CloudError::Auth(format!("TLS client auth config error: {e}")))?;

    Ok(Transport::tls_with_config(tls_config.into()))
}

// ----------------------------------------------------------------
// Event loop
// ----------------------------------------------------------------

/// Spawn a tokio task that polls the MQTT event loop forever.
///
/// The event loop handles connection, reconnection, and incoming packets.
/// Errors are logged and retried with a short delay (rumqttc auto-reconnects
/// on the next poll after a transient error).
fn spawn_event_loop(mut eventloop: EventLoop, endpoint: String) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match eventloop.poll().await {
                Ok(rumqttc::Event::Incoming(rumqttc::Incoming::ConnAck(_))) => {
                    tracing::info!(endpoint = %endpoint, "AWS IoT Core MQTT connected");
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(endpoint = %endpoint, "AWS IoT event loop error: {e}");
                    // rumqttc auto-reconnects on next poll; back off briefly to
                    // avoid tight error loops on persistent failures.
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
            }
        }
    })
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> AwsIotCoreConfig {
        AwsIotCoreConfig {
            endpoint: "a1b2c3.iot.us-east-1.amazonaws.com".into(),
            client_id: "opencrate-test".into(),
            thing_name: "gateway-1".into(),
            cert_pem_path: "/tmp/cert.pem".into(),
            key_pem_path: "/tmp/key.pem".into(),
            root_ca_path: None,
            topic_prefix: "opencrate".into(),
            use_shadow: false,
        }
    }

    #[test]
    fn topic_resolution() {
        let connector = AwsIotConnector::new(test_config());

        // Bare suffix (as the unit test previously asserted) still works.
        assert_eq!(
            connector.resolve_topic(&CloudEventType::Telemetry, "ahu-1/zone-temp"),
            "opencrate/telemetry/ahu-1/zone-temp"
        );
        assert_eq!(
            connector.resolve_topic(&CloudEventType::Alarm, "high-temp"),
            "opencrate/alarms/high-temp"
        );
        assert_eq!(
            connector.resolve_topic(&CloudEventType::DeviceStatus, "bacnet/192001"),
            "opencrate/status/bacnet/192001"
        );
        assert_eq!(
            connector.resolve_topic(&CloudEventType::FddFault, "stuck-valve"),
            "opencrate/fdd/stuck-valve"
        );
    }

    #[test]
    fn topic_resolution_strips_duplicate_category_from_publisher_suffix() {
        // CloudPublisher produces suffixes that already include the category
        // ("telemetry/{device}/{point}") so Azure/GCP can carry the event
        // class. AWS must dedupe so we don't emit
        // `opencrate/telemetry/telemetry/...` topics.
        let connector = AwsIotConnector::new(test_config());

        assert_eq!(
            connector.resolve_topic(&CloudEventType::Telemetry, "telemetry/ahu-1/zone-temp"),
            "opencrate/telemetry/ahu-1/zone-temp"
        );
        assert_eq!(
            connector.resolve_topic(&CloudEventType::Alarm, "alarms/ahu-1/high-temp"),
            "opencrate/alarms/ahu-1/high-temp"
        );
        assert_eq!(
            connector.resolve_topic(&CloudEventType::DeviceStatus, "status/bacnet/192001"),
            "opencrate/status/bacnet/192001"
        );
        assert_eq!(
            connector.resolve_topic(&CloudEventType::FddFault, "fdd/ahu-1"),
            "opencrate/fdd/ahu-1"
        );

        // Only strips when the prefix matches the event category — a
        // "telemetry_..." suffix (distinct from "telemetry/...") is left alone.
        assert_eq!(
            connector.resolve_topic(&CloudEventType::Telemetry, "telemetry-raw/foo"),
            "opencrate/telemetry/telemetry-raw/foo"
        );
    }

    #[test]
    fn shadow_payload_structure() {
        let mut config = test_config();
        config.use_shadow = true;
        let connector = AwsIotConnector::new(config);

        let messages = vec![
            CloudMessage {
                topic_suffix: "ahu-1/zone-temp".into(),
                payload: r#"{"value":72.5}"#.into(),
                event_type: CloudEventType::Telemetry,
                timestamp_ms: 1700000000000,
            },
            CloudMessage {
                topic_suffix: "high-temp".into(),
                payload: r#"{"severity":"critical"}"#.into(),
                event_type: CloudEventType::Alarm,
                timestamp_ms: 1700000001000,
            },
        ];

        let payload = connector.build_shadow_payload(&messages);
        let parsed: serde_json::Value = serde_json::from_str(&payload).unwrap();

        let reported = &parsed["state"]["reported"];
        assert!(reported["telemetry_ahu-1_zone-temp"]["value"].is_number());
        assert_eq!(
            reported["alarm_high-temp"]["severity"].as_str().unwrap(),
            "critical"
        );
        assert_eq!(reported["last_update_ms"].as_i64().unwrap(), 1700000001000);
    }

    #[test]
    fn new_connector_starts_disconnected() {
        let connector = AwsIotConnector::new(test_config());
        // Client should be None before connect() is called.
        let client = connector.client.try_lock().unwrap();
        assert!(client.is_none());
    }

    #[test]
    fn tls_transport_rejects_missing_cert() {
        let result = build_tls_transport("/nonexistent/cert.pem", "/nonexistent/key.pem", None);
        assert!(result.is_err());
        let err = match result {
            Err(e) => e.to_string(),
            Ok(_) => panic!("expected error"),
        };
        assert!(err.contains("failed to read client cert"), "got: {err}");
    }
}
