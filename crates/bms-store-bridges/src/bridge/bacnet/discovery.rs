use std::collections::HashMap;

use rustbac_client::walk::walk_device;
use rustbac_core::types::ObjectType;

use crate::bridge::traits::BridgeError;
use crate::store::point_store::{PointKey, PointStore};

use super::conversion::{client_to_point_value, is_point_object, object_point_id};
use super::loop_cov_poll::run_cov_with_poll_fallback;
use super::loop_event_poll::run_event_poll_loop;
use super::loop_monitor::run_device_monitor_loop;
use super::loop_time_sync::run_time_sync_loop;
use super::loop_trend_sync::run_trend_log_sync_loop;
use super::transport::TransportClient;
use super::{BacnetDevice, BacnetObject, TrendLogRef};

impl super::BacnetBridge {
    /// Re-scan the BACnet network: run a fresh Who-Is broadcast, walk any new
    /// devices, merge them into the existing device list, populate PointStore
    /// for new devices, and restart the background loops (COV/poll, time sync,
    /// event poll, trend log sync) so they pick up the updated device set.
    ///
    /// Returns the list of *newly* discovered devices (devices that weren't
    /// already in `self.devices`).
    pub async fn rescan(&mut self, store: PointStore) -> Result<Vec<BacnetDevice>, BridgeError> {
        let tc = self.require_transport()?.clone();
        let discovery_timeout = self.discovery_timeout;

        // Pause background loops so they don't consume I-Am responses
        // during the Who-Is recv window. They get restarted at the end.
        for h in [
            self.cov_handle.take(),
            self.poll_handle.take(),
            self.time_sync_handle.take(),
            self.event_poll_handle.take(),
            self.trend_log_handle.take(),
            self.monitor_handle.take(),
        ]
        .into_iter()
        .flatten()
        {
            h.abort();
        }

        tracing::info!(
            timeout_secs = discovery_timeout.as_secs(),
            "BACnet rescan: sending Who-Is broadcast"
        );
        let discovered = match with_client!(&tc, |c| c.who_is(None, discovery_timeout).await) {
            Ok(devs) => devs,
            Err(e) => {
                tracing::error!("BACnet rescan: discovery failed: {e}");
                let _ = self.restart_background_loops(tc, store);
                return Err(BridgeError::ConnectionFailed(format!("Who-Is failed: {e}")));
            }
        };

        if discovered.is_empty() {
            tracing::info!("BACnet rescan: no devices discovered");
            self.last_scan_instances = std::collections::HashSet::new();
            let _ = self.restart_background_loops(tc, store);
            return Ok(vec![]);
        }

        // Record which instances responded to Who-Is (used by DiscoveryService
        // to only mark these devices Online, not stale cached ones).
        self.last_scan_instances = discovered
            .iter()
            .filter_map(|d| d.device_id.map(|id| id.instance()))
            .collect();

        tracing::info!(
            count = discovered.len(),
            "BACnet rescan: devices discovered"
        );

        // Walk each device and collect results
        let mut scanned_devices = Vec::new();
        for dev in &discovered {
            let device_id = match dev.device_id {
                Some(id) => id,
                None => continue,
            };

            match with_client!(&tc, |c| walk_device(c, dev.address, device_id).await) {
                Ok(walk_result) => {
                    let mut objects = Vec::new();
                    let mut trend_logs = Vec::new();

                    for o in walk_result.objects {
                        if o.object_id.object_type() == ObjectType::TrendLog {
                            trend_logs.push(TrendLogRef {
                                object_id: o.object_id,
                                object_name: o.object_name,
                            });
                        } else if is_point_object(o.object_id.object_type()) {
                            let classification =
                                rustbac_client::point::classify_point(o.object_id.object_type());
                            objects.push(BacnetObject {
                                object_id: o.object_id,
                                object_name: o.object_name,
                                description: o.description,
                                units: o.units,
                                present_value: o.present_value,
                                writable: classification.writable,
                            });
                        }
                    }

                    scanned_devices.push(BacnetDevice {
                        device_id,
                        address: dev.address,
                        vendor: walk_result.device_info.vendor_name,
                        model: walk_result.device_info.model_name,
                        firmware_revision: walk_result.device_info.firmware_revision,
                        location: walk_result.device_info.location,
                        description: walk_result.device_info.description,
                        max_apdu: walk_result.device_info.max_apdu_length,
                        segmentation: walk_result.device_info.segmentation_supported,
                        protocol_version: walk_result.device_info.protocol_version,
                        app_software_version: walk_result.device_info.application_software_version,
                        objects,
                        trend_logs,
                    });
                }
                Err(e) => {
                    tracing::warn!(
                        instance = device_id.instance(),
                        "BACnet rescan: device walk failed: {e}"
                    );
                }
            }
        }

        // Merge: keep existing, add new, update objects for re-walked devices
        let existing_instances: std::collections::HashSet<u32> = self
            .devices
            .iter()
            .map(|d| d.device_id.instance())
            .collect();

        let mut new_devices = Vec::new();
        for dev in scanned_devices {
            let inst = dev.device_id.instance();
            if existing_instances.contains(&inst) {
                // Update existing device's objects and metadata in place.
                // Clear stale points from PointStore first — the object list may have changed.
                let device_key = format!("bacnet-{inst}");
                store.remove_device_points(&device_key);

                if let Some(existing) = self
                    .devices
                    .iter_mut()
                    .find(|d| d.device_id.instance() == inst)
                {
                    existing.objects = dev.objects;
                    existing.trend_logs = dev.trend_logs;
                    existing.address = dev.address;
                    existing.vendor = dev.vendor;
                    existing.model = dev.model;
                    existing.firmware_revision = dev.firmware_revision;
                    existing.location = dev.location;
                    existing.description = dev.description;
                    existing.max_apdu = dev.max_apdu;
                    existing.segmentation = dev.segmentation;
                    existing.protocol_version = dev.protocol_version;
                    existing.app_software_version = dev.app_software_version;

                    // Repopulate PointStore with current object set
                    for obj in &existing.objects {
                        let point_id = object_point_id(obj);
                        let key = PointKey {
                            device_instance_id: device_key.clone(),
                            point_id: point_id.clone(),
                        };
                        if let Some(pv) = &obj.present_value {
                            store.set(key, client_to_point_value(pv, obj.object_id.object_type()));
                        }
                    }
                }
            } else {
                new_devices.push(dev);
            }
        }

        // Add new devices and populate PointStore + point_map for them
        for dev in &new_devices {
            let dev_instance = dev.device_id.instance();
            let device_key = format!("bacnet-{dev_instance}");

            for obj in &dev.objects {
                let point_id = object_point_id(obj);
                let key = PointKey {
                    device_instance_id: device_key.clone(),
                    point_id: point_id.clone(),
                };

                if let Some(pv) = &obj.present_value {
                    store.set(key, client_to_point_value(pv, obj.object_id.object_type()));
                }

                self.point_map
                    .insert((dev_instance, obj.object_id.instance()), obj.object_id);
            }

            tracing::info!(
                instance = dev_instance,
                points = dev.objects.len(),
                "BACnet rescan: new device added"
            );
        }

        self.devices.extend(new_devices.clone());

        // Also refresh point_map for existing (re-walked) devices
        for dev in &self.devices {
            let dev_instance = dev.device_id.instance();
            for obj in &dev.objects {
                self.point_map
                    .insert((dev_instance, obj.object_id.instance()), obj.object_id);
            }
        }

        // Restart all background loops with the updated device set
        self.restart_background_loops(tc, store)?;

        let total_points: usize = self.devices.iter().map(|d| d.objects.len()).sum();
        tracing::info!(
            devices = self.devices.len(),
            points = total_points,
            "BACnet rescan: monitoring updated"
        );

        Ok(new_devices)
    }

