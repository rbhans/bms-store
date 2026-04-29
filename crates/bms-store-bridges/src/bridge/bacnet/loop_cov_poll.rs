use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use rustbac_client::{BacnetClient, CovManagerBuilder, CovSubscriptionSpec};
use rustbac_core::types::PropertyId;
use rustbac_datalink::DataLink;

use crate::event::bus::{Event, EventBus};
use crate::store::point_store::{PointKey, PointStatusFlags, PointStore};

use super::backoff::DeviceBackoff;
use super::conversion::{apply_bacnet_status_flags, client_to_point_value, object_point_id};
use super::transport::TransportClient;
use super::BacnetDevice;

pub(super) async fn run_cov_with_poll_fallback(
    tc: TransportClient,
    store: PointStore,
    devices: &[BacnetDevice],
    poll_interval: Duration,
    cov_lifetime: u32,
    event_bus: Option<EventBus>,
) {
    // CovManagerBuilder is generic over DataLink, so dispatch on transport type.
    // If COV fails to start, fall back to plain polling.
    let cov_ok = match &tc {
        TransportClient::Ip(client) => {
            run_cov_inner(
                Arc::clone(client),
                store.clone(),
                devices,
                poll_interval,
                cov_lifetime,
                event_bus.clone(),
            )
            .await
        }
        TransportClient::Sc(client) => {
            run_cov_inner(
                Arc::clone(client),
                store.clone(),
                devices,
                poll_interval,
                cov_lifetime,
                event_bus.clone(),
            )
            .await
        }
        TransportClient::Mstp(client) => {
            run_cov_inner(
                Arc::clone(client),
                store.clone(),
                devices,
                poll_interval,
                cov_lifetime,
                event_bus.clone(),
            )
            .await
        }
        TransportClient::Ip6(client) => {
            run_cov_inner(
                Arc::clone(client),
                store.clone(),
                devices,
                poll_interval,
                cov_lifetime,
                event_bus.clone(),
            )
            .await
        }
    };
    if !cov_ok {
        poll_loop(tc, store, devices, poll_interval, event_bus).await;
    }
}

/// Returns true if COV ran successfully, false if it failed to start (caller should fall back).
async fn run_cov_inner<D: DataLink + 'static>(
    client: Arc<BacnetClient<D>>,
    store: PointStore,
    devices: &[BacnetDevice],
    poll_interval: Duration,
    cov_lifetime: u32,
    _event_bus: Option<EventBus>,
) -> bool {
    let mut builder = CovManagerBuilder::new(Arc::clone(&client))
        .poll_interval(poll_interval)
        .silence_threshold(Duration::from_secs((cov_lifetime as u64) / 2))
        .renewal_fraction(0.75);

    let mut process_id: u32 = 1;
    let mut sub_map: HashMap<u32, (String, String)> = HashMap::new(); // process_id -> (device_key, point_id)

    for dev in devices {
        let dev_key = format!("bacnet-{}", dev.device_id.instance());
        for obj in &dev.objects {
            let point_id = object_point_id(obj);
            sub_map.insert(process_id, (dev_key.clone(), point_id));

            builder = builder.subscribe(CovSubscriptionSpec {
                address: dev.address,
                object_id: obj.object_id,
                property_id: None, // subscribe to all properties
                lifetime_seconds: cov_lifetime,
                cov_increment: None,
                confirmed: false,
                subscriber_process_id: process_id,
            });
            process_id += 1;
        }
    }

    let mut manager = match builder.build() {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("BACnet: COV manager failed to start: {e}, falling back to polling");
            return false;
        }
    };

    // Process COV updates and write to PointStore
    while let Some(update) = manager.recv().await {
        // Find the device/point this update belongs to
        // Look through devices to find the matching one
        for dev in devices {
            let dk = format!("bacnet-{}", dev.device_id.instance());
            let point_id_str = dev
                .objects
                .iter()
                .find(|o| o.object_id == update.object_id)
                .map(object_point_id);

            if let Some(pid) = point_id_str {
                for prop in &update.values {
                    let key = PointKey {
                        device_instance_id: dk.clone(),
                        point_id: pid.clone(),
                    };
                    match prop.property_id {
                        PropertyId::PresentValue => {
                            store.set(
                                key,
                                client_to_point_value(&prop.value, update.object_id.object_type()),
                            );
                        }
                        PropertyId::StatusFlags => {
                            apply_bacnet_status_flags(&store, &key, &prop.value);
                        }
                        _ => {}
                    }
                }
                break;
            }
        }
    }
    true
}

