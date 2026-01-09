//! Individual drone session handling

use anyhow::Result;
use resqterra_shared::{
    codec::{self, FrameDecoder},
    safety, Envelope, DroneState,
};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex};

/// Handle to send messages to a specific drone
#[derive(Clone)]
pub struct SessionHandle {
    pub device_id: String,
    pub addr: SocketAddr,
    writer: Arc<Mutex<WriteHalf<TcpStream>>>,
    pub connected_at: Instant,
    pub last_heartbeat: Arc<Mutex<Instant>>,
}

impl SessionHandle {
    /// Send an envelope to this drone
    pub async fn send(&self, envelope: &Envelope) -> Result<()> {
        let encoded = codec::encode(envelope)?;
        let mut writer = self.writer.lock().await;
        writer.write_all(&encoded).await?;
        Ok(())
    }

    /// Check if the session is still alive (heartbeat not timed out)
    pub async fn is_alive(&self) -> bool {
        let last = *self.last_heartbeat.lock().await;
        last.elapsed().as_millis() < safety::HEARTBEAT_TIMEOUT_MS as u128
    }

    /// Update the last heartbeat time
    pub async fn update_heartbeat(&self) {
        *self.last_heartbeat.lock().await = Instant::now();
    }

    /// Get time since last heartbeat
    pub async fn time_since_heartbeat(&self) -> std::time::Duration {
        self.last_heartbeat.lock().await.elapsed()
    }
}

/// Active drone session
pub struct DroneSession {
    pub handle: SessionHandle,
    reader: ReadHalf<TcpStream>,
    decoder: FrameDecoder,
    read_buf: Vec<u8>,
}

impl DroneSession {
    /// Create a new drone session from a TCP stream
    pub fn new(stream: TcpStream, addr: SocketAddr) -> Self {
        let (reader, writer) = tokio::io::split(stream);
        let now = Instant::now();

        let handle = SessionHandle {
            device_id: String::new(), // Will be set on first message
            addr,
            writer: Arc::new(Mutex::new(writer)),
            connected_at: now,
            last_heartbeat: Arc::new(Mutex::new(now)),
        };

        Self {
            handle,
            reader,
            decoder: FrameDecoder::new(),
            read_buf: vec![0u8; 4096],
        }
    }

    /// Get a cloneable handle for sending messages
    pub fn get_handle(&self) -> SessionHandle {
        self.handle.clone()
    }

    /// Read the next envelope from this session
    /// Returns None if the connection is closed
    pub async fn recv(&mut self) -> Option<Envelope> {
        loop {
            // First try to decode from existing buffer
            match self.decoder.decode_next() {
                Ok(Some(envelope)) => {
                    // Update device ID from header if not set
                    if self.handle.device_id.is_empty() {
                        if let Some(ref header) = envelope.header {
                            self.handle.device_id = header.device_id.clone();
                        }
                    }

                    // Update heartbeat time for heartbeat messages
                    if let Some(resqterra_shared::envelope::Payload::Heartbeat(_)) = &envelope.payload {
                        self.handle.update_heartbeat().await;
                    }

                    return Some(envelope);
                }
                Ok(None) => {
                    // Need more data
                }
                Err(e) => {
                    eprintln!("Decode error from {}: {}", self.handle.addr, e);
                    return None;
                }
            }

            // Read more data
            match self.reader.read(&mut self.read_buf).await {
                Ok(0) => return None, // Connection closed
                Ok(n) => {
                    self.decoder.extend(&self.read_buf[..n]);
                }
                Err(e) => {
                    eprintln!("Read error from {}: {}", self.handle.addr, e);
                    return None;
                }
            }
        }
    }

    /// Get the device ID (may be empty until first message received)
    pub fn device_id(&self) -> &str {
        &self.handle.device_id
    }

    /// Get the remote address
    pub fn addr(&self) -> SocketAddr {
        self.handle.addr
    }
}

/// Drone state tracked by the server
#[derive(Debug, Clone)]
pub struct DroneInfo {
    pub device_id: String,
    pub addr: SocketAddr,
    pub state: DroneState,
    pub last_heartbeat: Instant,
    pub connected_at: Instant,
    pub pending_commands: u32,
}

impl DroneInfo {
    pub fn new(device_id: String, addr: SocketAddr) -> Self {
        let now = Instant::now();
        Self {
            device_id,
            addr,
            state: DroneState::DroneUnknown,
            last_heartbeat: now,
            connected_at: now,
            pending_commands: 0,
        }
    }
}
