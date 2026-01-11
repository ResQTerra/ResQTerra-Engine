//! Bluetooth device discovery for finding relay nodes

use anyhow::{anyhow, Result};
use bluer::{Adapter, Address, Device};
use std::collections::HashSet;
use std::time::Duration;
use tokio::time::timeout;
use tracing::info;

/// Configuration for Bluetooth discovery
#[derive(Debug, Clone)]
pub struct BtDiscoveryConfig {
    /// How long to scan for devices
    pub scan_duration: Duration,
    /// Known relay MAC addresses (preferred)
    pub known_relays: Vec<Address>,
    /// Device name prefix to match
    pub name_prefix: Option<String>,
}

impl Default for BtDiscoveryConfig {
    fn default() -> Self {
        Self {
            scan_duration: Duration::from_secs(10),
            known_relays: Vec::new(),
            name_prefix: Some("ResQTerra-Relay".into()),
        }
    }
}

/// Information about a discovered relay device
#[derive(Debug, Clone)]
pub struct RelayDevice {
    /// Bluetooth MAC address
    pub address: Address,
    /// Signal strength (if available)
    pub rssi: Option<i16>,
}

/// Bluetooth device discovery service
pub struct BtDiscovery {
    config: BtDiscoveryConfig,
}

impl BtDiscovery {
    /// Create a new discovery service
    pub fn new(config: BtDiscoveryConfig) -> Self {
        Self { config }
    }

    /// Get the default Bluetooth adapter
    pub async fn get_adapter() -> Result<Adapter> {
        let session = bluer::Session::new().await?;
        let adapter = session.default_adapter().await?;
        adapter.set_powered(true).await?;
        Ok(adapter)
    }

    /// Discover relay devices
    pub async fn discover_relays(&self, adapter: &Adapter) -> Result<Vec<RelayDevice>> {
        let mut relays = Vec::new();
        let mut seen: HashSet<Address> = HashSet::new();

        // First, check known relays
        for &addr in &self.config.known_relays {
            if let Ok(device) = adapter.device(addr) {
                if let Ok(true) = device.is_connected().await {
                    relays.push(RelayDevice {
                        address: addr,
                        rssi: device.rssi().await.ok().flatten(),
                    });
                    seen.insert(addr);
                }
            }
        }

        // Start discovery
        let discover = adapter.discover_devices().await?;
        tokio::pin!(discover);

        // Scan for the configured duration
        let scan_result = timeout(self.config.scan_duration, async {
            use futures::StreamExt;
            while let Some(evt) = discover.next().await {
                if let bluer::AdapterEvent::DeviceAdded(addr) = evt {
                    if seen.contains(&addr) {
                        continue;
                    }

                    if let Ok(device) = adapter.device(addr) {
                        if self.is_relay_device(&device).await {
                            relays.push(RelayDevice {
                                address: addr,
                                rssi: device.rssi().await.ok().flatten(),
                            });
                            seen.insert(addr);
                        }
                    }
                }
            }
        })
        .await;

        // Timeout is expected, not an error
        if scan_result.is_err() {
            info!("[BT] Discovery scan completed");
        }

        // Sort by signal strength (strongest first)
        relays.sort_by(|a, b| {
            let rssi_a = a.rssi.unwrap_or(i16::MIN);
            let rssi_b = b.rssi.unwrap_or(i16::MIN);
            rssi_b.cmp(&rssi_a)
        });

        Ok(relays)
    }

    /// Check if a device is a relay (by name prefix or known address)
    async fn is_relay_device(&self, device: &Device) -> bool {
        // Check if it's a known relay
        let addr = device.address();
        if self.config.known_relays.contains(&addr) {
            return true;
        }

        // Check name prefix
        if let Some(ref prefix) = self.config.name_prefix {
            if let Ok(Some(name)) = device.name().await {
                if name.starts_with(prefix) {
                    return true;
                }
            }
        }

        false
    }

    /// Find the best relay device (strongest signal)
    pub async fn find_best_relay(&self, adapter: &Adapter) -> Result<RelayDevice> {
        let relays = self.discover_relays(adapter).await?;
        relays
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("No relay devices found"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = BtDiscoveryConfig::default();
        assert_eq!(config.scan_duration, Duration::from_secs(10));
        assert!(config.known_relays.is_empty());
        assert_eq!(config.name_prefix, Some("ResQTerra-Relay".into()));
    }
}
