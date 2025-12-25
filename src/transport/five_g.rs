use tokio::net::TcpStream;
use tokio::io::AsyncWriteExt;

pub async fn send(data: &[u8]) -> anyhow::Result<()> {
    let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    stream.write_all(data).await?;
    Ok(())
}
