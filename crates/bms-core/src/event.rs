//! Event bus for cross-system communication.
//!
//! The [`EventBus`] is the central event channel for the platform. Subsystems
//! publish events here, and consumers can subscribe to react to changes.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::types::PointValue;

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
    AlarmRaised {
        alarm_id: i64,
        node_id: String,
    },
    AlarmCleared {
        alarm_id: i64,
        node_id: String,
    },
    AlarmAcknowledged {
        alarm_id: i64,
    },
    ScheduleWritten {
        assignment_id: i64,
        node_id: String,
        value: PointValue,
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
}

impl Event {
    /// Returns the event type discriminant as a static string.
    pub fn event_type_name(&self) -> &'static str {
        match self {
            Event::ValueChanged { .. } => "ValueChanged",
            Event::StatusChanged { .. } => "StatusChanged",
            Event::AlarmRaised { .. } => "AlarmRaised",
            Event::AlarmCleared { .. } => "AlarmCleared",
            Event::AlarmAcknowledged { .. } => "AlarmAcknowledged",
            Event::ScheduleWritten { .. } => "ScheduleWritten",
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
