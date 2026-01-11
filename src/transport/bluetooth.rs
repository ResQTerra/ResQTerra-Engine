//! Bluetooth transport layer using RFCOMM

use crate::transport::rfcomm::{RfcommConfig, RfcommConnector};
use crate::transport::traits::TransportConnector;
use anyhow::Result;
use async_trait::async_trait;
use bluer::Address;

/// Bluetooth connector for establishing a connection to a relay node
pub struct BluetoothConnector {
    /// Inner RFCOMM connector
    inner: RfcommConnector,
}

impl BluetoothConnector {
    /// Create a new Bluetooth connector with a specific target address
    pub fn new(relay_address: Address) -> Self {
        let config = RfcommConfig {
            relay_address: Some(relay_address),
            ..Default::default()
        };
        Self {
            inner: RfcommConnector::new(config),
        }
    }

    /// Create a new Bluetooth connector that discovers the best relay
    pub fn new_discovered() -> Self {
        let config = RfcommConfig::default();
        Self {
            inner: RfcommConnector::new(config),
        }
    }
}

use crate::transport::traits::TransportStream;
use std::pin::Pin;

#[async_trait]
impl TransportConnector for BluetoothConnector {
    /// Connect to the Bluetooth relay
    async fn connect(&self) -> Result<Pin<Box<dyn TransportStream>>> {
        println!("[Transport] Attempting to connect via Bluetooth...");
        self.inner.connect().await
    }

    /// Get the transport name
    fn name(&self) -> &'static str {
        "Bluetooth"
    }
}