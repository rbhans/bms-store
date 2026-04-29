use std::sync::Arc;

use rustbac_bacnet_sc::BacnetScTransport;
use rustbac_client::BacnetClient;
use rustbac_datalink::BacnetIpTransport;
use rustbac_mstp::MstpTransport;

/// Internal enum that wraps both BACnet/IP and BACnet/SC client types,
/// allowing the bridge to be non-generic while supporting both transports.
#[derive(Clone)]
pub(crate) enum TransportClient {
    Ip(Arc<BacnetClient<BacnetIpTransport>>),
    Sc(Arc<BacnetClient<BacnetScTransport>>),
    Mstp(Arc<BacnetClient<MstpTransport>>),
    Ip6(Arc<BacnetClient<rustbac_datalink::BacnetIp6Transport>>),
}
