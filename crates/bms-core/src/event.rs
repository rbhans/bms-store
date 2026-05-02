//! Event bus for cross-system communication.
//!
//! The [`EventBus`] is the central event channel for the platform. Subsystems
//! publish events here, and consumers can subscribe to react to changes.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::types::PointValue;

// ---------------------------------------------------------------------------
// Quality types (used by Event::QualityChanged)
// ---------------------------------------------------------------------------

/// Reason for a quality-change event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QualityReason {
    /// Point has not been updated within `expected_poll_interval × N_TOLERANCE`.
    Stale,
    /// The owning bridge reported it cannot reach the device/network.
    BridgeDown,
    /// Point has returned to normal quality after a Stale or BridgeDown event.
    Recovered,
    /// Quality flag set by operator or override logic.
    ManualOverride,
    /// Point has been placed out-of-service.
    OutOfService,
}

/// Monotonic sequence number for journaled events.
pub type EventSeq = i64;

/// Platform-wide event for cross-system communication.
///
/// Subscribe to the [`EventBus`] to receive these events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    ValueChanged {
        node_id: String,
        value: PointValue,
        timestamp_ms: i64,
    },
    StatusChanged {
        node_id: String,
        flags: u8,
    },
    EntityCreated {
        entity_id: String,
    },
    EntityUpdated {
        entity_id: String,
    },
    EntityDeleted {
        entity_id: String,
    },
    DeviceDiscovered {
        bridge_type: String,
        device_key: String,
    },
    DeviceDown {
        bridge_type: String,
        device_key: String,
    },
    DeviceRecovered {
        bridge_type: String,
        device_key: String,
    },
    DeviceAccepted {
        device_key: String,
        protocol: String,
        point_count: usize,
    },
    DiscoveryScanComplete {
        protocol: String,
        device_count: usize,
    },
    DeviceMonitorCycle {
        protocol: String,
        network_id: String,
        online: usize,
        offline: usize,
        new_devices: usize,
    },
    ObjectListChanged {
        device_key: String,
        old_count: usize,
        new_count: usize,
    },
    FddFaultRaised {
        fault_id: i64,
        rule_id: i64,
        equip_id: String,
        severity: String,
    },
    FddFaultCleared {
        fault_id: i64,
        rule_id: i64,
        equip_id: String,
    },
    /// A point's quality flags changed (e.g. became stale, bridge went down, recovered).
    ///
    /// Edge-triggered: only emitted when the flags actually change state.
    QualityChanged {
        node_id: String,
        flags: u8,
        reason: QualityReason,
    },
    /// A bridge-level quality event covering all points owned by a bridge/network.
    ///
    /// Emitted once when a bridge goes down or recovers, instead of thousands
    /// of individual QualityChanged events.  Consumers can use this as a
    /// bulk-invalidation signal.
    BridgeQualityChanged {
        bridge_type: String,
        network_id: String,
        reason: QualityReason,
        affected_device_count: usize,
    },
    /// Operator-facing notification — surfaces in the GUI as a toast banner.
    ///
    /// Use this for failures and important state changes that the operator
    /// would otherwise only see in the log file (bridge errors, scan
    /// failures, config-write failures, etc.). Consumers subscribe to the
    /// event bus and route Toast events into their UI's notification queue.
    Toast {
        level: ToastLevel,
        /// Short single-line message (≤120 chars). Goes in the toast title.
        message: String,
        /// Optional longer body shown when the toast is expanded.
        detail: Option<String>,
        /// Subsystem that emitted this — `"bridge.bacnet"`, `"discovery"`,
        /// `"storage"`, etc. Lets the GUI group/filter by source.
        source: String,
    },
}

/// Severity for [`Event::Toast`] notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToastLevel {
    Info,
    Warn,
    Error,
}

impl Event {
    /// Returns the event type discriminant as a static string.
    pub fn event_type_name(&self) -> &'static str {
        match self {
            Event::ValueChanged { .. } => "ValueChanged",
            Event::StatusChanged { .. } => "StatusChanged",
            Event::EntityCreated { .. } => "EntityCreated",
            Event::EntityUpdated { .. } => "EntityUpdated",
            Event::EntityDeleted { .. } => "EntityDeleted",
            Event::DeviceDiscovered { .. } => "DeviceDiscovered",
            Event::DeviceDown { .. } => "DeviceDown",
            Event::DeviceRecovered { .. } => "DeviceRecovered",
            Event::DeviceAccepted { .. } => "DeviceAccepted",
            Event::DiscoveryScanComplete { .. } => "DiscoveryScanComplete",
            Event::DeviceMonitorCycle { .. } => "DeviceMonitorCycle",
            Event::ObjectListChanged { .. } => "ObjectListChanged",
            Event::FddFaultRaised { .. } => "FddFaultRaised",
            Event::FddFaultCleared { .. } => "FddFaultCleared",
            Event::QualityChanged { .. } => "QualityChanged",
            Event::BridgeQualityChanged { .. } => "BridgeQualityChanged",
            Event::Toast { .. } => "Toast",
        }
    }
}

impl Event {
    /// Convenience for emitting a Toast event with no detail body.
    pub fn toast(level: ToastLevel, source: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Toast {
            level,
            message: message.into(),
            detail: None,
            source: source.into(),
        }
    }

    /// Convenience for emitting a Toast event with a detail body.
    pub fn toast_with_detail(
        level: ToastLevel,
        source: impl Into<String>,
        message: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self::Toast {
            level,
            message: message.into(),
            detail: Some(detail.into()),
            source: source.into(),
        }
    }
}

/// Trait for durable event journal backends.
///
/// Implemented in the storage crate (SQLite-backed); bms-core only defines
/// the interface so that `EventBus` can optionally persist events without
/// depending on `rusqlite`.
pub trait EventJournalBackend: Send + Sync + 'static {
    /// Persist an event to the journal. Implementations must be non-blocking
    /// (use internal buffering / channel).
    fn append(&self, event: &Event);
}

const BUS_CAPACITY: usize = 4096;

/// Broadcast-based event bus. `Arc<Event>` avoids cloning large payloads.
///
/// When constructed with [`EventBus::with_journal`], every published event is
/// also persisted to a durable backend for crash recovery and replay.
///
/// # Usage
///
/// ```rust
/// use bms_core::{EventBus, Event, PointValue};
///
/// let bus = EventBus::new();
/// let mut rx = bus.subscribe();
///
/// bus.publish(Event::ValueChanged {
///     node_id: "ahu-1/dat".into(),
///     value: PointValue::Float(72.5),
///     timestamp_ms: 1000,
/// });
/// ```
#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<Arc<Event>>,
    journal: Option<Arc<dyn EventJournalBackend>>,
}

impl EventBus {
    /// Create an in-memory-only event bus (the default).
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(BUS_CAPACITY);
        EventBus { tx, journal: None }
    }

    /// Create an event bus with an attached durable journal.
    pub fn with_journal(journal: Arc<dyn EventJournalBackend>) -> Self {
        let (tx, _) = broadcast::channel(BUS_CAPACITY);
        EventBus {
            tx,
            journal: Some(journal),
        }
    }

    /// Publish an event to all subscribers (and to the journal, if attached).
    pub fn publish(&self, event: Event) {
        if let Some(ref j) = self.journal {
            j.append(&event);
        }
        let _ = self.tx.send(Arc::new(event));
    }

    /// Subscribe to receive events.
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<Event>> {
        self.tx.subscribe()
    }

    /// Returns true if a durable journal is attached.
    pub fn has_journal(&self) -> bool {
        self.journal.is_some()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
