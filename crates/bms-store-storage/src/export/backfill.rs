use std::time::{SystemTime, UNIX_EPOCH};

use tokio_util::sync::CancellationToken;

use crate::export::{
    ExportConnector, ExportConnectorConfig, ExportError, ExportSample, InfluxDbConfig,
};
use crate::store::export_store::ExportStore;
use crate::store::history_store::{HistoryQuery, HistoryStore};
use crate::store::point_store::PointStore;

use super::influxdb::InfluxDbConnector;

/// Run a historical backfill for a single connector.
///
/// Queries HistoryStore for all known points in the given time range,
/// chunks by day, and writes batches through the connector.
pub async fn run_backfill(
    connector_config: &ExportConnectorConfig,
    export_store: &ExportStore,
    history_store: &HistoryStore,
    point_store: &PointStore,
    start_ms: i64,
    end_ms: i64,
    cancel: CancellationToken,
) -> Result<i64, ExportError> {
    let connector = build_connector(connector_config)
        .ok_or_else(|| ExportError::Config("unsupported connector type".into()))?;

    // Test connection first
    connector.test_connection().await?;

    let _ = export_store
        .update_status(&connector_config.id, now_ms(), 0, None, "backfilling")
        .await;

    let keys = point_store.all_keys();
    let day_ms: i64 = 86_400_000;
    let mut total_rows: i64 = 0;

    for key in &keys {
        if cancel.is_cancelled() {
            let _ = export_store
                .update_status(
                    &connector_config.id,
                    now_ms(),
                    total_rows,
                    Some("cancelled"),
                    "idle",
                )
                .await;
            return Ok(total_rows);
        }

        // Query in day-sized chunks
        let mut chunk_start = start_ms;
        while chunk_start < end_ms {
            if cancel.is_cancelled() {
                break;
            }

            let chunk_end = std::cmp::min(chunk_start + day_ms, end_ms);

            let query = HistoryQuery {
                device_id: key.device_instance_id.clone(),
                point_id: key.point_id.clone(),
                start_ms: chunk_start,
                end_ms: chunk_end,
                max_results: Some(0), // uncapped
            };

            match history_store.query(query).await {
                Ok(result) => {
                    if !result.samples.is_empty() {
                        let samples: Vec<ExportSample> = result
                            .samples
                            .iter()
                            .map(|s| ExportSample {
                                point_key: format!("{}/{}", key.device_instance_id, key.point_id),
                                device_id: key.device_instance_id.clone(),
                                point_id: key.point_id.clone(),
                                value: s.value,
                                timestamp_ms: s.timestamp_ms,
                            })
                            .collect();

                        match connector.write_history_batch(&samples).await {
                            Ok(n) => {
                                total_rows += n as i64;
                            }
                            Err(e) => {
                                let _ = export_store
                                    .update_status(
                                        &connector_config.id,
                                        now_ms(),
                                        total_rows,
                                        Some(&e.to_string()),
                                        "error",
                                    )
                                    .await;
                                return Err(e);
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        device = %key.device_instance_id,
                        point = %key.point_id,
                        error = %e,
                        "Backfill query failed, skipping chunk"
                    );
                }
            }

            chunk_start = chunk_end;
        }
    }

    let _ = export_store
        .update_status(&connector_config.id, now_ms(), total_rows, None, "idle")
        .await;
    connector.close().await;

    tracing::info!(
        connector = %connector_config.id,
        rows = total_rows,
        "Backfill completed"
    );

    Ok(total_rows)
}

/// Build a connector instance from persisted config.
fn build_connector(config: &ExportConnectorConfig) -> Option<Box<dyn ExportConnector>> {
    match config.connector_type.as_str() {
        "influxdb" => {
            let influx_cfg: InfluxDbConfig = serde_json::from_str(&config.config).ok()?;
            Some(Box::new(InfluxDbConnector::new(influx_cfg)))
        }
        #[cfg(feature = "export-postgres")]
        "postgresql" => {
            let pg_cfg: crate::export::PostgresConfig =
                serde_json::from_str(&config.config).ok()?;
            Some(Box::new(crate::export::postgres::PostgresConnector::new(
                pg_cfg,
            )))
        }
        _ => None,
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
