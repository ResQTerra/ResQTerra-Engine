//! Connection manager with persistent connections and automatic reconnection

use anyhow::{anyhow, Result};
use bluer::rfcomm::{SocketAddr as RfcommAddr, Stream as RfcommStream};
use bluer::Address as BtAddress;
use resqterra_shared::{
    codec::{self, FrameDecoder},
    safety, DroneState, Envelope, Header, Heartbeat, MessageType,
};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{interval, timeout, Instant};

/// Events emitted by the connection manager
#[derive(Debug, Clone)]
pub enum ConnectionEvent {
    /// Successfully connected to server
    Connected { transport: Transport },
    /// Disconnected from server
    Disconnected { reason: String },
    /// Received an envelope from server
    Received(Envelope),
    /// Failed to connect after all retries
    ConnectionFailed { reason: String },
    /// Transport switched (e.g., 5G -> Bluetooth)
    TransportSwitched { from: Transport, to: Transport },
}

/// Available transport types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    FiveG,
    Bluetooth,
}

impl std::fmt::Display for Transport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Transport::FiveG => write!(f, "5G"),
            Transport::Bluetooth => write!(f, "Bluetooth"),
        }
    }
}

/// Bluetooth transport mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BluetoothMode {
    /// Use real RFCOMM Bluetooth (requires BlueZ)
    Rfcomm,
    /// Use TCP simulation (for development)
    #[default]
    TcpSimulation,
}

/// Bluetooth configuration
#[derive(Debug, Clone)]
pub struct BluetoothConfig {
    /// Bluetooth transport mode
    pub mode: BluetoothMode,
    /// Known relay Bluetooth address (MAC)
    pub relay_address: Option<String>,
    /// RFCOMM channel number
    pub channel: u8,
    /// TCP simulation address (when mode is TcpSimulation)
    pub tcp_address: String,
}

impl Default for BluetoothConfig {
    fn default() -> Self {
        Self {
            mode: BluetoothMode::TcpSimulation,
            relay_address: None,
            channel: 1,
            tcp_address: "127.0.0.1:9000".into(),
        }
    }
}

/// Configuration for connection manager
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// Device ID for this edge device
    pub device_id: String,
    /// 5G server address
    pub server_5g: String,
    /// Bluetooth configuration
    pub bluetooth: BluetoothConfig,
    /// Reconnection delay (initial)
    pub reconnect_delay: Duration,
    /// Maximum reconnection delay
    pub max_reconnect_delay: Duration,
    /// Connection timeout
    pub connect_timeout: Duration,
    /// Read timeout (should be > heartbeat interval)
    pub read_timeout: Duration,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            device_id: "edge-001".into(),
            server_5g: "127.0.0.1:8080".into(),
            bluetooth: BluetoothConfig::default(),
            reconnect_delay: Duration::from_secs(1),
            max_reconnect_delay: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(5),
            read_timeout: Duration::from_secs(15), // > heartbeat timeout
        }
    }
}

/// A unified stream that can be either TCP or RFCOMM
enum ConnectionStream {
    Tcp(TcpStream),
    Rfcomm(RfcommStream),
}

impl ConnectionStream {
    /// Split the stream into read and write halves
    fn into_split(self) -> (ConnectionReader, ConnectionWriter) {
        match self {
            ConnectionStream::Tcp(stream) => {
                let (r, w) = stream.into_split();
                (ConnectionReader::Tcp(r), ConnectionWriter::Tcp(w))
            }
            ConnectionStream::Rfcomm(stream) => {
                let (r, w) = stream.into_split();
                (ConnectionReader::Rfcomm(r), ConnectionWriter::Rfcomm(w))
            }
        }
    }
}

/// Read half of a connection
enum ConnectionReader {
    Tcp(tokio::net::tcp::OwnedReadHalf),
    Rfcomm(bluer::rfcomm::stream::OwnedReadHalf),
}

impl ConnectionReader {
    async fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            ConnectionReader::Tcp(r) => r.read(buf).await,
            ConnectionReader::Rfcomm(r) => r.read(buf).await,
        }
    }
}

/// Write half of a connection
enum ConnectionWriter {
    Tcp(tokio::net::tcp::OwnedWriteHalf),
    Rfcomm(bluer::rfcomm::stream::OwnedWriteHalf),
}

