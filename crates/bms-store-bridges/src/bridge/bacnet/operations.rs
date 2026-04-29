use std::net::SocketAddrV4;
use std::time::Duration;

use rustbac_client::{
    BroadcastDistributionEntry, ClientDataValue, DiscoveredObject, ForeignDeviceTableEntry,
};
use rustbac_core::services::device_management::{DeviceCommunicationState, ReinitializeState};
use rustbac_core::services::subscribe_cov_property::SubscribeCovPropertyRequest;
use rustbac_core::types::{ObjectId, PropertyId};
use rustbac_datalink::BacnetIpTransport;

use crate::bridge::traits::BridgeError;
use rustbac_client::BacnetClient;

use super::conversion::client_to_point_value;
use super::loop_time_sync::now_bacnet_utc;
use super::transport::TransportClient;
use super::{BacnetDevice, BacnetEventInfo, PriorityArrayInfo, RouterEntry};

use crate::config::profile::PointValue;

impl super::BacnetBridge {
    // -----------------------------------------------------------------------
    // Device management operations
    // -----------------------------------------------------------------------

    pub(super) fn require_transport(&self) -> Result<&TransportClient, BridgeError> {
        self.transport
            .as_ref()
            .ok_or_else(|| BridgeError::ConnectionFailed("BACnet bridge not started".into()))
    }

    /// Reboot a BACnet device (coldstart or warmstart).
    pub async fn reinitialize_device(
        &self,
        device_instance: u32,
        warmstart: bool,
    ) -> Result<(), BridgeError> {
        let tc = self.require_transport()?;
        let dev = self.find_device(device_instance)?;
        let state = if warmstart {
            ReinitializeState::Warmstart
        } else {
            ReinitializeState::Coldstart
        };
        with_client!(tc, |c| c
            .reinitialize_device(dev.address, state, None)
            .await
            .map_err(|e| BridgeError::Protocol(format!(
                "ReinitializeDevice failed: {e}"
            ))))?;
        Ok(())
    }

    /// Enable or disable communication on a BACnet device.
    pub async fn device_communication_control(
        &self,
        device_instance: u32,
        enable: bool,
        duration_minutes: Option<u16>,
    ) -> Result<(), BridgeError> {
        let tc = self.require_transport()?;
        let dev = self.find_device(device_instance)?;
        let state = if enable {
            DeviceCommunicationState::Enable
        } else {
            DeviceCommunicationState::Disable
        };
        let duration_secs = duration_minutes.map(|m| m.saturating_mul(60));
        with_client!(tc, |c| c
            .device_communication_control(dev.address, duration_secs, state, None)
            .await
            .map_err(|e| BridgeError::Protocol(format!(
                "DeviceCommunicationControl failed: {e}"
            ))))?;
        Ok(())
    }

    /// Synchronize time on a BACnet device to the current system UTC time.
    pub async fn sync_time(&self, device_instance: u32) -> Result<(), BridgeError> {
        let tc = self.require_transport()?;
        let dev = self.find_device(device_instance)?;
        let (date, time) = now_bacnet_utc();
        with_client!(tc, |c| c
            .time_synchronize(dev.address, date, time, true)
            .await
            .map_err(|e| BridgeError::Protocol(format!(
                "TimeSynchronization failed: {e}"
            ))))?;
        Ok(())
    }

    /// Poll the device for active event/alarm information.
    pub async fn get_event_info(
        &self,
        device_instance: u32,
    ) -> Result<Vec<BacnetEventInfo>, BridgeError> {
        let tc = self.require_transport()?;
        let dev = self.find_device(device_instance)?;
        let result = with_client!(tc, |c| c
            .get_event_information(dev.address, None)
            .await
            .map_err(|e| BridgeError::Protocol(format!("GetEventInformation failed: {e}"))))?;
        Ok(result
            .summaries
            .into_iter()
            .map(|s| BacnetEventInfo {
                object_id: s.object_id,
                event_state: s.event_state_raw,
                acknowledged_transitions: Some(s.acknowledged_transitions.data),
                notify_type: Some(s.notify_type),
                event_enable: Some(s.event_enable.data),
                event_priorities: Some(s.event_priorities),
            })
            .collect())
    }

    // -----------------------------------------------------------------------
    // BBMD management (BACnet/IP only)
    // -----------------------------------------------------------------------

