use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:9000").await?;
    println!("Relay listening on :9000");

    loop {
        let (mut socket, _) = listener.accept().await?;
        tokio::spawn(async move {
            let mut buf = vec![0u8; 1024];
            let n = socket.read(&mut buf).await.unwrap();

            let mut server = TcpStream::connect("127.0.0.1:8080").await.unwrap();
            server.write_all(&buf[..n]).await.unwrap();
        });
    }
}
