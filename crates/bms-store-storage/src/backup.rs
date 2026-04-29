//! Scheduled backup system.
//!
//! Periodically exports the project to `.ocrate` archives with configurable
//! retention (keep N most recent backups).

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Configuration for automatic backups.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    /// Whether automatic backups are enabled.
    pub enabled: bool,
    /// Interval between backups in hours.
    pub interval_hours: u64,
    /// Number of backups to retain (older ones are deleted).
    pub retention_count: usize,
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_hours: 24,
            retention_count: 7,
        }
    }
}

/// Metadata about an existing backup file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupInfo {
    pub filename: String,
    pub size_bytes: u64,
    pub created_ms: i64,
}

/// The backup scheduler — manages periodic backups for a project.
pub struct BackupScheduler {
    project_id: String,
    backup_dir: PathBuf,
    config_path: PathBuf,
    config: BackupConfig,
    shutdown: tokio_util::sync::CancellationToken,
}

impl BackupScheduler {
    /// Create a new backup scheduler for the given project.
    pub fn new(project_id: &str, data_dir: &Path) -> Self {
        let backup_dir = backup_directory();
        let config_path = data_dir.join("backup_config.json");
        let config = load_config(&config_path).unwrap_or_default();

        Self {
            project_id: project_id.to_string(),
            backup_dir,
            config_path,
            config,
            shutdown: tokio_util::sync::CancellationToken::new(),
        }
    }

    /// Get the current backup configuration.
    pub fn config(&self) -> &BackupConfig {
        &self.config
    }

    /// Update the backup configuration, persist it, and restart the scheduler task.
    pub fn set_config(&mut self, config: BackupConfig) {
        // Cancel the old task
        self.shutdown.cancel();
        // Replace with a fresh token for the new task
        self.shutdown = tokio_util::sync::CancellationToken::new();
        self.config = config;
        save_config(&self.config_path, &self.config).ok();
        // Start a new task with the updated config
        self.start();
    }

    /// Trigger an immediate backup. Returns the backup file path on success.
    pub fn backup_now(&self) -> Result<PathBuf, String> {
        std::fs::create_dir_all(&self.backup_dir).map_err(|e| e.to_string())?;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let filename = format!("backup-{}-{}.ocrate", self.project_id, timestamp);
        let dest = self.backup_dir.join(&filename);

        crate::project::export_project(&self.project_id, &dest).map_err(|e| e.to_string())?;

        // Enforce retention
        self.enforce_retention();

        tracing::info!(file = %filename, "Backup created");
        Ok(dest)
    }

    /// List available backups for this project, most recent first.
    pub fn list_backups(&self) -> Vec<BackupInfo> {
        let prefix = format!("backup-{}-", self.project_id);
        let mut backups = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&self.backup_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with(&prefix) && name.ends_with(".ocrate") {
                    if let Ok(meta) = entry.metadata() {
                        let created_ms = meta
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_millis() as i64)
                            .unwrap_or(0);
                        backups.push(BackupInfo {
                            filename: name,
                            size_bytes: meta.len(),
                            created_ms,
                        });
                    }
                }
            }
        }

        backups.sort_by(|a, b| b.created_ms.cmp(&a.created_ms));
        backups
    }

    /// Start the periodic backup task. Returns immediately.
    pub fn start(&self) {
        if !self.config.enabled || self.config.interval_hours == 0 {
            tracing::info!("Backup scheduler disabled");
            return;
        }

        let interval = Duration::from_secs(self.config.interval_hours * 3600);
        let project_id = self.project_id.clone();
        let backup_dir = self.backup_dir.clone();
        let retention = self.config.retention_count;
        let token = self.shutdown.clone();

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.tick().await; // skip first immediate tick

            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        if let Err(e) = run_backup(&project_id, &backup_dir, retention) {
                            tracing::error!(error = %e, "Scheduled backup failed");
                        }
                    }
                    _ = token.cancelled() => {
                        tracing::info!("Backup scheduler stopped");
                        break;
                    }
                }
            }
        });

        tracing::info!(
            interval_hours = self.config.interval_hours,
            retention = self.config.retention_count,
            "Backup scheduler started"
        );
    }

    /// Stop the periodic backup task.
    pub fn stop(&self) {
        self.shutdown.cancel();
    }

    fn enforce_retention(&self) {
        let backups = self.list_backups();
        if backups.len() > self.config.retention_count {
            for old in &backups[self.config.retention_count..] {
                let path = self.backup_dir.join(&old.filename);
                if let Err(e) = std::fs::remove_file(&path) {
                    tracing::warn!(file = %old.filename, error = %e, "Failed to remove old backup");
                }
            }
        }
    }
}

fn run_backup(project_id: &str, backup_dir: &Path, retention: usize) -> Result<(), String> {
    std::fs::create_dir_all(backup_dir).map_err(|e| e.to_string())?;

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let filename = format!("backup-{}-{}.ocrate", project_id, timestamp);
    let dest = backup_dir.join(&filename);

    crate::project::export_project(project_id, &dest).map_err(|e| e.to_string())?;

    // Enforce retention
    let prefix = format!("backup-{}-", project_id);
    let mut backups: Vec<(String, i64)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(backup_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(&prefix) && name.ends_with(".ocrate") {
                let modified = entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                backups.push((name, modified));
            }
        }
    }
    backups.sort_by(|a, b| b.1.cmp(&a.1));
    if backups.len() > retention {
        for (name, _) in &backups[retention..] {
            let _ = std::fs::remove_file(backup_dir.join(name));
        }
    }

    tracing::info!(file = %filename, "Scheduled backup created");
    Ok(())
}

fn backup_directory() -> PathBuf {
    crate::project::opencrate_home().join("backups")
}

fn load_config(path: &Path) -> Option<BackupConfig> {
    let data = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

fn save_config(path: &Path, config: &BackupConfig) -> Result<(), String> {
    let data = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    std::fs::write(path, data).map_err(|e| e.to_string())
}
