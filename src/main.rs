mod command;
mod connection;
mod protocol;
mod safety;
mod transport;

use command::CommandExecutor;
use connection::{ConnectionConfig, ConnectionEvent, ConnectionManager};
use protocol::*;
use safety::{SafetyAction, SafetyMonitor};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let config = ConnectionConfig {
        device_id: "edge-001".into(),
        server_5g: "127.0.0.1:8080".into(),
        server_bt: "127.0.0.1:9000".into(),
        ..Default::default()
    };

    println!("Edge device starting: {}", config.device_id);
    println!("  5G server: {}", config.server_5g);
    println!("  BT relay:  {}", config.server_bt);

    let mut conn = ConnectionManager::new(config.clone());

    // Create command executor (shares sequence_id with connection manager internally)
    let cmd_executor = Arc::new(CommandExecutor::new(
        config.device_id.clone(),
        Arc::new(std::sync::atomic::AtomicU64::new(1000)), // Start from 1000 to avoid conflicts
    ));

    // Create safety monitor
    let safety_monitor = Arc::new(SafetyMonitor::new());
    let _safety_handle = safety_monitor.start_monitoring().await;
    println!("Safety monitor started");

    // Spawn safety action handler
    let safety_clone = safety_monitor.clone();
    let conn_clone = conn.get_sender();
    tokio::spawn(async move {
        handle_safety_actions(safety_clone, conn_clone).await;
    });

    // Main event loop
    loop {
        match conn.recv().await {
            Some(ConnectionEvent::Connected { transport }) => {
                println!("Connected via {}", transport);
            }
            Some(ConnectionEvent::Disconnected { reason }) => {
                println!("Disconnected: {}", reason);
            }
            Some(ConnectionEvent::TransportSwitched { from, to }) => {
                println!("Transport switched: {} -> {}", from, to);
            }
            Some(ConnectionEvent::ConnectionFailed { reason }) => {
                eprintln!("Connection failed: {}", reason);
            }
            Some(ConnectionEvent::Received(envelope)) => {
                handle_server_message(&envelope, &conn, &cmd_executor, &safety_monitor).await;
            }
            None => {
                eprintln!("Connection manager closed");
                break;
            }
        }
    }
}

async fn handle_server_message(
    envelope: &Envelope,
    conn: &ConnectionManager,
    cmd_executor: &CommandExecutor,
    safety_monitor: &SafetyMonitor,
) {
    let header = match &envelope.header {
        Some(h) => h,
        None => {
            eprintln!("Received envelope without header");
            return;
        }
    };

    let msg_type = MessageType::try_from(header.msg_type).unwrap_or(MessageType::MsgUnknown);

    println!(
        "Received from server: seq={} type={:?}",
        header.sequence_id, msg_type
    );

    match &envelope.payload {
        Some(envelope::Payload::Command(cmd)) => {
            // Execute command and get ACK response
            let ack_envelope = cmd_executor.execute(cmd, header).await;

            // Send ACK back to server
            if let Err(e) = conn.send(ack_envelope).await {
                eprintln!("Failed to send ACK: {}", e);
            }
        }
        Some(envelope::Payload::Heartbeat(hb)) => {
            // Update safety monitor with server heartbeat
            safety_monitor.update_server_heartbeat().await;
            println!("  Server heartbeat: healthy={}", hb.healthy);
        }
        Some(envelope::Payload::Ack(ack)) => {
            let status = AckStatus::try_from(ack.status).unwrap_or(AckStatus::AckUnknown);
            println!(
                "  Server ACK: for_seq={} status={:?}",
                ack.ack_sequence_id, status
            );
        }
        _ => {
            println!("  Unhandled payload type");
        }
    }
}

/// Handle safety actions triggered by the monitor
async fn handle_safety_actions(
    safety_monitor: Arc<SafetyMonitor>,
    sender: tokio::sync::mpsc::Sender<Envelope>,
) {
    loop {
        match safety_monitor.recv_action().await {
            Some(SafetyAction::ReturnToHome { reason }) => {
                println!("[MAIN] Safety RTH triggered: {}", reason);
                // TODO: Send RTH command to flight controller via MAVLink
                // For now, just log it
            }
            Some(SafetyAction::EmergencyStop { reason }) => {
                println!("[MAIN] EMERGENCY STOP: {}", reason);
                // TODO: Send emergency stop to flight controller
            }
            Some(SafetyAction::StateChanged { from, to }) => {
                println!("[MAIN] State changed: {:?} -> {:?}", from, to);
            }
            Some(SafetyAction::None) => {}
            None => {
                eprintln!("[MAIN] Safety monitor channel closed");
                break;
            }
        }
    }
    let _ = sender; // Keep sender alive for potential future use
}
