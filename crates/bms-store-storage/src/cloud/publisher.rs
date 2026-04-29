use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::Mutex;
use tokio::time::{interval, MissedTickBehavior};

use crate::event::bus::{Event, EventBus};
use crate::event::durable_sub::DurableSubscription;
use crate::event::journal::EventJournal;
use crate::store::cloud_store::CloudStore;

use super::{build_connector, CloudBridgeConfig, CloudConnector, CloudEventType, CloudMessage};

/// Default flush interval in seconds (faster than export — cloud prefers frequent small publishes).
const DEFAULT_FLUSH_INTERVAL_SECS: u64 = 5;
/// Default maximum buffer size before forced flush.
const DEFAULT_MAX_BUFFER_SIZE: usize = 100;
/// Health check interval in seconds (token refresh, reconnect, etc.).
const HEALTH_CHECK_INTERVAL_SECS: u64 = 30;

struct ActiveBridge {
    id: String,
    connector: Box<dyn CloudConnector>,
    on_values: bool,
    on_alarms: bool,
    on_fdd: bool,
    on_device_status: bool,
    /// Consecutive error count for backoff.
    error_count: u32,
    /// When to retry after errors (None = can publish immediately).
    retry_after_ms: Option<i64>,
    /// Hash of connection config to detect changes requiring reconnect.
    config_hash: u64,
}

/// Hash the connection-relevant fields of a bridge config (provider + config JSON).
fn bridge_config_hash(c: &CloudBridgeConfig) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    c.provider.hash(&mut h);
    c.config.hash(&mut h);
    h.finish()
}

/// EventBus subscriber that buffers and publishes building events to configured cloud platforms.
pub struct CloudPublisher {
    cloud_store: CloudStore,
}

impl CloudPublisher {
    pub fn new(cloud_store: CloudStore) -> Self {
        Self { cloud_store }
    }

    /// Start the cloud publisher as a background tokio task.
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
        let mut config_rx = self.cloud_store.subscribe();
        let journal_clone = journal.cloned();

        tokio::spawn(async move {
            let connectors: Arc<Mutex<HashMap<String, ActiveBridge>>> =
                Arc::new(Mutex::new(HashMap::new()));

            let mut buffer: Vec<CloudMessage> = Vec::new();

            // Initial config load
            reload_connectors(&self.cloud_store, &connectors).await;

            let mut flush_ticker = interval(Duration::from_secs(DEFAULT_FLUSH_INTERVAL_SECS));
            flush_ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

            let mut health_ticker = interval(Duration::from_secs(HEALTH_CHECK_INTERVAL_SECS));
            health_ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

            if let Some(ref j) = journal_clone {
                let mut durable = DurableSubscription::new("cloud-publisher", j, live_rx).await;
                loop {
                    tokio::select! {
                        result = durable.recv() => {
                            match result {
                                Ok(event) => {
                                    buffer_event(&event, &mut buffer);
                                    if buffer.len() >= DEFAULT_MAX_BUFFER_SIZE {
                                        flush_buffers(
                                            &mut buffer, &connectors, &self.cloud_store,
                                        ).await;
                                    }
                                }
                                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                                _ => continue,
                            }
                        }
                        _ = config_rx.changed() => {
                            tracing::info!("Cloud config changed, reloading connectors");
                            reload_connectors(&self.cloud_store, &connectors).await;
                        }
                        _ = flush_ticker.tick() => {
                            if !buffer.is_empty() {
                                flush_buffers(
                                    &mut buffer, &connectors, &self.cloud_store,
                                ).await;
                            }
                            durable.commit_latest();
                        }
                        _ = health_ticker.tick() => {
                            run_health_checks(&connectors).await;
                        }
                        _ = async { match &shutdown { Some(t) => t.cancelled().await, None => std::future::pending().await } } => {
                            tracing::info!("Cloud publisher shutting down");
                            flush_buffers(
                                &mut buffer, &connectors, &self.cloud_store,
                            ).await;
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
                                Ok(event) => {
                                    buffer_event(&event, &mut buffer);
                                    if buffer.len() >= DEFAULT_MAX_BUFFER_SIZE {
                                        flush_buffers(
                                            &mut buffer, &connectors, &self.cloud_store,
                                        ).await;
                                    }
                                }
                                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                    tracing::warn!(skipped = n, "Cloud publisher lagged, skipping events");
                                }
                                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                    tracing::info!("EventBus closed, cloud publisher stopping");
                                    break;
                                }
                            }
                        }
                        _ = config_rx.changed() => {
                            tracing::info!("Cloud config changed, reloading connectors");
                            reload_connectors(&self.cloud_store, &connectors).await;
                        }
                        _ = flush_ticker.tick() => {
                            if !buffer.is_empty() {
                                flush_buffers(
                                    &mut buffer, &connectors, &self.cloud_store,
                                ).await;
                            }
                        }
                        _ = health_ticker.tick() => {
                            run_health_checks(&connectors).await;
                        }
                        _ = async { match &shutdown { Some(t) => t.cancelled().await, None => std::future::pending().await } } => {
                            tracing::info!("Cloud publisher shutting down");
                            flush_buffers(
                                &mut buffer, &connectors, &self.cloud_store,
                            ).await;
                            break;
                        }
                    }
                }
            }
        });
    }
}

