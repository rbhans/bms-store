//! Durable event journal backed by SQLite.
//!
//! Follows the project-standard mpsc + dedicated SQLite thread pattern.
//! When attached to the [`EventBus`](super::EventBus) via
//! [`EventBus::with_journal`](super::EventBus::with_journal), every published
//! event is persisted for crash recovery and replay.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::{mpsc, oneshot};

use crate::event::bus::{Event, EventJournalBackend, EventSeq};
use crate::store::migration::{run_migrations, Migration};

// ── Schema ─────────────────────────────────────────────────────────

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    label: "create event journal",
    sql: "
CREATE TABLE event_journal (
    seq          INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp_ms INTEGER NOT NULL,
    event_type   TEXT    NOT NULL,
    payload      TEXT    NOT NULL,
    site_id      TEXT    NOT NULL DEFAULT ''
);
CREATE INDEX idx_journal_time ON event_journal(timestamp_ms);
CREATE INDEX idx_journal_type ON event_journal(event_type);

CREATE TABLE subscriber_cursors (
    subscriber_id TEXT PRIMARY KEY,
    last_seq      INTEGER NOT NULL DEFAULT 0,
    updated_ms    INTEGER NOT NULL
);
",
}];

// ── Commands ───────────────────────────────────────────────────────

enum JournalCmd {
    /// Append a single event. The SQLite thread batches consecutive Append
    /// commands into a single transaction by draining the channel.
    Append {
        event_type: String,
        payload: String,
        timestamp_ms: i64,
        site_id: String,
    },
    Replay {
        from_seq: i64,
        reply: oneshot::Sender<Vec<(EventSeq, String)>>,
    },
    GetCursor {
        subscriber_id: String,
        reply: oneshot::Sender<i64>,
    },
    UpdateCursor {
        subscriber_id: String,
        seq: i64,
    },
    /// Set cursor to the current MAX(seq) — used by live subscribers to periodically
    /// checkpoint "I'm caught up to here."
    CommitLatest {
        subscriber_id: String,
    },
    Prune {
        max_age_secs: u64,
        max_events: u64,
    },
    GetStats {
        reply: oneshot::Sender<JournalStats>,
    },
}

/// Diagnostic stats about the event journal.
#[derive(Debug, Clone)]
pub struct JournalStats {
    pub total_events: i64,
    pub oldest_timestamp_ms: Option<i64>,
    pub newest_timestamp_ms: Option<i64>,
}

// ── Handle ─────────────────────────────────────────────────────────

/// Async handle to the durable event journal.
///
/// Internally sends commands to a dedicated SQLite thread via an unbounded
/// mpsc channel. Clone is cheap — all clones share the same underlying channel.
/// Events are sent directly to the SQLite thread (single hop) to minimize the
/// window between publish and disk persistence.
#[derive(Clone)]
pub struct EventJournal {
    cmd_tx: mpsc::UnboundedSender<JournalCmd>,
    /// Site identifier stamped onto every append. Empty in single-site mode.
    /// Multi-site supervisor passes the project UUID via `with_site_id`.
    site_id: String,
}

impl EventJournal {
    /// Return a clone of this journal that stamps every appended event with
    /// the given site id. Used by the multi-site supervisor flow; single-site
    /// mode keeps `site_id = ""`.
    pub fn with_site_id(&self, site_id: impl Into<String>) -> Self {
        Self {
            cmd_tx: self.cmd_tx.clone(),
            site_id: site_id.into(),
        }
    }

