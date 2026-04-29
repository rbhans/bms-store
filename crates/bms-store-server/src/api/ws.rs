use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio::sync::Mutex;

use crate::config::profile::PointValue;
use crate::event::bus::Event;

use super::auth::AuthUser;
use super::error::ApiError;
use super::ApiState;

/// Maximum WebSocket connections allowed per user.
const MAX_CONNECTIONS_PER_USER: usize = 5;

/// Maximum WebSocket message size (64 KB).
const MAX_MESSAGE_SIZE: usize = 64 * 1024;

/// Maximum WebSocket frame size (64 KB).
const MAX_FRAME_SIZE: usize = 64 * 1024;

/// Idle timeout for WebSocket connections (4 hours).
const IDLE_TIMEOUT: Duration = Duration::from_secs(4 * 60 * 60);

/// WebSocket upgrade handler. JWT validated via AuthUser extractor (query param ?token=...).
///
/// Enforces:
/// - Max 5 concurrent connections per user (429 if exceeded)
/// - 64 KB message/frame size cap
/// - 4-hour idle timeout
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<ApiState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, ApiError> {
    // Check and increment connection count before upgrade
    {
        let mut conns = state.ws_connections.lock().await;
        let count = conns.entry(auth.user_id.clone()).or_insert(0);
        if *count >= MAX_CONNECTIONS_PER_USER {
            tracing::warn!(
                user_id = %auth.user_id,
                count = *count,
                "WebSocket connection limit reached"
            );
            return Err(ApiError::TooManyRequests);
        }
        *count += 1;
    }

    let rx = state.event_bus.subscribe();
    let ws_connections = state.ws_connections.clone();
    let user_id = auth.user_id.clone();

    let socket_state = state.clone();
    let response = ws
        .max_message_size(MAX_MESSAGE_SIZE)
        .max_frame_size(MAX_FRAME_SIZE)
        .on_upgrade(move |socket| async move {
            handle_socket(socket, rx, auth, socket_state).await;

            // Decrement connection count on disconnect
            decrement_connection(&ws_connections, &user_id).await;
        });

    Ok(response)
}

/// Decrement the connection count for a user, removing the entry if it reaches zero.
async fn decrement_connection(ws_connections: &Arc<Mutex<HashMap<String, usize>>>, user_id: &str) {
    let mut conns = ws_connections.lock().await;
    if let Some(count) = conns.get_mut(user_id) {
        *count = count.saturating_sub(1);
        if *count == 0 {
            conns.remove(user_id);
        }
    }
}

#[derive(Deserialize)]
struct SubscribeMsg {
    subscribe: SubscribeSpec,
}

#[derive(Clone, Default, Deserialize)]
struct SubscribeObject {
    #[serde(default)]
    node_ids: Vec<String>,
    #[serde(default)]
    event_types: Vec<String>,
    #[serde(default)]
    since_seq: Option<i64>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum SubscribeSpec {
    Legacy(Vec<String>),
    Object(SubscribeObject),
}

#[derive(Clone, Default)]
struct SubscriptionFilter {
    event_types: Option<Vec<String>>,
    node_ids: Option<Vec<String>>,
}

#[derive(Serialize)]
struct WsEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    seq: Option<i64>,
    #[serde(flatten)]
    data: serde_json::Value,
}

