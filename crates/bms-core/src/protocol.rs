//! Protocol abstraction — traits for building protocol bridges.
//!
//! The new push-based model: protocol drivers push [`RawProtocolValue`]s
//! to a [`ValueSink`], which normalizes and stores them.

use std::pin::Pin;

use crate::node::ProtocolBinding;
use crate::types::PointValue;

/// Raw protocol value from a bridge before normalization.
///
/// Protocol-agnostic: any protocol pushes raw data as JSON with a protocol tag.
#[derive(Debug, Clone)]
pub struct RawProtocolValue {
    /// Protocol identifier (e.g. "bacnet", "modbus", "knx")
    pub protocol: String,
    /// Device key within the protocol (e.g. device instance, host:unit combo)
    pub device_key: String,
    /// Point key within the device (e.g. object type+instance, register address)
    pub point_key: String,
    /// Raw data from the protocol — interpretation is protocol-specific
    pub raw_data: serde_json::Value,
}

impl RawProtocolValue {
    pub fn new(
        protocol: impl Into<String>,
        device_key: impl Into<String>,
        point_key: impl Into<String>,
        raw_data: serde_json::Value,
    ) -> Self {
        Self {
            protocol: protocol.into(),
            device_key: device_key.into(),
            point_key: point_key.into(),
            raw_data,
        }
    }
}

/// Trait for protocol drivers — the new push-based protocol abstraction.
///
/// Drivers push raw values to a [`ValueSink`] instead of writing to a store directly.
/// This decouples protocol logic from storage.
///
/// All methods return `Pin<Box<dyn Future>>` for dyn-compatibility, allowing the
/// platform to store heterogeneous drivers as `Box<dyn ProtocolDriver>`.
pub trait ProtocolDriver: Send {
    fn start(
        &mut self,
        sink: Box<dyn ValueSink>,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), DriverError>> + Send + '_>>;

    fn stop(
        &mut self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), DriverError>> + Send + '_>>;

    fn write(
        &self,
        binding: &ProtocolBinding,
        value: PointValue,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), DriverError>> + Send + '_>>;

    fn protocol_name(&self) -> &str;
}

/// Receives raw values from protocol drivers.
pub trait ValueSink: Send + Sync {
    /// Called when a new raw value arrives from a device.
    fn on_value(&self, raw: RawProtocolValue);
    /// Called when a device goes online or offline.
    fn on_device_status(&self, device_key: &str, online: bool);
}

/// Errors from protocol drivers.
#[derive(Debug, thiserror::Error)]
pub enum DriverError {
    #[error("connection failed: {0}")]
    ConnectionFailed(String),
    #[error("write rejected: {0}")]
    WriteRejected(String),
    #[error("protocol error: {0}")]
    Protocol(String),
}

/// Errors from bridge operations (used by [`ProtocolBridgeHandle`](crate::plugin::ProtocolBridgeHandle)).
#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Point not found: device={device_id}, point={point_id}")]
    PointNotFound { device_id: String, point_id: String },
    #[error("Write rejected: {0}")]
    WriteRejected(String),
    #[error("Protocol error: {0}")]
    Protocol(String),
}