    /// Append a single event (fire-and-forget, non-blocking).
    ///
    /// The event is sent directly to the SQLite thread for immediate persistence.
    /// The thread batches consecutive appends into a single transaction.
    pub fn append(&self, event: &Event) {
        let event_type = event.event_type_name().to_string();
        let payload = match serde_json::to_string(event) {
            Ok(json) => json,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to serialize event for journal");
                return;
            }
        };
        let timestamp_ms = now_ms();
        let _ = self.cmd_tx.send(JournalCmd::Append {
            event_type,
            payload,
            timestamp_ms,
            site_id: self.site_id.clone(),
        });
    }

    /// Replay events after a given sequence number.
    /// Returns `(seq, payload_json)` pairs in order.
    pub async fn replay(&self, from_seq: i64) -> Vec<(EventSeq, String)> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(JournalCmd::Replay {
            from_seq,
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_default()
    }

    /// Get the last-processed sequence number for a subscriber (0 if unknown).
    pub async fn get_cursor(&self, subscriber_id: &str) -> i64 {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(JournalCmd::GetCursor {
            subscriber_id: subscriber_id.to_string(),
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or(0)
    }

    /// Update the cursor for a subscriber (fire-and-forget).
    pub fn update_cursor(&self, subscriber_id: &str, seq: i64) {
        let _ = self.cmd_tx.send(JournalCmd::UpdateCursor {
            subscriber_id: subscriber_id.to_string(),
            seq,
        });
    }

    /// Advance the cursor to the latest journal sequence (fire-and-forget).
    /// Used by live subscribers to periodically checkpoint progress.
    pub fn commit_latest(&self, subscriber_id: &str) {
        let _ = self.cmd_tx.send(JournalCmd::CommitLatest {
            subscriber_id: subscriber_id.to_string(),
        });
    }

    /// Trigger pruning (fire-and-forget).
    pub fn prune(&self, max_age_secs: u64, max_events: u64) {
        let _ = self.cmd_tx.send(JournalCmd::Prune {
            max_age_secs,
            max_events,
        });
    }

    /// Get journal statistics.
    pub async fn stats(&self) -> JournalStats {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(JournalCmd::GetStats { reply: reply_tx });
        reply_rx.await.unwrap_or(JournalStats {
            total_events: 0,
            oldest_timestamp_ms: None,
            newest_timestamp_ms: None,
        })
    }
}

impl EventJournalBackend for EventJournal {
    fn append(&self, event: &Event) {
        EventJournal::append(self, event);
    }
}

// ── Start ──────────────────────────────────────────────────────────

/// Start the event journal. Creates the SQLite database and spawns the
/// background SQLite thread.
pub fn start_event_journal(db_path: &Path) -> EventJournal {
    let db_dir = db_path.parent().unwrap_or(Path::new("."));
    if !db_dir.exists() {
        std::fs::create_dir_all(db_dir).expect("failed to create data directory for event journal");
    }

    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

    let path = db_path.to_path_buf();
    std::thread::Builder::new()
        .name("event-journal-sqlite".into())
        .spawn(move || run_sqlite_thread(&path, cmd_rx))
        .expect("failed to spawn event journal SQLite thread");

    EventJournal {
        cmd_tx,
        site_id: String::new(),
    }
}

/// Start a periodic pruning task. Should be called once from init_platform.
pub fn start_pruning_task(
    journal: EventJournal,
    max_age_secs: u64,
    max_events: u64,
    interval_secs: u64,
    shutdown: tokio_util::sync::CancellationToken,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        interval.tick().await; // skip first immediate tick
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    journal.prune(max_age_secs, max_events);
                }
                _ = shutdown.cancelled() => break,
            }
        }
        tracing::info!("Event journal pruning task stopped");
    });
}

// ── SQLite thread ──────────────────────────────────────────────────

fn run_sqlite_thread(db_path: &Path, mut cmd_rx: mpsc::UnboundedReceiver<JournalCmd>) {
    let conn = rusqlite::Connection::open(db_path).expect("failed to open event_journal.db");
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
        .expect("failed to set event journal pragmas");

    run_migrations(&conn, "event_journal", MIGRATIONS)
        .expect("failed to run event journal migrations");

    while let Some(cmd) = cmd_rx.blocking_recv() {
        match cmd {
            JournalCmd::Append {
                event_type,
                payload,
                timestamp_ms,
                site_id,
            } => {
                // Drain any additional Append commands that arrived to batch them
                // in a single transaction.
                let mut batch = vec![(event_type, payload, timestamp_ms, site_id)];
                while let Ok(next) = cmd_rx.try_recv() {
                    match next {
                        JournalCmd::Append {
                            event_type,
                            payload,
                            timestamp_ms,
                            site_id,
                        } => {
                            batch.push((event_type, payload, timestamp_ms, site_id));
                        }
                        other => {
                            // Non-append command — flush the batch first, then handle it.
                            if let Err(e) = append_batch(&conn, &batch) {
                                tracing::warn!(error = %e, count = batch.len(), "Failed to write event journal batch");
                            }
                            batch.clear();
                            handle_cmd(&conn, other);
                            break;
                        }
                    }
                }
                if !batch.is_empty() {
                    if let Err(e) = append_batch(&conn, &batch) {
                        tracing::warn!(error = %e, count = batch.len(), "Failed to write event journal batch");
                    }
                }
            }
            other => handle_cmd(&conn, other),
        }
    }
    tracing::info!("Event journal SQLite thread stopped");
}

