use resqterra_shared::{codec, Envelope, MessageType};
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:8080").await?;
    println!("Server listening on :8080");

    loop {
        let (mut socket, addr) = listener.accept().await?;
        println!("Connection from: {}", addr);

        tokio::spawn(async move {
            let mut decoder = codec::FrameDecoder::new();
            let mut buf = vec![0u8; 4096];

            loop {
                match socket.read(&mut buf).await {
                    Ok(0) => {
                        println!("Client disconnected: {}", addr);
                        break;
                    }
                    Ok(n) => {
                        decoder.extend(&buf[..n]);

                        // Process all complete frames
                        while let Ok(Some(envelope)) = decoder.decode_next() {
                            handle_envelope(&envelope);
                        }
                    }
                    Err(e) => {
                        eprintln!("Read error from {}: {}", addr, e);
                        break;
                    }
                }
            }
        });
    }
}

fn handle_envelope(envelope: &Envelope) {
    let header = match &envelope.header {
        Some(h) => h,
        None => {
            eprintln!("Received envelope without header");
            return;
        }
    };

    let msg_type = MessageType::try_from(header.msg_type).unwrap_or(MessageType::MsgUnknown);

    match &envelope.payload {
        Some(resqterra_shared::envelope::Payload::Heartbeat(hb)) => {
            println!(
                "[{}] seq={} HEARTBEAT: uptime={}ms state={:?} healthy={}",
                header.device_id,
                header.sequence_id,
                hb.uptime_ms,
                resqterra_shared::DroneState::try_from(hb.state).unwrap_or(resqterra_shared::DroneState::DroneUnknown),
                hb.healthy
            );
        }
        Some(resqterra_shared::envelope::Payload::Telemetry(tel)) => {
            println!(
                "[{}] seq={} TELEMETRY: state={:?} uptime={}s",
                header.device_id,
                header.sequence_id,
                resqterra_shared::DroneState::try_from(tel.state).unwrap_or(resqterra_shared::DroneState::DroneUnknown),
                tel.uptime_seconds
            );
        }
        Some(resqterra_shared::envelope::Payload::SensorData(data)) => {
            println!(
                "[{}] seq={} SENSOR_DATA: type={} mission={} chunk={}/{}",
                header.device_id,
                header.sequence_id,
                data.sensor_type,
                data.mission_id,
                data.chunk_index,
                data.total_chunks
            );
        }
        Some(resqterra_shared::envelope::Payload::Ack(ack)) => {
            println!(
                "[{}] seq={} ACK: for_seq={} cmd={} status={:?}",
                header.device_id,
                header.sequence_id,
                ack.ack_sequence_id,
                ack.command_id,
                resqterra_shared::AckStatus::try_from(ack.status).unwrap_or(resqterra_shared::AckStatus::AckUnknown)
            );
        }
        Some(resqterra_shared::envelope::Payload::Command(_)) => {
            println!(
                "[{}] seq={} COMMAND received (server should not receive commands)",
                header.device_id, header.sequence_id
            );
        }
        None => {
            println!(
                "[{}] seq={} {:?}: (no payload)",
                header.device_id, header.sequence_id, msg_type
            );
        }
    }
}
