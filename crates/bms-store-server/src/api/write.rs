use crate::bridge::traits::BridgeError;
use crate::config::profile::PointValue;
use crate::plugin::BridgeRegistry;

#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    #[error("point not found: {device_id}/{point_id}")]
    PointNotFound { device_id: String, point_id: String },
    #[error("bridge error: {0}")]
    Bridge(String),
}

/// Route a write to the appropriate protocol bridge via BridgeRegistry.
/// Tries each registered bridge; PointNotFound means "not mine, try next".
pub async fn route_write(
    device_id: &str,
    point_id: &str,
    value: PointValue,
    priority: Option<u8>,
    registry: &BridgeRegistry,
) -> Result<(), WriteError> {
    registry
        .route_write(device_id, point_id, value, priority)
        .await
        .map_err(|e| match e {
            BridgeError::PointNotFound {
                device_id,
                point_id,
            } => WriteError::PointNotFound {
                device_id,
                point_id,
            },
            other => WriteError::Bridge(other.to_string()),
        })
}