/// Buffer relevant events into the cloud message buffer.
fn buffer_event(event: &Event, buffer: &mut Vec<CloudMessage>) {
    let now = now_ms();

    match event {
        Event::ValueChanged {
            node_id,
            value,
            timestamp_ms,
        } => {
            let (device_id, point_id) = split_node_id(node_id);
            let payload = serde_json::json!({
                "node_id": node_id,
                "device_id": device_id,
                "point_id": point_id,
                "value": value.as_f64(),
                "timestamp_ms": timestamp_ms,
            })
            .to_string();
            buffer.push(CloudMessage {
                topic_suffix: format!("telemetry/{device_id}/{point_id}"),
                payload,
                event_type: CloudEventType::Telemetry,
                timestamp_ms: *timestamp_ms,
            });
        }
        Event::AlarmRaised { alarm_id, node_id } => {
            let payload = serde_json::json!({
                "event": "alarm_raised",
                "alarm_id": alarm_id,
                "node_id": node_id,
                "timestamp_ms": now,
            })
            .to_string();
            buffer.push(CloudMessage {
                topic_suffix: format!("alarms/{node_id}"),
                payload,
                event_type: CloudEventType::Alarm,
                timestamp_ms: now,
            });
        }
        Event::AlarmCleared { alarm_id, node_id } => {
            let payload = serde_json::json!({
                "event": "alarm_cleared",
                "alarm_id": alarm_id,
                "node_id": node_id,
                "timestamp_ms": now,
            })
            .to_string();
            buffer.push(CloudMessage {
                topic_suffix: format!("alarms/{node_id}"),
                payload,
                event_type: CloudEventType::Alarm,
                timestamp_ms: now,
            });
        }
        Event::FddFaultRaised {
            fault_id,
            rule_id,
            equip_id,
            severity,
        } => {
            let payload = serde_json::json!({
                "event": "fdd_fault_raised",
                "fault_id": fault_id,
                "rule_id": rule_id,
                "equip_id": equip_id,
                "severity": severity,
                "timestamp_ms": now,
            })
            .to_string();
            buffer.push(CloudMessage {
                topic_suffix: format!("fdd/{equip_id}"),
                payload,
                event_type: CloudEventType::FddFault,
                timestamp_ms: now,
            });
        }
        Event::FddFaultCleared {
            fault_id,
            rule_id,
            equip_id,
        } => {
            let payload = serde_json::json!({
                "event": "fdd_fault_cleared",
                "fault_id": fault_id,
                "rule_id": rule_id,
                "equip_id": equip_id,
                "timestamp_ms": now,
            })
            .to_string();
            buffer.push(CloudMessage {
                topic_suffix: format!("fdd/{equip_id}"),
                payload,
                event_type: CloudEventType::FddFault,
                timestamp_ms: now,
            });
        }
        Event::DeviceDown {
            bridge_type,
            device_key,
        } => {
            let payload = serde_json::json!({
                "event": "device_down",
                "device_key": device_key,
                "protocol": bridge_type,
                "timestamp_ms": now,
            })
            .to_string();
            buffer.push(CloudMessage {
                topic_suffix: format!("status/{device_key}"),
                payload,
                event_type: CloudEventType::DeviceStatus,
                timestamp_ms: now,
            });
        }
        Event::DeviceRecovered {
            bridge_type,
            device_key,
        } => {
            let payload = serde_json::json!({
                "event": "device_recovered",
                "device_key": device_key,
                "protocol": bridge_type,
                "timestamp_ms": now,
            })
            .to_string();
            buffer.push(CloudMessage {
                topic_suffix: format!("status/{device_key}"),
                payload,
                event_type: CloudEventType::DeviceStatus,
                timestamp_ms: now,
            });
        }
        // Other events not published to cloud
        _ => {}
    }
}

