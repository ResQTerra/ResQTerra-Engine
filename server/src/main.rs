use tokio::net::TcpListener;
use tokio::io::AsyncReadExt;
use prost::Message;

#[derive(Clone, PartialEq, Message)]
struct SensorPacket {
    #[prost(string, tag = "1")]
    device_id: String,

    #[prost(uint64, tag = "2")]
    timestamp: u64,

    #[prost(string, tag = "3")]
    payload: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:8080").await?;
    println!("Server listening on :8080");

    loop {
        let (mut socket, _) = listener.accept().await?;
        tokio::spawn(async move {
            let mut buf = vec![0u8; 1024];
            let n = socket.read(&mut buf).await.unwrap();

            let pkt = SensorPacket::decode(&buf[..n]).unwrap();
            println!(
                "received → device={} ts={} payload={}",
                pkt.device_id, pkt.timestamp, pkt.payload
            );
        });
    }
}
