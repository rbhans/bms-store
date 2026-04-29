pub mod backfill;
pub mod influxdb;
pub mod publisher;

#[cfg(feature = "export-postgres")]
pub mod postgres;

use serde::{Deserialize, Serialize};

// ----------------------------------------------------------------
// Connector type
// ----------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorType {
    InfluxDb,
    #[cfg(feature = "export-postgres")]
    PostgreSql,
}

impl ConnectorType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InfluxDb => "influxdb",
            #[cfg(feature = "export-postgres")]
            Self::PostgreSql => "postgresql",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "influxdb" => Some(Self::InfluxDb),
            #[cfg(feature = "export-postgres")]
            "postgresql" => Some(Self::PostgreSql),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::InfluxDb => "InfluxDB",
            #[cfg(feature = "export-postgres")]
            Self::PostgreSql => "PostgreSQL",
        }
    }
}

// ----------------------------------------------------------------
// Connector config (JSON-serialized in store)
// ----------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfluxDbConfig {
    pub url: String,
    pub token: String,
    pub org: String,
    pub bucket: String,
    #[serde(default = "default_measurement")]
    pub measurement: String,
}

fn default_measurement() -> String {
    "point_value".to_string()
}

#[cfg(feature = "export-postgres")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresConfig {
    pub host: String,
    #[serde(default = "default_pg_port")]
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub use_tls: bool,
    #[serde(default = "default_pg_schema")]
    pub schema: String,
    #[serde(default = "default_pg_prefix")]
    pub table_prefix: String,
}

#[cfg(feature = "export-postgres")]
fn default_pg_port() -> u16 {
    5432
}

#[cfg(feature = "export-postgres")]
fn default_pg_schema() -> String {
    "opencrate".to_string()
}

#[cfg(feature = "export-postgres")]
fn default_pg_prefix() -> String {
    "oc_".to_string()
}

// ----------------------------------------------------------------
// Export sample / alarm (data to push)
// ----------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ExportSample {
    pub point_key: String,
    pub device_id: String,
    pub point_id: String,
    pub value: f64,
    pub timestamp_ms: i64,
}

#[derive(Debug, Clone)]
pub struct ExportAlarm {
    pub alarm_id: i64,
    pub node_id: String,
    pub severity: String,
    pub state: String,
    pub timestamp_ms: i64,
    pub value: Option<f64>,
    pub note: Option<String>,
}

// ----------------------------------------------------------------
// Export connector (persisted config row)
// ----------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportConnectorConfig {
    pub id: String,
    pub name: String,
    pub connector_type: String,
    /// JSON-serialized InfluxDbConfig or PostgresConfig.
    pub config: String,
    pub enabled: bool,
    pub on_values: bool,
    pub on_alarms: bool,
    pub on_fdd: bool,
    pub created_ms: i64,
    pub updated_ms: i64,
}

// ----------------------------------------------------------------
// Export status (per-connector sync state)
// ----------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportStatus {
    pub connector_id: String,
    pub last_sync_ms: i64,
    pub rows_exported: i64,
    pub last_error: Option<String>,
    /// idle, syncing, error, backfilling
    pub state: String,
}

// ----------------------------------------------------------------
// Error
// ----------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("connection error: {0}")]
    Connection(String),
    #[error("write error: {0}")]
    Write(String),
    #[error("config error: {0}")]
    Config(String),
    #[error("timeout")]
    Timeout,
}

// ----------------------------------------------------------------
// Connector trait
// ----------------------------------------------------------------

#[async_trait::async_trait]
pub trait ExportConnector: Send + Sync {
    /// Test the connection. Returns Ok(()) or a descriptive error.
    async fn test_connection(&self) -> Result<(), ExportError>;

    /// Write a batch of time-series samples.
    async fn write_history_batch(&self, samples: &[ExportSample]) -> Result<usize, ExportError>;

    /// Write a batch of alarm events.
    async fn write_alarm_batch(&self, alarms: &[ExportAlarm]) -> Result<usize, ExportError>;

    /// Close the connection gracefully.
    async fn close(&self);
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connector_type_roundtrip() {
        assert_eq!(
            ConnectorType::from_str("influxdb"),
            Some(ConnectorType::InfluxDb)
        );
        assert_eq!(ConnectorType::InfluxDb.as_str(), "influxdb");
        assert_eq!(ConnectorType::InfluxDb.label(), "InfluxDB");
        assert_eq!(ConnectorType::from_str("unknown"), None);
    }

    #[cfg(feature = "export-postgres")]
    #[test]
    fn postgres_connector_type_roundtrip() {
        assert_eq!(
            ConnectorType::from_str("postgresql"),
            Some(ConnectorType::PostgreSql)
        );
        assert_eq!(ConnectorType::PostgreSql.as_str(), "postgresql");
    }

    #[test]
    fn influxdb_config_defaults() {
        let json = r#"{"url":"http://localhost:8086","token":"tok","org":"myorg","bucket":"bms"}"#;
        let cfg: InfluxDbConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.measurement, "point_value");
    }

    #[cfg(feature = "export-postgres")]
    #[test]
    fn postgres_config_defaults() {
        let json = r#"{"host":"localhost","database":"bms","username":"user","password":"pass"}"#;
        let cfg: PostgresConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.port, 5432);
        assert_eq!(cfg.schema, "opencrate");
        assert_eq!(cfg.table_prefix, "oc_");
        assert!(!cfg.use_tls);
    }
}
