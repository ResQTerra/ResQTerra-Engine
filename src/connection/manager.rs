//! Connection manager with persistent connections and automatic reconnection

use crate::transport::{
    bluetooth::BluetoothConnector,
    tcp::TcpConnector,
    traits::{TransportConnector, TransportStream},
};
use anyhow::{anyhow, Result};
use bluer::Address as BtAddress;
use resqterra_shared::{
    codec::{self, FrameDecoder},
    safety, DroneState, Envelope, Header, Heartbeat, MessageType,
};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
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

/// Configuration for connection manager
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// Device ID for this edge device
    pub device_id: String,
    /// 5G server address
    pub server_5g: String,
    /// Known relay Bluetooth address (MAC)
    pub relay_address: Option<String>,
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
            relay_address: None,
            reconnect_delay: Duration::from_secs(1),
            max_reconnect_delay: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(5),
            read_timeout: Duration::from_secs(15), // > heartbeat timeout
        }
    }
}

/// Manages persistent connection to server with failover
pub struct ConnectionManager {
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
            outbound_tx,
            event_rx,
        }
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

    /// Get a clone of the sender for outbound messages
    pub fn get_sender(&self) -> mpsc::Sender<Envelope> {
        self.outbound_tx.clone()
    }
}

/// Main connection loop with reconnection logic
async fn connection_loop(
    config: ConnectionConfig,
    sequence_id: Arc<AtomicU64>,
    mut outbound_rx: mpsc::Receiver<Envelope>,
    event_tx: mpsc::Sender<ConnectionEvent>,
) {
    let connectors = create_connectors(&config);
    let mut reconnect_delay = config.reconnect_delay;

    loop {
        let mut connected_stream: Option<Pin<Box<dyn TransportStream>>> = None;

        for (transport, connector) in &connectors {
            println!("[Manager] Trying transport: {}", connector.name());
            let connect_result = timeout(config.connect_timeout, connector.connect()).await;

            match connect_result {
                Ok(Ok(stream)) => {
                    connected_stream = Some(stream);
                    let _ = event_tx
                        .send(ConnectionEvent::Connected {
                            transport: *transport,
                        })
                        .await;
                    break;
                }
                Ok(Err(e)) => {
                    eprintln!(
                        "[Manager] Failed to connect with {}: {}",
                        connector.name(),
                        e
                    );
                    if *transport == Transport::FiveG {
                        let _ = event_tx
                            .send(ConnectionEvent::TransportSwitched {
                                from: Transport::FiveG,
                                to: Transport::Bluetooth,
                            })
                            .await;
                    }
                }
                Err(_) => {
                    eprintln!("[Manager] Timeout connecting with {}", connector.name());
                    if *transport == Transport::FiveG {
                        let _ = event_tx
                            .send(ConnectionEvent::TransportSwitched {
                                from: Transport::FiveG,
                                to: Transport::Bluetooth,
                            })
                            .await;
                    }
                }
            }
        }

        if let Some(stream) = connected_stream {
            reconnect_delay = config.reconnect_delay; // Reset delay

            if let Err(reason) =
                handle_connection(stream, &config, &sequence_id, &mut outbound_rx, &event_tx).await
            {
                let _ = event_tx
                    .send(ConnectionEvent::Disconnected {
                        reason: reason.to_string(),
                    })
                    .await;
            }
        } else {
            let _ = event_tx
                .send(ConnectionEvent::ConnectionFailed {
                    reason: "All transports failed".to_string(),
                })
                .await;

            // Wait before reconnecting
            tokio::time::sleep(reconnect_delay).await;

            // Exponential backoff
            reconnect_delay = std::cmp::min(reconnect_delay * 2, config.max_reconnect_delay);
        }
    }
}

fn create_connectors(config: &ConnectionConfig) -> Vec<(Transport, Box<dyn TransportConnector>)> {
    let mut connectors: Vec<(Transport, Box<dyn TransportConnector>)> = Vec::new();

    // 5G (TCP)
    connectors.push((
        Transport::FiveG,
        Box::new(TcpConnector::new(&config.server_5g, "5G")),
    ));

    // Bluetooth
    if let Some(addr_str) = &config.relay_address {
        if let Ok(addr) = addr_str.parse::<BtAddress>() {
            connectors.push((
                Transport::Bluetooth,
                Box::new(BluetoothConnector::new(addr)),
            ));
        } else {
            eprintln!("[Manager] Invalid Bluetooth address: {}", addr_str);
        }
    } else {
        connectors.push((
            Transport::Bluetooth,
            Box::new(BluetoothConnector::new_discovered()),
        ));
    }

    connectors
}

/// Handle an active connection
async fn handle_connection(
    mut stream: Pin<Box<dyn TransportStream>>,
    config: &ConnectionConfig,
    sequence_id: &Arc<AtomicU64>,
    outbound_rx: &mut mpsc::Receiver<Envelope>,
    event_tx: &mpsc::Sender<ConnectionEvent>,
) -> Result<()> {
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
                stream.write_all(&encoded).await?;
            }

            // Send outbound messages
            Some(envelope) = outbound_rx.recv() => {
                let encoded = codec::encode(&envelope)?;
                stream.write_all(&encoded).await?;
            }

            // Read incoming messages
            result = timeout(config.read_timeout, stream.read(&mut read_buf)) => {
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
