use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::{mpsc, oneshot};

use crate::config::loader::LoadedDevice;
use crate::config::profile::PointKind;
use crate::store::point_store::{PointKey, PointStore};

// ----------------------------------------------------------------
// Public types
// ----------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct HistoryQuery {
    pub device_id: String,
    pub point_id: String,
    pub start_ms: i64,
    pub end_ms: i64,
    /// Max results to return. None uses default cap (2000).
    /// Some(0) means uncapped (for CSV export).
    pub max_results: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HistoryResult {
    pub device_id: String,
    pub point_id: String,
    pub samples: Vec<HistorySample>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HistorySample {
    pub timestamp_ms: i64,
    pub value: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error("database error: {0}")]
    Db(String),
    #[error("channel closed")]
    ChannelClosed,
}

// ----------------------------------------------------------------
// Configuration constants
// ----------------------------------------------------------------

/// Hot tier: full COV resolution.
const HOT_RETENTION_MS: i64 = 48 * 3600 * 1000;
/// Warm tier: 1-minute rollups.
const WARM_RETENTION_MS: i64 = 90 * 24 * 3600 * 1000;
/// Cold tier: 15-minute rollups.
const COLD_RETENTION_MS: i64 = 2 * 365 * 24 * 3600 * 1000;

const WARM_INTERVAL_MS: i64 = 60 * 1000; // 1 minute
const COLD_INTERVAL_MS: i64 = 15 * 60 * 1000; // 15 minutes
const ARCHIVE_INTERVAL_MS: i64 = 3600 * 1000; // 1 hour

/// Max data points returned per query.
const MAX_QUERY_RESULTS: i64 = 2000;

/// Batch flush interval (seconds).
const FLUSH_INTERVAL_SECS: u64 = 1;

/// Heartbeat interval (seconds) — poll silent points.
const HEARTBEAT_SECS: u64 = 15 * 60;

/// How often hot→warm downsampling runs (seconds).
const DOWNSAMPLE_HOT_INTERVAL_SECS: u64 = 3600;

/// How often warm→cold downsampling runs (seconds).
const DOWNSAMPLE_WARM_INTERVAL_SECS: u64 = 24 * 3600;

/// How often cold→archive downsampling runs (seconds).
const DOWNSAMPLE_COLD_INTERVAL_SECS: u64 = 7 * 24 * 3600;

// ----------------------------------------------------------------
// Commands sent to the SQLite thread
// ----------------------------------------------------------------

enum HistoryCmd {
    /// Batch insert into hot tier.
    BatchInsert(Vec<(String, i64, f64)>), // (point_key, timestamp_ms, value)
    /// Query: automatically picks tier based on time range.
    Query {
        query: HistoryQuery,
        reply: oneshot::Sender<Result<HistoryResult, HistoryError>>,
    },
    /// Get min/max timestamps for a point (across all tiers).
    TimeRange {
        point_key: String,
        reply: oneshot::Sender<Option<(i64, i64)>>,
    },
    /// Run hot→warm downsampling.
    DownsampleHot,
    /// Run warm→cold downsampling.
    DownsampleWarm,
    /// Run cold→archive downsampling.
    DownsampleCold,
}

// ----------------------------------------------------------------
// HistoryStore — async handle to the SQLite thread
// ----------------------------------------------------------------

#[derive(Clone)]
pub struct HistoryStore {
    cmd_tx: mpsc::UnboundedSender<HistoryCmd>,
}

impl HistoryStore {
    pub async fn query(&self, q: HistoryQuery) -> Result<HistoryResult, HistoryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(HistoryCmd::Query {
                query: q,
                reply: reply_tx,
            })
            .map_err(|_| HistoryError::ChannelClosed)?;
        reply_rx.await.map_err(|_| HistoryError::ChannelClosed)?
    }

    pub async fn time_range(&self, device_id: &str, point_id: &str) -> Option<(i64, i64)> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let point_key = format!("{device_id}:{point_id}");
        let _ = self.cmd_tx.send(HistoryCmd::TimeRange {
            point_key,
            reply: reply_tx,
        });
        reply_rx.await.ok().flatten()
    }

    /// Insert historical samples directly (e.g. from TrendLog backfill).
    /// Each tuple is (point_key, timestamp_ms, value).
    pub async fn backfill(&self, samples: Vec<(String, i64, f64)>) {
        let _ = self.cmd_tx.send(HistoryCmd::BatchInsert(samples));
    }
}

