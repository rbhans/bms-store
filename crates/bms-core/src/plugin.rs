//! Extension traits for optional protocol, history, alarm, logic, and import/export modules.
//!
//! Implement one or more of these traits to extend the platform's capabilities.

use std::pin::Pin;

use crate::alarm::{AlarmConfig, AlarmState};
use crate::protocol::BridgeError;
use crate::types::{NodeId, PointValue};

// ----------------------------------------------------------------
// Protocol plugin
// ----------------------------------------------------------------

/// Plugin that provides a protocol bridge (BACnet, Modbus, KNX, etc.)
///
/// Implement this trait to register a new protocol with the platform.
pub trait ProtocolPlugin: Send + Sync {
    /// Protocol identifier string (e.g. "bacnet", "modbus", "knx").
    fn protocol_id(&self) -> &str;
    /// Human-readable display name (e.g. "BACnet", "Modbus TCP/RTU").
    fn display_name(&self) -> &str;
}

/// A running protocol bridge handle — protocol-agnostic interface.
///
/// This is the object-safe trait that all bridges expose for write routing.
/// Protocol-specific operations are available via `as_any()` downcasting.
pub trait ProtocolBridgeHandle: Send + Sync {
    /// Write a value to a point on this bridge.
    fn write_point(
        &self,
        device_id: &str,
        point_id: &str,
        value: PointValue,
        priority: Option<u8>,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), BridgeError>> + Send + '_>>;

    /// Stop the bridge and clean up resources.
    fn stop(
        &mut self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), BridgeError>> + Send + '_>>;

    /// Downcast to protocol-specific bridge type.
    fn as_any(&self) -> &dyn std::any::Any;

    /// Mutable downcast to protocol-specific bridge type.
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

// ----------------------------------------------------------------
// History backend
// ----------------------------------------------------------------

/// Plugin that provides a history storage backend.
///
/// Implement this to store trend data in an external system
/// (e.g. InfluxDB, TimescaleDB, cloud storage).
pub trait HistoryBackend: Send + Sync {
    fn write_batch(
        &self,
        samples: Vec<HistorySample>,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), HistoryBackendError>> + Send + '_>>;

    fn query(
        &self,
        query: HistoryQuery,
    ) -> Pin<
        Box<
            dyn std::future::Future<Output = Result<HistoryResult, HistoryBackendError>>
                + Send
                + '_,
        >,
    >;
}

/// A single history sample (one point value at one time).
#[derive(Debug, Clone)]
pub struct HistorySample {
    pub node_id: NodeId,
    pub timestamp_ms: i64,
    pub value: f64,
}

impl HistorySample {
    pub fn new(node_id: impl Into<NodeId>, timestamp_ms: i64, value: f64) -> Self {
        Self {
            node_id: node_id.into(),
            timestamp_ms,
            value,
        }
    }
}

/// Query parameters for retrieving history data.
#[derive(Debug, Clone)]
pub struct HistoryQuery {
    pub node_id: NodeId,
    pub start_ms: i64,
    pub end_ms: i64,
    pub max_results: Option<i64>,
}

impl HistoryQuery {
    pub fn new(node_id: impl Into<NodeId>, start_ms: i64, end_ms: i64) -> Self {
        Self {
            node_id: node_id.into(),
            start_ms,
            end_ms,
            max_results: None,
        }
    }

    pub fn with_max_results(mut self, max: i64) -> Self {
        self.max_results = Some(max);
        self
    }
}

/// Result of a history query.
#[derive(Debug, Clone)]
pub struct HistoryResult {
    pub node_id: NodeId,
    pub samples: Vec<HistorySample>,
}

impl HistoryResult {
    pub fn new(node_id: impl Into<NodeId>, samples: Vec<HistorySample>) -> Self {
        Self {
            node_id: node_id.into(),
            samples,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HistoryBackendError {
    #[error("backend error: {0}")]
    Backend(String),
}

// ----------------------------------------------------------------
// Alarm evaluator
// ----------------------------------------------------------------

/// Plugin that provides custom alarm evaluation logic.
///
/// The platform calls `evaluate()` when a point value changes to determine
/// whether an alarm condition exists.
pub trait AlarmEvaluator: Send + Sync {
    fn evaluate(&self, config: &AlarmConfig, value: &PointValue, prev: AlarmState) -> AlarmState;
}

// ----------------------------------------------------------------
// Logic engine
// ----------------------------------------------------------------

/// Plugin that provides a custom logic/program engine.
pub trait LogicEnginePlugin: Send + Sync {
    fn name(&self) -> &str;
    fn evaluate(&self, ctx: &LogicContext) -> Vec<(NodeId, PointValue)>;
}

/// Context passed to a logic engine evaluation cycle.
pub struct LogicContext {
    pub tick_ms: i64,
    pub inputs: Vec<(NodeId, PointValue)>,
}

// ----------------------------------------------------------------
// Import/export
// ----------------------------------------------------------------

/// Plugin for importing/exporting node data in various formats.
pub trait ImportExportPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn supported_formats(&self) -> Vec<String>;
    fn import(&self, data: &[u8], format: &str) -> Result<Vec<ImportedNode>, ImportExportError>;
    fn export(&self, nodes: &[ExportNode], format: &str) -> Result<Vec<u8>, ImportExportError>;
}

/// A node imported from an external format.
#[derive(Debug, Clone)]
pub struct ImportedNode {
    pub id: NodeId,
    pub node_type: String,
    pub dis: String,
    pub parent_id: Option<NodeId>,
    pub tags: Vec<(String, Option<String>)>,
}

/// A node prepared for export.
#[derive(Debug, Clone)]
pub struct ExportNode {
    pub id: NodeId,
    pub node_type: String,
    pub dis: String,
    pub parent_id: Option<NodeId>,
    pub tags: Vec<(String, Option<String>)>,
    pub refs: Vec<(String, NodeId)>,
}

#[derive(Debug, thiserror::Error)]
pub enum ImportExportError {
    #[error("format error: {0}")]
    Format(String),
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),
}
