mod command;
mod connection;
mod mavlink;
mod protocol;
mod safety;
mod transport;

use command::CommandExecutor;
use connection::{ConnectionConfig, ConnectionEvent, ConnectionManager};
use mavlink::{FcConfig, FcConnectionType, FcEvent, FlightController, MavCommandSender, TelemetryReader};
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

    // Create flight controller connection
    let fc_config = FcConfig {
        connection: FcConnectionType::Udp {
            address: "127.0.0.1:14550".into(), // SITL default
        },
        ..Default::default()
    };
    let mut flight_controller = FlightController::new(fc_config.clone());
    let mav_cmd_sender = Arc::new(MavCommandSender::new(
        fc_config.target_system,
        fc_config.target_component,
    ));
    let telemetry_reader = Arc::new(TelemetryReader::new());
    println!("Flight controller bridge initialized (UDP:14550)");

    // Spawn flight controller event handler
    let telemetry_clone = telemetry_reader.clone();
    let safety_clone = safety_monitor.clone();
    tokio::spawn(async move {
        handle_fc_events(&mut flight_controller, telemetry_clone, safety_clone).await;
    });

    // Spawn safety action handler with MAVLink integration
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

/// Handle events from the flight controller
async fn handle_fc_events(
    fc: &mut FlightController,
    telemetry: Arc<TelemetryReader>,
    _safety: Arc<SafetyMonitor>,
) {
    loop {
        match fc.recv().await {
            Some(FcEvent::Connected) => {
                println!("[FC] Connected to flight controller");
            }
            Some(FcEvent::Disconnected { reason }) => {
                println!("[FC] Disconnected: {}", reason);
            }
            Some(FcEvent::Heartbeat {
                autopilot,
                mav_type,
                system_status,
                base_mode,
                custom_mode,
            }) => {
                println!(
                    "[FC] Heartbeat: type={} autopilot={} status={} mode={} custom={}",
                    mav_type, autopilot, system_status, base_mode, custom_mode
                );
            }
            Some(FcEvent::Message(msg)) => {
                // Process telemetry messages
                telemetry.process_message(&msg).await;
            }
            None => {
                eprintln!("[FC] Flight controller channel closed");
                break;
            }
        }
    }
}
