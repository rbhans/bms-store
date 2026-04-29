use std::sync::Arc;

use rustbac_client::{BacnetClient, ObjectStoreHandler};
use rustbac_core::types::{ObjectType, PropertyId};
use rustbac_mstp::{MstpConfig, MstpTransport};

use crate::bridge::traits::BridgeError;
use crate::config::profile::PointValue;
use crate::store::point_store::{PointKey, PointStore};

use super::config::{resolve_interface_index, BacnetMode};
use super::conversion::{
    client_to_point_value, is_point_object, object_point_id, point_value_to_client,
};
use super::transport::TransportClient;
use super::{BacnetDevice, BacnetObject, TrendLogRef};

impl crate::bridge::traits::PointSource for super::BacnetBridge {
    async fn start(&mut self, store: PointStore) -> Result<(), BridgeError> {
        self.store = Some(store.clone());

        // Build optional server handler for inline request dispatch.
        let server_handler: Option<(Arc<ObjectStoreHandler>, u32)> =
            match (&self.server_object_store, self.server_device_instance) {
                (Some(obj_store), Some(dev_id)) => Some((
                    Arc::new(ObjectStoreHandler::new(Arc::clone(obj_store))),
                    dev_id,
                )),
                _ => None,
            };

        // 1. Create BACnet client (Normal, Foreign, or Secure Connect)
        match &self.bacnet_config.mode {
            BacnetMode::Normal => {
                let mut client = BacnetClient::new()
                    .await
                    .map_err(|e| BridgeError::ConnectionFailed(format!("BACnet/IP init: {e}")))?;
                if let Some((handler, dev_id)) = server_handler {
                    client = client.with_server_handler(handler, dev_id, 0);
                }
                let tc = TransportClient::Ip(Arc::new(client));
                self.start_with_transport(tc, store.clone()).await?;
            }
            BacnetMode::Foreign { bbmd_addr, ttl } => {
                tracing::info!(%bbmd_addr, ttl, "BACnet: registering as foreign device with BBMD");
                let mut client =
                    BacnetClient::new_foreign(*bbmd_addr, *ttl)
                        .await
                        .map_err(|e| {
                            BridgeError::ConnectionFailed(format!("BACnet/IP foreign init: {e}"))
                        })?;
                if let Some((handler, dev_id)) = server_handler {
                    client = client.with_server_handler(handler, dev_id, 0);
                }
                let tc = TransportClient::Ip(Arc::new(client));
                self.start_with_transport(tc, store.clone()).await?;
            }
            BacnetMode::SecureConnect { hub_endpoint } => {
                tracing::info!(hub_endpoint, "BACnet: connecting to SC hub");
                let mut client = BacnetClient::new_sc(hub_endpoint.clone())
                    .await
                    .map_err(|e| BridgeError::ConnectionFailed(format!("BACnet/SC init: {e}")))?;
                if let Some((handler, dev_id)) = server_handler {
                    client = client.with_server_handler(handler, dev_id, 0);
                }
                let tc = TransportClient::Sc(Arc::new(client));
                self.start_with_transport(tc, store.clone()).await?;
            }
            BacnetMode::Mstp {
                port,
                baud_rate,
                mac_address,
                max_master,
            } => {
                tracing::info!(port, baud_rate, mac_address, "BACnet: opening MS/TP");
                let config = MstpConfig {
                    port: port.clone(),
                    baud_rate: *baud_rate,
                    mac_address: *mac_address,
                    max_master: *max_master,
                    max_info_frames: 1,
                };
                let transport = MstpTransport::new(config)
                    .await
                    .map_err(|e| BridgeError::ConnectionFailed(format!("MS/TP init: {e}")))?;
                let mut client = BacnetClient::with_datalink(transport);
                if let Some((handler, dev_id)) = server_handler {
                    client = client.with_server_handler(handler, dev_id, 0);
                }
                let tc = TransportClient::Mstp(Arc::new(client));
                self.start_with_transport(tc, store.clone()).await?;
            }
            BacnetMode::Ipv6 {
                multicast_group,
                interface,
            } => {
                let if_index: u32 = resolve_interface_index(interface);
                tracing::info!(%multicast_group, if_index, "BACnet: binding IPv6 multicast");
                let mut client = BacnetClient::new_ipv6(*multicast_group, if_index)
                    .await
                    .map_err(|e| BridgeError::ConnectionFailed(format!("BACnet/IPv6 init: {e}")))?;
                if let Some((handler, dev_id)) = server_handler {
                    client = client.with_server_handler(handler, dev_id, 0);
                }
                let tc = TransportClient::Ip6(Arc::new(client));
                self.start_with_transport(tc, store.clone()).await?;
            }
        }

        let total_points: usize = self.devices.iter().map(|d| d.objects.len()).sum();
        tracing::info!(
            devices = self.devices.len(),
            points = total_points,
            "BACnet bridge started"
        );

        Ok(())
    }