/// Simple periodic polling fallback when COV is unavailable.
async fn poll_loop(
    tc: TransportClient,
    store: PointStore,
    devices: &[BacnetDevice],
    interval: Duration,
    event_bus: Option<EventBus>,
) {
    let mut backoffs: HashMap<u32, DeviceBackoff> = devices
        .iter()
        .map(|d| (d.device_id.instance(), DeviceBackoff::new()))
        .collect();

    loop {
        for dev in devices {
            let instance = dev.device_id.instance();
            let dev_key = format!("bacnet-{instance}");

            let backoff = backoffs.entry(instance).or_default();
            if backoff.should_skip() {
                continue;
            }

            // Build batch read requests for PresentValue + StatusFlags
            let requests: Vec<(rustbac_core::types::ObjectId, PropertyId)> = dev
                .objects
                .iter()
                .flat_map(|o| {
                    vec![
                        (o.object_id, PropertyId::PresentValue),
                        (o.object_id, PropertyId::StatusFlags),
                    ]
                })
                .collect();

            if requests.is_empty() {
                continue;
            }

            match with_client!(&tc, |c| c.read_many(dev.address, &requests).await) {
                Ok(results) => {
                    let was_down = backoff.was_down;
                    backoff.record_success();

                    // Clear DOWN on all points for this device on success
                    for obj in &dev.objects {
                        let key = PointKey {
                            device_instance_id: dev_key.clone(),
                            point_id: object_point_id(obj),
                        };
                        store.clear_status(&key, PointStatusFlags::DOWN);
                    }

                    // Process results
                    for ((obj_id, prop_id), value) in &results {
                        if let Some(obj) = dev.objects.iter().find(|o| o.object_id == *obj_id) {
                            let point_id = object_point_id(obj);
                            let key = PointKey {
                                device_instance_id: dev_key.clone(),
                                point_id,
                            };
                            match prop_id {
                                PropertyId::PresentValue => {
                                    store.clear_status(&key, PointStatusFlags::FAULT);
                                    store.set(
                                        key,
                                        client_to_point_value(value, obj_id.object_type()),
                                    );
                                }
                                PropertyId::StatusFlags => {
                                    apply_bacnet_status_flags(&store, &key, value);
                                }
                                _ => {}
                            }
                        }
                    }

                    // Publish recovery event if device was previously down
                    if was_down {
                        backoff.was_down = false;
                        if let Some(ref bus) = event_bus {
                            bus.publish(Event::DeviceRecovered {
                                bridge_type: "bacnet".into(),
                                device_key: dev_key.clone(),
                            });
                        }
                        tracing::info!(instance, "BACnet: device recovered");
                    }
                }
                Err(e) => {
                    backoff.record_failure();
                    tracing::warn!(
                        instance,
                        failures = backoff.failures,
                        "BACnet: poll failed for device: {e}"
                    );

                    // Set DOWN on all points for this device
                    for obj in &dev.objects {
                        let key = PointKey {
                            device_instance_id: dev_key.clone(),
                            point_id: object_point_id(obj),
                        };
                        store.set_status(&key, PointStatusFlags::DOWN);
                    }

                    // Publish DeviceDown after threshold
                    if backoff.is_down() && !backoff.was_down {
                        backoff.was_down = true;
                        if let Some(ref bus) = event_bus {
                            bus.publish(Event::DeviceDown {
                                bridge_type: "bacnet".into(),
                                device_key: dev_key.clone(),
                            });
                        }
                        tracing::error!(
                            instance,
                            failures = backoff.failures,
                            "BACnet: device marked DOWN"
                        );
                    }
                }
            }
        }

        tokio::time::sleep(interval).await;
    }
}
