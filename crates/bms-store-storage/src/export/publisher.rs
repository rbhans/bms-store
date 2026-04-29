use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::Mutex;
use tokio::time::{interval, MissedTickBehavior};

use crate::event::bus::{Event, EventBus};
use crate::event::durable_sub::DurableSubscription;
use crate::event::journal::EventJournal;
use crate::export::{
    ExportAlarm, ExportConnector, ExportConnectorConfig, ExportSample, InfluxDbConfig,
};
use crate::store::export_store::ExportStore;

use super::influxdb::InfluxDbConnector;

/// Default flush interval in seconds.
const DEFAULT_FLUSH_INTERVAL_SECS: u64 = 10;
/// Default maximum buffer size before forced flush.
const DEFAULT_MAX_BUFFER_SIZE: usize = 1000;

struct ActiveConnector {
    id: String,
    connector: Box<dyn ExportConnector>,
    on_values: bool,
    on_alarms: bool,
    on_fdd: bool,
    /// Consecutive error count for backoff.
    error_count: u32,
    /// When to retry after errors (None = can write immediately).
    retry_after_ms: Option<i64>,
    /// Hash of connection config to detect changes requiring reconnect.
    config_hash: u64,
}

/// Hash the connection-relevant fields of a connector config (type + config JSON).
fn connector_config_hash(c: &ExportConnectorConfig) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    c.connector_type.hash(&mut h);
    c.config.hash(&mut h);
    h.finish()
}

/// EventBus subscriber that buffers and exports building events to configured external databases.
pub struct ExportPublisher {
    export_store: ExportStore,
}

impl ExportPublisher {
    pub fn new(export_store: ExportStore) -> Self {
        Self { export_store }
    }

    /// Start the export publisher as a background tokio task.
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
        let mut config_rx = self.export_store.subscribe();
        let journal_clone = journal.cloned();

        tokio::spawn(async move {
            let connectors: Arc<Mutex<HashMap<String, ActiveConnector>>> =
                Arc::new(Mutex::new(HashMap::new()));

            let mut sample_buffer: Vec<ExportSample> = Vec::new();
            let mut alarm_buffer: Vec<ExportAlarm> = Vec::new();

            // Initial config load
            reload_connectors(&self.export_store, &connectors).await;

            let mut flush_ticker = interval(Duration::from_secs(DEFAULT_FLUSH_INTERVAL_SECS));
            flush_ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

            if let Some(ref j) = journal_clone {
                let mut durable = DurableSubscription::new("export-publisher", j, live_rx).await;
                loop {
                    tokio::select! {
                        result = durable.recv() => {
                            match result {
                                Ok(event) => {
                                    buffer_event(&event, &mut sample_buffer, &mut alarm_buffer);
                                    if sample_buffer.len() >= DEFAULT_MAX_BUFFER_SIZE
                                        || alarm_buffer.len() >= DEFAULT_MAX_BUFFER_SIZE
                                    {
                                        flush_buffers(
                                            &mut sample_buffer, &mut alarm_buffer,
                                            &connectors, &self.export_store,
                                        ).await;
                                    }
                                }
                                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                                _ => continue,
                            }
                        }
                        _ = config_rx.changed() => {
                            tracing::info!("Export config changed, reloading connectors");
                            reload_connectors(&self.export_store, &connectors).await;
                        }
                        _ = flush_ticker.tick() => {
                            if !sample_buffer.is_empty() || !alarm_buffer.is_empty() {
                                flush_buffers(
                                    &mut sample_buffer, &mut alarm_buffer,
                                    &connectors, &self.export_store,
                                ).await;
                            }
                            durable.commit_latest();
                        }
                        _ = async { match &shutdown { Some(t) => t.cancelled().await, None => std::future::pending().await } } => {
                            tracing::info!("Export publisher shutting down");
                            flush_buffers(
                                &mut sample_buffer, &mut alarm_buffer,
                                &connectors, &self.export_store,
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
                                    buffer_event(&event, &mut sample_buffer, &mut alarm_buffer);
                                    if sample_buffer.len() >= DEFAULT_MAX_BUFFER_SIZE
                                        || alarm_buffer.len() >= DEFAULT_MAX_BUFFER_SIZE
                                    {
                                        flush_buffers(
                                            &mut sample_buffer, &mut alarm_buffer,
                                            &connectors, &self.export_store,
                                        ).await;
                                    }
                                }
                                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                    tracing::warn!(skipped = n, "Export publisher lagged, skipping events");
                                }
                                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                    tracing::info!("EventBus closed, export publisher stopping");
                                    break;
                                }
                            }
                        }
                        _ = config_rx.changed() => {
                            tracing::info!("Export config changed, reloading connectors");
                            reload_connectors(&self.export_store, &connectors).await;
                        }
                        _ = flush_ticker.tick() => {
                            if !sample_buffer.is_empty() || !alarm_buffer.is_empty() {
                                flush_buffers(
                                    &mut sample_buffer, &mut alarm_buffer,
                                    &connectors, &self.export_store,
                                ).await;
                            }
                        }
                        _ = async { match &shutdown { Some(t) => t.cancelled().await, None => std::future::pending().await } } => {
                            tracing::info!("Export publisher shutting down");
                            flush_buffers(
                                &mut sample_buffer, &mut alarm_buffer,
                                &connectors, &self.export_store,
                            ).await;
                            break;
                        }
                    }
                }
            }
        });
    }
}

