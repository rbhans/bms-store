use std::path::Path;

use rusqlite::Connection;

use super::db::{create_atlas_schema, AtlasDb};
use super::model::AtlasStats;

/// Result of checking for Atlas updates.
#[derive(Debug, Clone)]
pub struct UpdateCheck {
    pub local: Option<AtlasStats>,
    pub remote: Option<AtlasStats>,
    pub update_available: bool,
}

/// Errors that can occur during Atlas sync operations.
#[derive(Debug, thiserror::Error)]
pub enum AtlasError {
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("API error: {0}")]
    Api(String),
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

const ATLAS_API_BASE: &str = "https://bas-atlas.com/api";

/// Check if a remote update is available by comparing stats.
pub async fn check_for_updates(local: Option<&AtlasDb>) -> Result<UpdateCheck, AtlasError> {
    let local_stats = local.and_then(|db| db.stats().ok());

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{ATLAS_API_BASE}/atlas/stats"))
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(AtlasError::Api(format!("HTTP {}", resp.status())));
    }

    let remote_stats: AtlasStats = resp.json().await?;

    let update_available = match &local_stats {
        Some(local) => {
            remote_stats.total_points != local.total_points
                || remote_stats.total_equipment != local.total_equipment
                || remote_stats.version != local.version
        }
        None => true, // No local data → update available
    };

    Ok(UpdateCheck {
        local: local_stats,
        remote: Some(remote_stats),
        update_available,
    })
}

