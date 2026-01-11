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
use resqterra_shared::{Envelope, Header, MessageType};

use tracing::{info, error, warn, debug};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .init();

    let config = ConnectionConfig {
        device_id: "edge-001".into(),
        server_5g: "127.0.0.1:8080".into(),
        ..Default::default()
    };

    info!("Edge device starting: {}", config.device_id);
    info!("  5G server: {}", config.server_5g);

    let mut conn = ConnectionManager::new(config.clone());

    // Create flight controller connection
    let fc_config = FcConfig {
        connection: FcConnectionType::Udp {
            address: "127.0.0.1:14550".into(), // SITL default
        },
        ..Default::default()
    };
    let (flight_controller, mut fc_receiver) = FlightController::new(fc_config.clone());
    let mav_cmd_sender = Arc::new(MavCommandSender::new(
        flight_controller.clone(),
        fc_config.target_system,
        fc_config.target_component,
    ));
    let telemetry_reader = Arc::new(TelemetryReader::new());
    info!("Flight controller bridge initialized (UDP:14550)");

    // Create command executor (shares sequence_id with connection manager internally)
    let sequence_id = Arc::new(std::sync::atomic::AtomicU64::new(1000));
    let cmd_executor = Arc::new(CommandExecutor::new(
        config.device_id.clone(),
        sequence_id.clone(),
        mav_cmd_sender.clone(),
    ));

    // Create safety monitor
    let safety_monitor = Arc::new(SafetyMonitor::new());
    let _safety_handle = safety_monitor.start_monitoring().await;
    info!("Safety monitor started");

    // Spawn flight controller event handler
    let telemetry_clone = telemetry_reader.clone();
    let safety_clone = safety_monitor.clone();
    tokio::spawn(async move {
        handle_fc_events(&mut fc_receiver, telemetry_clone, safety_clone).await;
    });

    // Spawn safety action handler with MAVLink integration
    let safety_clone = safety_monitor.clone();
    let conn_clone = conn.get_sender();
    let mav_cmd_sender_clone = mav_cmd_sender.clone();
    tokio::spawn(async move {
        handle_safety_actions(safety_clone, conn_clone, mav_cmd_sender_clone).await;
    });

    // Spawn telemetry streaming task
    let telemetry_clone = telemetry_reader.clone();
    let conn_clone = conn.get_sender();
    let device_id_clone = config.device_id.clone();
    let seq_clone = sequence_id.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(tokio::time::Duration::from_millis(1000));
        loop {
            ticker.tick().await;
            let telemetry = telemetry_clone.get_telemetry().await;
            let seq = seq_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            
            let envelope = Envelope {
                header: Some(Header::new(&device_id_clone, MessageType::MsgTelemetry, seq)),
                payload: Some(resqterra_shared::envelope::Payload::Telemetry(telemetry)),
            };

            if let Err(e) = conn_clone.send(envelope).await {
                error!("Failed to stream telemetry: {}", e);
            }
        }
    });

    // Main event loop
    loop {
        match conn.recv().await {
            Some(ConnectionEvent::Connected { transport }) => {
                info!("Connected via {}", transport);
            }
            Some(ConnectionEvent::Disconnected { reason }) => {
                warn!("Disconnected: {}", reason);
            }
            Some(ConnectionEvent::TransportSwitched { from, to }) => {
                info!("Transport switched: {} -> {}", from, to);
            }
            Some(ConnectionEvent::ConnectionFailed { reason }) => {
                error!("Connection failed: {}", reason);
            }
            Some(ConnectionEvent::Received(envelope)) => {
                handle_server_message(&envelope, &conn, &cmd_executor, &safety_monitor).await;
            }
            None => {
                error!("Connection manager closed");
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
            error!("Received envelope without header");
            return;
        }
    };

    let msg_type = MessageType::try_from(header.msg_type).unwrap_or(MessageType::MsgUnknown);

    debug!(
        "Received from server: seq={} type={:?}",
        header.sequence_id, msg_type
    );

    match &envelope.payload {
        Some(envelope::Payload::Command(cmd)) => {
            // Execute command and get ACK response
            let ack_envelope = cmd_executor.execute(cmd, header).await;

            // Send ACK back to server
            if let Err(e) = conn.send(ack_envelope).await {
                error!("Failed to send ACK: {}", e);
            }
        }
        Some(envelope::Payload::Heartbeat(hb)) => {
            // Update safety monitor with server heartbeat
            safety_monitor.update_server_heartbeat().await;
            debug!("  Server heartbeat: healthy={}", hb.healthy);
        }
        Some(envelope::Payload::Ack(ack)) => {
            let status = AckStatus::try_from(ack.status).unwrap_or(AckStatus::AckUnknown);
            debug!(
                "  Server ACK: for_seq={} status={:?}",
                ack.ack_sequence_id, status
            );
        }
        _ => {
            debug!("  Unhandled payload type");
        }
    }
}

/// Handle safety actions triggered by the monitor
async fn handle_safety_actions(
    safety_monitor: Arc<SafetyMonitor>,
    _sender: tokio::sync::mpsc::Sender<Envelope>,
    mav_cmd_sender: Arc<MavCommandSender>,
) {
    loop {
        match safety_monitor.recv_action().await {
            Some(SafetyAction::ReturnToHome { reason }) => {
                info!("[MAIN] Safety RTH triggered: {}", reason);
                let rth_params = resqterra_shared::ReturnToHome {
                    altitude_m: 0.0,
                    speed_mps: 0.0,
                };
                if let Err(e) = mav_cmd_sender.return_to_home(&rth_params).await {
                    error!("Failed to send safety RTH: {}", e);
                }
            }
            Some(SafetyAction::EmergencyStop { reason }) => {
                error!("[MAIN] EMERGENCY STOP: {}", reason);
                if let Err(e) = mav_cmd_sender.emergency_stop().await {
                    error!("Failed to send safety emergency stop: {}", e);
                }
            }
            Some(SafetyAction::StateChanged { from, to }) => {
                info!("[MAIN] State changed: {:?} -> {:?}", from, to);
            }
            None => {
                error!("[MAIN] Safety monitor channel closed");
                break;
            }
        }
    }
}

/// Handle events from the flight controller
async fn handle_fc_events(
    fc_receiver: &mut mavlink::FcEventReceiver,
    telemetry: Arc<TelemetryReader>,
    _safety: Arc<SafetyMonitor>,
) {
    loop {
        match fc_receiver.recv().await {
            Some(FcEvent::Connected) => {
                info!("[FC] Connected to flight controller");
            }
            Some(FcEvent::Disconnected { reason }) => {
                warn!("[FC] Disconnected: {}", reason);
            }
            Some(FcEvent::Heartbeat {
                autopilot,
                mav_type,
                system_status,
                base_mode,
                custom_mode,
            }) => {
                debug!(
                    "[FC] Heartbeat: type={} autopilot={} status={} mode={} custom={}",
                    mav_type, autopilot, system_status, base_mode, custom_mode
                );
            }
            Some(FcEvent::Message(msg)) => {
                // Process telemetry messages
                telemetry.process_message(&msg).await;
            }
            None => {
                error!("[FC] Flight controller channel closed");
                break;
            }
        }
    }
}