/// Flush buffered messages to all active cloud connectors.
///
/// Buffers are only cleared after ALL connectors have had a chance to process them.
/// Connectors in backoff skip the flush but the data remains for the next tick.
async fn flush_buffers(
    buffer: &mut Vec<CloudMessage>,
    connectors: &Arc<Mutex<HashMap<String, ActiveBridge>>>,
    store: &CloudStore,
) {
    let now = now_ms();
    let mut conns = connectors.lock().await;

    // Track whether any connector is still in backoff and couldn't process
    let mut any_skipped = false;

    for ac in conns.values_mut() {
        // Check backoff
        if let Some(retry_after) = ac.retry_after_ms {
            if now < retry_after {
                if has_relevant_messages(buffer, ac) {
                    any_skipped = true;
                }
                continue;
            }
            // Backoff period expired, allow retry
            ac.retry_after_ms = None;
        }

        // Filter messages by connector's event toggles
        let filtered: Vec<&CloudMessage> = buffer
            .iter()
            .filter(|m| match m.event_type {
                CloudEventType::Telemetry => ac.on_values,
                CloudEventType::Alarm => ac.on_alarms,
                CloudEventType::FddFault => ac.on_fdd,
                CloudEventType::DeviceStatus => ac.on_device_status,
            })
            .collect();

        if filtered.is_empty() {
            continue;
        }

        // Collect into owned slice for the trait method
        let batch: Vec<CloudMessage> = filtered.into_iter().cloned().collect();

        match ac.connector.publish_batch(&batch).await {
            Ok(n) => {
                ac.error_count = 0;
                let _ = store
                    .update_status(&ac.id, now, n as i64, None, "idle")
                    .await;
            }
            Err(e) => {
                tracing::warn!(connector = %ac.id, error = %e, "Cloud publish failed");
                any_skipped = true;
                apply_backoff(ac);
                let _ = store
                    .update_status(&ac.id, now, 0, Some(&e.to_string()), "error")
                    .await;
            }
        }
    }

    // Only clear buffer if no connector was skipped due to backoff.
    // If a connector is in backoff, keep data so it can be retried on next tick.
    // Cap retained buffer to prevent unbounded growth.
    if !any_skipped {
        buffer.clear();
    } else {
        const MAX_RETAINED: usize = DEFAULT_MAX_BUFFER_SIZE * 10;
        if buffer.len() > MAX_RETAINED {
            let drain_count = buffer.len() - MAX_RETAINED;
            tracing::warn!(
                dropped = drain_count,
                "Cloud message buffer overflow, dropping oldest"
            );
            buffer.drain(..drain_count);
        }
    }
}

/// Check whether the buffer contains messages relevant to a connector's event toggles.
fn has_relevant_messages(buffer: &[CloudMessage], ac: &ActiveBridge) -> bool {
    buffer.iter().any(|m| match m.event_type {
        CloudEventType::Telemetry => ac.on_values,
        CloudEventType::Alarm => ac.on_alarms,
        CloudEventType::FddFault => ac.on_fdd,
        CloudEventType::DeviceStatus => ac.on_device_status,
    })
}

