use std::time::Duration;

use rumqttc::{AsyncClient, EventLoop, MqttOptions, QoS, Transport};

use crate::store::mqtt_store::MqttBrokerConfig;

#[derive(Debug, thiserror::Error)]
pub enum MqttError {
    #[error("connection error: {0}")]
    Connection(String),
    #[error("publish error: {0}")]
    Publish(String),
    #[error("config error: {0}")]
    Config(String),
}

/// Active MQTT connection to a broker.
pub struct MqttConnection {
    pub client: AsyncClient,
    pub broker_id: i64,
    pub broker_name: String,
}

/// Connection status reported to the GUI.
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Connecting,
    Error(String),
}

impl ConnectionStatus {
    pub fn label(&self) -> &str {
        match self {
            Self::Connected => "Connected",
            Self::Disconnected => "Disconnected",
            Self::Connecting => "Connecting",
            Self::Error(_) => "Error",
        }
    }
}

/// Convert QoS level (0, 1, 2) to rumqttc QoS enum.
pub fn qos_from_u8(qos: u8) -> QoS {
    match qos {
        0 => QoS::AtMostOnce,
        1 => QoS::AtLeastOnce,
        2 => QoS::ExactlyOnce,
        _ => QoS::AtMostOnce,
    }
}

impl MqttConnection {
    /// Create a new MQTT connection from broker config.
    /// Returns the connection handle and the event loop (caller must poll it).
    pub fn connect(config: &MqttBrokerConfig) -> Result<(Self, EventLoop), MqttError> {
        let client_id = if config.client_id.is_empty() {
            format!("opencrate-{}", &uuid::Uuid::new_v4().to_string()[..8])
        } else {
            config.client_id.clone()
        };

        let mut options = MqttOptions::new(&client_id, &config.host, config.port);
        options.set_keep_alive(Duration::from_secs(config.keep_alive_secs as u64));
        options.set_clean_session(config.clean_session);

        if !config.username.is_empty() {
            options.set_credentials(&config.username, &config.password);
        }

        if config.use_tls {
            options.set_transport(Transport::tls_with_default_config());
        }

        let (client, eventloop) = AsyncClient::new(options, 256);

        Ok((
            Self {
                client,
                broker_id: config.id,
                broker_name: config.name.clone(),
            },
            eventloop,
        ))
    }
}
