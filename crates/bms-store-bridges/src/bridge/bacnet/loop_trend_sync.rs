use std::collections::HashMap;
use std::time::Duration;

use rustbac_client::ClientDataValue;
use rustbac_core::types::PropertyId;

use crate::store::history_store::HistoryStore;

use super::conversion::trend_log_items_to_samples;
use super::transport::TransportClient;
use super::BacnetDevice;

/// Periodically reads new TrendLog records from all devices and inserts into HistoryStore.
/// Tracks the last-read record count per TrendLog to only fetch incremental records.
pub(super) async fn run_trend_log_sync_loop(
    tc: TransportClient,
    devices: &[BacnetDevice],
    history_store: HistoryStore,
    sync_interval: Duration,
) {
    // Wait for startup to settle
    tokio::time::sleep(Duration::from_secs(60)).await;

    // Track last-known record count per (device_instance, trendlog_instance)
    let mut last_counts: HashMap<(u32, u32), u32> = HashMap::new();

    loop {
        for dev in devices {
            let dev_instance = dev.device_id.instance();
            let dev_key = format!("bacnet-{dev_instance}");

            for tl in &dev.trend_logs {
                let tl_instance = tl.object_id.instance();
                let tl_key = (dev_instance, tl_instance);

                // Read current record count
                let current_count = match with_client!(&tc, |c| c
                    .read_property(dev.address, tl.object_id, PropertyId::RecordCount,)
                    .await)
                {
                    Ok(ClientDataValue::Unsigned(n)) => n,
                    _ => continue,
                };

                let last_count = last_counts.get(&tl_key).copied().unwrap_or(0);

                if current_count <= last_count {
                    // No new records
                    last_counts.insert(tl_key, current_count);
                    continue;
                }

                // Read only the new records
                let new_start = (last_count + 1) as i32;
                let fallback_name = format!("TrendLog-{tl_instance}");
                let point_id = tl.object_name.as_deref().unwrap_or(&fallback_name);
                let point_key = format!("{dev_key}:{point_id}");

                // Read in batches of 100
                let batch_size: i16 = 100;
                let mut index = new_start;
                let mut total = 0usize;

                while index <= current_count as i32 {
                    let remaining = current_count as i32 - index + 1;
                    let count = batch_size.min(remaining as i16);

                    let items = match with_client!(&tc, |c| c
                        .read_range_by_position(
                            dev.address,
                            tl.object_id,
                            PropertyId::LogBuffer,
                            None,
                            index,
                            count,
                        )
                        .await)
                    {
                        Ok(result) => result.items,
                        Err(e) => {
                            tracing::error!(
                                device = dev_key,
                                tl_instance,
                                "BACnet: TrendLog sync failed: {e}"
                            );
                            break;
                        }
                    };

                    if items.is_empty() {
                        break;
                    }

                    let samples = trend_log_items_to_samples(&items);
                    let batch: Vec<(String, i64, f64)> = samples
                        .iter()
                        .map(|(ts, v)| (point_key.clone(), *ts, *v))
                        .collect();
                    total += batch.len();
                    history_store.backfill(batch).await;

                    index += count as i32;
                }

                if total > 0 {
                    tracing::info!(
                        records = total,
                        device = dev_key,
                        point_id,
                        "BACnet: TrendLog synced"
                    );
                }

                last_counts.insert(tl_key, current_count);
            }
        }

        tokio::time::sleep(sync_interval).await;
    }
}
