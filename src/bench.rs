//! In-process bench harness for `bms-stored --bench`.
//!
//! Item C.5 in `docs/v1-criteria.md`. Boots a temp project + storage
//! runtime, materializes N synthetic points, drives W updates per
//! second for D seconds, runs M concurrent history queries, and
//! reports throughput + latency percentiles.
//!
//! Defaults are tuned to the v1.0 baseline criterion (10 000 points
//! at 1 Hz for 60 s, 100 history queries). Tune via env vars:
//!
//! - `BMS_BENCH_POINTS` (default 10000)
//! - `BMS_BENCH_DURATION_SECS` (default 60)
//! - `BMS_BENCH_QUERIES` (default 100)
//!
//! CPU% and RSS are NOT measured (need OS-specific APIs); the bench
//! reports the in-process latency numbers used to gate the v1.0
//! perf criterion. Use `top -pid $(pgrep bms-stored)` while the bench
//! runs for the CPU/memory targets.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bms_store_storage::config::profile::PointValue;
use bms_store_storage::store::point_store::PointKey;

/// Run the bench harness. Returns Ok(()) on completion regardless of
/// whether targets were met — the user reads the printed numbers.
pub async fn run() -> Result<(), String> {
    let n_points: usize = env_or("BMS_BENCH_POINTS", 10_000);
    let duration_secs: u64 = env_or("BMS_BENCH_DURATION_SECS", 60);
    let n_queries: usize = env_or("BMS_BENCH_QUERIES", 100);

    println!(
        "[bench] starting · points={n_points} duration={duration_secs}s queries={n_queries}"
    );

    // ---- Boot ------------------------------------------------------
    let tmp = tempdir_or("bench-project")?;
    write_minimal_project(&tmp)?;
    let storage = bms_store_storage::boot::boot_project(tmp.clone())
        .await
        .map_err(|e| format!("boot_project: {e}"))?;
    let (_bridges, _report) = bms_store_bridges::boot::boot_bridges(&storage)
        .await
        .map_err(|e| format!("boot_bridges: {e}"))?;
    println!("[bench] runtime booted");

    // ---- Materialize points ---------------------------------------
    let device_id = "bench-dev";
    let keys: Vec<PointKey> = (0..n_points)
        .map(|i| PointKey {
            device_instance_id: device_id.into(),
            point_id: format!("p{i:06}"),
        })
        .collect();
    for k in &keys {
        storage
            .point_store
            .insert_default(k.clone(), PointValue::Float(0.0));
    }
    println!("[bench] materialized {n_points} points");

    // ---- Write throughput + latency -------------------------------
    let mut latencies_ns: Vec<u64> = Vec::with_capacity(n_points * duration_secs as usize);
    let total_target = n_points * duration_secs as usize;
    let start = Instant::now();
    let interval = Duration::from_secs(1);
    for tick in 0..duration_secs {
        let tick_start = Instant::now();
        let v = tick as f64;
        for k in &keys {
            let t0 = Instant::now();
            storage.point_store.set(k.clone(), PointValue::Float(v));
            latencies_ns.push(t0.elapsed().as_nanos() as u64);
        }
        // Pace to ~1 Hz per point — sleep the remainder of this 1s tick.
        let elapsed = tick_start.elapsed();
        if elapsed < interval {
            tokio::time::sleep(interval - elapsed).await;
        } else {
            // Falling behind — log and continue.
            eprintln!(
                "[bench] tick {tick} took {elapsed:?} > 1s; throughput target missed"
            );
        }
    }
    let total_elapsed = start.elapsed();
    let throughput = total_target as f64 / total_elapsed.as_secs_f64();
    println!(
        "[bench] writes complete · {} sets in {:?} · throughput={:.0} sets/s",
        total_target, total_elapsed, throughput
    );
    print_latency("write set", &mut latencies_ns);

    // ---- History query latency -------------------------------------
    // Each point gets `duration_secs` samples. Backfill them so the
    // history table is populated.
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis() as i64;
    let mut samples = Vec::with_capacity(n_points * duration_secs as usize);
    for (idx, k) in keys.iter().enumerate() {
        let key = format!("{}:{}", k.device_instance_id, k.point_id);
        for tick in 0..duration_secs as i64 {
            samples.push((key.clone(), now_ms - tick * 1000, idx as f64 + tick as f64));
        }
    }
    storage.history_store.backfill(samples).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut q_latencies_ns: Vec<u64> = Vec::with_capacity(n_queries);
    let history_arc = Arc::new(storage.history_store.clone());
    let queried_keys: Vec<PointKey> = keys
        .iter()
        .step_by(keys.len().max(1) / n_queries.max(1))
        .take(n_queries)
        .cloned()
        .collect();
    for k in &queried_keys {
        let t0 = Instant::now();
        let _ = history_arc
            .query(bms_store_storage::store::history_store::HistoryQuery {
                device_id: k.device_instance_id.clone(),
                point_id: k.point_id.clone(),
                start_ms: now_ms - 86_400_000,
                end_ms: now_ms + 1000,
                max_results: Some(1000),
            })
            .await
            .map_err(|e| format!("history query: {e}"))?;
        q_latencies_ns.push(t0.elapsed().as_nanos() as u64);
    }
    print_latency("history query", &mut q_latencies_ns);

    // ---- Targets summary ------------------------------------------
    let p99_query_ms = pct(&mut q_latencies_ns.clone(), 99) as f64 / 1_000_000.0;
    let target_query_p99_ms = 200.0;
    println!(
        "[bench] history query p99 {p99_query_ms:.1}ms (target ≤{:.0}ms): {}",
        target_query_p99_ms,
        if p99_query_ms <= target_query_p99_ms { "PASS" } else { "FAIL" }
    );

    storage.shutdown.cancel();
    let _ = std::fs::remove_dir_all(&tmp);
    println!("[bench] done");
    Ok(())
}