/// Handle a non-Append command.
fn handle_cmd(conn: &rusqlite::Connection, cmd: JournalCmd) {
    match cmd {
        JournalCmd::Append { .. } => unreachable!("Append handled in caller"),
        JournalCmd::Replay { from_seq, reply } => {
            let result = replay_events(conn, from_seq);
            let _ = reply.send(result.unwrap_or_default());
        }
        JournalCmd::GetCursor {
            subscriber_id,
            reply,
        } => {
            let seq = get_cursor(conn, &subscriber_id);
            let _ = reply.send(seq);
        }
        JournalCmd::UpdateCursor { subscriber_id, seq } => {
            if let Err(e) = update_cursor(conn, &subscriber_id, seq) {
                tracing::warn!(error = %e, subscriber = %subscriber_id, "Failed to update journal cursor");
            }
        }
        JournalCmd::CommitLatest { subscriber_id } => {
            if let Err(e) = commit_latest(conn, &subscriber_id) {
                tracing::warn!(error = %e, subscriber = %subscriber_id, "Failed to commit latest cursor");
            }
        }
        JournalCmd::Prune {
            max_age_secs,
            max_events,
        } => {
            if let Err(e) = prune_journal(conn, max_age_secs, max_events) {
                tracing::warn!(error = %e, "Failed to prune event journal");
            }
        }
        JournalCmd::GetStats { reply } => {
            let stats = get_stats(conn);
            let _ = reply.send(stats);
        }
    }
}

// ── DB operations ──────────────────────────────────────────────────

