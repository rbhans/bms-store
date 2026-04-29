//! Durable event subscription with replay-then-live semantics.
//!
//! A [`DurableSubscription`] first replays any events missed since the
//! subscriber's last persisted cursor, then seamlessly transitions to the
//! live broadcast stream. This guarantees at-least-once delivery across
//! process restarts (when the event journal is enabled).

use std::collections::VecDeque;
use std::sync::Arc;

use tokio::sync::broadcast;

use crate::event::bus::Event;
use crate::event::journal::EventJournal;

use super::bus::EventSeq;

/// A durable subscription that replays missed events on creation,
/// then transitions to the live broadcast stream.
///
/// Provides **at-least-once** delivery: events published during the overlap
/// between replay and live may be delivered twice. All four migrated
/// subscribers (alarm router, MQTT, webhooks, export) are idempotent or
/// side-effect-tolerant, so this is acceptable.
pub struct DurableSubscription {
    subscriber_id: String,
    journal: EventJournal,
    /// Buffered replay events (drained first before switching to live).
    replay_buffer: VecDeque<Arc<Event>>,
    /// Live broadcast receiver (used after replay is drained).
    live_rx: broadcast::Receiver<Arc<Event>>,
    /// Last sequence number processed. Updated by `commit()`.
    last_seq: EventSeq,
}

impl DurableSubscription {
    /// Create a new durable subscription.
    ///
    /// **Important**: The `live_rx` must be obtained from `event_bus.subscribe()`
    /// *before* calling this constructor to ensure no gap between replay and live.
    pub async fn new(
        subscriber_id: &str,
        journal: &EventJournal,
        live_rx: broadcast::Receiver<Arc<Event>>,
    ) -> Self {
        let cursor = journal.get_cursor(subscriber_id).await;
        let replayed = journal.replay(cursor).await;

        let mut replay_buffer = VecDeque::with_capacity(replayed.len());
        let mut max_seq = cursor;

        for (seq, payload_json) in replayed {
            match serde_json::from_str::<Event>(&payload_json) {
                Ok(event) => {
                    if seq > max_seq {
                        max_seq = seq;
                    }
                    replay_buffer.push_back(Arc::new(event));
                }
                Err(e) => {
                    tracing::warn!(
                        seq,
                        error = %e,
                        "Skipping undeserializable event during replay"
                    );
                }
            }
        }

        if !replay_buffer.is_empty() {
            tracing::info!(
                subscriber = subscriber_id,
                replayed = replay_buffer.len(),
                from_seq = cursor,
                to_seq = max_seq,
                "Replaying missed events from journal"
            );
        }

        DurableSubscription {
            subscriber_id: subscriber_id.to_string(),
            journal: journal.clone(),
            replay_buffer,
            live_rx,
            last_seq: cursor,
        }
    }

    /// Receive the next event.
    ///
    /// During replay, returns buffered events from the journal. After the
    /// replay buffer is drained, returns events from the live broadcast.
    /// Returns `Err(RecvError::Closed)` when the event bus shuts down.
    pub async fn recv(&mut self) -> Result<Arc<Event>, broadcast::error::RecvError> {
        // Phase 1: drain replay buffer.
        if let Some(event) = self.replay_buffer.pop_front() {
            return Ok(event);
        }

        // Phase 2: live broadcast.
        loop {
            match self.live_rx.recv().await {
                Ok(event) => return Ok(event),
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(
                        subscriber = %self.subscriber_id,
                        skipped = n,
                        "Durable subscriber lagged on live stream"
                    );
                    // Continue to get the next available event.
                }
                Err(broadcast::error::RecvError::Closed) => {
                    return Err(broadcast::error::RecvError::Closed);
                }
            }
        }
    }

    /// Commit progress — persist the cursor to the journal so that replay
    /// starts from here on next restart.
    pub fn commit(&mut self, seq: EventSeq) {
        if seq > self.last_seq {
            self.last_seq = seq;
            self.journal.update_cursor(&self.subscriber_id, seq);
        }
    }

    /// Commit progress to the latest journal entry. Used during live mode
    /// when individual sequence numbers aren't available.
    ///
    /// **No-op while replay is in progress** — calling this before the replay
    /// buffer is fully drained would advance the cursor past unprocessed events.
    pub fn commit_latest(&self) {
        if !self.replay_buffer.is_empty() {
            return;
        }
        self.journal.commit_latest(&self.subscriber_id);
    }

    /// Returns true if the replay buffer has been fully drained.
    pub fn is_live(&self) -> bool {
        self.replay_buffer.is_empty()
    }

    /// Returns the subscriber ID.
    pub fn subscriber_id(&self) -> &str {
        &self.subscriber_id
    }
}