    /// Require that the transport is BACnet/IP; return a reference to the inner
    /// `BacnetClient<BacnetIpTransport>`.  Returns an error for SC and MS/TP
    /// transports since BBMD operations are only defined for BACnet/IP.
    fn require_ip_transport(
        &self,
    ) -> Result<&std::sync::Arc<BacnetClient<BacnetIpTransport>>, BridgeError> {
        match self.require_transport()? {
            TransportClient::Ip(c) => Ok(c),
            TransportClient::Sc(_) | TransportClient::Mstp(_) | TransportClient::Ip6(_) => {
                Err(BridgeError::Protocol(
                    "BBMD operations are only supported on BACnet/IP transport".into(),
                ))
            }
        }
    }

    /// Read the Broadcast Distribution Table from the BBMD.
    pub async fn read_bdt(&self) -> Result<Vec<BroadcastDistributionEntry>, BridgeError> {
        let client = self.require_ip_transport()?;
        client
            .read_broadcast_distribution_table()
            .await
            .map_err(|e| {
                BridgeError::Protocol(format!("ReadBroadcastDistributionTable failed: {e}"))
            })
    }

    /// Write (replace) the Broadcast Distribution Table on the BBMD.
    pub async fn write_bdt(
        &self,
        entries: &[BroadcastDistributionEntry],
    ) -> Result<(), BridgeError> {
        let client = self.require_ip_transport()?;
        client
            .write_broadcast_distribution_table(entries)
            .await
            .map_err(|e| {
                BridgeError::Protocol(format!("WriteBroadcastDistributionTable failed: {e}"))
            })
    }

    /// Read the Foreign Device Table from the BBMD.
    pub async fn read_fdt(&self) -> Result<Vec<ForeignDeviceTableEntry>, BridgeError> {
        let client = self.require_ip_transport()?;
        client
            .read_foreign_device_table()
            .await
            .map_err(|e| BridgeError::Protocol(format!("ReadForeignDeviceTable failed: {e}")))
    }

    /// Delete a specific entry from the BBMD's Foreign Device Table.
    pub async fn delete_fdt_entry(&self, address: SocketAddrV4) -> Result<(), BridgeError> {
        let client = self.require_ip_transport()?;
        client
            .delete_foreign_device_table_entry(address)
            .await
            .map_err(|e| {
                BridgeError::Protocol(format!("DeleteForeignDeviceTableEntry failed: {e}"))
            })
    }

    // -----------------------------------------------------------------------
    // Discovery (Who-Has)
    // -----------------------------------------------------------------------

    /// Broadcast Who-Has by ObjectId and collect I-Have responses.
    pub async fn who_has_by_id(
        &self,
        object_id: ObjectId,
        timeout: Duration,
    ) -> Result<Vec<DiscoveredObject>, BridgeError> {
        let tc = self.require_transport()?;
        with_client!(tc, |c| c
            .who_has_object_id(None, object_id, timeout)
            .await
            .map_err(|e| BridgeError::Protocol(format!(
                "WhoHas(id) failed: {e}"
            ))))
    }

    /// Broadcast Who-Has by object name and collect I-Have responses.
    pub async fn who_has_by_name(
        &self,
        name: &str,
        timeout: Duration,
    ) -> Result<Vec<DiscoveredObject>, BridgeError> {
        let tc = self.require_transport()?;
        with_client!(tc, |c| c
            .who_has_object_name(None, name, timeout)
            .await
            .map_err(|e| BridgeError::Protocol(format!(
                "WhoHas(name) failed: {e}"
            ))))
    }

    // -----------------------------------------------------------------------
    // Private transfer
    // -----------------------------------------------------------------------

    /// Send a confirmed private transfer request for vendor-specific integrations.
    pub async fn private_transfer(
        &self,
        device_instance: u32,
        vendor_id: u32,
        service_number: u32,
        params: Option<&[u8]>,
    ) -> Result<(u32, u32, Option<Vec<u8>>), BridgeError> {
        let tc = self.require_transport()?;
        let dev = self.find_device(device_instance)?;
        let ack = with_client!(tc, |c| c
            .private_transfer(dev.address, vendor_id, service_number, params)
            .await
            .map_err(|e| BridgeError::Protocol(format!("PrivateTransfer failed: {e}"))))?;
        Ok((ack.vendor_id, ack.service_number, ack.result_block))
    }

    // -----------------------------------------------------------------------
    // COV property subscriptions
    // -----------------------------------------------------------------------

    /// Subscribe to changes of a specific property on a remote BACnet device.
    pub async fn subscribe_cov_property(
        &self,
        device_instance: u32,
        object_id: ObjectId,
        property_id: PropertyId,
        cov_increment: Option<f32>,
        lifetime: u32,
    ) -> Result<(), BridgeError> {
        let tc = self.require_transport()?;
        let dev = self.find_device(device_instance)?;
        let request = SubscribeCovPropertyRequest {
            subscriber_process_id: 0,
            monitored_object_id: object_id,
            issue_confirmed_notifications: Some(false),
            lifetime_seconds: Some(lifetime),
            monitored_property_id: property_id,
            monitored_property_array_index: None,
            cov_increment,
            invoke_id: 0,
        };
        with_client!(tc, |c| c
            .subscribe_cov_property(dev.address, request)
            .await
            .map_err(|e| BridgeError::Protocol(format!(
                "SubscribeCOVProperty failed: {e}"
            ))))
    }

