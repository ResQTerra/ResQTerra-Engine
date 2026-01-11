//! TCP transport implementation for 5G and relay connections

use crate::transport::traits::{TransportConnector, TransportStream};
use anyhow::Result;
use async_trait::async_trait;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;

/// TCP stream wrapper implementing TransportStream
pub struct TcpTransportStream {
    inner: TcpStream,
}

impl TcpTransportStream {
    pub fn new(stream: TcpStream) -> Self {
        Self { inner: stream }
    }
}

impl AsyncRead for TcpTransportStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for TcpTransportStream {
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
impl TransportStream for TcpTransportStream {
    async fn shutdown(&mut self) -> Result<()> {
        tokio::io::AsyncWriteExt::shutdown(&mut self.inner).await?;
        Ok(())
    }
}

/// TCP connector for connecting to a server address
pub struct TcpConnector {
    address: String,
    name: &'static str,
}

impl TcpConnector {
    /// Create a new TCP connector
    pub fn new(address: &str, name: &'static str) -> Self {
        Self {
            address: address.to_string(),
            name,
        }
    }
}

#[async_trait]
impl TransportConnector for TcpConnector {
    async fn connect(&self) -> Result<Pin<Box<dyn TransportStream>>> {
        let stream = TcpStream::connect(&self.address).await?;
        Ok(Box::pin(TcpTransportStream::new(stream)))
    }

    fn name(&self) -> &'static str {
        self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tcp_connector_names() {
        let five_g = TcpConnector::new("127.0.0.1:8080", "5G");
        assert_eq!(five_g.name(), "5G");

        let relay = TcpConnector::new("127.0.0.1:9000", "Relay");
        assert_eq!(relay.name(), "Relay");
    }
}
