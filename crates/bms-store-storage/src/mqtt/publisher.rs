use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio::time::{interval, MissedTickBehavior};

use crate::config::profile::PointValue;
use crate::event::bus::{Event, EventBus};
use crate::event::durable_sub::DurableSubscription;
use crate::event::journal::EventJournal;
use crate::store::mqtt_store::{MqttEventType, MqttStore, MqttTopicPattern};

use super::client::{qos_from_u8, ConnectionStatus, MqttConnection};
use super::topic;

struct ActiveBroker {
    client: rumqttc::AsyncClient,
    topics: Vec<MqttTopicPattern>,
    #[allow(dead_code)]
    status: ConnectionStatus,
    event_loop_handle: JoinHandle<()>,
    /// Hash of connection-relevant config fields to detect changes requiring reconnect.
    config_hash: u64,
}

/// Hash the connection-relevant fields of a broker config.
fn broker_config_hash(c: &crate::store::mqtt_store::MqttBrokerConfig) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    c.host.hash(&mut h);
    c.port.hash(&mut h);
    c.client_id.hash(&mut h);
    c.username.hash(&mut h);
    c.password.hash(&mut h);
    c.use_tls.hash(&mut h);
    c.clean_session.hash(&mut h);
    c.keep_alive_secs.hash(&mut h);
    h.finish()
}

/// EventBus subscriber that publishes building events to configured MQTT brokers.
pub struct MqttPublisher {
    mqtt_store: MqttStore,
}

impl MqttPublisher {
    pub fn new(mqtt_store: MqttStore) -> Self {
        Self { mqtt_store }
    }

    /// Start the MQTT publisher as a background tokio task.
    ///
    /// When `journal` is provided, uses a [`DurableSubscription`] to replay
    /// missed events from the journal on startup.
    pub fn start(
        self,
        event_bus: &EventBus,
        shutdown: Option<tokio_util::sync::CancellationToken>,
        journal: Option<&EventJournal>,
    ) {
        let live_rx = event_bus.subscribe();
        let mut config_rx = self.mqtt_store.subscribe();
        let journal_clone = journal.cloned();

        tokio::spawn(async move {
            let connections: Arc<RwLock<HashMap<i64, ActiveBroker>>> =
                Arc::new(RwLock::new(HashMap::new()));

            // Initial config load
            reload_config(&self.mqtt_store, &connections).await;

            let mut health_ticker = interval(std::time::Duration::from_secs(30));
            health_ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

            if let Some(ref j) = journal_clone {
                let mut durable = DurableSubscription::new("mqtt-publisher", j, live_rx).await;
                loop {
                    tokio::select! {
                        result = durable.recv() => {
                            match result {
                                Ok(event) => handle_event(&event, &connections).await,
                                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                                _ => continue,
                            }
                        }
                        _ = config_rx.changed() => {
                            tracing::info!("MQTT config changed, reloading");
                            reload_config(&self.mqtt_store, &connections).await;
                        }
                        _ = health_ticker.tick() => {
                            let conns = connections.read().await;
                            if !conns.is_empty() {
                                let connected = conns.values()
                                    .filter(|b| b.status == ConnectionStatus::Connected)
                                    .count();
                                let total = conns.len();
                                tracing::debug!(connected, total, "MQTT health check");
                            }
                            durable.commit_latest();
                        }
                        _ = async { match &shutdown { Some(t) => t.cancelled().await, None => std::future::pending().await } } => {
                            tracing::info!("MQTT publisher shutting down");
                            durable.commit_latest();
                            break;
                        }
                    }
                }
            } else {
                let mut event_rx = live_rx;
                loop {
                    tokio::select! {
                        result = event_rx.recv() => {
                            match result {
                                Ok(event) => handle_event(&event, &connections).await,
                                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                    tracing::warn!(skipped = n, "MQTT publisher lagged, skipping events");
                                }
                                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                    tracing::info!("EventBus closed, MQTT publisher stopping");
                                    break;
                                }
                            }
                        }
                        _ = config_rx.changed() => {
                            tracing::info!("MQTT config changed, reloading");
                            reload_config(&self.mqtt_store, &connections).await;
                        }
                        _ = health_ticker.tick() => {
                            let conns = connections.read().await;
                            if !conns.is_empty() {
                                let connected = conns.values()
                                    .filter(|b| b.status == ConnectionStatus::Connected)
                                    .count();
                                let total = conns.len();
                                tracing::debug!(connected, total, "MQTT health check");
                            }
                        }
                        _ = async { match &shutdown { Some(t) => t.cancelled().await, None => std::future::pending().await } } => {
                            tracing::info!("MQTT publisher shutting down");
                            break;
                        }
                    }
                }
            }
        });
    }
}

