use std::time::Duration;

use tokio_util::sync::CancellationToken;
use tracing;

use crate::store::energy_store::EnergyStore;
use crate::store::history_store::HistoryStore;

use super::consumption::{day_start_ms, month_start_ms};
use super::rollup::{compute_daily_rollup, compute_monthly_rollup};

/// Start the background rollup scheduler.
///
/// Runs every 15 minutes, computing today's partial rollup and finalizing
/// yesterday's if missing. Uses CancellationToken for graceful shutdown.
pub fn start_energy_rollup_scheduler(
    energy_store: EnergyStore,
    history_store: HistoryStore,
    cancel: CancellationToken,
) {
    tokio::spawn(async move {
        let interval = Duration::from_secs(15 * 60);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::info!("Energy rollup scheduler shutting down");
                    break;
                }
                _ = tokio::time::sleep(interval) => {
                    if let Err(e) = run_rollup_cycle(&energy_store, &history_store).await {
                        tracing::warn!(error = %e, "Energy rollup cycle failed");
                    }
                }
            }
        }
    });
}

async fn run_rollup_cycle(
    energy_store: &EnergyStore,
    history_store: &HistoryStore,
) -> Result<(), String> {
    let meters = energy_store.list_meters().await;
    if meters.is_empty() {
        return Ok(());
    }

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    let today_start = day_start_ms(now_ms);
    let yesterday_start = today_start - 86_400_000;

    for meter in &meters {
        // Look up the meter's utility rate for cost calculation.
        let rate_info = if let Some(rate_id) = meter.utility_rate_id {
            energy_store.get_rate(rate_id).await
        } else {
            None
        };

        let rate_ref = rate_info.as_ref().and_then(|r| {
            serde_json::from_str::<super::cost::RateConfig>(&r.config)
                .ok()
                .map(|cfg| (cfg, r.currency.clone()))
        });

        // Compute yesterday's final rollup if missing.
        let existing_yesterday = energy_store
            .get_rollup(meter.id, "daily", yesterday_start)
            .await;
        if existing_yesterday.is_none() {
            let rollup = compute_daily_rollup(
                &meter.node_id,
                meter.id,
                yesterday_start,
                history_store,
                rate_ref.as_ref().map(|(cfg, cur)| (cfg, cur.as_str())),
                0.0, // TODO: fetch degree-days from weather history
                0.0,
            )
            .await;
            energy_store.upsert_rollup(&rollup).await;
            tracing::debug!(
                meter_id = meter.id,
                day = yesterday_start,
                kwh = rollup.consumption_kwh,
                "Finalized yesterday's energy rollup"
            );
        }

        // Compute today's partial rollup (will be overwritten next cycle).
        let rollup = compute_daily_rollup(
            &meter.node_id,
            meter.id,
            today_start,
            history_store,
            rate_ref.as_ref().map(|(cfg, cur)| (cfg, cur.as_str())),
            0.0,
            0.0,
        )
        .await;
        energy_store.upsert_rollup(&rollup).await;
    }

    // Monthly rollup: if today is the 1st (or 2nd to catch timezone edge),
    // finalize last month's rollup from its daily rollups.
    let this_month_start = month_start_ms(now_ms);
    let last_month_start = month_start_ms(this_month_start - 86_400_000);
    let day_of_month = (today_start - this_month_start) / 86_400_000;

    if day_of_month <= 1 {
        for meter in &meters {
            let existing = energy_store
                .get_rollup(meter.id, "monthly", last_month_start)
                .await;
            if existing.is_none() {
                let dailies = energy_store
                    .query_rollups(meter.id, "daily", last_month_start, this_month_start)
                    .await;
                if !dailies.is_empty() {
                    let daily_rollups: Vec<_> = dailies
                        .iter()
                        .map(|d| super::rollup::EnergyRollup {
                            meter_id: d.meter_id,
                            period_type: d.period_type.clone(),
                            period_start_ms: d.period_start_ms,
                            consumption_kwh: d.consumption_kwh,
                            peak_demand_kw: d.peak_demand_kw,
                            peak_demand_ms: d.peak_demand_ms,
                            avg_kw: d.avg_kw,
                            cost: d.cost,
                            hdd: d.hdd,
                            cdd: d.cdd,
                        })
                        .collect();
                    let monthly =
                        compute_monthly_rollup(meter.id, last_month_start, &daily_rollups);
                    energy_store.upsert_rollup(&monthly).await;
                    tracing::info!(
                        meter_id = meter.id,
                        month = last_month_start,
                        kwh = monthly.consumption_kwh,
                        "Finalized monthly energy rollup"
                    );
                }
            }
        }
    }

    tracing::debug!(meter_count = meters.len(), "Energy rollup cycle complete");
    Ok(())
}
