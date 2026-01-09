mod protocol;
mod transport;

use protocol::*;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    let device_id = "edge-001";
    let mut sequence_id: u64 = 0;

    println!("Edge device starting: {}", device_id);

    loop {
        sequence_id += 1;

        // Create a heartbeat message wrapped in an envelope
        let envelope = Envelope {
            header: Some(Header::new(device_id, MessageType::MsgHeartbeat, sequence_id)),
            payload: Some(envelope::Payload::Heartbeat(Heartbeat::new(
                sequence_id * 5000, // uptime in ms
                DroneState::DroneIdle,
                0,    // pending commands
                true, // healthy
            ))),
        };

        // Encode with length-prefix framing
        let encoded = match codec::encode(&envelope) {
            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!("Encode error: {}", e);
                continue;
            }
        };

        // Try 5G first, fallback to Bluetooth
        if let Err(e) = transport::five_g::send(&encoded).await {
            println!("5G failed ({}), trying Bluetooth...", e);
            if let Err(e) = transport::bluetooth::send(&encoded).await {
                eprintln!("Bluetooth also failed: {}", e);
            }
        }

        sleep(Duration::from_secs(5)).await;
    }
}