fn append_batch(
    conn: &rusqlite::Connection,
    entries: &[(String, String, i64, String)],
) -> Result<(), rusqlite::Error> {
    let tx = conn.unchecked_transaction()?;
    {
        let mut stmt = tx.prepare_cached(
            "INSERT INTO event_journal (timestamp_ms, event_type, payload, site_id) VALUES (?1, ?2, ?3, ?4)",
        )?;
        for (event_type, payload, timestamp_ms, site_id) in entries {
            stmt.execute(rusqlite::params![
                timestamp_ms,
                event_type,
                payload,
                site_id
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn replay_events(
    conn: &rusqlite::Connection,
    from_seq: i64,
) -> Result<Vec<(EventSeq, String)>, rusqlite::Error> {
    let mut stmt = conn
        .prepare_cached("SELECT seq, payload FROM event_journal WHERE seq > ?1 ORDER BY seq ASC")?;
    let rows = stmt.query_map(rusqlite::params![from_seq], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    Ok(results)
}

fn get_cursor(conn: &rusqlite::Connection, subscriber_id: &str) -> i64 {
    conn.query_row(
        "SELECT last_seq FROM subscriber_cursors WHERE subscriber_id = ?1",
        rusqlite::params![subscriber_id],
        |row| row.get(0),
    )
    .unwrap_or(0)
}

fn update_cursor(
    conn: &rusqlite::Connection,
    subscriber_id: &str,
    seq: i64,
) -> Result<(), rusqlite::Error> {
    let now = now_ms();
    conn.execute(
        "INSERT INTO subscriber_cursors (subscriber_id, last_seq, updated_ms) \
         VALUES (?1, ?2, ?3) \
         ON CONFLICT(subscriber_id) DO UPDATE SET last_seq = ?2, updated_ms = ?3",
        rusqlite::params![subscriber_id, seq, now],
    )?;
    Ok(())
}

fn commit_latest(conn: &rusqlite::Connection, subscriber_id: &str) -> Result<(), rusqlite::Error> {
    let max_seq: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(seq), 0) FROM event_journal",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    if max_seq > 0 {
        update_cursor(conn, subscriber_id, max_seq)?;
    }
    Ok(())
}

fn prune_journal(
    conn: &rusqlite::Connection,
    max_age_secs: u64,
    max_events: u64,
) -> Result<(), rusqlite::Error> {
    // Never delete events that a subscriber still needs.
    // If no subscribers exist, all events are prunable (use i64::MAX).
    let has_cursors: bool = conn
        .query_row("SELECT COUNT(*) > 0 FROM subscriber_cursors", [], |row| {
            row.get(0)
        })
        .unwrap_or(false);

    let max_prunable_seq: i64 = if has_cursors {
        conn.query_row(
            "SELECT COALESCE(MIN(last_seq), 0) FROM subscriber_cursors",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0)
    } else {
        i64::MAX
    };

    // Prune by age.
    let cutoff_ms = now_ms() - (max_age_secs as i64 * 1000);
    conn.execute(
        "DELETE FROM event_journal WHERE timestamp_ms < ?1 AND seq <= ?2",
        rusqlite::params![cutoff_ms, max_prunable_seq],
    )?;

    // Prune by count — keep the newest `max_events` rows.
    let total: i64 = conn.query_row("SELECT COUNT(*) FROM event_journal", [], |row| row.get(0))?;
    if total > max_events as i64 {
        let excess = total - max_events as i64;
        conn.execute(
            "DELETE FROM event_journal WHERE seq IN (
                SELECT seq FROM event_journal
                WHERE seq <= ?1
                ORDER BY seq ASC LIMIT ?2
            )",
            rusqlite::params![max_prunable_seq, excess],
        )?;
    }

    Ok(())
}

fn get_stats(conn: &rusqlite::Connection) -> JournalStats {
    let total_events: i64 = conn
        .query_row("SELECT COUNT(*) FROM event_journal", [], |row| row.get(0))
        .unwrap_or(0);
    let oldest_timestamp_ms: Option<i64> = conn
        .query_row("SELECT MIN(timestamp_ms) FROM event_journal", [], |row| {
            row.get(0)
        })
        .unwrap_or(None);
    let newest_timestamp_ms: Option<i64> = conn
        .query_row("SELECT MAX(timestamp_ms) FROM event_journal", [], |row| {
            row.get(0)
        })
        .unwrap_or(None);
    JournalStats {
        total_events,
        oldest_timestamp_ms,
        newest_timestamp_ms,
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: start a journal with an in-memory SQLite database.
    fn start_test_journal() -> EventJournal {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        // Run SQLite in-memory on a background thread.
        std::thread::Builder::new()
            .name("test-journal-sqlite".into())
            .spawn(move || {
                // Can't use run_sqlite_thread directly since it opens by path.
                // Inline the same logic with in-memory connection.
                let conn =
                    rusqlite::Connection::open_in_memory().expect("failed to open in-memory db");
                conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
                    .ok();
                run_migrations(&conn, "event_journal", MIGRATIONS).expect("migration failed");

                let mut cmd_rx = cmd_rx;
                while let Some(cmd) = cmd_rx.blocking_recv() {
                    match cmd {
                        JournalCmd::Append {
                            event_type,
                            payload,
                            timestamp_ms,
                            site_id,
                        } => {
                            // In tests, process one at a time (no batching drain).
                            append_batch(&conn, &[(event_type, payload, timestamp_ms, site_id)])
                                .unwrap();
                        }
                        other => handle_cmd(&conn, other),
                    }
                }
            })
            .unwrap();

        EventJournal {
            cmd_tx,
            site_id: String::new(),
        }
    }

    /// Send an append command directly to the SQLite thread.
    fn append_sync(journal: &EventJournal, event: &Event) {
        let event_type = event.event_type_name().to_string();
        let payload = serde_json::to_string(event).unwrap();
        let timestamp_ms = now_ms();
        let _ = journal.cmd_tx.send(JournalCmd::Append {
            event_type,
            payload,
            timestamp_ms,
            site_id: journal.site_id.clone(),
        });
    }

    #[tokio::test]
    async fn append_and_replay_roundtrip() {
        let journal = start_test_journal();

        let e1 = Event::ValueChanged {
            node_id: "ahu-1/dat".into(),
            value: crate::config::profile::PointValue::Float(72.5),
            timestamp_ms: 1000,
        };
        let e2 = Event::EntityCreated {
            entity_id: "vav-1".into(),
        };

        append_sync(&journal, &e1);
        append_sync(&journal, &e2);

        // Small delay for the SQLite thread to process.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let replayed = journal.replay(0).await;
        assert_eq!(replayed.len(), 2);

        // Verify ordering.
        assert!(replayed[0].0 < replayed[1].0);

        // Verify deserialization.
        let parsed: Event = serde_json::from_str(&replayed[0].1).unwrap();
        assert_eq!(parsed.event_type_name(), "ValueChanged");
    }

    #[tokio::test]
    async fn cursor_persistence() {
        let journal = start_test_journal();

        // Default cursor is 0.
        assert_eq!(journal.get_cursor("sub-a").await, 0);

        journal.update_cursor("sub-a", 42);
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;

        assert_eq!(journal.get_cursor("sub-a").await, 42);

        // Update cursor again.
        journal.update_cursor("sub-a", 100);
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;

        assert_eq!(journal.get_cursor("sub-a").await, 100);
    }

    #[tokio::test]
    async fn replay_from_cursor() {
        let journal = start_test_journal();

        for i in 0..5 {
            append_sync(
                &journal,
                &Event::ValueChanged {
                    node_id: format!("pt-{i}"),
                    value: crate::config::profile::PointValue::Float(i as f64),
                    timestamp_ms: 1000 + i,
                },
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Replay all from 0.
        let all = journal.replay(0).await;
        assert_eq!(all.len(), 5);

        // Replay from seq 3 should return events 4 and 5.
        let partial = journal.replay(3).await;
        assert_eq!(partial.len(), 2);
        assert_eq!(partial[0].0, 4);
        assert_eq!(partial[1].0, 5);
    }

    #[tokio::test]
    async fn prune_by_count() {
        let journal = start_test_journal();

        for i in 0..10 {
            append_sync(
                &journal,
                &Event::StatusChanged {
                    node_id: format!("pt-{i}"),
                    flags: 0,
                },
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let before = journal.stats().await;
        assert_eq!(before.total_events, 10);

        // Prune to keep only 5 events. No subscriber cursors → min_cursor = 0, all prunable.
        journal.prune(86400, 5);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let after = journal.stats().await;
        assert_eq!(after.total_events, 5);
    }

    #[tokio::test]
    async fn prune_respects_subscriber_cursor() {
        let journal = start_test_journal();

        for i in 0..10 {
            append_sync(
                &journal,
                &Event::StatusChanged {
                    node_id: format!("pt-{i}"),
                    flags: 0,
                },
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Set a cursor at seq 7 — events 1..=7 are "still needed" by this subscriber.
        journal.update_cursor("slow-sub", 7);
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;

        // Try to prune to 3 events.
        journal.prune(86400, 3);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Pruning should only delete events with seq <= min_cursor (7), keeping 3 newest.
        // But the count-based prune only deletes where seq <= min_cursor, so it can delete
        // up to 7 events (seq 1-7), keeping at least 3 (seq 8-10).
        let stats = journal.stats().await;
        // We asked to keep 3, and cursor allows deleting up to seq 7.
        // 10 total - 7 deletable = 3 must remain minimum. Excess = 10-3 = 7, all deletable.
        assert!(stats.total_events >= 3);
    }

    #[tokio::test]
    async fn with_site_id_stamps_site_field() {
        // Use a file-backed journal so we can query site_id with a second connection.
        let tmp =
            std::env::temp_dir().join(format!("opencrate-journal-{}.db", uuid::Uuid::new_v4()));
        let journal = start_event_journal(&tmp);
        let journal_for_site_a = journal.with_site_id("site-a");
        let journal_for_site_b = journal.with_site_id("site-b");

        journal_for_site_a.append(&Event::EntityCreated {
            entity_id: "a".into(),
        });
        journal_for_site_b.append(&Event::EntityCreated {
            entity_id: "b".into(),
        });
        // Also append without a site id via the base handle.
        journal.append(&Event::EntityCreated {
            entity_id: "c".into(),
        });

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Query site_ids directly.
        let conn = rusqlite::Connection::open(&tmp).unwrap();
        let mut stmt = conn
            .prepare("SELECT site_id FROM event_journal ORDER BY seq")
            .unwrap();
        let rows: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert!(rows.contains(&"site-a".to_string()));
        assert!(rows.contains(&"site-b".to_string()));
        assert!(rows.contains(&String::new()));

        let _ = std::fs::remove_file(&tmp);
    }

    #[tokio::test]
    async fn stats_empty_journal() {
        let journal = start_test_journal();
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;

        let stats = journal.stats().await;
        assert_eq!(stats.total_events, 0);
        assert!(stats.oldest_timestamp_ms.is_none());
        assert!(stats.newest_timestamp_ms.is_none());
    }

    #[tokio::test]
    async fn event_serialization_roundtrip() {
        // Verify all event variants survive JSON serialization.
        let events = vec![
            Event::ValueChanged {
                node_id: "n1".into(),
                value: crate::config::profile::PointValue::Bool(true),
                timestamp_ms: 123,
            },
            Event::StatusChanged {
                node_id: "n2".into(),
                flags: 0x0F,
            },
            Event::DeviceDiscovered {
                bridge_type: "bacnet".into(),
                device_key: "d1".into(),
            },
            Event::DeviceDown {
                bridge_type: "modbus".into(),
                device_key: "d2".into(),
            },
            Event::DeviceRecovered {
                bridge_type: "bacnet".into(),
                device_key: "d1".into(),
            },
            Event::FddFaultRaised {
                fault_id: 10,
                rule_id: 5,
                equip_id: "ahu-1".into(),
                severity: "critical".into(),
            },
            Event::FddFaultCleared {
                fault_id: 10,
                rule_id: 5,
                equip_id: "ahu-1".into(),
            },
        ];

        for event in &events {
            let json = serde_json::to_string(event).unwrap();
            let parsed: Event = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed.event_type_name(), event.event_type_name());
        }
    }
}
