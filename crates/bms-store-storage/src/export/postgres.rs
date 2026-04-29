use std::sync::Arc;

use tokio::sync::Mutex;
use tokio_postgres::{Client, NoTls};

use crate::export::{ExportAlarm, ExportConnector, ExportError, ExportSample, PostgresConfig};

/// PostgreSQL connector for exporting history and alarm data.
pub struct PostgresConnector {
    config: PostgresConfig,
    client: Arc<Mutex<Option<Client>>>,
}

impl PostgresConnector {
    pub fn new(config: PostgresConfig) -> Self {
        if config.use_tls {
            tracing::warn!(
                "PostgreSQL export: use_tls=true but TLS is not yet supported — connecting without TLS. \
                 Set use_tls=false to suppress this warning."
            );
        }
        Self {
            config,
            client: Arc::new(Mutex::new(None)),
        }
    }

    async fn connect(&self) -> Result<Client, ExportError> {
        let conn_str = format!(
            "host={} port={} dbname={} user={} password={}",
            self.config.host,
            self.config.port,
            self.config.database,
            self.config.username,
            self.config.password,
        );

        let (client, connection) = tokio_postgres::connect(&conn_str, NoTls)
            .await
            .map_err(|e| ExportError::Connection(e.to_string()))?;

        // Spawn the connection manager task
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                tracing::error!(error = %e, "PostgreSQL export connection lost");
            }
        });

        Ok(client)
    }

    async fn get_or_connect(
        &self,
    ) -> Result<tokio::sync::MutexGuard<'_, Option<Client>>, ExportError> {
        let mut guard = self.client.lock().await;
        if guard.is_none() {
            let client = self.connect().await?;
            self.ensure_schema(&client).await?;
            *guard = Some(client);
        }
        Ok(guard)
    }

    async fn ensure_schema(&self, client: &Client) -> Result<(), ExportError> {
        let schema = &self.config.schema;
        let prefix = &self.config.table_prefix;

        client
            .batch_execute(&format!(
                "CREATE SCHEMA IF NOT EXISTS \"{schema}\";

                CREATE TABLE IF NOT EXISTS \"{schema}\".\"{prefix}history\" (
                    point_key   TEXT NOT NULL,
                    device_id   TEXT NOT NULL,
                    point_id    TEXT NOT NULL,
                    value       DOUBLE PRECISION NOT NULL,
                    timestamp_ms BIGINT NOT NULL
                );

                CREATE INDEX IF NOT EXISTS idx_{prefix}history_point_ts
                    ON \"{schema}\".\"{prefix}history\" (point_key, timestamp_ms);

                CREATE TABLE IF NOT EXISTS \"{schema}\".\"{prefix}alarms\" (
                    alarm_id    BIGINT NOT NULL,
                    node_id     TEXT NOT NULL,
                    severity    TEXT NOT NULL,
                    state       TEXT NOT NULL,
                    timestamp_ms BIGINT NOT NULL,
                    value       DOUBLE PRECISION,
                    note        TEXT
                );

                CREATE INDEX IF NOT EXISTS idx_{prefix}alarms_node_ts
                    ON \"{schema}\".\"{prefix}alarms\" (node_id, timestamp_ms);",
            ))
            .await
            .map_err(|e| ExportError::Connection(format!("schema setup failed: {}", e)))?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl ExportConnector for PostgresConnector {
    async fn test_connection(&self) -> Result<(), ExportError> {
        let client = self.connect().await?;
        client
            .simple_query("SELECT 1")
            .await
            .map_err(|e| ExportError::Connection(e.to_string()))?;
        self.ensure_schema(&client).await?;
        Ok(())
    }

    async fn write_history_batch(&self, samples: &[ExportSample]) -> Result<usize, ExportError> {
        if samples.is_empty() {
            return Ok(0);
        }

        let guard = self.get_or_connect().await?;
        let client = guard.as_ref().unwrap();
        let schema = &self.config.schema;
        let prefix = &self.config.table_prefix;

        let batch_size = 500;
        let mut total = 0;

        for chunk in samples.chunks(batch_size) {
            let mut sql = format!(
                "INSERT INTO \"{schema}\".\"{prefix}history\" (point_key, device_id, point_id, value, timestamp_ms) VALUES "
            );
            let mut params: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> = Vec::new();
            let mut param_idx = 1;

            for (i, s) in chunk.iter().enumerate() {
                if i > 0 {
                    sql.push_str(", ");
                }
                sql.push_str(&format!(
                    "(${}, ${}, ${}, ${}, ${})",
                    param_idx,
                    param_idx + 1,
                    param_idx + 2,
                    param_idx + 3,
                    param_idx + 4,
                ));
                params.push(Box::new(s.point_key.clone()));
                params.push(Box::new(s.device_id.clone()));
                params.push(Box::new(s.point_id.clone()));
                params.push(Box::new(s.value));
                params.push(Box::new(s.timestamp_ms));
                param_idx += 5;
            }

            let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = params
                .iter()
                .map(|p| p.as_ref() as &(dyn tokio_postgres::types::ToSql + Sync))
                .collect();
            client
                .execute(&sql, &param_refs)
                .await
                .map_err(|e| ExportError::Write(e.to_string()))?;
            total += chunk.len();
        }

        Ok(total)
    }

    async fn write_alarm_batch(&self, alarms: &[ExportAlarm]) -> Result<usize, ExportError> {
        if alarms.is_empty() {
            return Ok(0);
        }

        let guard = self.get_or_connect().await?;
        let client = guard.as_ref().unwrap();
        let schema = &self.config.schema;
        let prefix = &self.config.table_prefix;

        let batch_size = 500;
        let mut total = 0;

        for chunk in alarms.chunks(batch_size) {
            let mut sql = format!(
                "INSERT INTO \"{schema}\".\"{prefix}alarms\" (alarm_id, node_id, severity, state, timestamp_ms, value, note) VALUES "
            );
            let mut params: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> = Vec::new();
            let mut param_idx = 1;

            for (i, a) in chunk.iter().enumerate() {
                if i > 0 {
                    sql.push_str(", ");
                }
                sql.push_str(&format!(
                    "(${}, ${}, ${}, ${}, ${}, ${}, ${})",
                    param_idx,
                    param_idx + 1,
                    param_idx + 2,
                    param_idx + 3,
                    param_idx + 4,
                    param_idx + 5,
                    param_idx + 6,
                ));
                params.push(Box::new(a.alarm_id));
                params.push(Box::new(a.node_id.clone()));
                params.push(Box::new(a.severity.clone()));
                params.push(Box::new(a.state.clone()));
                params.push(Box::new(a.timestamp_ms));
                params.push(Box::new(a.value));
                params.push(Box::new(a.note.clone()));
                param_idx += 7;
            }

            let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = params
                .iter()
                .map(|p| p.as_ref() as &(dyn tokio_postgres::types::ToSql + Sync))
                .collect();
            client
                .execute(&sql, &param_refs)
                .await
                .map_err(|e| ExportError::Write(e.to_string()))?;
            total += chunk.len();
        }

        Ok(total)
    }

    async fn close(&self) {
        let mut guard = self.client.lock().await;
        *guard = None;
    }
}
