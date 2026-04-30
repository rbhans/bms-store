use std::time::Duration;

use crate::event::bus::{Event, EventBus};
use crate::event::durable_sub::DurableSubscription;
use crate::event::journal::EventJournal;
use crate::store::node_store::NodeStore;

use super::model::{
    DeliveryStatus, FormattedPayload, Provider, WebhookEndpoint, WebhookEventType, WebhookPayload,
};
use super::providers::format_for_provider;

use crate::store::webhook_store::WebhookStore;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const RETRY_DELAYS: [Duration; 3] = [
    Duration::from_secs(5),
    Duration::from_secs(30),
    Duration::from_secs(120),
];

/// EventBus subscriber that dispatches webhook notifications to configured endpoints.
pub struct WebhookDispatcher {
    webhook_store: WebhookStore,
    node_store: NodeStore,
    project_name: String,
}

impl WebhookDispatcher {
    pub fn new(
        webhook_store: WebhookStore,
        node_store: NodeStore,
        project_name: String,
    ) -> Self {
        Self {
            webhook_store,
            node_store,
            project_name,
        }
    }

    /// Start the dispatcher as a background tokio task.
    ///
    /// When `journal` is provided, uses a [`DurableSubscription`] to replay
    /// missed events from the journal on startup.
    pub fn start(self, event_bus: &EventBus, journal: Option<&EventJournal>) {
        let live_rx = event_bus.subscribe();
        let journal_clone = journal.cloned();
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .unwrap_or_default();

        tokio::spawn(async move {
            if let Some(ref j) = journal_clone {
                let mut durable = DurableSubscription::new("webhook-dispatcher", j, live_rx).await;
                let mut commit_ticker = tokio::time::interval(std::time::Duration::from_secs(60));
                commit_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    tokio::select! {
                        result = durable.recv() => {
                            match result {
                                Ok(event) => self.handle_event(&event, &client).await,
                                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                                _ => continue,
                            }
                        }
                        _ = commit_ticker.tick() => {
                            durable.commit_latest();
                        }
                    }
                }
                durable.commit_latest();
            } else {
                let mut event_rx = live_rx;
                loop {
                    match event_rx.recv().await {
                        Ok(event) => self.handle_event(&event, &client).await,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(
                                skipped = n,
                                "Webhook dispatcher lagged, skipping events"
                            );
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            tracing::info!("EventBus closed, webhook dispatcher stopping");
                            break;
                        }
                    }
                }
            }
        });
    }

    async fn handle_event(&self, event: &Event, client: &reqwest::Client) {
        // Classify the event and extract key fields
        enum EventKind {
            Device {
                bridge_type: String,
                device_key: String,
            },
            Fdd {
                fault_id: i64,
                equip_id: String,
                severity: String,
                rule_id: i64,
            },
        }

        let (event_type, kind) = match event {
            Event::DeviceDown {
                bridge_type,
                device_key,
            } => (
                WebhookEventType::DeviceDown,
                EventKind::Device {
                    bridge_type: bridge_type.clone(),
                    device_key: device_key.clone(),
                },
            ),
            Event::DeviceRecovered {
                bridge_type,
                device_key,
            } => (
                WebhookEventType::DeviceRecovered,
                EventKind::Device {
                    bridge_type: bridge_type.clone(),
                    device_key: device_key.clone(),
                },
            ),
            Event::FddFaultRaised {
                fault_id,
                rule_id,
                equip_id,
                severity,
            } => (
                WebhookEventType::FddFaultRaised,
                EventKind::Fdd {
                    fault_id: *fault_id,
                    equip_id: equip_id.clone(),
                    severity: severity.clone(),
                    rule_id: *rule_id,
                },
            ),
            Event::FddFaultCleared {
                fault_id,
                rule_id,
                equip_id,
            } => (
                WebhookEventType::FddFaultCleared,
                EventKind::Fdd {
                    fault_id: *fault_id,
                    equip_id: equip_id.clone(),
                    severity: String::new(),
                    rule_id: *rule_id,
                },
            ),
            _ => return,
        };

        // Check global pause
        let paused = self
            .webhook_store
            .get_config("paused")
            .await
            .unwrap_or_default();
        if paused == "true" {
            return;
        }

        let endpoints = self.webhook_store.list_enabled_endpoints().await;
        if endpoints.is_empty() {
            return;
        }

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        // Build payload based on event kind
        let (payload, severity_for_filter) = match kind {
            EventKind::Device {
                bridge_type,
                device_key,
            } => {
                let p = WebhookPayload {
                    event_type,
                    alarm_id: None,
                    node_id: Some(device_key),
                    device_id: Some(bridge_type),
                    point_id: None,
                    alarm_type: None,
                    severity: None,
                    trigger_value: None,
                    message: None,
                    timestamp_ms: now_ms,
                    project_name: self.project_name.clone(),
                };
                (p, None)
            }
            EventKind::Fdd {
                fault_id,
                equip_id,
                severity,
                rule_id,
            } => {
                let sev_str = if severity.is_empty() {
                    None
                } else {
                    Some(severity.clone())
                };
                let msg = Some(format!(
                    "FDD fault on equipment {equip_id} (rule #{rule_id})"
                ));
                let p = WebhookPayload {
                    event_type,
                    alarm_id: Some(fault_id),
                    node_id: Some(equip_id),
                    device_id: None,
                    point_id: None,
                    alarm_type: Some("fdd".to_string()),
                    severity: sev_str.clone(),
                    trigger_value: None,
                    message: msg,
                    timestamp_ms: now_ms,
                    project_name: self.project_name.clone(),
                };
                (p, sev_str)
            }
        };

        for ep in &endpoints {
            if !ep.accepts_event(event_type) {
                continue;
            }

            // Severity filter for alarm and FDD events
            if let Some(ref sev) = severity_for_filter {
                if !severity_meets_minimum(sev, &ep.min_severity) {
                    continue;
                }
            }

            // Tag filter
            if !self.matches_tags(ep, payload.node_id.as_deref()).await {
                continue;
            }

            let provider = ep.parsed_provider().unwrap_or(Provider::Generic);
            // For PagerDuty, `secret` holds the routing key (not a signing secret).
            // For Generic, `secret` is the HMAC signing key.
            let formatted = format_for_provider(
                provider,
                &payload,
                ep.secret.as_deref(),
                ep.secret.as_deref().unwrap_or(""),
            );

            let client = client.clone();
            let ep_clone = ep.clone();
            let store = self.webhook_store.clone();
            tokio::spawn(async move {
                send_with_retry(&client, &ep_clone, &formatted, &store, event_type).await;
            });
        }
    }

    async fn matches_tags(&self, ep: &WebhookEndpoint, node_id: Option<&str>) -> bool {
        let filters = ep.parsed_tag_filters();
        if filters.is_empty() {
            return true; // No filters = match everything
        }
        let node_id = match node_id {
            Some(id) => id,
            None => return true, // No node = can't filter, allow through
        };

        // Try to get node tags from NodeStore
        match self.node_store.get_node(node_id).await {
            Ok(record) => {
                // AND logic: all filters must match
                for f in &filters {
                    let tag_value = record.tags.get(&f.tag);
                    let matched = match f.op.as_str() {
                        "=" => tag_value.and_then(|v| v.as_deref()) == Some(&f.value),
                        "!=" => tag_value.and_then(|v| v.as_deref()) != Some(&f.value),
                        "exists" => tag_value.is_some(),
                        _ => true,
                    };
                    if !matched {
                        return false;
                    }
                }
                true
            }
            Err(_) => true, // Can't resolve tags, allow through
        }
    }
}