    /// Abort existing background tasks and restart them with the current device set.
    pub(super) fn restart_background_loops(
        &mut self,
        tc: TransportClient,
        store: PointStore,
    ) -> Result<(), BridgeError> {
        // Abort existing handles
        for h in [
            self.cov_handle.take(),
            self.poll_handle.take(),
            self.time_sync_handle.take(),
            self.event_poll_handle.take(),
            self.trend_log_handle.take(),
            self.monitor_handle.take(),
        ]
        .into_iter()
        .flatten()
        {
            h.abort();
        }

        // Restart COV + poll
        let cov_tc = tc.clone();
        let cov_store = store.clone();
        let cov_devices = self.devices.clone();
        let poll_interval = self.poll_interval;
        let cov_lifetime = self.cov_lifetime;
        let cov_event_bus = self.event_bus.clone();
        let cov_handle = tokio::spawn(async move {
            run_cov_with_poll_fallback(
                cov_tc,
                cov_store,
                &cov_devices,
                poll_interval,
                cov_lifetime,
                cov_event_bus,
            )
            .await;
        });
        self.cov_handle = Some(cov_handle);

        // Restart time sync
        let ts_tc = tc.clone();
        let ts_devices = self.devices.clone();
        let ts_handle = tokio::spawn(async move {
            run_time_sync_loop(ts_tc, &ts_devices).await;
        });
        self.time_sync_handle = Some(ts_handle);

        // Restart event poll
        let ev_tc = tc.clone();
        let ev_devices = self.devices.clone();
        let ev_event_bus = self.event_bus.clone();
        let ev_store = store.clone();
        let ev_handle = tokio::spawn(async move {
            run_event_poll_loop(ev_tc, ev_store, &ev_devices, ev_event_bus).await;
        });
        self.event_poll_handle = Some(ev_handle);

        // Restart trend log sync if applicable
        let has_trend_logs = self.devices.iter().any(|d| !d.trend_logs.is_empty());
        if has_trend_logs {
            if let Some(history_store) = &self.history_store {
                let tl_tc = tc.clone();
                let tl_devices = self.devices.clone();
                let tl_history = history_store.clone();
                let tl_sync_interval = self.trend_log_sync_interval;
                let tl_handle = tokio::spawn(async move {
                    run_trend_log_sync_loop(tl_tc, &tl_devices, tl_history, tl_sync_interval).await;
                });
                self.trend_log_handle = Some(tl_handle);
            }
        }

        // Restart background device monitor (A1 + A2)
        if !self.monitor_interval.is_zero() && !self.devices.is_empty() {
            let mon_tc = tc;
            let mon_instances = self.accepted_device_instances();
            let mon_event_bus = self.event_bus.clone();
            let mon_interval = self.monitor_interval;
            let mon_network_id = self.network_id.clone();
            let mon_object_check_cycles = self.object_check_cycles;
            let mon_device_object_counts: HashMap<u32, usize> = self
                .devices
                .iter()
                .map(|d| (d.device_id.instance(), d.objects.len()))
                .collect();
            let mon_device_addrs: HashMap<u32, rustbac_datalink::DataLinkAddress> = self
                .devices
                .iter()
                .map(|d| (d.device_id.instance(), d.address))
                .collect();
            let mon_handle = tokio::spawn(async move {
                run_device_monitor_loop(
                    mon_tc,
                    mon_instances,
                    mon_event_bus,
                    mon_interval,
                    mon_network_id,
                    mon_object_check_cycles,
                    mon_device_object_counts,
                    mon_device_addrs,
                )
                .await;
            });
            self.monitor_handle = Some(mon_handle);
        }

        Ok(())
    }
}
