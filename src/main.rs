mod protocol;
mod transport;

use protocol::*;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    loop {
        let packet = SensorPacket {
            device_id: "edge-001".into(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            payload: "hello from edge".into(),
        };

        let encoded = encode(&packet);

        // simulate: try 5G first, fallback to BT
        if transport::five_g::send(&encoded).await.is_err() {
            println!("5G failed → Bluetooth fallback");
            let _ = transport::bluetooth::send(&encoded).await;
        }

        sleep(Duration::from_secs(5)).await;
    }
}