/// Download the full Atlas dataset from the API and write to a local SQLite database.
/// The progress callback receives values from 0.0 to 1.0.
pub async fn download_atlas(
    db_path: &Path,
    mut progress: impl FnMut(f32),
) -> Result<AtlasStats, AtlasError> {
    let client = reqwest::Client::new();

    // Use a temporary file, rename atomically on success
    let tmp_path = db_path.with_extension("db.tmp");

    // Create a fresh database
    {
        // Remove tmp if it exists from a previous failed attempt
        let _ = std::fs::remove_file(&tmp_path);

        let conn = Connection::open(&tmp_path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        create_atlas_schema(&conn)?;
    }

    progress(0.02);

    // Fetch equipment list
    let equip_resp = client
        .get(format!("{ATLAS_API_BASE}/atlas/equipment"))
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await?;
    if !equip_resp.status().is_success() {
        return Err(AtlasError::Api(format!(
            "Equipment list: HTTP {}",
            equip_resp.status()
        )));
    }
    let equip_list: Vec<serde_json::Value> = equip_resp.json().await?;
    let equip_count = equip_list.len() as f32;

    progress(0.05);

    // Fetch equipment details + write to DB
    {
        let conn = Connection::open(&tmp_path)?;
        let mut insert_equip = conn.prepare(
            "INSERT OR REPLACE INTO atlas_equipment (id, name, abbreviation, category, haystack_tags, brick) VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
        )?;
        let mut insert_alias = conn.prepare(
            "INSERT OR IGNORE INTO atlas_equip_aliases (alias, equip_id) VALUES (?1, ?2)",
        )?;
        let mut insert_typical = conn.prepare(
            "INSERT OR IGNORE INTO atlas_equip_typical_points (equip_id, point_id) VALUES (?1, ?2)",
        )?;

        conn.execute("BEGIN", [])?;

        for (i, equip_summary) in equip_list.iter().enumerate() {
            let equip_id = equip_summary["id"].as_str().unwrap_or_default();
            if equip_id.is_empty() {
                continue;
            }

            // Fetch detail
            let detail_resp = client
                .get(format!(
                    "{ATLAS_API_BASE}/atlas/equipment-detail/{equip_id}"
                ))
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await;

            let detail = match detail_resp {
                Ok(r) if r.status().is_success() => r.json::<serde_json::Value>().await.ok(),
                _ => None,
            };

            let detail = detail.as_ref().unwrap_or(equip_summary);

            let name = detail["name"].as_str().unwrap_or_default();
            let abbr = detail["abbreviation"].as_str();
            let category = detail["category"].as_str().unwrap_or_default();
            let haystack = detail["haystack_tags"].as_str().unwrap_or_default();
            let brick = detail["brick"].as_str();

            insert_equip.execute(rusqlite::params![
                equip_id, name, abbr, category, haystack, brick
            ])?;

            // Insert aliases
            if let Some(aliases) = detail["aliases"].as_array() {
                for alias_val in aliases {
                    if let Some(alias) = alias_val.as_str() {
                        let norm = normalize_alias(alias);
                        if !norm.is_empty() {
                            insert_alias.execute(rusqlite::params![norm, equip_id])?;
                        }
                    }
                }
            }
            // Also insert the name itself as an alias
            let norm_name = normalize_alias(name);
            if !norm_name.is_empty() {
                insert_alias.execute(rusqlite::params![norm_name, equip_id])?;
            }

            // Insert typical points
            if let Some(typical) = detail["typical_points"].as_array() {
                for tp in typical {
                    if let Some(pid) = tp.as_str() {
                        insert_typical.execute(rusqlite::params![equip_id, pid])?;
                    }
                }
            }

            progress(0.05 + 0.40 * (i as f32 / equip_count));
        }

        conn.execute("COMMIT", [])?;
    }

    progress(0.45);

    // Fetch points list
    let points_resp = client
        .get(format!("{ATLAS_API_BASE}/atlas/points"))
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await?;
    if !points_resp.status().is_success() {
        return Err(AtlasError::Api(format!(
            "Points list: HTTP {}",
            points_resp.status()
        )));
    }
    let points_list: Vec<serde_json::Value> = points_resp.json().await?;
    let point_count = points_list.len() as f32;

    progress(0.50);

    // Fetch point details + write to DB
    {
        let conn = Connection::open(&tmp_path)?;
        let mut insert_point = conn.prepare(
            "INSERT OR REPLACE INTO atlas_points (id, name, category, haystack_tags, kind, point_function, units, brick) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
        )?;
        let mut insert_alias = conn.prepare(
            "INSERT OR IGNORE INTO atlas_point_aliases (alias, point_id) VALUES (?1, ?2)",
        )?;

        conn.execute("BEGIN", [])?;

        for (i, point_summary) in points_list.iter().enumerate() {
            let point_id = point_summary["id"].as_str().unwrap_or_default();
            if point_id.is_empty() {
                continue;
            }

            // Fetch detail
            let detail_resp = client
                .get(format!("{ATLAS_API_BASE}/atlas/points/{point_id}"))
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await;

            let detail = match detail_resp {
                Ok(r) if r.status().is_success() => r.json::<serde_json::Value>().await.ok(),
                _ => None,
            };

            let detail = detail.as_ref().unwrap_or(point_summary);

            let name = detail["name"].as_str().unwrap_or_default();
            let category = detail["category"].as_str().unwrap_or_default();
            let haystack = detail["haystack_tags"].as_str().unwrap_or_default();
            let kind = detail["kind"].as_str().unwrap_or("Number");
            let point_fn = detail["point_function"].as_str().unwrap_or("sensor");
            let units = detail["units"].as_str();
            let brick = detail["brick"].as_str();

            insert_point.execute(rusqlite::params![
                point_id, name, category, haystack, kind, point_fn, units, brick
            ])?;

            // Insert aliases
            if let Some(aliases) = detail["aliases"].as_array() {
                for alias_val in aliases {
                    if let Some(alias) = alias_val.as_str() {
                        let norm = normalize_alias(alias);
                        if !norm.is_empty() {
                            insert_alias.execute(rusqlite::params![norm, point_id])?;
                        }
                    }
                }
            }
            // Insert name as alias too
            let norm_name = normalize_alias(name);
            if !norm_name.is_empty() {
                insert_alias.execute(rusqlite::params![norm_name, point_id])?;
            }

            progress(0.50 + 0.45 * (i as f32 / point_count));
        }

        conn.execute("COMMIT", [])?;
    }

    // Write metadata
    {
        let conn = Connection::open(&tmp_path)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        conn.execute(
            "INSERT OR REPLACE INTO atlas_meta (key, value) VALUES ('version', '1.0')",
            [],
        )?;
        conn.execute(
            "INSERT OR REPLACE INTO atlas_meta (key, value) VALUES ('updated_ms', ?1)",
            rusqlite::params![now.to_string()],
        )?;
    }

    progress(0.98);

    // Atomic rename
    std::fs::rename(&tmp_path, db_path)?;

    progress(1.0);

    // Return stats from the newly created database
    let db = AtlasDb::open(db_path)?;
    let stats = db.stats()?;
    Ok(stats)
}

/// Remove the Atlas database file.
pub fn remove_atlas(db_path: &Path) -> Result<(), AtlasError> {
    if db_path.exists() {
        std::fs::remove_file(db_path)?;
        // Also remove WAL/SHM files
        let wal = db_path.with_extension("db-wal");
        let shm = db_path.with_extension("db-shm");
        let _ = std::fs::remove_file(wal);
        let _ = std::fs::remove_file(shm);
    }
    Ok(())
}

fn normalize_alias(input: &str) -> String {
    let lower = input.to_lowercase();
    let cleaned: String = lower
        .chars()
        .map(|c| {
            if c == '-' || c == '_' || c == '.' {
                ' '
            } else {
                c
            }
        })
        .collect();
    cleaned.split_whitespace().collect::<Vec<&str>>().join(" ")
}