// ----------------------------------------------------------------
// SQLite schema & thread
// ----------------------------------------------------------------

use super::migration::{run_migrations, Migration};

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    label: "initial history schema",
    sql: "
CREATE TABLE IF NOT EXISTS history_hot (
    point_key TEXT NOT NULL,
    ts INTEGER NOT NULL,
    value REAL NOT NULL,
    PRIMARY KEY (point_key, ts)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS history_warm (
    point_key TEXT NOT NULL,
    ts INTEGER NOT NULL,
    min REAL NOT NULL,
    max REAL NOT NULL,
    avg REAL NOT NULL,
    last REAL NOT NULL,
    sample_count INTEGER NOT NULL,
    PRIMARY KEY (point_key, ts)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS history_cold (
    point_key TEXT NOT NULL,
    ts INTEGER NOT NULL,
    min REAL NOT NULL,
    max REAL NOT NULL,
    avg REAL NOT NULL,
    last REAL NOT NULL,
    sample_count INTEGER NOT NULL,
    PRIMARY KEY (point_key, ts)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS history_archive (
    point_key TEXT NOT NULL,
    ts INTEGER NOT NULL,
    min REAL NOT NULL,
    max REAL NOT NULL,
    avg REAL NOT NULL,
    last REAL NOT NULL,
    sample_count INTEGER NOT NULL,
    PRIMARY KEY (point_key, ts)
) WITHOUT ROWID;
",
}];

fn run_sqlite_thread(db_path: &Path, mut cmd_rx: mpsc::UnboundedReceiver<HistoryCmd>) {
    let conn = rusqlite::Connection::open(db_path).expect("failed to open history database");
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
        .expect("failed to set pragmas");
    run_migrations(&conn, "history", MIGRATIONS).expect("history: schema migration failed");

    while let Some(cmd) = cmd_rx.blocking_recv() {
        match cmd {
            HistoryCmd::BatchInsert(rows) => {
                if let Ok(tx) = conn.unchecked_transaction() {
                    {
                        let mut stmt = tx
                            .prepare_cached(
                                "INSERT OR IGNORE INTO history_hot (point_key, ts, value) VALUES (?1, ?2, ?3)",
                            )
                            .unwrap();
                        for (point_key, ts, value) in &rows {
                            let _ = stmt.execute(rusqlite::params![point_key, ts, value]);
                        }
                    }
                    let _ = tx.commit();
                }
            }
            HistoryCmd::Query { query, reply } => {
                let result = execute_query(&conn, &query);
                let _ = reply.send(result);
            }
            HistoryCmd::TimeRange { point_key, reply } => {
                let result = get_time_range(&conn, &point_key);
                let _ = reply.send(result);
            }
            HistoryCmd::DownsampleHot => {
                downsample_hot_to_warm(&conn);
            }
            HistoryCmd::DownsampleWarm => {
                downsample_warm_to_cold(&conn);
            }
            HistoryCmd::DownsampleCold => {
                downsample_cold_to_archive(&conn);
            }
        }
    }
}

// ----------------------------------------------------------------
// Query execution — tier-aware
// ----------------------------------------------------------------

fn execute_query(
    conn: &rusqlite::Connection,
    q: &HistoryQuery,
) -> Result<HistoryResult, HistoryError> {
    let point_key = format!("{}:{}", q.device_id, q.point_id);
    let range_ms = q.end_ms - q.start_ms;

    // Pick tier based on range duration:
    // < 2 hours  → hot (full resolution)
    // < 7 days   → warm (1-min rollups)
    // < 6 months → cold (15-min rollups)
    // else       → archive (1-hour rollups)
    let samples = if range_ms <= 2 * 3600 * 1000 {
        // Try hot tier first, fall back to warm if not enough data
        let hot = query_hot(conn, &point_key, q.start_ms, q.end_ms)?;
        if hot.is_empty() {
            query_rollup(conn, "history_warm", &point_key, q.start_ms, q.end_ms)?
        } else {
            hot
        }
    } else if range_ms <= 7 * 24 * 3600 * 1000 {
        // Stitch hot + warm for recent portion
        let mut result = query_rollup(conn, "history_warm", &point_key, q.start_ms, q.end_ms)?;
        // Overlay hot data for the last 48h
        let hot_start = q.end_ms - HOT_RETENTION_MS;
        if hot_start < q.end_ms {
            let hot = query_hot(conn, &point_key, hot_start.max(q.start_ms), q.end_ms)?;
            if !hot.is_empty() {
                // Remove warm samples that overlap with hot data
                let hot_min_ts = hot.first().map(|s| s.timestamp_ms).unwrap_or(i64::MAX);
                result.retain(|s| s.timestamp_ms < hot_min_ts);
                result.extend(hot);
            }
        }
        result
    } else if range_ms <= 180 * 24 * 3600 * 1000 {
        query_rollup(conn, "history_cold", &point_key, q.start_ms, q.end_ms)?
    } else {
        query_rollup(conn, "history_archive", &point_key, q.start_ms, q.end_ms)?
    };

    // Cap results by downsampling on the fly (0 = uncapped for export)
    let cap = q.max_results.unwrap_or(MAX_QUERY_RESULTS);
    let samples = if cap > 0 && samples.len() as i64 > cap {
        downsample_results(samples, cap as usize)
    } else {
        samples
    };

    Ok(HistoryResult {
        device_id: q.device_id.clone(),
        point_id: q.point_id.clone(),
        samples,
    })
}

fn query_hot(
    conn: &rusqlite::Connection,
    point_key: &str,
    start_ms: i64,
    end_ms: i64,
) -> Result<Vec<HistorySample>, HistoryError> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT ts, value FROM history_hot
             WHERE point_key = ?1 AND ts BETWEEN ?2 AND ?3
             ORDER BY ts",
        )
        .map_err(|e| HistoryError::Db(e.to_string()))?;
    let rows = stmt
        .query_map(rusqlite::params![point_key, start_ms, end_ms], |row| {
            Ok(HistorySample {
                timestamp_ms: row.get(0)?,
                value: row.get(1)?,
            })
        })
        .map_err(|e| HistoryError::Db(e.to_string()))?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

