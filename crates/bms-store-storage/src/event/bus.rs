// Re-exported from the bms-core crate — the canonical definitions live there.
pub use bms_core::{Event, EventBus, EventJournalBackend, EventSeq};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::profile::PointValue;

    #[tokio::test]
    async fn publish_subscribe_roundtrip() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();

        bus.publish(Event::ValueChanged {
            node_id: "ahu-1/dat".into(),
            value: PointValue::Float(72.5),
            timestamp_ms: 1000,
        });

        let event = rx.recv().await.unwrap();
        match event.as_ref() {
            Event::ValueChanged { node_id, .. } => {
                assert_eq!(node_id, "ahu-1/dat");
            }
            _ => panic!("wrong event type"),
        }
    }

    #[tokio::test]
    async fn multiple_subscribers() {
        let bus = EventBus::new();
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        bus.publish(Event::EntityCreated {
            entity_id: "vav-1".into(),
        });

        assert!(rx1.recv().await.is_ok());
        assert!(rx2.recv().await.is_ok());
    }

    #[test]
    fn no_subscribers_is_ok() {
        let bus = EventBus::new();
        // Should not panic
        bus.publish(Event::DeviceDiscovered {
            bridge_type: "bacnet".into(),
            device_key: "bacnet-1000".into(),
        });
    }
}
