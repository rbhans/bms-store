use std::collections::HashSet;
use std::time::Duration;

use rustbac_client::EventNotification;
use rustbac_core::types::ObjectId;

use crate::event::bus::EventBus;
use crate::store::point_store::{PointKey, PointStatusFlags, PointStore};

use super::conversion::object_point_id;
use super::transport::TransportClient;
use super::BacnetDevice;

/// How often to poll devices for event/alarm notifications.
pub(super) const EVENT_POLL_INTERVAL_SECS: u64 = 60;

pub(super) async fn run_event_poll_loop(
    tc: TransportClient,
    store: PointStore,
    devices: &[BacnetDevice],
    _event_bus: Option<EventBus>,
) {
    // Wait for initial startup to settle
    tokio::time::sleep(Duration::from_secs(15)).await;

    loop {
        // Drain any unsolicited event notifications first (global receive, not
        // per-device). The notification's initiating_device_id tells us which
        // device it came from.
        loop {
            match with_client!(&tc, |c| c
                .recv_event_notification(Duration::from_millis(100))
                .await)
            {
                Ok(Some(notification)) => {
                    // Extract the correct device key from the notification itself
                    let notif_dev_key =
                        format!("bacnet-{}", notification.initiating_device_id.instance());
                    handle_event_notification(&store, &notif_dev_key, &notification);
                }
                Ok(None) => break, // no more pending notifications
                Err(_) => break,   // timeout or error, stop draining
            }
        }

        // Poll GetEventInformation for each device
        for dev in devices {
            let dev_key = format!("bacnet-{}", dev.device_id.instance());

            match with_client!(&tc, |c| c.get_event_information(dev.address, None).await) {
                Ok(result) => {
                    // Collect the set of object IDs currently in alarm
                    let alarmed_objects: HashSet<ObjectId> = result
                        .summaries
                        .iter()
                        .filter(|s| s.event_state_raw != 0)
                        .map(|s| s.object_id)
                        .collect();

                    // Clear ALARM flags for objects that have returned to normal
                    // (i.e. they are known objects on this device but are NOT in
                    // the current alarm summary).
                    for obj in &dev.objects {
                        if !alarmed_objects.contains(&obj.object_id) {
                            let pid = object_point_id(obj);
                            let key = PointKey {
                                device_instance_id: dev_key.clone(),
                                point_id: pid.clone(),
                            };
                            // clear_status is a no-op if the flag is not set
                            store.clear_status(&key, PointStatusFlags::ALARM);
                        }
                    }

                    // Set ALARM flags for objects currently in alarm
                    for summary in &result.summaries {
                        if summary.event_state_raw != 0 {
                            let point_id = dev
                                .objects
                                .iter()
                                .find(|o| o.object_id == summary.object_id)
                                .map(object_point_id);

                            if let Some(pid) = point_id {
                                let key = PointKey {
                                    device_instance_id: dev_key.clone(),
                                    point_id: pid.clone(),
                                };
                                store.set_status(&key, PointStatusFlags::ALARM);
                            }
                        }
                    }
                }
                Err(e) => {
                    // Not all devices support GetEventInformation — this is expected
                    // Only log at debug level to avoid spam
                    let _ = e; // suppress unused warning
                }
            }
        }

        tokio::time::sleep(Duration::from_secs(EVENT_POLL_INTERVAL_SECS)).await;
    }
}

/// Process an unsolicited BACnet EventNotification.
pub(super) fn handle_event_notification(
    store: &PointStore,
    dev_key: &str,
    notification: &EventNotification,
) {
    let instance = notification.event_object_id.instance();
    let event_type = notification.event_type;

    // Map to_state to alarm action
    let is_alarm = notification
        .to_state
        .map(|s| s != rustbac_client::EventState::Normal)
        .unwrap_or(notification.to_state_raw != 0);

    let point_id = format!(
        "{}-{}",
        notification.event_object_id.object_type(),
        instance
    );

    let key = PointKey {
        device_instance_id: dev_key.to_string(),
        point_id: point_id.clone(),
    };

    if is_alarm {
        store.set_status(&key, PointStatusFlags::ALARM);
    } else {
        store.clear_status(&key, PointStatusFlags::ALARM);
    }

    tracing::info!(
        device = dev_key,
        object = instance,
        event_type,
        to_state = notification.to_state_raw,
        message = ?notification.message_text,
        "BACnet: event notification received"
    );
}