fn pct(latencies_ns: &mut [u64], p: u8) -> u64 {
    if latencies_ns.is_empty() {
        return 0;
    }
    latencies_ns.sort_unstable();
    let idx = ((p as usize * latencies_ns.len()) / 100).min(latencies_ns.len() - 1);
    latencies_ns[idx]
}

fn print_latency(label: &str, latencies_ns: &mut [u64]) {
    if latencies_ns.is_empty() {
        println!("[bench] {label}: no samples");
        return;
    }
    let mean_ns: u64 = latencies_ns.iter().sum::<u64>() / latencies_ns.len() as u64;
    let p50 = pct(latencies_ns, 50) as f64 / 1000.0;
    let p95 = pct(latencies_ns, 95) as f64 / 1000.0;
    let p99 = pct(latencies_ns, 99) as f64 / 1000.0;
    let max = latencies_ns.last().copied().unwrap_or(0) as f64 / 1000.0;
    println!(
        "[bench] {label} ({} samples): mean={:.1}µs p50={:.1}µs p95={:.1}µs p99={:.1}µs max={:.1}µs",
        latencies_ns.len(),
        mean_ns as f64 / 1000.0,
        p50,
        p95,
        p99,
        max
    );
}

fn env_or<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

fn tempdir_or(label: &str) -> Result<PathBuf, String> {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let path = std::env::temp_dir().join(format!("bms-stored-{label}-{pid}-{nanos}"));
    std::fs::create_dir_all(&path).map_err(|e| format!("create temp dir {}: {e}", path.display()))?;
    Ok(path)
}

fn write_minimal_project(root: &PathBuf) -> Result<(), String> {
    let scenario = serde_json::json!({
        "scenario": { "id": "bench", "name": "bms-stored bench" },
        "settings": { "tick_rate_ms": 1000, "realtime": false },
        "devices": []
    });
    let project = serde_json::json!({
        "id": "bench",
        "name": "bms-stored bench",
        "description": "Synthesized",
        "created_ms": 0,
        "version": "0.1.0"
    });
    std::fs::write(
        root.join("scenario.json"),
        serde_json::to_vec_pretty(&scenario).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("write scenario.json: {e}"))?;
    std::fs::write(
        root.join("project.json"),
        serde_json::to_vec_pretty(&project).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("write project.json: {e}"))?;
    std::fs::create_dir_all(root.join("profiles")).map_err(|e| format!("mkdir: {e}"))?;
    Ok(())
}
