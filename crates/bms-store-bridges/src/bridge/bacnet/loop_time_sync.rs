use std::time::Duration;

use super::transport::TransportClient;
use super::BacnetDevice;

/// How often to send UTC time synchronization to all devices.
pub(super) const TIME_SYNC_INTERVAL_SECS: u64 = 4 * 3600; // every 4 hours

pub(super) async fn run_time_sync_loop(tc: TransportClient, devices: &[BacnetDevice]) {
    // Initial sync shortly after startup
    tokio::time::sleep(Duration::from_secs(30)).await;
    loop {
        let (date, time) = now_bacnet_utc();
        for dev in devices {
            if let Err(e) = with_client!(&tc, |c| c
                .time_synchronize(dev.address, date, time, true)
                .await)
            {
                tracing::warn!(
                    instance = dev.device_id.instance(),
                    "BACnet: time sync failed: {e}"
                );
            }
        }
        tokio::time::sleep(Duration::from_secs(TIME_SYNC_INTERVAL_SECS)).await;
    }
}

/// Convert current system UTC time to BACnet Date + Time.
pub(crate) fn now_bacnet_utc() -> (rustbac_core::types::Date, rustbac_core::types::Time) {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Simple UTC date/time calculation
    let days_since_epoch = (secs / 86400) as i64;
    let time_of_day = secs % 86400;

    // Days from 1970-01-01
    // Algorithm: convert days since epoch to year/month/day
    let (year, month, day, weekday) = days_to_ymd(days_since_epoch);

    let date = rustbac_core::types::Date {
        year_since_1900: ((year - 1900).clamp(0, 255)) as u8,
        month: month as u8,
        day: day as u8,
        weekday: weekday as u8,
    };
    let time = rustbac_core::types::Time {
        hour: (time_of_day / 3600) as u8,
        minute: ((time_of_day % 3600) / 60) as u8,
        second: (time_of_day % 60) as u8,
        hundredths: 0,
    };
    (date, time)
}

/// Convert days since Unix epoch to (year, month, day, weekday).
/// Weekday: 1=Monday..7=Sunday (BACnet convention).
pub(crate) fn days_to_ymd(days: i64) -> (i32, i32, i32, i32) {
    // 1970-01-01 was a Thursday (weekday=4)
    let weekday = ((days % 7 + 4 - 1) % 7 + 1) as i32; // 1=Mon..7=Sun

    // Civil calendar conversion (Euclidean affine algorithm)
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i32 + (era as i32) * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };

    (year, m as i32, d as i32, weekday)
}