impl ConnectionWriter {
    async fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        match self {
            ConnectionWriter::Tcp(w) => w.write_all(buf).await,
            ConnectionWriter::Rfcomm(w) => w.write_all(buf).await,
        }
    }
}

/// Manages persistent connection to server with failover
pub struct ConnectionManager {
    config: ConnectionConfig,
    sequence_id: Arc<AtomicU64>,
    /// Channel to send envelopes to the server
    outbound_tx: mpsc::Sender<Envelope>,
    /// Channel to receive connection events
    event_rx: mpsc::Receiver<ConnectionEvent>,
}

impl ConnectionManager {
    /// Create a new connection manager and start the connection loop
    pub fn new(config: ConnectionConfig) -> Self {
        let (outbound_tx, outbound_rx) = mpsc::channel::<Envelope>(100);
        let (event_tx, event_rx) = mpsc::channel::<ConnectionEvent>(100);
        let sequence_id = Arc::new(AtomicU64::new(0));

        // Spawn the connection loop
        let config_clone = config.clone();
        let seq_clone = sequence_id.clone();
        tokio::spawn(async move {
            connection_loop(config_clone, seq_clone, outbound_rx, event_tx).await;
        });

        Self {
            config,
            sequence_id,
            outbound_tx,
            event_rx,
        }
    }

