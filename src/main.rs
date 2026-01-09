mod connection;
mod protocol;
mod transport;

use connection::{ConnectionConfig, ConnectionEvent, ConnectionManager};
use protocol::*;

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

    let mut conn = ConnectionManager::new(config);

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
                handle_server_message(&envelope, &conn).await;
            }
            None => {
                eprintln!("Connection manager closed");
                break;
            }
        }
    }
}

async fn handle_server_message(envelope: &Envelope, conn: &ConnectionManager) {
    let header = match &envelope.header {
        Some(h) => h,
        None => {
            eprintln!("Received envelope without header");
            return;
        }
    };

    println!(
        "Received from server: seq={} type={:?}",
        header.sequence_id,
        MessageType::try_from(header.msg_type).unwrap_or(MessageType::MsgUnknown)
    );

    match &envelope.payload {
        Some(envelope::Payload::Command(cmd)) => {
            println!(
                "  Command: id={} type={:?}",
                cmd.command_id,
                CommandType::try_from(cmd.cmd_type).unwrap_or(CommandType::CmdUnknown)
            );

            // Send ACK back
            let ack_envelope = Envelope {
                header: Some(Header::new(
                    conn.device_id(),
                    MessageType::MsgAck,
                    conn.next_sequence_id(),
                )),
                payload: Some(envelope::Payload::Ack(Ack::received(
                    header.sequence_id,
                    cmd.command_id,
                ))),
            };

            if let Err(e) = conn.send(ack_envelope).await {
                eprintln!("Failed to send ACK: {}", e);
            }
        }
        Some(envelope::Payload::Heartbeat(hb)) => {
            println!("  Server heartbeat: healthy={}", hb.healthy);
        }
        _ => {
            println!("  Unhandled payload type");
        }
    }
}
