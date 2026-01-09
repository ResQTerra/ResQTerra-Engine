mod command;
mod session;

use command::{CommandDispatcher, TimeoutTracker};
use resqterra_shared::{
    envelope, AckStatus, Command, CommandType, DroneState, Envelope, Header,
    Heartbeat, MessageType, now_ms,
};
use session::{DroneSession, SessionManager};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::time::{interval, Duration};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:8080").await?;
    let session_manager = Arc::new(SessionManager::new());
    let sequence_id = Arc::new(AtomicU64::new(0));

    // Create command dispatcher
    let dispatcher = Arc::new(CommandDispatcher::new(
        session_manager.clone(),
        sequence_id.clone(),
    ));

    println!("Server listening on :8080");
    println!("Waiting for drone connections...");

    // Spawn heartbeat monitor
    let sm_clone = session_manager.clone();
    tokio::spawn(async move {
        heartbeat_monitor(sm_clone).await;
    });

    // Spawn command timeout tracker
    let disp_clone = dispatcher.clone();
    tokio::spawn(async move {
        let tracker = TimeoutTracker::new(disp_clone);
        tracker.run().await;
    });

    // Spawn demo command sender
    let disp_clone = dispatcher.clone();
    tokio::spawn(async move {
        demo_command_sender(disp_clone).await;
    });

    loop {
        let (stream, addr) = listener.accept().await?;
        println!("New connection from: {}", addr);

        let sm = session_manager.clone();
        let seq = sequence_id.clone();
        let disp = dispatcher.clone();

        tokio::spawn(async move {
            handle_drone_session(stream, addr, sm, seq, disp).await;
        });
    }
}

async fn handle_drone_session(
    stream: tokio::net::TcpStream,
    addr: std::net::SocketAddr,
    session_manager: Arc<SessionManager>,
    sequence_id: Arc<AtomicU64>,
    dispatcher: Arc<CommandDispatcher>,
) {
    let mut session = DroneSession::new(stream, addr);

    // Read messages until disconnect
    while let Some(envelope) = session.recv().await {
        // Register session once we know the device ID
        if !session.device_id().is_empty() {
            session_manager.register(session.get_handle()).await;
        }

        handle_envelope(
            &envelope,
            &session,
            &session_manager,
            &sequence_id,
            &dispatcher,
        )
        .await;
    }

    // Unregister on disconnect
    let device_id = session.device_id();
    if !device_id.is_empty() {
        println!("Drone disconnected: {} ({})", device_id, addr);
        session_manager.unregister(device_id).await;
    } else {
        println!("Client disconnected: {}", addr);
    }
}

async fn handle_envelope(
    envelope: &Envelope,
    session: &DroneSession,
    session_manager: &SessionManager,
    sequence_id: &AtomicU64,
    dispatcher: &CommandDispatcher,
) {
    let header = match &envelope.header {
        Some(h) => h,
        None => {
            eprintln!("Received envelope without header from {}", session.addr());
            return;
        }
    };

    let device_id = &header.device_id;
    let msg_type = MessageType::try_from(header.msg_type).unwrap_or(MessageType::MsgUnknown);

    match &envelope.payload {
        Some(envelope::Payload::Heartbeat(hb)) => {
            session_manager.update_heartbeat(device_id).await;

            let state = DroneState::try_from(hb.state).unwrap_or(DroneState::DroneUnknown);
            session_manager.update_state(device_id, state).await;

            println!(
                "[{}] HEARTBEAT: uptime={}ms state={:?} healthy={} pending={}",
                device_id, hb.uptime_ms, state, hb.healthy, hb.pending_commands
            );

            // Send heartbeat response
            let seq = sequence_id.fetch_add(1, Ordering::SeqCst) + 1;
            let response = Envelope {
                header: Some(Header::new("server", MessageType::MsgHeartbeat, seq)),
                payload: Some(envelope::Payload::Heartbeat(Heartbeat::new(
                    0,
                    DroneState::DroneUnknown,
                    dispatcher.pending_count_for(device_id).await as u32,
                    true,
                ))),
            };

            if let Err(e) = session.get_handle().send(&response).await {
                eprintln!("Failed to send heartbeat response to {}: {}", device_id, e);
            }
        }

        Some(envelope::Payload::Telemetry(tel)) => {
            let state = DroneState::try_from(tel.state).unwrap_or(DroneState::DroneUnknown);
            session_manager.update_state(device_id, state).await;

            println!(
                "[{}] TELEMETRY: state={:?} uptime={}s",
                device_id, state, tel.uptime_seconds
            );

            if let Some(ref pos) = tel.position {
                println!(
                    "  Position: lat={:.6} lon={:.6} alt={:.1}m",
                    pos.latitude, pos.longitude, pos.altitude_m
                );
            }

            if let Some(ref bat) = tel.battery {
                println!(
                    "  Battery: {}% ({:.1}V, {:.1}A)",
                    bat.remaining_percent, bat.voltage, bat.current
                );
            }
        }

        Some(envelope::Payload::SensorData(data)) => {
            println!(
                "[{}] SENSOR_DATA: type={} mission={} chunk={}/{}  size={}",
                device_id,
                data.sensor_type,
                data.mission_id,
                data.chunk_index,
                data.total_chunks,
                data.data.len()
            );
        }

        Some(envelope::Payload::Ack(ack)) => {
            // Forward ACK to dispatcher for tracking
            dispatcher.handle_ack(device_id, ack).await;
        }

        Some(envelope::Payload::Command(_)) => {
            println!(
                "[{}] WARNING: Received COMMAND from drone (unexpected)",
                device_id
            );
        }

        None => {
            println!("[{}] {:?}: (no payload)", device_id, msg_type);
        }
    }
}

/// Monitor for dead drone sessions
async fn heartbeat_monitor(session_manager: Arc<SessionManager>) {
    let mut check_interval = interval(Duration::from_secs(5));

    loop {
        check_interval.tick().await;

        let dead = session_manager.remove_dead_sessions().await;
        for device_id in dead {
            println!("Drone {} timed out (no heartbeat)", device_id);
        }
    }
}

/// Demo: Send test commands to connected drones
async fn demo_command_sender(dispatcher: Arc<CommandDispatcher>) {
    let mut cmd_interval = interval(Duration::from_secs(20));

    // Wait a bit before starting
    tokio::time::sleep(Duration::from_secs(5)).await;

    loop {
        cmd_interval.tick().await;

        // Send STATUS_REQUEST to all drones
        let cmd = Command {
            command_id: dispatcher.next_command_id(),
            cmd_type: CommandType::CmdStatusRequest.into(),
            expires_at_ms: now_ms() + 10000, // 10 second expiry
            priority: 1,
            params: Some(resqterra_shared::command::Params::StatusRequest(
                resqterra_shared::StatusRequest {
                    requested_fields: vec![],
                },
            )),
        };

        println!("\n>>> Broadcasting STATUS_REQUEST to all drones");
        let sent = dispatcher.broadcast_command(cmd).await;
        if sent.is_empty() {
            println!("    No drones connected");
        } else {
            println!("    Sent to {} drone(s)", sent.len());
        }
    }
}
