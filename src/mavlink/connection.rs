//! Flight Controller Connection
//!
//! Manages connection to ArduPilot/PX4 flight controllers via serial or UDP.

use anyhow::{anyhow, Result};
use mavlink::ardupilotmega::MavMessage;
use mavlink::{MavConnection, MavHeader};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

/// Connection type for flight controller
#[derive(Debug, Clone)]
pub enum FcConnectionType {
    /// Serial port connection (e.g., "/dev/ttyACM0" or "/dev/serial0")
    Serial { port: String, baud: u32 },
    /// UDP connection (e.g., "127.0.0.1:14550")
    Udp { address: String },
    /// TCP connection (e.g., "127.0.0.1:5760")
    Tcp { address: String },
}

impl Default for FcConnectionType {
    fn default() -> Self {
        // Default to SITL UDP for development
        Self::Udp {
            address: "127.0.0.1:14550".into(),
        }
    }
}

/// Configuration for flight controller connection
#[derive(Debug, Clone)]
pub struct FcConfig {
    /// Connection type and parameters
    pub connection: FcConnectionType,
    /// System ID for this companion computer
    pub system_id: u8,
    /// Component ID for this companion computer
    pub component_id: u8,
    /// Target system ID (flight controller)
    pub target_system: u8,
    /// Target component ID (autopilot)
    pub target_component: u8,
}

impl Default for FcConfig {
    fn default() -> Self {
        Self {
            connection: FcConnectionType::default(),
            system_id: 255,      // Companion computer
            component_id: 190,   // MAV_COMP_ID_ONBOARD_COMPUTER
            target_system: 1,    // Autopilot
            target_component: 1, // MAV_COMP_ID_AUTOPILOT1
        }
    }
}

/// Events from the flight controller
#[derive(Debug, Clone)]
pub enum FcEvent {
    /// Connection established
    Connected,
    /// Connection lost
    Disconnected { reason: String },
    /// Received a MAVLink message
    Message(MavMessage),
    /// Heartbeat received from FC
    Heartbeat {
        autopilot: u8,
        mav_type: u8,
        system_status: u8,
        base_mode: u8,
        custom_mode: u32,
    },
}

/// Flight controller connection manager
pub struct FlightController {
    config: FcConfig,
    /// Connection handle (wrapped for thread safety)
    connection: Arc<RwLock<Option<Box<dyn MavConnection<MavMessage> + Send + Sync>>>>,
    /// Channel for outgoing messages
    outbound_tx: mpsc::Sender<MavMessage>,
    /// Channel for incoming events
    event_rx: mpsc::Receiver<FcEvent>,
    /// Flag indicating if connected
    connected: Arc<RwLock<bool>>,
}

impl FlightController {
    /// Create a new flight controller connection
    pub fn new(config: FcConfig) -> Self {
        let (outbound_tx, outbound_rx) = mpsc::channel::<MavMessage>(100);
        let (event_tx, event_rx) = mpsc::channel::<FcEvent>(100);
        let connected = Arc::new(RwLock::new(false));

        let fc = Self {
            config: config.clone(),
            connection: Arc::new(RwLock::new(None)),
            outbound_tx,
            event_rx,
            connected: connected.clone(),
        };

        // Spawn the connection handler
        let conn_arc = fc.connection.clone();
        let connected_clone = connected;
        tokio::spawn(async move {
            connection_loop(config, conn_arc, outbound_rx, event_tx, connected_clone).await;
        });

        fc
    }

    /// Check if connected to flight controller
    pub async fn is_connected(&self) -> bool {
        *self.connected.read().await
    }

    /// Send a MAVLink message to the flight controller
    pub async fn send(&self, msg: MavMessage) -> Result<()> {
        self.outbound_tx
            .send(msg)
            .await
            .map_err(|_| anyhow!("FC connection closed"))
    }

    /// Receive the next event from the flight controller
    pub async fn recv(&mut self) -> Option<FcEvent> {
        self.event_rx.recv().await
    }

    /// Get the configuration
    pub fn config(&self) -> &FcConfig {
        &self.config
    }

    /// Create MAVLink header for sending messages
    pub fn make_header(&self) -> MavHeader {
        MavHeader {
            system_id: self.config.system_id,
            component_id: self.config.component_id,
            sequence: 0, // Will be set by connection
        }
    }
}

