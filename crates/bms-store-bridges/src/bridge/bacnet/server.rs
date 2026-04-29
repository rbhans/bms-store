use std::sync::Arc;

use rustbac_client::{ClientDataValue, ObjectStore};
use rustbac_core::types::{ObjectId, ObjectType, PropertyId};

use crate::config::profile::PointValue;
use crate::store::point_store::PointStore;

impl super::BacnetBridge {
    // -----------------------------------------------------------------------
    // BACnet server (exposes local points as BACnet objects)
    // -----------------------------------------------------------------------

    /// Initialize the server object store and populate it with current point values.
    ///
    /// This creates the `ObjectStore` that represents local points as BACnet
    /// objects. When the bridge starts, the server handler is attached inline
    /// to the BACnet client so incoming requests are dispatched automatically.
    pub fn init_server_store(&mut self, device_instance: u32, store: &PointStore) {
        let object_store = Arc::new(ObjectStore::new());

        // Populate the object store with current point values from PointStore.
        // Each point is exposed as a BACnet analog-value object keyed by its
        // position in the store.
        let keys = store.all_keys();
        let mut obj_index: u32 = 1; // start object instances at 1
        for key in &keys {
            if let Some(entry) = store.get(key) {
                let object_id = ObjectId::new(ObjectType::AnalogValue, obj_index);
                let cdv = match &entry.value {
                    PointValue::Float(f) => ClientDataValue::Real(*f as f32),
                    PointValue::Integer(i) => ClientDataValue::Real(*i as f32),
                    PointValue::Bool(b) => ClientDataValue::Real(if *b { 1.0 } else { 0.0 }),
                };
                object_store.set(object_id, PropertyId::PresentValue, cdv);
                // Also set ObjectName from the point key
                object_store.set(
                    object_id,
                    PropertyId::ObjectName,
                    ClientDataValue::CharacterString(format!(
                        "{}/{}",
                        key.device_instance_id, key.point_id
                    )),
                );
                obj_index += 1;
            }
        }

        self.server_device_instance = Some(device_instance);
        self.server_object_store = Some(object_store);

        tracing::info!(
            device_instance,
            objects = obj_index - 1,
            "BACnet: server object store initialized"
        );
    }

    /// Get a reference to the server's object store for syncing point values.
    pub fn server_object_store(&self) -> Option<&Arc<ObjectStore>> {
        self.server_object_store.as_ref()
    }

    /// Get the configured server device instance number.
    pub fn server_device_instance(&self) -> Option<u32> {
        self.server_device_instance
    }
}
