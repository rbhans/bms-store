use std::collections::HashMap;

use crate::bridge::traits::BridgeError;
use crate::config::profile::PointValue;

use super::BacnetBridge;

/// Manages multiple concurrent BACnet network bridges, each identified by a network_id.
pub struct BacnetNetworks {
    bridges: HashMap<String, BacnetBridge>,
}

impl BacnetNetworks {
    pub fn new() -> Self {
        Self {
            bridges: HashMap::new(),
        }
    }

    /// Insert a bridge for the given network_id.
    pub fn insert(&mut self, network_id: String, bridge: BacnetBridge) {
        self.bridges.insert(network_id, bridge);
    }

    /// Remove a bridge by network_id (used for parallel scan: remove -> scan -> reinsert).
    pub fn remove(&mut self, network_id: &str) -> Option<BacnetBridge> {
        self.bridges.remove(network_id)
    }

    /// Get a bridge by network_id.
    pub fn get(&self, network_id: &str) -> Option<&BacnetBridge> {
        self.bridges.get(network_id)
    }

    /// Get a mutable bridge by network_id.
    pub fn get_mut(&mut self, network_id: &str) -> Option<&mut BacnetBridge> {
        self.bridges.get_mut(network_id)
    }

    /// Iterate all bridges.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &BacnetBridge)> {
        self.bridges.iter()
    }

    /// Iterate all bridges mutably.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&String, &mut BacnetBridge)> {
        self.bridges.iter_mut()
    }

    /// Returns all network IDs in sorted (deterministic) order.
    pub fn network_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.bridges.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// Returns true if there are no bridges.
    pub fn is_empty(&self) -> bool {
        self.bridges.is_empty()
    }

    /// Number of bridges.
    pub fn len(&self) -> usize {
        self.bridges.len()
    }

    /// Returns the first bridge (deterministic: sorted by network_id).
    /// Useful for operations that need any bridge but shouldn't silently fail
    /// when multiple networks exist.
    pub fn first(&self) -> Option<&BacnetBridge> {
        if self.bridges.is_empty() {
            return None;
        }
        if self.bridges.len() == 1 {
            return self.bridges.values().next();
        }
        // Deterministic: pick the lexicographically smallest network_id
        let mut keys: Vec<&String> = self.bridges.keys().collect();
        keys.sort();
        keys.first().and_then(|k| self.bridges.get(*k))
    }

    /// Returns the first bridge mutably (deterministic: sorted by network_id).
    pub fn first_mut(&mut self) -> Option<&mut BacnetBridge> {
        if self.bridges.is_empty() {
            return None;
        }
        if self.bridges.len() == 1 {
            return self.bridges.values_mut().next();
        }
        let mut keys: Vec<String> = self.bridges.keys().cloned().collect();
        keys.sort();
        keys.first().and_then(|k| self.bridges.get_mut(k))
    }

    /// Returns the single bridge if there's only one (convenience for simple setups).
    pub fn single(&self) -> Option<&BacnetBridge> {
        if self.bridges.len() == 1 {
            self.bridges.values().next()
        } else {
            None
        }
    }

    /// Returns the single bridge mutably if there's only one.
    pub fn single_mut(&mut self) -> Option<&mut BacnetBridge> {
        if self.bridges.len() == 1 {
            self.bridges.values_mut().next()
        } else {
            None
        }
    }

    /// Stop all bridges.
    pub async fn stop_all(&mut self) {
        use crate::bridge::traits::PointSource;
        for (network_id, bridge) in self.bridges.iter_mut() {
            tracing::info!(network_id, "Stopping BACnet bridge");
            let _ = bridge.stop().await;
        }
    }

    /// Parse a device_id string like "bacnet-1000" or "bacnet-ip-main-1000" into
    /// (Option<network_id>, instance). The instance is always the last segment.
    /// The network_id is the middle portion between "bacnet-" and the final "-{instance}".
    pub fn parse_device_id(device_id: &str) -> Option<(Option<String>, u32)> {
        let rest = device_id.strip_prefix("bacnet-")?;
        // The instance is always the last '-'-separated segment that parses as u32
        let instance: u32 = rest.rsplit('-').next().and_then(|s| s.parse().ok())?;
        // Network_id is everything between "bacnet-" and the final "-{instance}"
        let inst_str = instance.to_string();
        let network_id = if rest == inst_str {
            // Simple format: "bacnet-1000" -> no network qualifier
            None
        } else if let Some(prefix) = rest.strip_suffix(&format!("-{inst_str}")) {
            // Network-qualified: "bacnet-ip-main-1000" -> network "ip-main"
            if prefix.is_empty() {
                None
            } else {
                Some(prefix.to_string())
            }
        } else {
            None
        };
        Some((network_id, instance))
    }

    /// Find which network owns a device, using the device_id string for deterministic routing.
    /// Prefers the embedded network_id from the device_id (e.g., "bacnet-ip-main-1000" -> "ip-main").
    /// Falls back to scanning all bridges by instance number.
    pub fn find_network_for_device_id(&self, device_id: &str) -> Option<String> {
        if let Some((Some(net_id), _instance)) = Self::parse_device_id(device_id) {
            // Direct match -- the device_id embeds its network
            if self.bridges.contains_key(&net_id) {
                return Some(net_id);
            }
        }
        // Fall back to instance-based search (legacy "bacnet-1000" format or "default" network)
        if let Some((_net, instance)) = Self::parse_device_id(device_id) {
            return self
                .find_network_for_device(instance)
                .map(|s| s.to_string());
        }
        None
    }

    /// Find which network owns a given device instance ID (by checking each bridge's device list).
    pub fn find_network_for_device(&self, device_instance: u32) -> Option<&str> {
        for (network_id, bridge) in &self.bridges {
            for dev in bridge.discovered_devices() {
                if dev.device_id.instance() == device_instance {
                    return Some(network_id);
                }
            }
        }
        None
    }

    /// Get a bridge for a specific device, using the full device_id for deterministic routing.
    /// Prefers network_id embedded in device_id; falls back to instance scan; then first bridge.
    pub fn bridge_for_device_id(&self, device_id: &str) -> Option<&BacnetBridge> {
        if let Some(net_id) = self.find_network_for_device_id(device_id) {
            return self.bridges.get(&net_id);
        }
        self.first()
    }

    /// Get a bridge for a specific device instance.
    /// Returns the bridge that has this device in its discovered list.
    /// Falls back to the first bridge if no match is found (the device
    /// may not be in the discovery list yet but might still be reachable).
    pub fn bridge_for_device(&self, device_instance: u32) -> Option<&BacnetBridge> {
        if let Some(net_id) = self.find_network_for_device(device_instance) {
            return self.bridges.get(net_id);
        }
        self.first()
    }

    /// Find the bridge managing a given device, using full device_id (mutable).
    pub fn bridge_for_device_id_mut(&mut self, device_id: &str) -> Option<&mut BacnetBridge> {
        let net_id = Self::parse_device_id(device_id).and_then(|(net, inst)| {
            if let Some(net_id) = net {
                if self.bridges.contains_key(&net_id) {
                    return Some(net_id);
                }
            }
            self.find_network_for_device(inst).map(|s| s.to_string())
        });
        if let Some(ref nid) = net_id {
            return self.bridges.get_mut(nid);
        }
        self.first_mut()
    }

    /// Find the bridge managing a given device instance (mutable).
    pub fn bridge_for_device_mut(&mut self, device_instance: u32) -> Option<&mut BacnetBridge> {
        // Need to find network first, then get_mut (can't borrow twice)
        let net_id = self
            .find_network_for_device(device_instance)
            .map(|s| s.to_string());
        if let Some(ref nid) = net_id {
            return self.bridges.get_mut(nid);
        }
        self.first_mut()
    }
}