/// Apply exponential backoff after a publish failure.
fn apply_backoff(ac: &mut ActiveBridge) {
    ac.error_count += 1;
    let delay_secs = std::cmp::min(2u64.pow(ac.error_count), 300);
    ac.retry_after_ms = Some(now_ms() + (delay_secs * 1000) as i64);
    tracing::debug!(
        connector = %ac.id,
        error_count = ac.error_count,
        delay_secs,
        "Cloud connector backoff"
    );
}

/// Run periodic health checks on all active connectors (token refresh, reconnect, etc.).
async fn run_health_checks(connectors: &Arc<Mutex<HashMap<String, ActiveBridge>>>) {
    let mut conns = connectors.lock().await;
    for ac in conns.values_mut() {
        if let Err(e) = ac.connector.health_check().await {
            tracing::warn!(connector = %ac.id, error = %e, "Cloud health check failed");
        }
    }
}

/// Reload connector map from the store configuration.
/// Detects config changes (provider/credentials etc.) and rebuilds the connector.
async fn reload_connectors(
    store: &CloudStore,
    connectors: &Arc<Mutex<HashMap<String, ActiveBridge>>>,
) {
    let configs = store.list_enabled_bridges().await;
    let mut conns = connectors.lock().await;

    // Remove connectors no longer in config
    let active_ids: Vec<String> = conns.keys().cloned().collect();
    let new_ids: Vec<&str> = configs.iter().map(|c| c.id.as_str()).collect();

    for id in &active_ids {
        if !new_ids.contains(&id.as_str()) {
            if let Some(ac) = conns.remove(id) {
                ac.connector.close().await;
                tracing::info!(connector = %id, "Removed cloud connector");
            }
        }
    }

    // Add/update connectors
    for config in &configs {
        let new_hash = bridge_config_hash(config);

        if let Some(existing) = conns.get_mut(&config.id) {
            // Always update event toggles
            existing.on_values = config.on_values;
            existing.on_alarms = config.on_alarms;
            existing.on_fdd = config.on_fdd;
            existing.on_device_status = config.on_device_status;

            if existing.config_hash == new_hash {
                // Connection config unchanged — no rebuild needed
                continue;
            }

            // Connection config changed — close old connector and rebuild below
            existing.connector.close().await;
            conns.remove(&config.id);
            tracing::info!(
                connector = %config.id,
                name = %config.name,
                "Reconnecting cloud connector (config changed)"
            );
        }

        match build_connector(&config.provider, &config.config) {
            Ok(mut connector) => {
                // Establish connection before inserting
                if let Err(e) = connector.connect().await {
                    tracing::error!(
                        connector = %config.id,
                        provider = %config.provider,
                        error = %e,
                        "Failed to connect cloud connector"
                    );
                    let _ = store
                        .update_status(
                            &config.id,
                            now_ms(),
                            0,
                            Some(&e.to_string()),
                            "disconnected",
                        )
                        .await;
                    continue;
                }

                tracing::info!(
                    connector = %config.id,
                    name = %config.name,
                    provider = %config.provider,
                    "Started cloud connector"
                );

                conns.insert(
                    config.id.clone(),
                    ActiveBridge {
                        id: config.id.clone(),
                        connector,
                        on_values: config.on_values,
                        on_alarms: config.on_alarms,
                        on_fdd: config.on_fdd,
                        on_device_status: config.on_device_status,
                        error_count: 0,
                        retry_after_ms: None,
                        config_hash: new_hash,
                    },
                );
            }
            Err(e) => {
                tracing::error!(
                    connector = %config.id,
                    provider = %config.provider,
                    error = %e,
                    "Failed to build cloud connector"
                );
                let _ = store
                    .update_status(&config.id, now_ms(), 0, Some(&e.to_string()), "error")
                    .await;
            }
        }
    }
}

/// Split "device_id/point_id" into (device_id, point_id).
fn split_node_id(node_id: &str) -> (String, String) {
    match node_id.split_once('/') {
        Some((dev, pt)) => (dev.to_string(), pt.to_string()),
        None => (node_id.to_string(), String::new()),
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