/// Buffer relevant events into the sample or alarm buffers.
fn buffer_event(event: &Event, samples: &mut Vec<ExportSample>, alarms: &mut Vec<ExportAlarm>) {
    match event {
        Event::ValueChanged {
            node_id,
            value,
            timestamp_ms,
        } => {
            let (device_id, point_id) = split_node_id(node_id);
            samples.push(ExportSample {
                point_key: node_id.clone(),
                device_id,
                point_id,
                value: value.as_f64(),
                timestamp_ms: *timestamp_ms,
            });
        }
        Event::AlarmRaised { alarm_id, node_id } => {
            alarms.push(ExportAlarm {
                alarm_id: *alarm_id,
                node_id: node_id.clone(),
                severity: String::new(), // severity not in event, left for enrichment
                state: "raised".into(),
                timestamp_ms: now_ms(),
                value: None,
                note: None,
            });
        }
        Event::AlarmCleared { alarm_id, node_id } => {
            alarms.push(ExportAlarm {
                alarm_id: *alarm_id,
                node_id: node_id.clone(),
                severity: String::new(),
                state: "cleared".into(),
                timestamp_ms: now_ms(),
                value: None,
                note: None,
            });
        }
        Event::FddFaultRaised {
            fault_id,
            equip_id,
            severity,
            ..
        } => {
            alarms.push(ExportAlarm {
                alarm_id: *fault_id,
                node_id: equip_id.clone(),
                severity: severity.clone(),
                state: "fdd_raised".into(),
                timestamp_ms: now_ms(),
                value: None,
                note: None,
            });
        }
        Event::FddFaultCleared {
            fault_id, equip_id, ..
        } => {
            alarms.push(ExportAlarm {
                alarm_id: *fault_id,
                node_id: equip_id.clone(),
                severity: String::new(),
                state: "fdd_cleared".into(),
                timestamp_ms: now_ms(),
                value: None,
                note: None,
            });
        }
        _ => {}
    }
}

/// Flush buffered data to all active connectors.
///
/// Buffers are only cleared after ALL connectors have had a chance to process them.
/// Connectors in backoff skip the flush but the data remains for the next tick.
async fn flush_buffers(
    samples: &mut Vec<ExportSample>,
    alarms: &mut Vec<ExportAlarm>,
    connectors: &Arc<Mutex<HashMap<String, ActiveConnector>>>,
    store: &ExportStore,
) {
    let now = now_ms();
    let mut conns = connectors.lock().await;

    // Track whether any connector is still in backoff and couldn't process
    let mut any_skipped = false;

    for ac in conns.values_mut() {
        // Check backoff
        if let Some(retry_after) = ac.retry_after_ms {
            if now < retry_after {
                // This connector is in backoff — it needs the data later
                if (!samples.is_empty() && ac.on_values)
                    || (!alarms.is_empty() && (ac.on_alarms || ac.on_fdd))
                {
                    any_skipped = true;
                }
                continue;
            }
            // Backoff period expired, allow retry
            ac.retry_after_ms = None;
        }

        let mut total_rows: i64 = 0;
        let mut had_error = false;

        // Write samples
        if ac.on_values && !samples.is_empty() {
            match ac.connector.write_history_batch(samples).await {
                Ok(n) => {
                    total_rows += n as i64;
                }
                Err(e) => {
                    tracing::warn!(connector = %ac.id, error = %e, "Export write failed");
                    had_error = true;
                    any_skipped = true;
                    apply_backoff(ac);
                    let _ = store
                        .update_status(&ac.id, now, 0, Some(&e.to_string()), "error")
                        .await;
                }
            }
        }

        // Write alarms (only if values didn't error)
        if !had_error && (ac.on_alarms || ac.on_fdd) && !alarms.is_empty() {
            let filtered: Vec<ExportAlarm> = alarms
                .iter()
                .filter(|a| {
                    let is_fdd = a.state.starts_with("fdd_");
                    (ac.on_fdd && is_fdd) || (ac.on_alarms && !is_fdd)
                })
                .cloned()
                .collect();

            if !filtered.is_empty() {
                match ac.connector.write_alarm_batch(&filtered).await {
                    Ok(n) => {
                        total_rows += n as i64;
                    }
                    Err(e) => {
                        tracing::warn!(connector = %ac.id, error = %e, "Export alarm write failed");
                        any_skipped = true;
                        apply_backoff(ac);
                        let _ = store
                            .update_status(&ac.id, now, 0, Some(&e.to_string()), "error")
                            .await;
                        continue;
                    }
                }
            }
        }

        if !had_error && total_rows > 0 {
            ac.error_count = 0;
            let _ = store
                .update_status(&ac.id, now, total_rows, None, "idle")
                .await;
        }
    }

    // Only clear buffers if no connector was skipped due to backoff.
    // If a connector is in backoff, keep data so it can be retried on next tick.
    // Cap retained buffer to prevent unbounded growth.
    if !any_skipped {
        samples.clear();
        alarms.clear();
    } else {
        // Prevent unbounded buffer growth: cap at 10x normal buffer
        const MAX_RETAINED: usize = DEFAULT_MAX_BUFFER_SIZE * 10;
        if samples.len() > MAX_RETAINED {
            let drain_count = samples.len() - MAX_RETAINED;
            tracing::warn!(
                dropped = drain_count,
                "Export sample buffer overflow, dropping oldest"
            );
            samples.drain(..drain_count);
        }
        if alarms.len() > MAX_RETAINED {
            let drain_count = alarms.len() - MAX_RETAINED;
            tracing::warn!(
                dropped = drain_count,
                "Export alarm buffer overflow, dropping oldest"
            );
            alarms.drain(..drain_count);
        }
    }
}

