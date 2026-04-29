use crate::config::profile::PointValue;
use crate::store::point_store::PointStore;

// Re-exported from the bms-core crate — the canonical definition lives there.
pub use bms_core::BridgeError;

pub trait PointSource {
    fn start(
        &mut self,
        store: PointStore,
    ) -> impl std::future::Future<Output = Result<(), BridgeError>> + Send;

    fn stop(&mut self) -> impl std::future::Future<Output = Result<(), BridgeError>> + Send;

    fn write_point(
        &self,
        device_id: &str,
        point_id: &str,
        value: PointValue,
        priority: Option<u8>,
    ) -> impl std::future::Future<Output = Result<(), BridgeError>> + Send;
}