    /// Cancel a COV property subscription on a remote BACnet device.
    pub async fn cancel_cov_property_subscription(
        &self,
        device_instance: u32,
        object_id: ObjectId,
        property_id: PropertyId,
    ) -> Result<(), BridgeError> {
        let tc = self.require_transport()?;
        let dev = self.find_device(device_instance)?;
        with_client!(tc, |c| c
            .cancel_cov_property_subscription(dev.address, 0, object_id, property_id, None)
            .await
            .map_err(|e| BridgeError::Protocol(format!(
                "CancelCOVProperty failed: {e}"
            ))))
    }

    // -----------------------------------------------------------------------
    // Network routing (Phase 5D)
    // -----------------------------------------------------------------------

    /// Send a Who-Is-Router-To-Network request.
    ///
    /// `network_number` -- if `Some`, asks which router can reach that
    /// specific network; if `None`, asks for all reachable networks.
    ///
    /// Returns an error because the underlying BACnet client library does not
    /// yet support sending network-layer messages.  In most deployments this
    /// is handled by dedicated router appliances.
    pub async fn who_is_router_to_network(
        &self,
        _network_number: Option<u16>,
    ) -> Result<Vec<RouterEntry>, BridgeError> {
        let _tc = self.require_transport()?;
        Err(BridgeError::Protocol(
            "Who-Is-Router-To-Network is not yet supported: the BACnet client library \
             does not expose network-layer (NPDU) message APIs. In most deployments, \
             network routing is handled by dedicated router hardware."
                .to_string(),
        ))
    }

    /// Query the local routing table (I-Am-Router-To-Network responses).
    ///
    /// Returns an error because the underlying BACnet client library does not
    /// yet support network-layer message reception.
    pub async fn get_routing_table(&self) -> Result<Vec<RouterEntry>, BridgeError> {
        let _tc = self.require_transport()?;
        Err(BridgeError::Protocol(
            "Network routing table queries are not yet supported: the BACnet client \
             library does not expose network-layer (NPDU) message APIs. In most \
             deployments, network routing is handled by dedicated router hardware."
                .to_string(),
        ))
    }

    /// Read the priority array and relinquish default for a writable BACnet object.
    pub async fn read_priority_array(
        &self,
        device_instance: u32,
        object_id: ObjectId,
    ) -> Result<PriorityArrayInfo, BridgeError> {
        let tc = self.require_transport()?;
        let dev = self
            .devices
            .iter()
            .find(|d| d.device_id.instance() == device_instance)
            .ok_or_else(|| BridgeError::PointNotFound {
                device_id: format!("bacnet-{device_instance}"),
                point_id: format!("{:?}", object_id),
            })?;

        // Read PriorityArray (Constructed with 16 elements)
        let pa_value = with_client!(tc, |c| c
            .read_property(dev.address, object_id, PropertyId::PriorityArray)
            .await
            .map_err(|e| BridgeError::Protocol(format!("ReadPriorityArray: {e}"))))?;

        let mut levels: [Option<PointValue>; 16] = Default::default();
        if let ClientDataValue::Constructed { values, .. } = &pa_value {
            for (i, val) in values.iter().enumerate().take(16) {
                levels[i] = match val {
                    ClientDataValue::Null => None,
                    other => Some(client_to_point_value(other, object_id.object_type())),
                };
            }
        }

        // Read RelinquishDefault
        let rd_value = with_client!(tc, |c| c
            .read_property(dev.address, object_id, PropertyId::RelinquishDefault)
            .await);
        let relinquish_default = match rd_value {
            Ok(ref v) if !matches!(v, ClientDataValue::Null) => {
                Some(client_to_point_value(v, object_id.object_type()))
            }
            _ => None,
        };

        Ok(PriorityArrayInfo {
            levels,
            relinquish_default,
        })
    }

    pub(super) fn find_device(&self, device_instance: u32) -> Result<&BacnetDevice, BridgeError> {
        self.devices
            .iter()
            .find(|d| d.device_id.instance() == device_instance)
            .ok_or_else(|| BridgeError::PointNotFound {
                device_id: format!("bacnet-{device_instance}"),
                point_id: String::new(),
            })
    }
}