    async fn stop(&mut self) -> Result<(), BridgeError> {
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
        self.transport = None;
        Ok(())
    }

    async fn write_point(
        &self,
        device_id: &str,
        point_id: &str,
        value: PointValue,
        priority: Option<u8>,
    ) -> Result<(), BridgeError> {
        let tc = self.require_transport()?;

        // Find the device and object
        let (dev, obj) = self
            .devices
            .iter()
            .flat_map(|d| d.objects.iter().map(move |o| (d, o)))
            .find(|(d, o)| {
                let dev_key = format!("bacnet-{}", d.device_id.instance());
                dev_key == device_id && object_point_id(o) == point_id
            })
            .ok_or_else(|| BridgeError::PointNotFound {
                device_id: device_id.to_string(),
                point_id: point_id.to_string(),
            })?;

        if !obj.writable {
            return Err(BridgeError::WriteRejected(format!(
                "Object {} is not writable",
                obj.object_id.instance()
            )));
        }

        let bac_value = point_value_to_client(&value, obj.object_id.object_type());
        with_client!(tc, |c| c
            .write_many(
                dev.address,
                &[(obj.object_id, PropertyId::PresentValue, bac_value, priority)],
            )
            .await
            .map_err(|e| BridgeError::Protocol(format!(
                "WriteProperty failed: {e}"
            ))))?;

        // Update PointStore immediately so value is reflected without waiting for next poll/COV
        if let Some(store) = &self.store {
            store.set(
                PointKey {
                    device_instance_id: device_id.to_string(),
                    point_id: point_id.to_string(),
                },
                value,
            );
        }

        Ok(())
    }
}

impl super::BacnetBridge {
    /// Inject test devices for integration testing (test-only).
    #[cfg(test)]
    pub fn inject_test_devices(&mut self, devices: Vec<BacnetDevice>) {
        let mut instances = std::collections::HashSet::new();
        for dev in &devices {
            instances.insert(dev.device_id.instance());
        }
        self.devices = devices;
        self.last_scan_instances = instances;
    }

    /// Internal helper: discover devices and start background loops.
    pub(super) async fn start_with_transport(
        &mut self,
        tc: TransportClient,
        store: PointStore,
    ) -> Result<(), BridgeError> {
        let discovery_timeout = self.discovery_timeout;
        // Store transport early so it's available even if discovery finds nothing
        self.transport = Some(tc.clone());
        // 2. Discover devices via Who-Is broadcast
        tracing::info!(
            timeout_secs = discovery_timeout.as_secs(),
            "BACnet: sending Who-Is broadcast"
        );
        let discovered = match with_client!(&tc, |c| c.who_is(None, discovery_timeout).await) {
            Ok(devs) => devs,
            Err(e) => {
                tracing::warn!("BACnet: discovery failed ({e}), no devices found");
                return Ok(());
            }
        };

        if discovered.is_empty() {
            tracing::info!("BACnet: no devices discovered on the network");
            return Ok(());
        }

        tracing::info!(count = discovered.len(), "BACnet: devices discovered");

        // 3. Walk each discovered device to enumerate objects
        let mut all_devices = Vec::new();
        for dev in &discovered {
            let device_id = match dev.device_id {
                Some(id) => id,
                None => continue,
            };

            tracing::debug!(instance = device_id.instance(), "BACnet: walking device");

            match with_client!(&tc, |c| rustbac_client::walk::walk_device(
                c,
                dev.address,
                device_id
            )
            .await)
            {
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

                    tracing::info!(
                        instance = device_id.instance(),
                        points = objects.len(),
                        trend_logs = trend_logs.len(),
                        "BACnet: device walked"
                    );

                    all_devices.push(BacnetDevice {
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
                        "BACnet: device walk failed: {e}"
                    );
                }
            }
        }

        // 4. Populate PointStore with discovered objects
        for dev in &all_devices {
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
        }

        self.devices = all_devices;

        // 5. Start all background loops (COV/poll, time sync, event poll, trend log)
        self.restart_background_loops(tc, store)?;

        Ok(())
    }
}