/// Main connection loop
async fn connection_loop(
    config: FcConfig,
    connection: Arc<RwLock<Option<Box<dyn MavConnection<MavMessage> + Send + Sync>>>>,
    mut outbound_rx: mpsc::Receiver<MavMessage>,
    event_tx: mpsc::Sender<FcEvent>,
    connected: Arc<RwLock<bool>>,
) {
    loop {
        // Try to connect
        println!("[MAVLink] Connecting to flight controller...");

        let conn_result = match &config.connection {
            FcConnectionType::Serial { port, baud } => {
                let conn_str = format!("serial:{}:{}", port, baud);
                mavlink::connect::<MavMessage>(&conn_str)
            }
            FcConnectionType::Udp { address } => {
                let conn_str = format!("udpin:{}", address);
                mavlink::connect::<MavMessage>(&conn_str)
            }
            FcConnectionType::Tcp { address } => {
                let conn_str = format!("tcpin:{}", address);
                mavlink::connect::<MavMessage>(&conn_str)
            }
        };

        match conn_result {
            Ok(conn) => {
                println!("[MAVLink] Connected to flight controller");
                *connected.write().await = true;
                let _ = event_tx.send(FcEvent::Connected).await;

                // Store connection
                *connection.write().await = Some(conn);

                // Handle connection
                if let Err(e) = handle_connection(
                    &connection,
                    &config,
                    &mut outbound_rx,
                    &event_tx,
                ).await {
                    eprintln!("[MAVLink] Connection error: {}", e);
                    let _ = event_tx
                        .send(FcEvent::Disconnected {
                            reason: e.to_string(),
                        })
                        .await;
                }

                *connected.write().await = false;
                *connection.write().await = None;
            }
            Err(e) => {
                eprintln!("[MAVLink] Failed to connect: {}", e);
            }
        }

        // Wait before reconnecting
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    }
}

/// Handle an active connection
async fn handle_connection(
    connection: &Arc<RwLock<Option<Box<dyn MavConnection<MavMessage> + Send + Sync>>>>,
    config: &FcConfig,
    outbound_rx: &mut mpsc::Receiver<MavMessage>,
    event_tx: &mpsc::Sender<FcEvent>,
) -> Result<()> {
    let header = MavHeader {
        system_id: config.system_id,
        component_id: config.component_id,
        sequence: 0,
    };

    loop {
        tokio::select! {
            // Send outbound messages
            Some(msg) = outbound_rx.recv() => {
                let conn_guard = connection.read().await;
                if let Some(ref conn) = *conn_guard {
                    conn.send(&header, &msg)?;
                }
            }

            // Read incoming messages (with timeout)
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(10)) => {
                let conn_guard = connection.read().await;
                if let Some(ref conn) = *conn_guard {
                    // Try to receive without blocking
                    match conn.recv() {
                        Ok((_header, msg)) => {
                            // Handle heartbeat specially
                            if let MavMessage::HEARTBEAT(hb) = &msg {
                                let _ = event_tx.send(FcEvent::Heartbeat {
                                    autopilot: hb.autopilot as u8,
                                    mav_type: hb.mavtype as u8,
                                    system_status: hb.system_status as u8,
                                    base_mode: hb.base_mode.bits(),
                                    custom_mode: hb.custom_mode,
                                }).await;
                            }

                            let _ = event_tx.send(FcEvent::Message(msg)).await;
                        }
                        Err(mavlink::error::MessageReadError::Io(ref e))
                            if e.kind() == std::io::ErrorKind::WouldBlock => {
                            // No data available, continue
                        }
                        Err(e) => {
                            return Err(anyhow!("Read error: {}", e));
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = FcConfig::default();
        assert_eq!(config.system_id, 255);
        assert_eq!(config.target_system, 1);
    }

    #[test]
    fn test_connection_types() {
        let serial = FcConnectionType::Serial {
            port: "/dev/ttyACM0".into(),
            baud: 57600,
        };
        assert!(matches!(serial, FcConnectionType::Serial { .. }));

        let udp = FcConnectionType::Udp {
            address: "127.0.0.1:14550".into(),
        };
        assert!(matches!(udp, FcConnectionType::Udp { .. }));
    }
}