    /// Get the next sequence ID
    pub fn next_sequence_id(&self) -> u64 {
        self.sequence_id.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Send an envelope to the server
    pub async fn send(&self, envelope: Envelope) -> Result<()> {
        self.outbound_tx
            .send(envelope)
            .await
            .map_err(|_| anyhow!("Connection closed"))
    }

    /// Receive the next connection event
    pub async fn recv(&mut self) -> Option<ConnectionEvent> {
        self.event_rx.recv().await
    }

    /// Get the device ID
    pub fn device_id(&self) -> &str {
        &self.config.device_id
    }

    /// Get a clone of the sender for outbound messages
    pub fn get_sender(&self) -> mpsc::Sender<Envelope> {
        self.outbound_tx.clone()
    }
}

/// Connect via Bluetooth (either RFCOMM or TCP simulation)
async fn connect_bluetooth(config: &BluetoothConfig) -> Result<ConnectionStream> {
    match config.mode {
        BluetoothMode::TcpSimulation => {
            let stream = TcpStream::connect(&config.tcp_address).await?;
            Ok(ConnectionStream::Tcp(stream))
        }
        BluetoothMode::Rfcomm => {
            let addr = config
                .relay_address
                .as_ref()
                .ok_or_else(|| anyhow!("No relay address configured for RFCOMM mode"))?;

            let bt_addr: BtAddress = addr
                .parse()
                .map_err(|_| anyhow!("Invalid Bluetooth address: {}", addr))?;

            let socket_addr = RfcommAddr::new(bt_addr, config.channel);
            println!("[BT] Connecting via RFCOMM to {} channel {}", bt_addr, config.channel);

            let stream = RfcommStream::connect(socket_addr)
                .await
                .map_err(|e| anyhow!("RFCOMM connect failed: {}", e))?;

            println!("[BT] Connected via RFCOMM to {}", bt_addr);
            Ok(ConnectionStream::Rfcomm(stream))
        }
    }
}

/// Main connection loop with reconnection logic
async fn connection_loop(
    config: ConnectionConfig,
    sequence_id: Arc<AtomicU64>,
    mut outbound_rx: mpsc::Receiver<Envelope>,
    event_tx: mpsc::Sender<ConnectionEvent>,
) {
    let mut current_transport = Transport::FiveG;
    let mut reconnect_delay = config.reconnect_delay;

    loop {
        // Try to connect
        let connect_result: Result<ConnectionStream> = match current_transport {
            Transport::FiveG => {
                match timeout(config.connect_timeout, TcpStream::connect(&config.server_5g)).await {
                    Ok(Ok(stream)) => Ok(ConnectionStream::Tcp(stream)),
                    Ok(Err(e)) => Err(anyhow!("5G connection failed: {}", e)),
                    Err(_) => Err(anyhow!("5G connection timeout")),
                }
            }
            Transport::Bluetooth => {
                match timeout(config.connect_timeout, connect_bluetooth(&config.bluetooth)).await {
                    Ok(Ok(stream)) => Ok(stream),
                    Ok(Err(e)) => Err(anyhow!("Bluetooth connection failed: {}", e)),
                    Err(_) => Err(anyhow!("Bluetooth connection timeout")),
                }
            }
        };

        match connect_result {
            Ok(stream) => {
                // Connected successfully
                reconnect_delay = config.reconnect_delay; // Reset delay

                let _ = event_tx
                    .send(ConnectionEvent::Connected {
                        transport: current_transport,
                    })
                    .await;

                // Run the connection handler
                if let Err(reason) = handle_connection(
                    stream,
                    &config,
                    &sequence_id,
                    &mut outbound_rx,
                    &event_tx,
                )
                .await
                {
                    let _ = event_tx
                        .send(ConnectionEvent::Disconnected {
                            reason: reason.to_string(),
                        })
                        .await;
                }
            }
            Err(e) => {
                // Connection failed, try fallback
                if current_transport == Transport::FiveG {
                    let _ = event_tx
                        .send(ConnectionEvent::TransportSwitched {
                            from: Transport::FiveG,
                            to: Transport::Bluetooth,
                        })
                        .await;
                    current_transport = Transport::Bluetooth;
                    continue; // Try Bluetooth immediately
                } else {
                    // Both transports failed
                    let _ = event_tx
                        .send(ConnectionEvent::ConnectionFailed {
                            reason: format!("All transports failed: {}", e),
                        })
                        .await;
                }
            }
        }

        // Wait before reconnecting
        tokio::time::sleep(reconnect_delay).await;

        // Exponential backoff
        reconnect_delay = std::cmp::min(reconnect_delay * 2, config.max_reconnect_delay);

        // Reset to primary transport for next attempt
        current_transport = Transport::FiveG;
    }
}

/// Handle an active connection
async fn handle_connection(
    stream: ConnectionStream,
    config: &ConnectionConfig,
    sequence_id: &Arc<AtomicU64>,
    outbound_rx: &mut mpsc::Receiver<Envelope>,
    event_tx: &mpsc::Sender<ConnectionEvent>,
) -> Result<()> {
    let (mut reader, mut writer) = stream.into_split();

    let mut decoder = FrameDecoder::new();
    let mut read_buf = vec![0u8; 4096];

    // Heartbeat interval
    let mut heartbeat_interval = interval(Duration::from_millis(safety::HEARTBEAT_INTERVAL_MS));
    let start_time = Instant::now();

    loop {
        tokio::select! {
            // Send heartbeat
            _ = heartbeat_interval.tick() => {
                let seq = sequence_id.fetch_add(1, Ordering::SeqCst) + 1;
                let uptime_ms = start_time.elapsed().as_millis() as u64;

                let envelope = Envelope {
                    header: Some(Header::new(&config.device_id, MessageType::MsgHeartbeat, seq)),
                    payload: Some(resqterra_shared::envelope::Payload::Heartbeat(
                        Heartbeat::new(uptime_ms, DroneState::DroneIdle, 0, true),
                    )),
                };

                let encoded = codec::encode(&envelope)?;
                writer.write_all(&encoded).await?;
            }

            // Send outbound messages
            Some(envelope) = outbound_rx.recv() => {
                let encoded = codec::encode(&envelope)?;
                writer.write_all(&encoded).await?;
            }

            // Read incoming messages
            result = timeout(config.read_timeout, reader.read(&mut read_buf)) => {
                match result {
                    Ok(Ok(0)) => {
                        return Err(anyhow!("Server closed connection"));
                    }
                    Ok(Ok(n)) => {
                        decoder.extend(&read_buf[..n]);

                        // Process all complete frames
                        while let Ok(Some(envelope)) = decoder.decode_next() {
                            let _ = event_tx.send(ConnectionEvent::Received(envelope)).await;
                        }
                    }
                    Ok(Err(e)) => {
                        return Err(anyhow!("Read error: {}", e));
                    }
                    Err(_) => {
                        // Read timeout - this is expected if server doesn't send data
                        // We'll rely on heartbeat responses to detect disconnection
                    }
                }
            }
        }
    }
}
