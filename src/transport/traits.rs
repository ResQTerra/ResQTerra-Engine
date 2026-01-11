//! Transport trait abstraction for pluggable network backends

use anyhow::Result;
use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncWrite};

/// A transport stream that can read and write bytes
#[async_trait]
pub trait TransportStream: AsyncRead + AsyncWrite + Send + Unpin + 'static {
    /// Close the transport gracefully
    async fn shutdown(&mut self) -> Result<()>;
}

/// Factory for creating transport connections
#[async_trait]
pub trait TransportConnector: Send + Sync {
    /// The stream type this connector produces
    type Stream: TransportStream;

    /// Attempt to connect, returning a stream on success
    async fn connect(&self) -> Result<Self::Stream>;

    /// Human-readable name for this transport
    fn name(&self) -> &'static str;
}