fn query_rollup(
    conn: &rusqlite::Connection,
    table: &str,
    point_key: &str,
    start_ms: i64,
    end_ms: i64,
) -> Result<Vec<HistorySample>, HistoryError> {
    let sql = format!(
        "SELECT ts, avg FROM {table}
         WHERE point_key = ?1 AND ts BETWEEN ?2 AND ?3
         ORDER BY ts"
    );
    let mut stmt = conn
        .prepare_cached(&sql)
        .map_err(|e| HistoryError::Db(e.to_string()))?;
    let rows = stmt
        .query_map(rusqlite::params![point_key, start_ms, end_ms], |row| {
            Ok(HistorySample {
                timestamp_ms: row.get(0)?,
                value: row.get(1)?,
            })
        })
        .map_err(|e| HistoryError::Db(e.to_string()))?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Downsample a result set to fit within max_points by bucketing.
fn downsample_results(samples: Vec<HistorySample>, max_points: usize) -> Vec<HistorySample> {
    if samples.len() <= max_points || samples.is_empty() {
        return samples;
    }
    let first_ts = samples.first().unwrap().timestamp_ms;
    let last_ts = samples.last().unwrap().timestamp_ms;
    let bucket_size = ((last_ts - first_ts) / max_points as i64).max(1);

    let mut result: Vec<HistorySample> = Vec::with_capacity(max_points);
    let mut current_bucket = first_ts / bucket_size;
    let mut sum = 0.0;
    let mut count = 0;
    let mut bucket_ts = first_ts;

    for s in &samples {
        let b = s.timestamp_ms / bucket_size;
        if b != current_bucket && count > 0 {
            result.push(HistorySample {
                timestamp_ms: bucket_ts,
                value: sum / count as f64,
            });
            sum = 0.0;
            count = 0;
            current_bucket = b;
            bucket_ts = s.timestamp_ms;
        }
        if count == 0 {
            bucket_ts = s.timestamp_ms;
        }
        sum += s.value;
        count += 1;
    }
    if count > 0 {
        result.push(HistorySample {
            timestamp_ms: bucket_ts,
            value: sum / count as f64,
        });
    }
    result
}

fn get_time_range(conn: &rusqlite::Connection, point_key: &str) -> Option<(i64, i64)> {
    // Check across all tiers
    let tables = [
        "history_hot",
        "history_warm",
        "history_cold",
        "history_archive",
    ];
    let mut global_min: Option<i64> = None;
    let mut global_max: Option<i64> = None;

    for table in &tables {
        let sql = format!("SELECT MIN(ts), MAX(ts) FROM {table} WHERE point_key = ?1");
        if let Ok((Some(min), Some(max))) =
            conn.query_row(&sql, rusqlite::params![point_key], |row| {
                let min: Option<i64> = row.get(0)?;
                let max: Option<i64> = row.get(1)?;
                Ok((min, max))
            })
        {
            global_min = Some(global_min.map_or(min, |g: i64| g.min(min)));
            global_max = Some(global_max.map_or(max, |g: i64| g.max(max)));
        }
    }

    global_min.zip(global_max)
}

// ----------------------------------------------------------------
// Downsampling: hot → warm → cold → archive
// ----------------------------------------------------------------

fn downsample_hot_to_warm(conn: &rusqlite::Connection) {
    let cutoff = now_ms() - HOT_RETENTION_MS;
    rollup_and_delete(
        conn,
        "history_hot",
        "history_warm",
        WARM_INTERVAL_MS,
        cutoff,
    );
}

fn downsample_warm_to_cold(conn: &rusqlite::Connection) {
    let cutoff = now_ms() - WARM_RETENTION_MS;
    rollup_and_delete(
        conn,
        "history_warm",
        "history_cold",
        COLD_INTERVAL_MS,
        cutoff,
    );
}

fn downsample_cold_to_archive(conn: &rusqlite::Connection) {
    let cutoff = now_ms() - COLD_RETENTION_MS;
    rollup_and_delete(
        conn,
        "history_cold",
        "history_archive",
        ARCHIVE_INTERVAL_MS,
        cutoff,
    );
}

/// Roll up records from `src_table` older than `cutoff` into `dst_table` at `interval_ms` buckets,
/// then delete the source records.
fn rollup_and_delete(
    conn: &rusqlite::Connection,
    src_table: &str,
    dst_table: &str,
    interval_ms: i64,
    cutoff: i64,
) {
    // Source is hot tier (raw value column) vs rollup tiers (min/max/avg/last/sample_count)
    let is_hot_source = src_table == "history_hot";

    let Ok(tx) = conn.unchecked_transaction() else {
        return;
    };

    if is_hot_source {
        // Rollup raw values into buckets
        let insert_sql = format!(
            "INSERT OR REPLACE INTO {dst_table} (point_key, ts, min, max, avg, last, sample_count)
             SELECT point_key,
                    (ts / ?1) * ?1 AS bucket_ts,
                    MIN(value),
                    MAX(value),
                    AVG(value),
                    -- last value in each bucket (max ts)
                    (SELECT h2.value FROM {src_table} h2
                     WHERE h2.point_key = {src_table}.point_key
                       AND (h2.ts / ?1) * ?1 = (ts / ?1) * ?1
                     ORDER BY h2.ts DESC LIMIT 1),
                    COUNT(*)
             FROM {src_table}
             WHERE ts < ?2
             GROUP BY point_key, bucket_ts"
        );
        let _ = tx.execute(&insert_sql, rusqlite::params![interval_ms, cutoff]);
    } else {
        // Rollup rollup records into larger buckets
        let insert_sql = format!(
            "INSERT OR REPLACE INTO {dst_table} (point_key, ts, min, max, avg, last, sample_count)
             SELECT point_key,
                    (ts / ?1) * ?1 AS bucket_ts,
                    MIN(min),
                    MAX(max),
                    SUM(avg * sample_count) / SUM(sample_count),
                    -- last: from the sub-record with highest ts
                    (SELECT h2.last FROM {src_table} h2
                     WHERE h2.point_key = {src_table}.point_key
                       AND (h2.ts / ?1) * ?1 = (ts / ?1) * ?1
                     ORDER BY h2.ts DESC LIMIT 1),
                    SUM(sample_count)
             FROM {src_table}
             WHERE ts < ?2
             GROUP BY point_key, bucket_ts"
        );
        let _ = tx.execute(&insert_sql, rusqlite::params![interval_ms, cutoff]);
    }

    // Delete rolled-up source records
    let delete_sql = format!("DELETE FROM {src_table} WHERE ts < ?1");
    let _ = tx.execute(&delete_sql, rusqlite::params![cutoff]);

    let _ = tx.commit();
}

// ----------------------------------------------------------------
// COV tracking per point
// ----------------------------------------------------------------

struct TrackedPoint {
    point_key: String,
    device_id: String,
    point_id: String,
    cov_threshold: f64,
    /// True for binary/multistate (any-change semantics).
    any_change: bool,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// Build tracked points from loaded devices. Every point is tracked unless excluded.
fn build_tracked_points(devices: &[LoadedDevice]) -> Vec<TrackedPoint> {
    let mut tracked = Vec::new();
    for dev in devices {
        for pt in &dev.profile.points {
            if pt.history_exclude {
                continue;
            }

            let any_change = matches!(pt.kind, PointKind::Binary | PointKind::Multistate);

            // Resolve COV threshold:
            // 1. Point-level cov_increment
            // 2. BACnet protocol cov_increment
            // 3. Default based on kind
            let cov_threshold = pt
                .cov_increment
                .or_else(|| {
                    pt.protocols
                        .as_ref()
                        .and_then(|p| p.bacnet.as_ref())
                        .and_then(|b| b.cov_increment)
                })
                .unwrap_or_else(|| {
                    if any_change {
                        0.0 // any change triggers for binary/multistate
                    } else {
                        // 0.5% of constraint range, or 0.5 absolute
                        pt.constraints
                            .as_ref()
                            .and_then(|c| {
                                let min = c.min?;
                                let max = c.max?;
                                Some((max - min) * 0.005)
                            })
                            .unwrap_or(0.5)
                    }
                });

            tracked.push(TrackedPoint {
                point_key: format!("{}:{}", dev.instance_id, pt.id),
                device_id: dev.instance_id.clone(),
                point_id: pt.id.clone(),
                cov_threshold,
                any_change,
            });
        }
    }
    tracked
}

// ----------------------------------------------------------------
// History collector — COV + heartbeat + downsampling
// ----------------------------------------------------------------

/// Start the history collection system. Returns a `HistoryStore` handle for queries.
pub fn start_history_collector(store: &PointStore, devices: &[LoadedDevice]) -> HistoryStore {
    let db_dir = Path::new("data");
    if !db_dir.exists() {
        std::fs::create_dir_all(db_dir).expect("failed to create data directory");
    }
    start_history_collector_with_path(store, devices, &db_dir.join("history.db"), None)
}

pub fn start_history_collector_with_path(
    store: &PointStore,
    devices: &[LoadedDevice],
    db_path: &Path,
    shutdown: Option<tokio_util::sync::CancellationToken>,
) -> HistoryStore {
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

    // Start SQLite thread
    let path_clone = db_path.to_path_buf();
    std::thread::Builder::new()
        .name("history-sqlite".into())
        .spawn(move || run_sqlite_thread(&path_clone, cmd_rx))
        .expect("failed to spawn SQLite thread");

    let tracked = build_tracked_points(devices);

    // COV loop — subscribes to all point changes, buffers, flushes every FLUSH_INTERVAL_SECS
    {
        let mut cov_rx = store.subscribe_history();
        let cov_tx = cmd_tx.clone();
        let shutdown_token = shutdown.clone();

        // Build lookup: (device_id, point_id) → (threshold, any_change, point_key)
        let mut thresholds: HashMap<(String, String), (f64, bool, String)> = HashMap::new();
        for tp in &tracked {
            thresholds.insert(
                (tp.device_id.clone(), tp.point_id.clone()),
                (tp.cov_threshold, tp.any_change, tp.point_key.clone()),
            );
        }

        tokio::spawn(async move {
            let mut last_values: HashMap<String, f64> = HashMap::new();
            let mut last_record_time: HashMap<String, i64> = HashMap::new();
            let mut buffer: Vec<(String, i64, f64)> = Vec::new();

            let mut flush_interval =
                tokio::time::interval(tokio::time::Duration::from_secs(FLUSH_INTERVAL_SECS));
            flush_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    result = cov_rx.recv() => {
                        match result {
                            Ok((key, value)) => {
                                let lookup = (key.device_instance_id.clone(), key.point_id.clone());
                                if let Some((threshold, any_change, ref point_key)) = thresholds.get(&lookup) {
                                    let f = value.as_f64();
                                    let should_log = match last_values.get(point_key.as_str()) {
                                        Some(&last) => {
                                            if *any_change {
                                                (f - last).abs() > f64::EPSILON
                                            } else {
                                                (f - last).abs() >= *threshold
                                            }
                                        }
                                        None => true, // First value always recorded
                                    };
                                    if should_log {
                                        let ts = now_ms();
                                        last_values.insert(point_key.clone(), f);
                                        last_record_time.insert(point_key.clone(), ts);
                                        buffer.push((point_key.clone(), ts, f));
                                    }
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                    _ = flush_interval.tick() => {
                        if !buffer.is_empty() {
                            let batch: Vec<_> = std::mem::take(&mut buffer);
                            let _ = cov_tx.send(HistoryCmd::BatchInsert(batch));
                        }
                    }
                    _ = async {
                        match &shutdown_token {
                            Some(t) => t.cancelled().await,
                            None => std::future::pending().await,
                        }
                    } => {
                        tracing::info!("History COV collector shutting down");
                        break;
                    }
                }
            }
        });
    }

    // Heartbeat loop — poll silent points every HEARTBEAT_SECS
    {
        let hb_store = store.clone();
        let hb_tx = cmd_tx.clone();
        let shutdown_token = shutdown.clone();
        let hb_tracked: Vec<(String, String, String)> = tracked
            .iter()
            .map(|tp| {
                (
                    tp.point_key.clone(),
                    tp.device_id.clone(),
                    tp.point_id.clone(),
                )
            })
            .collect();

        tokio::spawn(async move {
            let mut ticker =
                tokio::time::interval(tokio::time::Duration::from_secs(HEARTBEAT_SECS));
            // Skip the immediate first tick
            ticker.tick().await;

            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        let ts = now_ms();
                        let mut batch = Vec::new();

                        for (point_key, device_id, point_id) in &hb_tracked {
                            let key = PointKey {
                                device_instance_id: device_id.clone(),
                                point_id: point_id.clone(),
                            };
                            if let Some(tv) = hb_store.get(&key) {
                                batch.push((point_key.clone(), ts, tv.value.as_f64()));
                            }
                        }

                        if !batch.is_empty() {
                            let _ = hb_tx.send(HistoryCmd::BatchInsert(batch));
                        }
                    }
                    _ = async {
                        match &shutdown_token {
                            Some(t) => t.cancelled().await,
                            None => std::future::pending().await,
                        }
                    } => {
                        tracing::info!("History heartbeat loop shutting down");
                        break;
                    }
                }
            }
        });
    }

    // Downsampling loops
    {
        let ds_tx = cmd_tx.clone();
        let shutdown_token = shutdown.clone();
        tokio::spawn(async move {
            let mut hot_ticker = tokio::time::interval(tokio::time::Duration::from_secs(
                DOWNSAMPLE_HOT_INTERVAL_SECS,
            ));
            hot_ticker.tick().await; // skip immediate
            loop {
                tokio::select! {
                    _ = hot_ticker.tick() => {
                        let _ = ds_tx.send(HistoryCmd::DownsampleHot);
                    }
                    _ = async {
                        match &shutdown_token {
                            Some(t) => t.cancelled().await,
                            None => std::future::pending().await,
                        }
                    } => {
                        tracing::info!("History hot downsample loop shutting down");
                        break;
                    }
                }
            }
        });
    }
    {
        let ds_tx = cmd_tx.clone();
        let shutdown_token = shutdown.clone();
        tokio::spawn(async move {
            let mut warm_ticker = tokio::time::interval(tokio::time::Duration::from_secs(
                DOWNSAMPLE_WARM_INTERVAL_SECS,
            ));
            warm_ticker.tick().await;
            loop {
                tokio::select! {
                    _ = warm_ticker.tick() => {
                        let _ = ds_tx.send(HistoryCmd::DownsampleWarm);
                    }
                    _ = async {
                        match &shutdown_token {
                            Some(t) => t.cancelled().await,
                            None => std::future::pending().await,
                        }
                    } => {
                        tracing::info!("History warm downsample loop shutting down");
                        break;
                    }
                }
            }
        });
    }
    {
        let ds_tx = cmd_tx.clone();
        let shutdown_token = shutdown;
        tokio::spawn(async move {
            let mut cold_ticker = tokio::time::interval(tokio::time::Duration::from_secs(
                DOWNSAMPLE_COLD_INTERVAL_SECS,
            ));
            cold_ticker.tick().await;
            loop {
                tokio::select! {
                    _ = cold_ticker.tick() => {
                        let _ = ds_tx.send(HistoryCmd::DownsampleCold);
                    }
                    _ = async {
                        match &shutdown_token {
                            Some(t) => t.cancelled().await,
                            None => std::future::pending().await,
                        }
                    } => {
                        tracing::info!("History cold downsample loop shutting down");
                        break;
                    }
                }
            }
        });
    }

    HistoryStore { cmd_tx }
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::profile::PointValue;
    use std::sync::atomic::{AtomicU32, Ordering};

    static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_db_path() -> std::path::PathBuf {
        let n = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join("opencrate-test");
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(format!("history-test-{n}.db"))
    }

    #[tokio::test]
    async fn insert_and_query() {
        let db_path = temp_db_path();
        let store = PointStore::new();
        let json = std::fs::read_to_string("profiles/ahu-single-duct.json").unwrap();
        let profile: crate::config::profile::DeviceProfile = serde_json::from_str(&json).unwrap();
        store.initialize_from_profile("ahu-1", &profile);

        let devices = vec![LoadedDevice {
            instance_id: "ahu-1".into(),
            profile,
        }];

        let history = start_history_collector_with_path(&store, &devices, &db_path, None);

        let ts = now_ms();
        // Write a value that exceeds the COV threshold
        store.set(
            PointKey {
                device_instance_id: "ahu-1".into(),
                point_id: "dat".into(),
            },
            PointValue::Float(72.5),
        );

        // Wait for COV detection + batch flush (1s interval + margin)
        tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;

        let result = history
            .query(HistoryQuery {
                device_id: "ahu-1".into(),
                point_id: "dat".into(),
                start_ms: ts - 1000,
                end_ms: ts + 60_000,
                max_results: None,
            })
            .await;

        assert!(result.is_ok());
        let r = result.unwrap();
        assert!(!r.samples.is_empty(), "should have recorded the COV change");
        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn time_range_empty() {
        let db_path = temp_db_path();
        let store = PointStore::new();
        let history = start_history_collector_with_path(&store, &[], &db_path, None);

        let range = history.time_range("nonexistent", "none").await;
        assert!(range.is_none());
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn downsample_results_works() {
        let samples: Vec<HistorySample> = (0..1000)
            .map(|i| HistorySample {
                timestamp_ms: i * 1000,
                value: i as f64,
            })
            .collect();
        let result = downsample_results(samples, 100);
        // Should be roughly the target count (±10% due to bucketing)
        assert!(result.len() <= 110, "got {} results", result.len());
        assert!(result.len() >= 90, "got {} results", result.len());
        // Should be monotonically increasing timestamps
        for w in result.windows(2) {
            assert!(w[1].timestamp_ms > w[0].timestamp_ms);
        }
    }

    #[test]
    fn build_tracked_points_all_points() {
        let json = std::fs::read_to_string("profiles/ahu-single-duct.json").unwrap();
        let profile: crate::config::profile::DeviceProfile = serde_json::from_str(&json).unwrap();
        let point_count = profile.points.len();

        let devices = vec![LoadedDevice {
            instance_id: "ahu-1".into(),
            profile,
        }];
        let tracked = build_tracked_points(&devices);
        // All points should be tracked (none excluded)
        assert_eq!(tracked.len(), point_count);
    }
}