async fn handle_socket(
    mut socket: WebSocket,
    mut rx: broadcast::Receiver<Arc<Event>>,
    _auth: AuthUser,
    state: ApiState,
) {
    // Default: send all event types
    let mut filters = SubscriptionFilter::default();
    let heartbeat = Duration::from_secs(30);
    let mut last_activity = tokio::time::Instant::now();

    loop {
        tokio::select! {
            // Incoming client messages
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        last_activity = tokio::time::Instant::now();
                        // Try to parse subscription filter
                        if let Ok(sub) = serde_json::from_str::<SubscribeMsg>(&text) {
                            let (next_filters, since_seq) = normalize_subscription(sub.subscribe);
                            filters = next_filters;
                            if let Some(since_seq) = since_seq {
                                if replay_events(&mut socket, &state, &filters, since_seq).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {
                        last_activity = tokio::time::Instant::now();
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }

            // EventBus events
            event = rx.recv() => {
                match event {
                    Ok(ev) => {
                        if let Some(ws_event) = event_to_ws(&ev, &filters, None) {
                            last_activity = tokio::time::Instant::now();
                            let json = serde_json::to_string(&ws_event).unwrap_or_default();
                            if socket.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "WebSocket client lagged behind EventBus");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }

            // Heartbeat ping + idle check
            _ = tokio::time::sleep(heartbeat) => {
                if last_activity.elapsed() > IDLE_TIMEOUT {
                    tracing::info!("WebSocket connection idle timeout");
                    break;
                }
                if socket.send(Message::Ping(vec![].into())).await.is_err() {
                    break;
                }
            }
        }
    }

    // Best-effort close frame
    let _ = socket.send(Message::Close(None)).await;
}

fn normalize_subscription(spec: SubscribeSpec) -> (SubscriptionFilter, Option<i64>) {
    match spec {
        SubscribeSpec::Legacy(event_types) => (
            SubscriptionFilter {
                event_types: Some(event_types),
                node_ids: None,
            },
            None,
        ),
        SubscribeSpec::Object(object) => {
            let event_types = if object.event_types.is_empty() {
                None
            } else {
                Some(object.event_types)
            };
            let node_ids = if object.node_ids.is_empty() {
                None
            } else {
                Some(object.node_ids)
            };
            (
                SubscriptionFilter {
                    event_types,
                    node_ids,
                },
                object.since_seq,
            )
        }
    }
}

async fn replay_events(
    socket: &mut WebSocket,
    state: &ApiState,
    filters: &SubscriptionFilter,
    since_seq: i64,
) -> Result<(), ()> {
    let Some(journal) = &state.event_journal else {
        return Ok(());
    };

    for (seq, payload) in journal.replay(since_seq).await {
        let Ok(event) = serde_json::from_str::<Event>(&payload) else {
            continue;
        };
        if let Some(ws_event) = event_to_ws(&event, filters, Some(seq)) {
            let json = serde_json::to_string(&ws_event).unwrap_or_default();
            socket
                .send(Message::Text(json.into()))
                .await
                .map_err(|_| ())?;
        }
    }
    Ok(())
}

fn event_to_ws(event: &Event, filters: &SubscriptionFilter, seq: Option<i64>) -> Option<WsEvent> {
    let (event_type, data) = match event {
        Event::ValueChanged {
            node_id,
            value,
            timestamp_ms,
        } => {
            let val = match value {
                PointValue::Bool(b) => serde_json::json!(b),
                PointValue::Integer(i) => serde_json::json!(i),
                PointValue::Float(f) => serde_json::json!(f),
            };
            (
                "values",
                serde_json::json!({
                    "node_id": node_id,
                    "value": val,
                    "timestamp_ms": timestamp_ms,
                }),
            )
        }
        Event::StatusChanged { node_id, flags } => (
            "status",
            serde_json::json!({
                "node_id": node_id,
                "flags": flags,
            }),
        ),
        Event::AlarmRaised { alarm_id, node_id } => (
            "alarms",
            serde_json::json!({
                "event": "raised",
                "alarm_id": alarm_id,
                "node_id": node_id,
            }),
        ),
        Event::AlarmCleared { alarm_id, node_id } => (
            "alarms",
            serde_json::json!({
                "event": "cleared",
                "alarm_id": alarm_id,
                "node_id": node_id,
            }),
        ),
        Event::AlarmAcknowledged { alarm_id } => (
            "alarms",
            serde_json::json!({
                "event": "acknowledged",
                "alarm_id": alarm_id,
            }),
        ),
        Event::ScheduleWritten {
            assignment_id,
            node_id,
            value,
        } => {
            let val = match value {
                PointValue::Bool(b) => serde_json::json!(b),
                PointValue::Integer(i) => serde_json::json!(i),
                PointValue::Float(f) => serde_json::json!(f),
            };
            (
                "schedules",
                serde_json::json!({
                    "event": "written",
                    "assignment_id": assignment_id,
                    "node_id": node_id,
                    "value": val,
                }),
            )
        }
        Event::DeviceDiscovered {
            bridge_type,
            device_key,
        } => (
            "discovery",
            serde_json::json!({
                "event": "discovered",
                "bridge_type": bridge_type,
                "device_key": device_key,
            }),
        ),
        Event::DeviceDown {
            bridge_type,
            device_key,
        } => (
            "discovery",
            serde_json::json!({
                "event": "device_down",
                "bridge_type": bridge_type,
                "device_key": device_key,
            }),
        ),
        Event::DeviceAccepted {
            device_key,
            protocol,
            point_count,
        } => (
            "discovery",
            serde_json::json!({
                "event": "accepted",
                "device_key": device_key,
                "protocol": protocol,
                "point_count": point_count,
            }),
        ),
        Event::DiscoveryScanComplete {
            protocol,
            device_count,
        } => (
            "discovery",
            serde_json::json!({
                "event": "scan_complete",
                "protocol": protocol,
                "device_count": device_count,
            }),
        ),
        _ => return None,
    };

    // Apply filter
    if let Some(ref event_types) = filters.event_types {
        if !event_types.iter().any(|s| s == event_type) {
            return None;
        }
    }

    if let Some(ref node_ids) = filters.node_ids {
        let node_id = event_node_id(event)?;
        if !node_ids.iter().any(|wanted| wanted == node_id) {
            return None;
        }
    }

    Some(WsEvent {
        event_type: event_type.to_string(),
        seq,
        data,
    })
}

fn event_node_id(event: &Event) -> Option<&str> {
    match event {
        Event::ValueChanged { node_id, .. }
        | Event::StatusChanged { node_id, .. }
        | Event::AlarmRaised { node_id, .. }
        | Event::AlarmCleared { node_id, .. }
        | Event::ScheduleWritten { node_id, .. } => Some(node_id),
        _ => None,
    }
}
