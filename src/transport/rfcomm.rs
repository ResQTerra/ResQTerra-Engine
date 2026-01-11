//! RFCOMM transport implementation for Bluetooth connections

use crate::transport::bt_discovery::{BtDiscovery, BtDiscoveryConfig, RelayDevice};
use crate::transport::traits::{TransportConnector, TransportStream};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use bluer::rfcomm::{SocketAddr as RfcommAddr, Stream as RfcommStream};
use bluer::Address;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// Default RFCOMM channel for ResQTerra relay service
pub const DEFAULT_RFCOMM_CHANNEL: u8 = 1;

/// RFCOMM stream wrapper implementing TransportStream
pub struct RfcommTransportStream {
    inner: RfcommStream,
    peer_addr: Address,
}

impl RfcommTransportStream {
    /// Create a new RFCOMM transport stream
    pub fn new(stream: RfcommStream, peer_addr: Address) -> Self {
        Self {
            inner: stream,
            peer_addr,
        }
    }

    /// Get the peer Bluetooth address
    pub fn peer_address(&self) -> Address {
        self.peer_addr
    }
}

impl AsyncRead for RfcommTransportStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for RfcommTransportStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

#[async_trait]
impl TransportStream for RfcommTransportStream {
    async fn shutdown(&mut self) -> Result<()> {
        tokio::io::AsyncWriteExt::shutdown(&mut self.inner).await?;
        Ok(())
    }
}

/// Configuration for RFCOMM connector
#[derive(Debug, Clone)]
pub struct RfcommConfig {
    /// Known relay address (if any)
    pub relay_address: Option<Address>,
    /// RFCOMM channel number
    pub channel: u8,
    /// Discovery configuration
    pub discovery: BtDiscoveryConfig,
}

impl Default for RfcommConfig {
    fn default() -> Self {
        Self {
            relay_address: None,
            channel: DEFAULT_RFCOMM_CHANNEL,
            discovery: BtDiscoveryConfig::default(),
        }
    }
}

/// RFCOMM connector for Bluetooth relay connections
pub struct RfcommConnector {
    config: RfcommConfig,
    /// Cached relay device from last discovery
    cached_relay: Option<RelayDevice>,
}

impl RfcommConnector {
    /// Create a new RFCOMM connector
    pub fn new(config: RfcommConfig) -> Self {
        Self {
            config,
            cached_relay: None,
        }
    }

    /// Create connector with a known relay address
    pub fn with_address(address: Address, channel: u8) -> Self {
        Self {
            config: RfcommConfig {
                relay_address: Some(address),
                channel,
                ..Default::default()
            },
            cached_relay: None,
        }
    }

    /// Discover and cache a relay device
    async fn discover_relay(&mut self) -> Result<RelayDevice> {
        let adapter = BtDiscovery::get_adapter().await?;
        let discovery = BtDiscovery::new(self.config.discovery.clone());
        let relay = discovery.find_best_relay(&adapter).await?;
        self.cached_relay = Some(relay.clone());
        Ok(relay)
    }
}

#[async_trait]
impl TransportConnector for RfcommConnector {
    type Stream = RfcommTransportStream;

    async fn connect(&self) -> Result<Self::Stream> {
        // Determine target address
        let target_addr = if let Some(addr) = self.config.relay_address {
            addr
        } else if let Some(ref relay) = self.cached_relay {
            relay.address
        } else {
            // Need to discover
            let adapter = BtDiscovery::get_adapter().await?;
            let discovery = BtDiscovery::new(self.config.discovery.clone());
            let relay = discovery.find_best_relay(&adapter).await?;
            relay.address
        };

        // Connect via RFCOMM
        let socket_addr = RfcommAddr::new(target_addr, self.config.channel);
        println!("[BT] Connecting to {} channel {}", target_addr, self.config.channel);

        let stream = RfcommStream::connect(socket_addr)
            .await
            .map_err(|e| anyhow!("RFCOMM connect failed: {}", e))?;

        println!("[BT] Connected to {}", target_addr);
        Ok(RfcommTransportStream::new(stream, target_addr))
    }

    fn name(&self) -> &'static str {
        "Bluetooth"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = RfcommConfig::default();
        assert!(config.relay_address.is_none());
        assert_eq!(config.channel, DEFAULT_RFCOMM_CHANNEL);
    }

    #[test]
    fn test_connector_with_address() {
        let addr = Address::new([0x00, 0x11, 0x22, 0x33, 0x44, 0x55]);
        let connector = RfcommConnector::with_address(addr, 5);
        assert_eq!(connector.config.relay_address, Some(addr));
        assert_eq!(connector.config.channel, 5);
    }
}
