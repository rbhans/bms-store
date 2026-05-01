use std::collections::{HashMap, HashSet};
use std::time::Duration;

use rustbac_client::ClientDataValue;
use rustbac_core::types::{ObjectId, ObjectType, PropertyId};

use bms_core::QualityReason;

use crate::event::bus::{Event, EventBus};

use super::transport::TransportClient;

/// Background monitor loop: periodically sends targeted Who-Is to known devices,
/// detects new devices via unsolicited I-Am, and optionally checks ObjectList length
/// for changes.
pub(super) async fn run_device_monitor_loop(
    tc: TransportClient,
    accepted_instances: Vec<u32>,
    event_bus: Option<EventBus>,
    monitor_interval: Duration,
    network_id: String,
    object_check_cycles: u32,
    device_object_counts: HashMap<u32, usize>,
    device_addrs: HashMap<u32, rustbac_datalink::DataLinkAddress>,
) {
    // Wait for startup to settle before monitoring
    tokio::time::sleep(Duration::from_secs(60)).await;

    let mut cycle: u32 = 0;
    // Track consecutive missed cycles per device instance
    let mut missed_cycles: HashMap<u32, u32> = HashMap::new();
    let mut was_down: HashSet<u32> = HashSet::new();

    loop {
        cycle = cycle.wrapping_add(1);

        // Send Who-Is targeting known device ranges + collect all replies (including new)
        let who_is_timeout = Duration::from_secs(3);
        let responding = match with_client!(&tc, |c| c.who_is(None, who_is_timeout).await) {
            Ok(devs) => devs,
            Err(e) => {
                tracing::debug!(network_id, "Device monitor: Who-Is failed: {e}");
                tokio::time::sleep(monitor_interval).await;
                continue;
            }
        };

        let responding_instances: HashSet<u32> = responding
            .iter()
            .filter_map(|d| d.device_id.map(|id| id.instance()))
            .collect();

        let accepted_set: HashSet<u32> = accepted_instances.iter().copied().collect();

        let mut online_count = 0usize;
        let mut offline_count = 0usize;
        let mut new_device_count = 0usize;

        // Check known devices
        for &inst in &accepted_instances {
            if responding_instances.contains(&inst) {
                // Device is responding
                *missed_cycles.entry(inst).or_insert(0) = 0;
                online_count += 1;

                // Publish recovery if previously down
                if was_down.remove(&inst) {
                    if let Some(ref bus) = event_bus {
                        bus.publish(Event::DeviceRecovered {
                            bridge_type: "bacnet-monitor".into(),
                            device_key: format!("bacnet-{inst}"),
                        });
                        // Bridge-level quality event for this device recovering
                        bus.publish(Event::BridgeQualityChanged {
                            bridge_type: "bacnet".into(),
                            network_id: network_id.clone(),
                            reason: QualityReason::Recovered,
                            affected_device_count: 1,
                        });
                    }
                    tracing::info!(
                        instance = inst,
                        network_id,
                        "Device monitor: device recovered"
                    );
                }
            } else {
                // Device not responding
                let missed = missed_cycles.entry(inst).or_insert(0);
                *missed += 1;

                if *missed >= 2 && !was_down.contains(&inst) {
                    // Mark as down after 2 consecutive misses
                    was_down.insert(inst);
                    offline_count += 1;
                    if let Some(ref bus) = event_bus {
                        bus.publish(Event::DeviceDown {
                            bridge_type: "bacnet-monitor".into(),
                            device_key: format!("bacnet-{inst}"),
                        });
                        // Bridge-level quality event: one event per device going down
                        // instead of individual QualityChanged per point.
                        bus.publish(Event::BridgeQualityChanged {
                            bridge_type: "bacnet".into(),
                            network_id: network_id.clone(),
                            reason: QualityReason::BridgeDown,
                            affected_device_count: 1,
                        });
                    }
                    tracing::warn!(
                        instance = inst,
                        network_id,
                        missed = *missed,
                        "Device monitor: device marked offline"
                    );
                } else if was_down.contains(&inst) {
                    offline_count += 1;
                }
            }
        }

        // Check for new (unknown) devices
        for &inst in &responding_instances {
            if !accepted_set.contains(&inst) {
                new_device_count += 1;
                if let Some(ref bus) = event_bus {
                    bus.publish(Event::DeviceDiscovered {
                        bridge_type: "bacnet-passive".into(),
                        device_key: format!("bacnet-{inst}"),
                    });
                }
                tracing::info!(
                    instance = inst,
                    network_id,
                    "Device monitor: new device detected"
                );
            }
        }

        // A2: Object list change detection (every N cycles)
        if object_check_cycles > 0 && cycle.is_multiple_of(object_check_cycles) {
            for (&inst, &expected_count) in &device_object_counts {
                let addr = match device_addrs.get(&inst) {
                    Some(a) => *a,
                    None => continue,
                };
                let device_oid = ObjectId::new(ObjectType::Device, inst);

                let current_count = match with_client!(&tc, |c| c
                    .read_property(addr, device_oid, PropertyId::ObjectList)
                    .await)
                {
                    Ok(ClientDataValue::Constructed { values, .. }) => values.len(),
                    Ok(ClientDataValue::ObjectId(_)) => 1,
                    _ => continue,
                };

                if current_count != expected_count {
                    tracing::info!(
                        instance = inst,
                        network_id,
                        old_count = expected_count,
                        new_count = current_count,
                        "Device monitor: object list changed"
                    );
                    if let Some(ref bus) = event_bus {
                        bus.publish(Event::ObjectListChanged {
                            device_key: format!("bacnet-{inst}"),
                            old_count: expected_count,
                            new_count: current_count,
                        });
                    }
                }
            }
        }

        // Publish monitor cycle summary
        if let Some(ref bus) = event_bus {
            bus.publish(Event::DeviceMonitorCycle {
                protocol: "bacnet".into(),
                network_id: network_id.clone(),
                online: online_count,
                offline: offline_count,
                new_devices: new_device_count,
            });
        }

        tokio::time::sleep(monitor_interval).await;
    }
}