/// Compare severity strings using a numeric ordering (info < warning < critical < life_safety).
fn severity_meets_minimum(event_severity: &str, min_severity: &str) -> bool {
    fn rank(s: &str) -> Option<u8> {
        match s {
            "info" => Some(0),
            "warning" => Some(1),
            "critical" => Some(2),
            "life_safety" => Some(3),
            _ => None,
        }
    }
    match (rank(event_severity), rank(min_severity)) {
        (Some(e), Some(m)) => e >= m,
        _ => true, // Unknown severity, let it through
    }
}

async fn send_with_retry(
    client: &reqwest::Client,
    endpoint: &WebhookEndpoint,
    payload: &FormattedPayload,
    store: &WebhookStore,
    event_type: WebhookEventType,
) {
    let now_ms = || {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64
    };

    let preview = if payload.body.len() > 200 {
        &payload.body[..200]
    } else {
        &payload.body
    };

    for attempt in 0..RETRY_DELAYS.len() {
        if attempt > 0 {
            tokio::time::sleep(RETRY_DELAYS[attempt - 1]).await;
        }

        let mut req = client
            .post(&endpoint.url)
            .header("Content-Type", &payload.content_type);

        // Add custom headers from endpoint config
        if let Some(ref headers_json) = endpoint.headers {
            if let Ok(headers) = serde_json::from_str::<serde_json::Value>(headers_json) {
                if let Some(obj) = headers.as_object() {
                    for (k, v) in obj {
                        if let Some(val) = v.as_str() {
                            req = req.header(k, val);
                        }
                    }
                }
            }
        }

        // Add provider-specific extra headers
        for (k, v) in &payload.extra_headers {
            req = req.header(k, v);
        }

        match req.body(payload.body.clone()).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                if resp.status().is_success() {
                    let _ = store
                        .log_delivery(
                            &endpoint.id,
                            event_type.as_str(),
                            now_ms(),
                            DeliveryStatus::Delivered.as_str(),
                            Some(status),
                            None,
                            Some(preview),
                        )
                        .await;
                    tracing::debug!(
                        endpoint = %endpoint.name,
                        status,
                        "Webhook delivered"
                    );
                    return;
                }

                let error_body = resp.text().await.unwrap_or_default();
                let err_msg = format!("HTTP {}: {}", status, truncate(&error_body, 200));

                if attempt == RETRY_DELAYS.len() - 1 {
                    let _ = store
                        .log_delivery(
                            &endpoint.id,
                            event_type.as_str(),
                            now_ms(),
                            DeliveryStatus::Failed.as_str(),
                            Some(status),
                            Some(&err_msg),
                            Some(preview),
                        )
                        .await;
                    tracing::warn!(
                        endpoint = %endpoint.name,
                        attempt = attempt + 1,
                        error = %err_msg,
                        "Webhook delivery failed (final attempt)"
                    );
                } else {
                    let _ = store
                        .log_delivery(
                            &endpoint.id,
                            event_type.as_str(),
                            now_ms(),
                            DeliveryStatus::Retrying.as_str(),
                            Some(status),
                            Some(&err_msg),
                            Some(preview),
                        )
                        .await;
                    tracing::debug!(
                        endpoint = %endpoint.name,
                        attempt = attempt + 1,
                        "Webhook delivery retrying"
                    );
                }
            }
            Err(e) => {
                let err_msg = format!("Network error: {}", e);
                if attempt == RETRY_DELAYS.len() - 1 {
                    let _ = store
                        .log_delivery(
                            &endpoint.id,
                            event_type.as_str(),
                            now_ms(),
                            DeliveryStatus::Failed.as_str(),
                            None,
                            Some(&err_msg),
                            Some(preview),
                        )
                        .await;
                    tracing::warn!(
                        endpoint = %endpoint.name,
                        attempt = attempt + 1,
                        error = %e,
                        "Webhook delivery failed (final attempt)"
                    );
                } else {
                    let _ = store
                        .log_delivery(
                            &endpoint.id,
                            event_type.as_str(),
                            now_ms(),
                            DeliveryStatus::Retrying.as_str(),
                            None,
                            Some(&err_msg),
                            Some(preview),
                        )
                        .await;
                }
            }
        }
    }
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() > max {
        &s[..max]
    } else {
        s
    }
}
