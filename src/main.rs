mod command;
mod connection;
mod protocol;
mod transport;

use command::CommandExecutor;
use connection::{ConnectionConfig, ConnectionEvent, ConnectionManager};
use protocol::*;
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
                handle_server_message(&envelope, &conn, &cmd_executor).await;
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