impl Default for BacnetNetworks {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::plugin::ProtocolBridgeHandle for BacnetNetworks {
    fn write_point(
        &self,
        device_id: &str,
        point_id: &str,
        value: PointValue,
        priority: Option<u8>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), BridgeError>> + Send + '_>>
    {
        use crate::bridge::traits::PointSource as _;
        let device_id = device_id.to_string();
        let point_id = point_id.to_string();
        Box::pin(async move {
            // Direct routing first -- use device_id to find the right network
            if let Some(net_id) = self.find_network_for_device_id(&device_id) {
                if let Some(bridge) = self.get(&net_id) {
                    match bridge
                        .write_point(&device_id, &point_id, value.clone(), priority)
                        .await
                    {
                        Ok(()) => return Ok(()),
                        Err(BridgeError::PointNotFound { .. }) => {}
                        Err(e) => return Err(e),
                    }
                }
            }
            // Fall back to iterating all networks
            for (_net_id, bridge) in self.iter() {
                match bridge
                    .write_point(&device_id, &point_id, value.clone(), priority)
                    .await
                {
                    Ok(()) => return Ok(()),
                    Err(BridgeError::PointNotFound { .. }) => continue,
                    Err(e) => return Err(e),
                }
            }
            Err(BridgeError::PointNotFound {
                device_id,
                point_id,
            })
        })
    }

    fn stop(
        &mut self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), BridgeError>> + Send + '_>>
    {
        Box::pin(async move {
            self.stop_all().await;
            Ok(())
        })
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