async fn reload_config(store: &MqttStore, connections: &Arc<RwLock<HashMap<i64, ActiveBroker>>>) {
    let configs = store.list_all_enabled_brokers().await;
    let mut conns = connections.write().await;

    // Find broker IDs to remove
    let active_ids: Vec<i64> = conns.keys().copied().collect();
    let new_ids: Vec<i64> = configs.iter().map(|(b, _)| b.id).collect();

    // Disconnect removed brokers
    for id in &active_ids {
        if !new_ids.contains(id) {
            if let Some(broker) = conns.remove(id) {
                broker.event_loop_handle.abort();
                let _ = broker.client.disconnect().await;
                tracing::info!(broker_id = id, "Disconnected removed MQTT broker");
            }
        }
    }

    // Connect new/changed brokers, update topics for unchanged ones
    for (config, topics) in configs {
        let new_hash = broker_config_hash(&config);

        if let Some(broker) = conns.get_mut(&config.id) {
            if broker.config_hash == new_hash {
                // Connection settings unchanged — just update topics
                broker.topics = topics;
                continue;
            }
            // Connection settings changed — disconnect old, will reconnect below
            broker.event_loop_handle.abort();
            let _ = broker.client.disconnect().await;
            conns.remove(&config.id);
            tracing::info!(broker_id = config.id, broker = %config.name, "Reconnecting MQTT broker (config changed)");
        }

        // New or changed broker — connect
        match MqttConnection::connect(&config) {
            Ok((conn, mut eventloop)) => {
                let client = conn.client.clone();
                let broker_name = conn.broker_name.clone();

                // Spawn event loop poller — keeps the connection alive
                let handle = tokio::spawn(async move {
                    loop {
                        match eventloop.poll().await {
                            Ok(_) => {}
                            Err(e) => {
                                tracing::warn!(
                                    broker = %broker_name,
                                    "MQTT event loop error: {e}"
                                );
                                // rumqttc auto-reconnects on next poll
                                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                            }
                        }
                    }
                });

                tracing::info!(
                    broker_id = config.id,
                    broker = %config.name,
                    host = %config.host,
                    port = config.port,
                    "Connected to MQTT broker"
                );

                conns.insert(
                    config.id,
                    ActiveBroker {
                        client,
                        topics,
                        status: ConnectionStatus::Connecting,
                        event_loop_handle: handle,
                        config_hash: new_hash,
                    },
                );
            }
            Err(e) => {
                tracing::error!(
                    broker = %config.name,
                    "Failed to connect to MQTT broker: {e}"
                );
            }
        }
    }
}

async fn handle_event(event: &Event, connections: &Arc<RwLock<HashMap<i64, ActiveBroker>>>) {
    let conns = connections.read().await;
    if conns.is_empty() {
        return;
    }

    let (event_type, payload, ctx) = match event {
        Event::ValueChanged {
            node_id,
            value,
            timestamp_ms,
        } => {
            let ctx = topic::context_from_value(node_id);
            let payload = serialize_value(node_id, value, *timestamp_ms);
            (MqttEventType::Value, payload, ctx)
        }
        Event::DeviceDown {
            bridge_type,
            device_key,
        } => {
            let ctx = topic::context_from_status(device_key, bridge_type);
            let payload = serialize_status(device_key, "down", bridge_type, now_ms());
            (MqttEventType::Status, payload, ctx)
        }
        Event::DeviceDiscovered {
            bridge_type,
            device_key,
        } => {
            let ctx = topic::context_from_status(device_key, bridge_type);
            let payload = serialize_status(device_key, "discovered", bridge_type, now_ms());
            (MqttEventType::Status, payload, ctx)
        }
        Event::DeviceRecovered {
            bridge_type,
            device_key,
        } => {
            let ctx = topic::context_from_status(device_key, bridge_type);
            let payload = serialize_status(device_key, "recovered", bridge_type, now_ms());
            (MqttEventType::Status, payload, ctx)
        }
        Event::FddFaultRaised {
            fault_id,
            rule_id,
            equip_id,
            severity,
        } => {
            let ctx = topic::context_from_alarm(equip_id, severity);
            let payload = serde_json::json!({
                "event": "fdd_fault_raised",
                "fault_id": fault_id,
                "rule_id": rule_id,
                "equip_id": equip_id,
                "severity": severity,
                "timestamp_ms": now_ms(),
            })
            .to_string();
            (MqttEventType::Alarm, payload, ctx)
        }
        Event::FddFaultCleared {
            fault_id,
            rule_id,
            equip_id,
        } => {
            let ctx = topic::context_from_alarm(equip_id, "");
            let payload = serde_json::json!({
                "event": "fdd_fault_cleared",
                "fault_id": fault_id,
                "rule_id": rule_id,
                "equip_id": equip_id,
                "timestamp_ms": now_ms(),
            })
            .to_string();
            (MqttEventType::Alarm, payload, ctx)
        }
        // Other events not published to MQTT
        _ => return,
    };

    for broker in conns.values() {
        for tp in &broker.topics {
            if !tp.enabled || tp.event_type != event_type {
                continue;
            }
            if !topic::matches_node_filter(ctx.node_id, &tp.node_filter) {
                continue;
            }
            let resolved = topic::resolve_topic(&tp.pattern, &ctx);
            let qos = qos_from_u8(tp.qos);
            if let Err(e) = broker
                .client
                .publish(&resolved, qos, tp.retain, payload.as_bytes())
                .await
            {
                tracing::warn!(topic = %resolved, "MQTT publish failed: {e}");
            }
        }
    }
}

// ----------------------------------------------------------------
// Payload serialization
// ----------------------------------------------------------------

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn serialize_value(node_id: &str, value: &PointValue, timestamp_ms: i64) -> String {
    let (val, vtype) = match value {
        PointValue::Bool(b) => (serde_json::json!(*b), "bool"),
        PointValue::Integer(i) => (serde_json::json!(*i), "integer"),
        PointValue::Float(f) => (serde_json::json!(*f), "float"),
    };
    serde_json::json!({
        "node_id": node_id,
        "value": val,
        "value_type": vtype,
        "timestamp_ms": timestamp_ms,
    })
    .to_string()
}

fn serialize_status(device_key: &str, event: &str, protocol: &str, timestamp_ms: i64) -> String {
    serde_json::json!({
        "device_key": device_key,
        "event": event,
        "protocol": protocol,
        "timestamp_ms": timestamp_ms,
    })
    .to_string()
}