/// Apply exponential backoff after a write failure.
fn apply_backoff(ac: &mut ActiveConnector) {
    ac.error_count += 1;
    let delay_secs = std::cmp::min(2u64.pow(ac.error_count), 300);
    ac.retry_after_ms = Some(now_ms() + (delay_secs * 1000) as i64);
    tracing::debug!(
        connector = %ac.id,
        error_count = ac.error_count,
        delay_secs,
        "Export connector backoff"
    );
}

/// Reload connector map from the store configuration.
/// Detects config changes (URL/token/bucket etc.) and rebuilds the connector.
async fn reload_connectors(
    store: &ExportStore,
    connectors: &Arc<Mutex<HashMap<String, ActiveConnector>>>,
) {
    let configs = store.list_enabled_connectors().await;
    let mut conns = connectors.lock().await;

    // Remove connectors no longer in config
    let active_ids: Vec<String> = conns.keys().cloned().collect();
    let new_ids: Vec<&str> = configs.iter().map(|c| c.id.as_str()).collect();

    for id in &active_ids {
        if !new_ids.contains(&id.as_str()) {
            if let Some(ac) = conns.remove(id) {
                ac.connector.close().await;
                tracing::info!(connector = %id, "Removed export connector");
            }
        }
    }

    // Add/update connectors
    for config in &configs {
        let new_hash = connector_config_hash(config);

        if let Some(existing) = conns.get_mut(&config.id) {
            // Always update event toggles
            existing.on_values = config.on_values;
            existing.on_alarms = config.on_alarms;
            existing.on_fdd = config.on_fdd;

            if existing.config_hash == new_hash {
                // Connection config unchanged — no rebuild needed
                continue;
            }

            // Connection config changed — close old connector and rebuild below
            existing.connector.close().await;
            conns.remove(&config.id);
            tracing::info!(connector = %config.id, name = %config.name, "Reconnecting export connector (config changed)");
        }

        match build_connector_from_config(config) {
            Some(connector) => {
                tracing::info!(
                    connector = %config.id,
                    name = %config.name,
                    connector_type = %config.connector_type,
                    "Started export connector"
                );
                conns.insert(
                    config.id.clone(),
                    ActiveConnector {
                        id: config.id.clone(),
                        connector,
                        on_values: config.on_values,
                        on_alarms: config.on_alarms,
                        on_fdd: config.on_fdd,
                        error_count: 0,
                        retry_after_ms: None,
                        config_hash: new_hash,
                    },
                );
            }
            None => {
                tracing::error!(
                    connector = %config.id,
                    connector_type = %config.connector_type,
                    "Failed to build export connector: unsupported type or invalid config"
                );
            }
        }
    }
}

/// Build a connector instance from persisted config (public for API test endpoint).
pub fn build_connector_from_config(
    config: &ExportConnectorConfig,
) -> Option<Box<dyn ExportConnector>> {
    match config.connector_type.as_str() {
        "influxdb" => {
            let influx_cfg: InfluxDbConfig = serde_json::from_str(&config.config).ok()?;
            Some(Box::new(InfluxDbConnector::new(influx_cfg)))
        }
        #[cfg(feature = "export-postgres")]
        "postgresql" => {
            let pg_cfg: crate::export::PostgresConfig =
                serde_json::from_str(&config.config).ok()?;
            Some(Box::new(crate::export::postgres::PostgresConnector::new(
                pg_cfg,
            )))
        }
        _ => None,
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
